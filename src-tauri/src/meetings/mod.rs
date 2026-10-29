//! Meeting capture: record the mic and the call app's audio as two tracks, transcribe after stop,
//! keep the result in SQLite, export it as Markdown or text.
mod capture;
mod store;
mod track;
mod transcript;

use crate::{dictation::{Transcriber, Utterance}, permissions, with_engine, Db};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    AppHandle, Manager, Wry,
};

pub const TRAY_ID: &str = "main";

struct AudioDir(PathBuf);

struct Active {
    id: i64,
    started: Instant,
    capture: capture::Capture,
}

#[derive(Default)]
struct Recorder(Mutex<Option<Active>>);

/// Tray items that change with the recording state.
struct TrayUi {
    menu: Menu<Wry>,
    record: MenuItem<Wry>,
    status: MenuItem<Wry>,
}

// shortcut: the record items are in the tray whether or not the Meetings module is switched on in Settings;
// gate them on `meetings_enabled` when the modules page starts to matter.
pub fn setup(app: &tauri::App, root: &Path, menu: Menu<Wry>, record: MenuItem<Wry>) -> Result<(), Box<dyn std::error::Error>> {
    let audio = root.join("audio");
    fs::create_dir_all(&audio)?;
    let queued = {
        let db = app.state::<Db>();
        let conn = db.0.lock().unwrap();
        store::init(&conn)?;
        store::recover(&conn)?
    };
    app.manage(AudioDir(audio));
    app.manage(Recorder::default());
    app.manage(TrayUi { menu, record, status: MenuItem::new(app, "", false, None::<&str>)? });
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
            show_recording(app, None);
            let _ = store::set_transcribing(&app.state::<Db>().0.lock().unwrap(), id);
            spawn_transcription(app, id);
        }
        None => {
            let active = begin(app)?;
            show_recording(app, Some(&active.capture));
            *slot = Some(active);
        }
    }
    Ok(())
}

fn begin(app: &AppHandle) -> Result<Active, String> {
    if !permissions::granted("Microphone") {
        permissions::request("Microphone");
        return Err("Myna needs Microphone access. Allow it when macOS asks (or in System Settings → Privacy & Security → Microphone), then start again.".into());
    }
    let db = app.state::<Db>();
    let id = store::create(&db.0.lock().unwrap(), now()).map_err(|e| e.to_string())?;
    let dir = app.state::<AudioDir>().0.join(id.to_string());
    let capture = fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| capture::start(&dir));
    let capture = capture.inspect_err(|_| {
        store::delete(&db.0.lock().unwrap(), id);
        let _ = fs::remove_dir_all(&dir);
    })?;
    let _ = store::set_source(&db.0.lock().unwrap(), id, capture.app.as_deref(), capture.note.as_deref());
    Ok(Active { id, started: Instant::now(), capture })
}

/// Menu text for the recording state: a dot plus words, never colour alone.
fn show_recording(app: &AppHandle, capture: Option<&capture::Capture>) {
    let ui = app.state::<TrayUi>();
    let _ = ui.record.set_text(if capture.is_some() { "Stop and transcribe" } else { "Start meeting recording" });
    match capture {
        Some(c) => {
            let source = c.app.as_ref().map_or("mic only".to_string(), |a| format!("system audio from {a}"));
            let _ = ui.status.set_text(format!("● Mic live · {source}"));
            let _ = ui.menu.insert(&ui.status, 0);
        }
        None => {
            let _ = ui.menu.remove(&ui.status);
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let _ = tray.set_title(None::<&str>);
            }
        }
    }
}

/// For failures with no window to show them in (the tray item).
pub fn alert(message: &str) {
    let _ = std::process::Command::new("osascript")
        .args(["-e", &format!("display alert \"Myna\" message {message:?}")])
        .spawn();
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

#[tauri::command]
pub fn list_meetings(db: tauri::State<Db>) -> Result<Vec<store::Meeting>, String> {
    store::list(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct Detail {
    meeting: store::Meeting,
    segments: Vec<store::Segment>,
}

#[tauri::command]
pub fn get_meeting(db: tauri::State<Db>, id: i64) -> Result<Detail, String> {
    let conn = db.0.lock().unwrap();
    Ok(Detail {
        meeting: store::get(&conn, id).map_err(|e| e.to_string())?,
        segments: store::segments(&conn, id).map_err(|e| e.to_string())?,
    })
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
