mod apple;
mod dictation;
mod download;
mod models;
mod permissions;
mod pill;
mod runtime;
mod storage;
mod voice;

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager, WindowEvent, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Result of creating the data folder and database: the folder path, or the error to show.
struct Storage(Result<String, String>);

/// Managed only when the database opened.
struct Data {
    db: Mutex<Connection>,
    models_dir: PathBuf,
    macos: u32,
    /// Model id -> cancel flag, for each download in progress.
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The mic recording while the dictation hotkey is held.
    recording: Mutex<Option<dictation::Recording>>,
    /// The loaded speech-to-text model and its id, kept between dictations (loading takes seconds).
    transcriber: Mutex<Loaded>,
    /// Bumped on every dictation, so an unload timer knows the model was used after it started.
    used: AtomicU64,
    /// The shortcut registered with macOS right now, if any.
    shortcut: Mutex<Option<String>>,
    /// Set while Settings records a new shortcut, so pressing the old one types into the recorder.
    paused: AtomicBool,
}

/// "Start voice typing" in the menubar menu, which shows the shortcut.
struct VoiceItem(MenuItem<Wry>);

type Loaded = Option<(String, Box<dyn runtime::Transcriber>)>;

#[tauri::command]
fn storage_status(storage: tauri::State<Storage>) -> Result<String, String> {
    storage.0.clone()
}

/// Asking macOS about Apple Speech waits on it briefly, so these run off the main thread.
#[tauri::command]
async fn models_list(d: tauri::State<'_, Data>) -> Result<Vec<models::Task>, String> {
    models::list(&d.db.lock().unwrap(), &d.models_dir, d.macos, &apple::status())
}

#[tauri::command]
async fn model_select(d: tauri::State<'_, Data>, task: String, id: String) -> Result<(), String> {
    models::select(&d.db.lock().unwrap(), &task, &id, d.macos, &apple::status())
}

/// What the Settings window hears while a model downloads: `{ id, state: "progress" | "done" | "canceled" | "failed", ... }`.
fn model_event(app: &AppHandle, id: &str, state: &str, extra: serde_json::Value) {
    let mut p = serde_json::json!({ "id": id, "state": state });
    if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    let _ = app.emit("model-state", p);
}

