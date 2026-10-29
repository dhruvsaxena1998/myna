//! Meeting capture: record the mic and the call app's audio as two tracks, transcribe after stop,
//! keep the result in SQLite, export it as Markdown or text.
mod capture;
mod screen;
pub(crate) mod mute;
pub(crate) mod shots;
pub(crate) mod store;
pub mod suggest;
pub mod summary;
mod track;
mod transcript;

use crate::{dictation::{Transcriber, Utterance}, permissions, with_engine, Db};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "main";

pub struct AudioDir(PathBuf);
pub struct ScreenshotsDir(PathBuf);

struct Active {
    id: i64,
    started: Instant,
    /// Held for its Drop, which stops the capture.
    _capture: capture::Capture,
    _shots: shots::Screenshots,
}

#[derive(Default)]
struct Recorder(Mutex<Option<Active>>);

pub fn setup(app: &tauri::App, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let (audio, screenshots) = (root.join("audio"), root.join("screenshots"));
    fs::create_dir_all(&audio)?;
    fs::create_dir_all(&screenshots)?;
    let queued = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        store::init(&conn)?;
        shots::init(&conn)?;
        summary::init(&conn)?;
        store::recover(&conn)?
    };
    app.manage(AudioDir(audio));
    app.manage(ScreenshotsDir(screenshots));
    app.manage(Recorder::default());
    for id in queued {
        spawn_transcription(app.handle(), id);
    }
    let handle = app.handle().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        // Holding the lock keeps a late tick from relabelling the tray after a stop cleared it.
        let rec = handle.state::<Recorder>();
        let slot = rec.0.lock().unwrap();
        if let (Some(a), Some(tray)) = (&*slot, handle.tray_by_id(TRAY_ID)) {
            let _ = tray.set_title(Some(format!("● {}", transcript::clock(a.started.elapsed().as_millis() as u64))));
        }
    });
    Ok(())
}

/// Quit path: stop capturing so both files are closed; the next launch transcribes them.
pub fn shutdown(app: &AppHandle) {
    if let Some(a) = app.state::<Recorder>().0.lock().unwrap().take() {
        let id = a.id;
        drop(a);
        let _ = store::set_transcribing(&app.state::<Db>().0.lock().unwrap(), id);
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Start when idle, stop and transcribe when recording.
pub fn toggle(app: &AppHandle) -> Result<(), String> {
    let rec = app.state::<Recorder>();
    let mut slot = rec.0.lock().unwrap();
    match slot.take() {
        Some(active) => {
            let id = active.id;
            drop(active); // tears down the tap and mic, closes both files
            clear_tray_title(app);
            set_tray_menu(app, false);
            let _ = store::set_transcribing(&app.state::<Db>().0.lock().unwrap(), id);
            spawn_transcription(app, id);
        }
        None => {
            *slot = Some(begin(app)?);
            set_tray_menu(app, true);
        }
    }
    drop(slot); // the pill reads the recording from the database, not from this lock
    crate::settle_pill(app);
    Ok(())
}

/// Flips the mic between live and muted, from the popover or the hotkey, and records when. Does nothing
/// outside a recording.
pub fn mute_toggle(app: &AppHandle) -> Result<(), String> {
    {
        let rec = app.state::<Recorder>();
        let slot = rec.0.lock().unwrap();
        let Some(a) = slot.as_ref() else { return Ok(()) };
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        let events = store::mute_events(&conn, a.id).map_err(|e| e.to_string())?;
        // A live mic mutes; muted, or not yet known, unmutes.
        let muted = mute::mic_now(&events, mute::always_record(&conn, a.id)).live;
        let at_ms = a.started.elapsed().as_millis() as u64;
        store::add_mute_event(&conn, a.id, &mute::Event { at_ms, muted, source: mute::Source::Manual }).map_err(|e| e.to_string())?;
    }
    crate::settle_pill(app);
    Ok(())
}

/// What the popover and the pill say about the mic; None when nothing is recording.
pub fn mic(conn: &Connection) -> Option<mute::Mic> {
    let id = store::recording(conn)?;
    Some(mute::mic_now(&store::mute_events(conn, id).ok()?, mute::always_record(conn, id)))
}

/// The right-click fallback menu, for when the popover page won't load. Stop shows only while recording.
pub fn set_tray_menu(app: &AppHandle, recording: bool) {
    use tauri::menu::{Menu, MenuItem};
    let build = || -> tauri::Result<Menu<tauri::Wry>> {
        let menu = Menu::new(app)?;
        if recording {
            menu.append(&MenuItem::with_id(app, "stop", "Stop recording", true, None::<&str>)?)?;
        }
        menu.append(&MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, "quit", "Quit Myna", true, None::<&str>)?)?;
        Ok(menu)
    };
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build()) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// A fallback-menu pick: "stop" only acts on a live recording, the rest go through the popover's paths.
pub fn tray_menu_pick(app: &AppHandle, id: &str) {
    match id {
        "stop" if app.state::<Recorder>().0.lock().unwrap().is_some() => {
            let _ = toggle(app);
        }
        "settings" | "quit" => crate::popover::popover_go(app.clone(), id.into(), None),
        _ => {}
    }
}

fn begin(app: &AppHandle) -> Result<Active, String> {
    if !permissions::granted("Microphone") {
        permissions::request("Microphone");
        return Err("Myna needs Microphone access. Allow it when macOS asks (or in System Settings → Privacy & Security → Microphone), then start again.".into());
    }
    let db = app.state::<Db>();
    // Taken before the mic opens so mute toggles line up with the mic track's clock.
    let started = Instant::now();
    let id = store::create(&db.0.lock().unwrap(), now()).map_err(|e| e.to_string())?;
    let dir = app.state::<AudioDir>().0.join(id.to_string());
    let capture = fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| capture::start(&dir));
    let capture = capture.inspect_err(|_| {
        store::delete(&db.0.lock().unwrap(), id);
        let _ = fs::remove_dir_all(&dir);
    })?;
    let _ = store::set_source(&db.0.lock().unwrap(), id, capture.app.as_deref(), capture.note.as_deref());
    let frames = app.state::<ScreenshotsDir>().0.join(id.to_string());
    Ok(Active { id, started, _capture: capture, _shots: shots::Screenshots::start(app, id, frames, started) })
}

