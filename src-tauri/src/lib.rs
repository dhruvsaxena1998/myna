mod apple;
mod dictation;
mod download;
mod look;
mod models;
mod permissions;
mod pill;
mod rules;
mod runtime;
mod session;
mod storage;
mod update;
mod voice;

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager, WindowEvent, Wry};
use session::Session;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Result of creating the data folder and database: the folder path, or the error to show.
struct Storage(Result<String, String>);

/// Managed only when the database opened.
struct Data {
    db: Arc<Mutex<Connection>>,
    models_dir: PathBuf,
    macos: u32,
    /// Model id -> cancel flag, for each download in progress.
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The shortcut registered with macOS right now, if any.
    shortcut: Mutex<Option<String>>,
    /// Set while Settings records a new shortcut, so pressing the old one types into the recorder.
    paused: AtomicBool,
}

/// "Start voice typing" in the menubar menu, which shows the shortcut.
struct VoiceItem(MenuItem<Wry>);

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
    let mut downloads = d.downloads.lock().unwrap();
    // Two models that need ONNX Runtime would fetch it at the same time and trip over each other's temp folder.
    if let Some(o) = models::all().iter().find(|o| m.runtime && o.runtime && o.id != id && downloads.contains_key(&o.id)) {
        return Err(format!("{} is downloading. Wait for it to finish.", o.name));
    }
    if downloads.insert(id.clone(), cancel.clone()).is_some() {
        return Err(format!("{} is already downloading.", m.name));
    }
    drop(downloads);
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
            let get = |x: &models::Model, p: &mut dyn FnMut(u64)| download::fetch(download::HF, x, &dir, &cancel, p);
            download::install(&m, &dir, &get, &mut progress)
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
    let downloading: Vec<String> = d.downloads.lock().unwrap().keys().cloned().collect();
    download::remove(&m, &d.models_dir, &downloading)
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
fn stt_unload_set(d: tauri::State<Data>, s: tauri::State<Arc<Session>>, secs: i64) -> Result<(), String> {
    models::set_unload_after(&d.db.lock().unwrap(), secs)?;
    s.schedule_unload(); // the new time counts from now
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
fn look_get(d: tauri::State<Data>) -> Result<look::Look, String> {
    look::get(&d.db.lock().unwrap(), d.macos)
}

/// Saves the look and tells every window, so each one restyles at once.
#[tauri::command]
fn look_set(app: AppHandle, d: tauri::State<Data>, look: look::Look) -> Result<(), String> {
    look::set(&d.db.lock().unwrap(), &look, d.macos)?;
    let theme = look.theme.clone();
    let _ = app.run_on_main_thread(move || look::apply_native(&theme));
    let _ = app.emit("look", look); // each page re-sends glass_set, which picks up the new tint
    Ok(())
}

/// A page says where it drew what the glass goes behind (the pill; the Settings panels); none takes it away.
#[tauri::command]
fn glass_set(app: AppHandle, window: tauri::Window, d: tauri::State<Data>, glass: Vec<look::Glass>) -> Result<(), String> {
    let l = look::get(&d.db.lock().unwrap(), d.macos)?;
    look::glass(&app, window.label(), if look::glass_on(&l) { glass } else { vec![] }, look::tint(&l));
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

fn prerelease(app: &AppHandle) -> bool {
    !app.package_info().version.pre.is_empty()
}

#[tauri::command]
fn updates_get(app: AppHandle, d: tauri::State<Data>) -> Result<update::Prefs, String> {
    update::get(&d.db.lock().unwrap(), prerelease(&app))
}

#[tauri::command]
fn updates_set(app: AppHandle, d: tauri::State<Data>, prefs: update::Prefs) -> Result<(), String> {
    let was = update::get(&d.db.lock().unwrap(), prerelease(&app))?;
    update::set(&d.db.lock().unwrap(), &prefs)?;
    if was.channel != prefs.channel {
        update::forget(&app);
    }
    Ok(())
}

#[tauri::command]
fn update_state(u: tauri::State<update::Updates>) -> update::State {
    u.state.lock().unwrap().clone()
}

/// The Check for updates button. The error comes back here and in the update state.
#[tauri::command]
async fn update_check(app: AppHandle, d: tauri::State<'_, Data>) -> Result<(), String> {
    let p = update::get(&d.db.lock().unwrap(), prerelease(&app))?;
    update::check(&app, update::FEEDS, &p, true).await
}

#[tauri::command]
async fn update_download(app: AppHandle) -> Result<(), String> {
    update::download(&app, false).await
}

/// Restart now: installs the downloaded update and relaunches, unless a dictation is under way.
#[tauri::command]
fn update_restart(app: AppHandle, s: tauri::State<Arc<Session>>) -> Result<(), String> {
    update::install(&app, s.busy())?;
    app.restart()
}

/// Automatic checks: at launch and every 24 h, unless Update mode is Off. Wakes hourly, so a Mac that
/// slept through the 24 h checks soon after it wakes.
fn watch_updates(app: AppHandle) {
    if cfg!(debug_assertions) {
        return; // a dev run would quietly download the latest release; the button still checks
    }
    std::thread::spawn(move || loop {
        let last = *app.state::<update::Updates>().last.lock().unwrap();
        let prefs = update::get(&app.state::<Data>().db.lock().unwrap(), prerelease(&app));
        if let Some(p) = prefs.ok().filter(|p| update::due(&p.mode, last, std::time::SystemTime::now())) {
            let _ = tauri::async_runtime::block_on(update::check(&app, update::FEEDS, &p, false));
        }
        std::thread::sleep(std::time::Duration::from_secs(3600));
    });
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
        .invoke_handler(tauri::generate_handler![debug_on, storage_status, models_list, model_select, model_download, model_cancel, model_delete, permissions_state, permission_ask, stt_unload_get, stt_unload_set, pill_look_get, pill_look_set, look_get, look_set, glass_set, voice_get, voice_set, voice_pause, updates_get, updates_set, update_state, update_check, update_download, update_restart])
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::Updates::default())
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            let dir = app.path().data_dir()?.join("Myna");
            let status = match storage::open(&dir) {
                Ok(db) => {
                    let db = Arc::new(Mutex::new(db));
                    let world = Box::new(session::Mac(app.handle().clone()));
                    app.manage(Session::new(world, db.clone(), dir.join("models"), models::macos_major()));
                    app.manage(Data {
                        db,
                        models_dir: dir.join("models"),
                        macos: models::macos_major(),
                        downloads: Default::default(),
                        shortcut: Default::default(),
                        paused: Default::default(),
                    });
                    app.handle().plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(|app, _, e| app.state::<Arc<Session>>().key(e.state() == ShortcutState::Pressed))
                            .build(),
                    )?;
                    if let Some(w) = app.get_webview_window("pill") {
                        pill::setup(&w);
                    }
                    if let Ok(l) = look::get(&app.state::<Data>().db.lock().unwrap(), models::macos_major()) {
                        look::apply_native(&l.theme);
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
                    "dictate" => app.state::<Arc<Session>>().toggle(),
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
            } else {
                watch_updates(app.handle().clone());
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