#[tauri::command]
fn model_download(app: AppHandle, d: tauri::State<Data>, id: String) -> Result<(), String> {
    let m = models::find(&id).ok_or_else(|| format!("Unknown model {id}"))?;
    if m.files.is_empty() && !m.system {
        return Err(format!("{} can't be downloaded yet.", m.name));
    }
    if let Some(n) = models::needs(&m, d.macos, &apple::status()) {
        return Err(format!("{}: {n}.", m.name));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    if d.downloads.lock().unwrap().insert(id.clone(), cancel.clone()).is_some() {
        return Err(format!("{} is already downloading.", m.name));
    }
    let dir = d.models_dir.clone();
    std::thread::spawn(move || {
        let mut last = u64::MAX;
        let mut progress = |pct: u64| {
            if pct != last && !cancel.load(Ordering::Relaxed) {
                last = pct;
                model_event(&app, &m.id, "progress", serde_json::json!({ "pct": pct }));
            }
        };
        // Apple's download can't be canceled; the row shows no Cancel for it.
        let result = if m.system {
            apple::download(|done| progress((done.clamp(0.0, 1.0) * 100.0) as u64)).map(|()| true)
        } else {
            download::fetch(download::HF, &m, &dir, &cancel, |done| progress(done * 100 / m.size))
        };
        if cancel.load(Ordering::Relaxed) {
            return; // model_cancel already answered, and a retry may own the entry now
        }
        app.state::<Data>().downloads.lock().unwrap().remove(&m.id); // before the event, so Try again works at once
        match result {
            Ok(true) => model_event(&app, &m.id, "done", serde_json::json!({})),
            Ok(false) => model_event(&app, &m.id, "canceled", serde_json::json!({})),
            Err(error) => model_event(&app, &m.id, "failed", serde_json::json!({ "error": error })),
        }
    });
    Ok(())
}

#[tauri::command]
fn model_cancel(app: AppHandle, d: tauri::State<Data>, id: String) {
    // Answer at once: the download thread may sit in a slow network read for a while before it notices.
    if let Some(c) = d.downloads.lock().unwrap().remove(&id) {
        c.store(true, Ordering::Relaxed);
        model_event(&app, &id, "canceled", serde_json::json!({}));
    }
}

#[tauri::command]
fn model_delete(d: tauri::State<Data>, id: String) -> Result<(), String> {
    let m = models::find(&id).ok_or_else(|| format!("Unknown model {id}"))?;
    if d.downloads.lock().unwrap().contains_key(&id) {
        return Err(format!("{} is downloading. Cancel it first.", m.name));
    }
    match std::fs::remove_dir_all(d.models_dir.join(&m.id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Couldn't remove {}: {e}", m.name)),
        _ => Ok(()),
    }
}

/// `MYNA_DEBUG=1` turns on debug aids in the UI (for now: the window size while resizing).
#[tauri::command]
fn debug_on() -> bool {
    std::env::var("MYNA_DEBUG").is_ok_and(|v| !v.is_empty() && v != "0")
}

#[tauri::command]
fn permissions_state() -> permissions::State {
    permissions::state()
}

/// Asks macOS for `kind` ("mic" or "ax"); answers once there's a new state to read.
#[tauri::command]
async fn permission_ask(kind: String) {
    match (kind.as_str(), permissions::mic()) {
        ("mic", permissions::Mic::Ask) => permissions::ask_mic(),
        ("mic", _) => permissions::open_pane("Privacy_Microphone"),
        _ => permissions::ask_ax(),
    }
}

#[tauri::command]
fn stt_unload_get(d: tauri::State<Data>) -> Result<i64, String> {
    models::unload_after(&d.db.lock().unwrap())
}

#[tauri::command]
fn stt_unload_set(app: AppHandle, d: tauri::State<Data>, secs: i64) -> Result<(), String> {
    models::set_unload_after(&d.db.lock().unwrap(), secs)?;
    schedule_unload(&app); // the new time counts from now
    Ok(())
}

#[tauri::command]
fn pill_look_get(d: tauri::State<Data>) -> Result<pill::Look, String> {
    pill::look(&d.db.lock().unwrap())
}

#[tauri::command]
fn pill_look_set(app: AppHandle, d: tauri::State<Data>, style: String, place: String) -> Result<(), String> {
    pill::set_look(&d.db.lock().unwrap(), &style, &place)?;
    let _ = app.emit_to("pill", "pill-look", pill::Look { style, place });
    Ok(())
}

#[tauri::command]
fn voice_get(d: tauri::State<Data>) -> Result<voice::Voice, String> {
    voice::get(&d.db.lock().unwrap())
}

/// Saves voice typing's settings and registers the shortcut at once. If macOS won't take the new
/// shortcut (another app has it), puts everything back as it was and says so.
#[tauri::command]
fn voice_set(app: AppHandle, d: tauri::State<Data>, voice: voice::Voice) -> Result<(), String> {
    let was = voice::get(&d.db.lock().unwrap())?;
    voice::set(&d.db.lock().unwrap(), &voice)?;
    d.paused.store(false, Ordering::SeqCst);
    if let Err(e) = sync_shortcut(&app) {
        voice::set(&d.db.lock().unwrap(), &was)?;
        let _ = sync_shortcut(&app);
        return Err(e);
    }
    Ok(())
}

/// Lets go of the shortcut while Settings records a new one, and takes it back after.
#[tauri::command]
fn voice_pause(app: AppHandle, d: tauri::State<Data>, paused: bool) -> Result<(), String> {
    d.paused.store(paused, Ordering::SeqCst);
    sync_shortcut(&app)
}

/// Registers the saved shortcut while voice typing is on (and not paused), and drops it otherwise.
/// The new one is taken before the old one goes, so a failure leaves the old one working.
fn sync_shortcut(app: &AppHandle) -> Result<(), String> {
    let d = app.state::<Data>();
    let v = voice::get(&d.db.lock().unwrap())?;
    let want = (v.on && !d.paused.load(Ordering::SeqCst)).then(|| v.key.clone());
    let mut have = d.shortcut.lock().unwrap();
    if *have != want {
        let gs = app.global_shortcut();
        if let Some(k) = &want {
            gs.register(k.as_str()).map_err(|e| {
                eprintln!("dictation: couldn't use {k}: {e}");
                "Another app is using that shortcut. Pick a different one.".to_string()
            })?;
        }
        if let Some(k) = have.take() {
            let _ = gs.unregister(k.as_str());
        }
        *have = want;
    }
    if let Some(item) = app.try_state::<VoiceItem>() {
        let _ = item.0.set_accelerator(Some(v.key.as_str()));
        let _ = item.0.set_enabled(v.on);
    }
    Ok(())
}

/// Frees the speech-to-text model once it has sat unused for the chosen time.
// shortcut: one sleeping thread per dictation; fine at human dictation rates.
fn schedule_unload(app: &AppHandle) {
    let d = app.state::<Data>();
    let mine = d.used.fetch_add(1, Ordering::SeqCst) + 1;
    let secs = models::unload_after(&d.db.lock().unwrap()).unwrap_or(-1);
    if secs < 0 {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs as u64));
        let d = app.state::<Data>();
        if d.used.load(Ordering::SeqCst) == mine && d.recording.lock().unwrap().is_none() {
            *d.transcriber.lock().unwrap() = None;
        }
    });
}