fn clear_tray_title(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(None::<&str>);
    }
}

// One model in memory at a time: queued meetings wait their turn.
static TRANSCRIBING: Mutex<()> = Mutex::new(());

/// The shared speech engine, borrowed one call at a time so a long meeting never holds up dictation.
struct Shared(AppHandle);

impl Transcriber for Shared {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        with_engine(&self.0, |t| t.transcribe(audio))?
    }
    fn transcribe_timed(&mut self, audio: &[f32]) -> Result<Vec<Utterance>, String> {
        with_engine(&self.0, |t| t.transcribe_timed(audio))?
    }
}

fn spawn_transcription(app: &AppHandle, id: i64) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _turn = TRANSCRIBING.lock().unwrap_or_else(|e| e.into_inner());
        let (db, audio) = (app.state::<Db>(), app.state::<AudioDir>());
        let result = transcript::run(&db.0, &audio.0, id, &mut Shared(app.clone()));
        if let Err(e) = result {
            store::fail(&db.0.lock().unwrap(), id, &e);
        }
    });
}

#[tauri::command(async)]
pub fn meeting_toggle(app: AppHandle) -> Result<(), String> {
    toggle(&app)
}

#[tauri::command(async)]
pub fn meeting_mute(app: AppHandle) -> Result<(), String> {
    mute_toggle(&app)
}

