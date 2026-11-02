mod dictation;
mod download;
mod models;
mod permissions;
mod runtime;
mod storage;

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager, WindowEvent};
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
}

type Loaded = Option<(String, Box<dyn runtime::Transcriber>)>;

// shortcut: fixed until the shortcut recorder lands (ticket 04.3).
const DICTATE: &str = "Alt+Space";

#[tauri::command]
fn storage_status(storage: tauri::State<Storage>) -> Result<String, String> {
    storage.0.clone()
}

#[tauri::command]
fn models_list(d: tauri::State<Data>) -> Result<Vec<models::Task>, String> {
    models::list(&d.db.lock().unwrap(), &d.models_dir, d.macos)
}

#[tauri::command]
fn model_select(d: tauri::State<Data>, task: String, id: String) -> Result<(), String> {
    models::select(&d.db.lock().unwrap(), &task, &id, d.macos)
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
    if m.files.is_empty() {
        return Err(format!("{} can't be downloaded yet.", m.name));
    }
    if !models::runs_here(&m, d.macos) {
        return Err(format!("{} needs macOS {}.", m.name, m.min_macos));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    if d.downloads.lock().unwrap().insert(id.clone(), cancel.clone()).is_some() {
        return Err(format!("{} is already downloading.", m.name));
    }
    let dir = d.models_dir.clone();
    std::thread::spawn(move || {
        let (total, mut last) = (m.size, u64::MAX);
        let result = download::fetch(download::HF, &m, &dir, &cancel, |done| {
            let pct = done * 100 / total;
            if pct != last && !cancel.load(Ordering::Relaxed) {
                last = pct;
                model_event(&app, &m.id, "progress", serde_json::json!({ "pct": pct }));
            }
        });
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
    let tasks = models::list(&d.db.lock().unwrap(), &d.models_dir, d.macos)?;
    let stt = tasks.into_iter().find(|t| t.id == "stt").ok_or("No speech-to-text task.")?;
    let id = stt.active.ok_or("No speech-to-text model runs on this Mac.")?;
    match stt.models.iter().find(|m| m.id == id) {
        Some(m) if m.installed => Ok((id.clone(), d.models_dir.join(&id))),
        Some(m) => Err(format!("Download {} on Settings › AI & privacy first.", m.name)),
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

fn dictate_start(app: &AppHandle) {
    let d = app.state::<Data>();
    let mut rec = d.recording.lock().unwrap();
    if rec.is_some() {
        return; // key repeat
    }
    match permissions::mic() {
        permissions::Mic::Allowed => {}
        permissions::Mic::Ask => {
            std::thread::spawn(permissions::ask_mic); // macOS's pop-up; the next press records
            return;
        }
        // shortcut: errors go to the log until the pill shows them (ticket 04.2).
        permissions::Mic::Denied => return eprintln!("dictation: Myna needs the Microphone. Allow it on Settings › Voice typing."),
    }
    *rec = Some(dictation::record());
    d.used.fetch_add(1, Ordering::SeqCst); // a pending unload must not fire mid-dictation
    // Load the model while the user talks.
    let app = app.clone();
    std::thread::spawn(move || {
        let d = app.state::<Data>();
        if let Err(e) = load_transcriber(&d, &mut d.transcriber.lock().unwrap()) {
            eprintln!("dictation: {e}");
        }
    });
}

fn dictate_stop(app: &AppHandle) {
    let Some(rec) = app.state::<Data>().recording.lock().unwrap().take() else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let d = app.state::<Data>();
        let run = || -> Result<(), String> {
            let audio = rec.finish()?;
            if audio.len() < runtime::RATE as usize / 4 {
                return Ok(()); // a tap, not speech
            }
            let mut slot = d.transcriber.lock().unwrap();
            load_transcriber(&d, &mut slot)?;
            let text = dictation::text(slot.as_mut().unwrap().1.as_mut(), &audio)?;
            drop(slot);
            if text.is_empty() { Ok(()) } else { dictation::paste(&text) }
        };
        if let Err(e) = run() {
            eprintln!("dictation: {e}");
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
        .invoke_handler(tauri::generate_handler![storage_status, models_list, model_select, model_download, model_cancel, model_delete, permissions_state, permission_ask, stt_unload_get, stt_unload_set])
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
                    });
                    app.handle().plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(|app, _, e| match e.state() {
                                ShortcutState::Pressed => dictate_start(app),
                                ShortcutState::Released => dictate_stop(app),
                            })
                            .build(),
                    )?;
                    if let Err(e) = app.global_shortcut().register(DICTATE) {
                        eprintln!("dictation: couldn't use {DICTATE}: {e}"); // another app holds it
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
            let menu = MenuBuilder::new(app).item(&settings_item).separator().item(&quit_item).build()?;
            TrayIconBuilder::with_id("myna")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Myna")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "settings" => open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

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