/// The active speech-to-text model's id and folder, if it's downloaded.
fn active_stt(d: &Data) -> Result<(String, PathBuf), String> {
    let tasks = models::list(&d.db.lock().unwrap(), &d.models_dir, d.macos, &apple::status())?;
    let stt = tasks.into_iter().find(|t| t.id == "stt").ok_or("No speech-to-text task.")?;
    let id = stt.active.ok_or("No speech-to-text model runs on this Mac")?;
    match stt.models.iter().find(|m| m.id == id) {
        Some(m) if m.installed => Ok((id.clone(), d.models_dir.join(&id))),
        Some(m) if m.system => Err(format!("Download {}’s language in Settings › AI & privacy", m.name)),
        Some(m) => Err(format!("Download {} in Settings › AI & privacy", m.name)),
        None => Err(format!("Unknown model {id}")),
    }
}

/// Loads the active model into `slot` unless it's already there.
fn load_transcriber(d: &Data, slot: &mut Loaded) -> Result<(), String> {
    let (id, dir) = active_stt(d)?;
    if slot.as_ref().is_none_or(|(loaded, _)| *loaded != id) {
        *slot = None; // free the old model before loading the new one
        *slot = Some((id.clone(), runtime::transcriber(&id, &dir)?));
    }
    Ok(())
}