#[tauri::command]
pub fn list_meetings(db: tauri::State<Db>) -> Result<Vec<store::Meeting>, String> {
    store::list(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_folders(db: tauri::State<Db>) -> Result<Vec<store::Folder>, String> {
    store::folders(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_folder(db: tauri::State<Db>, name: String) -> Result<store::Folder, String> {
    store::create_folder(&db.0.lock().unwrap(), &name)
}

#[tauri::command]
pub fn rename_folder(db: tauri::State<Db>, id: i64, name: String) -> Result<(), String> {
    store::rename_folder(&db.0.lock().unwrap(), id, &name)
}

#[tauri::command]
pub fn delete_folder(db: tauri::State<Db>, id: i64) -> Result<(), String> {
    store::delete_folder(&db.0.lock().unwrap(), id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_meeting(db: tauri::State<Db>, id: i64, folder: Option<i64>) -> Result<(), String> {
    store::move_meeting(&db.0.lock().unwrap(), id, folder)
}

#[derive(Serialize)]
pub struct Detail {
    meeting: store::Meeting,
    segments: Vec<store::Segment>,
    /// Screenshots kept, in time order, and how many were taken before duplicates were dropped.
    shots: Vec<shots::Shot>,
    shots_taken: usize,
    /// The track files on disk (mic, and system audio when it was tapped), for the player.
    audio: Vec<String>,
}

#[tauri::command]
pub fn get_meeting(db: tauri::State<Db>, audio: tauri::State<AudioDir>, frames: tauri::State<ScreenshotsDir>, id: i64) -> Result<Detail, String> {
    let conn = db.0.lock().unwrap();
    let (shots, shots_taken) = shots::list(&conn, id, &frames.0.join(id.to_string())).map_err(|e| e.to_string())?;
    let tracks = ["mic.wav", "system.wav"].map(|f| audio.0.join(id.to_string()).join(f));
    Ok(Detail {
        meeting: store::get(&conn, id).map_err(|e| e.to_string())?,
        segments: store::segments(&conn, id).map_err(|e| e.to_string())?,
        shots,
        shots_taken,
        audio: tracks.into_iter().filter(|p| p.exists()).map(|p| p.display().to_string()).collect(),
    })
}

/// Windows to offer as the screenshot target. Async: ScreenCaptureKit answers on its own queue.
#[tauri::command(async)]
pub fn screenshot_windows() -> Result<Vec<screen::Window>, String> {
    if !permissions::granted("Screen Recording") {
        return Err("Allow Screen Recording in Settings → Meetings to list windows.".into());
    }
    screen::windows()
}

/// Screenshots follow this window until it closes; None goes back to the app that has the mic.
#[tauri::command]
pub fn screenshot_pick(id: Option<u32>) {
    shots::choose(id)
}

/// Pulls a collapsed segment into the transcript (`included`), or puts it back.
#[tauri::command]
pub fn include_segment(db: tauri::State<Db>, id: i64, included: bool) -> Result<(), String> {
    store::include_segment(&db.0.lock().unwrap(), id, included).map_err(|e| e.to_string())
}

/// The audio of one segment as a WAV, for the Play button.
#[tauri::command(async)]
pub fn segment_clip(app: AppHandle, db: tauri::State<Db>, id: i64) -> Result<tauri::ipc::Response, String> {
    let (meeting, s) = store::segment(&db.0.lock().unwrap(), id).map_err(|e| e.to_string())?;
    let track = if s.speaker == store::ME { "mic.wav" } else { "system.wav" };
    track::clip(&app.state::<AudioDir>().0.join(meeting.to_string()).join(track), s.start_ms, s.end_ms).map(tauri::ipc::Response::new)
}

#[tauri::command]
pub fn retry_transcription(app: AppHandle, db: tauri::State<Db>, id: i64) -> Result<(), String> {
    {
        let conn = db.0.lock().unwrap();
        if store::get(&conn, id).map_err(|e| e.to_string())?.status != "failed" {
            return Ok(());
        }
        store::set_transcribing(&conn, id).map_err(|e| e.to_string())?;
    }
    spawn_transcription(&app, id);
    Ok(())
}

/// Writes `format` ("md" or "txt") into Downloads and reveals it in Finder. Returns the path.
#[tauri::command]
pub fn export_meeting(app: AppHandle, db: tauri::State<Db>, id: i64, format: String) -> Result<String, String> {
    let format = transcript::Format::parse(&format).ok_or("unknown export format")?;
    let (meeting, segments) = {
        let conn = db.0.lock().unwrap();
        (store::get(&conn, id).map_err(|e| e.to_string())?, store::segments(&conn, id).map_err(|e| e.to_string())?)
    };
    let path = app.path().download_dir().map_err(|e| e.to_string())?.join(transcript::file_name(&meeting, format));
    fs::write(&path, transcript::export(&meeting, &segments, format)).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
    Ok(path.display().to_string())
}