/// Starts recording. `hold`: it stops when the shortcut is let go; otherwise on the next tap, or at the time limit.
fn dictate_start(app: &AppHandle, hold: bool) {
    let d = app.state::<Data>();
    let mut rec = d.recording.lock().unwrap();
    if rec.is_some() {
        return; // key repeat
    }
    let Ok(v) = voice::get(&d.db.lock().unwrap()) else { return };
    if let Ok(look) = pill::look(&d.db.lock().unwrap()) {
        pill::place(app, look.place);
    }
    // Say what's missing before the user talks, not after.
    let missing = match permissions::mic() {
        permissions::Mic::Allowed => None,
        permissions::Mic::Ask => {
            std::thread::spawn(permissions::ask_mic); // macOS's pop-up; the next press records
            return;
        }
        permissions::Mic::Denied => Some(("Needs the Microphone", pill::ALLOW.to_string())),
    };
    let missing = missing
        .or_else(|| (!permissions::ax()).then(|| ("Needs Accessibility", pill::ALLOW.to_string())))
        .or_else(|| active_stt(&d).err().map(|e| ("No speech model", e)));
    if let Some((title, hint)) = missing {
        return pill::error(app, title, &hint);
    }
    *rec = Some(dictation::record());
    let mine = d.used.fetch_add(1, Ordering::SeqCst) + 1; // a pending unload must not fire mid-dictation
    pill::listening(app, hold, &v.key, voice::TAP_LIMIT_SECS);
    let loader = app.clone();
    std::thread::spawn(move || {
        let d = loader.state::<Data>();
        // Load the model while the user talks; a failure shows when they let go.
        let _ = load_transcriber(&d, &mut d.transcriber.lock().unwrap());
    });
    let app = app.clone();
    std::thread::spawn(move || {
        let d = app.state::<Data>();
        let started = std::time::Instant::now();
        // Level bars, until this recording ends.
        while d.used.load(Ordering::SeqCst) == mine {
            if !hold && started.elapsed().as_secs() >= voice::TAP_LIMIT_SECS {
                dictate_stop(&app);
                break;
            }
            match d.recording.lock().unwrap().as_ref() {
                Some(r) => pill::level(&app, r.level()),
                None => break,
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
}

/// The menu item, or a tap of the shortcut: start, or stop if already listening.
fn dictate_toggle(app: &AppHandle) {
    if app.state::<Data>().recording.lock().unwrap().is_some() {
        dictate_stop(app);
    } else {
        dictate_start(app, false);
    }
}

fn dictate_stop(app: &AppHandle) {
    let Some(rec) = app.state::<Data>().recording.lock().unwrap().take() else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let d = app.state::<Data>();
        let run = || -> Result<&'static str, String> {
            let audio = rec.finish()?;
            if audio.len() < runtime::RATE as usize / 4 {
                return Ok("idle"); // a tap, not speech
            }
            pill::show(&app, "writing");
            let mut slot = d.transcriber.lock().unwrap();
            load_transcriber(&d, &mut slot)?;
            let text = dictation::text(slot.as_mut().unwrap().1.as_mut(), &audio)?;
            drop(slot);
            if text.is_empty() {
                return Ok("nothing");
            }
            dictation::paste(&text)?;
            Ok("added")
        };
        match run() {
            Ok("nothing") => {
                let tap = voice::get(&d.db.lock().unwrap()).is_ok_and(|v| v.mode == "tap");
                pill::error(&app, "Didn’t hear anything", if tap { "Press the keys, talk, then press them again" } else { "Hold the keys while you talk" })
            }
            Ok(state) => pill::show(&app, state),
            Err(e) if e == dictation::NO_AX => pill::error(&app, "Needs Accessibility", pill::ALLOW),
            Err(e) => pill::error(&app, "Couldn’t add your words", &e),
        }
        schedule_unload(&app);
    });
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![debug_on, storage_status, models_list, model_select, model_download, model_cancel, model_delete, permissions_state, permission_ask, stt_unload_get, stt_unload_set, pill_look_get, pill_look_set, voice_get, voice_set, voice_pause])
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            let dir = app.path().data_dir()?.join("Myna");
            let status = match storage::open(&dir) {
                Ok(db) => {
                    app.manage(Data {
                        db: Mutex::new(db),
                        models_dir: dir.join("models"),
                        macos: models::macos_major(),
                        downloads: Default::default(),
                        recording: Default::default(),
                        transcriber: Default::default(),
                        used: Default::default(),
                        shortcut: Default::default(),
                        paused: Default::default(),
                    });
                    app.handle().plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(|app, _, e| {
                                let tap = voice::get(&app.state::<Data>().db.lock().unwrap()).is_ok_and(|v| v.mode == "tap");
                                match (e.state(), tap) {
                                    (ShortcutState::Pressed, false) => dictate_start(app, true),
                                    (ShortcutState::Released, false) => dictate_stop(app),
                                    // On release: a press may repeat while the key is down; a release comes once.
                                    (ShortcutState::Released, true) => dictate_toggle(app),
                                    (ShortcutState::Pressed, true) => {}
                                }
                            })
                            .build(),
                    )?;
                    if let Some(w) = app.get_webview_window("pill") {
                        pill::setup(&w);
                    }
                    Ok(dir.display().to_string())
                }
                Err(e) => Err(e),
            };
            let failed = status.is_err();
            app.manage(Storage(status));

            // A real NSMenu on both clicks: native look, highlight, keyboard and accessibility for free.
            // shortcut: Tauri menus have no section headers or badges; stories 04/05 build the NSMenu via objc2.
            let settings_item = MenuItemBuilder::with_id("settings", "Settings…").accelerator("Cmd+,").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Myna").accelerator("Cmd+Q").build(app)?;
            // The shortcut and enabled state are set by sync_shortcut below.
            let voice_item = MenuItemBuilder::with_id("dictate", "Start voice typing").enabled(false).build(app)?;
            let menu = MenuBuilder::new(app).item(&voice_item).separator().item(&settings_item).separator().item(&quit_item).build()?;
            app.manage(VoiceItem(voice_item));
            TrayIconBuilder::with_id("myna")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Myna")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "dictate" => dictate_toggle(app),
                    "settings" => open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if !failed && let Err(e) = sync_shortcut(app.handle()) {
                eprintln!("dictation: {e}"); // Voice typing still shows the saved shortcut; picking it again says why
            }
            if failed {
                open_settings(app.handle()); // never fail silently: Settings shows the error
            }
            Ok(())
        })
        .on_window_event(|w, e| {
            // Closing Settings hides it; the app lives on in the menubar.
            if let (WindowEvent::CloseRequested { api, .. }, "settings") = (e, w.label()) {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Myna");
}
