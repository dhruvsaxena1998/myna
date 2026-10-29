mod audio;
mod cleanup;
mod dictation;
mod hotkey;
mod meetings;
mod models;
mod paste;
mod permissions;
mod popover;
mod settings;
mod stt;

use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ActivationPolicy, AppHandle, Emitter, Manager, PhysicalPosition, RunEvent, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

struct Db(Mutex<rusqlite::Connection>);
struct ModelsDir(PathBuf);

const PILL_SIZE: (f64, f64) = (360.0, 64.0);
const PILL_MARGIN: f64 = 24.0;

#[derive(Default)]
struct Dictating {
    /// True from key press until the text is pasted (or the attempt fails).
    busy: AtomicBool,
    /// Present only while the hotkey is held.
    rec: Mutex<Option<audio::Recording>>,
}
struct HotkeyError(Mutex<Option<String>>);
/// Bumped whenever the pill is shown, so a stale "hide in 1.2 s" timer does not hide a newer state.
#[derive(Default)]
struct PillEpoch(AtomicU64);

/// What the pill window renders; the shape is the contract with `pill()` in ui/index.html.
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Pill {
    Idle { hotkey: String, toggle: bool },
    Listening { level: f32 },
    Transcribing,
    Done { words: usize },
    Error { message: String },
}

#[derive(Serialize)]
struct ModelRow {
    id: String,
    task: &'static str,
    name: String,
    desc: String,
    size: u64,
    status: models::Status,
    /// Bytes fetched so far while a download runs.
    downloading: Option<u64>,
    error: Option<String>,
    /// Added by the user under Advanced, as opposed to the curated list.
    custom: bool,
    /// A custom model that lives on disk where the user keeps it: nothing to download.
    local: bool,
}

#[derive(Serialize)]
struct ModelList {
    dir: String,
    models: Vec<ModelRow>,
}

#[tauri::command]
fn list_models(dir: tauri::State<ModelsDir>, jobs: tauri::State<models::Jobs>, db: tauri::State<Db>) -> ModelList {
    let jobs = jobs.lock().unwrap();
    let row = |m: &models::Model, status, custom: Option<&stt::Custom>, problem: Option<String>| {
        let job = jobs.get(m.id);
        let result = job.and_then(|j| j.result.lock().unwrap().clone());
        ModelRow {
            id: m.id.into(),
            task: m.task,
            name: m.name.into(),
            desc: m.desc.into(),
            size: m.files.iter().map(|f| f.size).sum(),
            status,
            downloading: job.filter(|_| result.is_none()).map(|j| j.done.load(Ordering::Relaxed)),
            error: result.and_then(|r| r.err()).or(problem),
            custom: custom.is_some(),
            local: custom.is_some_and(stt::Custom::local),
        }
    };
    let mut models: Vec<_> = models::MODELS.iter().map(|m| row(m, models::status(&dir.0, m), None, None)).collect();
    for c in stt::customs(&db.0.lock().unwrap()) {
        let (status, problem) = c.state(&dir.0);
        models.push(c.model(|m| row(m, status, Some(&c), problem)));
    }
    ModelList { dir: dir.0.display().to_string(), models }
}

#[tauri::command]
fn download_model(
    dir: tauri::State<ModelsDir>,
    jobs: tauri::State<Arc<models::Jobs>>,
    db: tauri::State<Db>,
    id: String,
) -> Result<(), String> {
    let curated = models::MODELS.iter().find(|m| m.id == id);
    let custom = stt::customs(&db.0.lock().unwrap()).into_iter().find(|c| c.id() == id && !c.local());
    if curated.is_none() && custom.is_none() {
        return Err("unknown model".into());
    }
    let job = {
        let mut map = jobs.lock().unwrap();
        if map.get(&id).is_some_and(|j| j.result.lock().unwrap().is_none()) {
            return Ok(());
        }
        let job = Arc::new(models::Job::default());
        map.insert(id.clone(), job.clone());
        job
    };
    let (root, jobs) = (dir.0.clone(), jobs.inner().clone());
    std::thread::spawn(move || {
        let fetch = |m: &models::Model| models::install(&root, m.id, m.files, models::http_open, &job);
        let res = match (&custom, curated) {
            (Some(c), _) => c.model(fetch),
            (_, Some(m)) => fetch(m),
            _ => unreachable!(),
        };
        match res {
            Ok(()) | Err(models::Stop::Cancelled) => {
                jobs.lock().unwrap().remove(&id);
            }
            Err(models::Stop::Failed(msg)) => *job.result.lock().unwrap() = Some(Err(msg)),
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_model(jobs: tauri::State<Arc<models::Jobs>>, id: String) {
    if let Some(j) = jobs.lock().unwrap().get(&id) {
        j.cancel.store(true, Ordering::Relaxed);
    }
}

/// Deletes a curated model's files, or forgets a custom one (and deletes only what Myna downloaded for it).
#[tauri::command]
fn remove_model(dir: tauri::State<ModelsDir>, jobs: tauri::State<Arc<models::Jobs>>, db: tauri::State<Db>, id: String) {
    jobs.lock().unwrap().remove(&id);
    models::remove(&dir.0, &id); // custom ids are folder names too; a local path never lives under here
    let db = db.0.lock().unwrap();
    let mut all = stt::customs(&db);
    if all.iter().any(|c| c.id() == id) {
        all.retain(|c| c.id() != id);
        let _ = stt::save(&db, &all);
        if settings::get(&db, "model_stt").as_deref() == Some(&id) {
            let _ = settings::set(&db, "model_stt", stt::PARAKEET);
        }
    }
}

/// Validates a custom model (a path, or a Hugging Face repo that gets looked up) and remembers it.
#[tauri::command]
async fn add_custom_model(db: tauri::State<'_, Db>, runtime: stt::Runtime, source: String) -> Result<(), String> {
    let c = tauri::async_runtime::spawn_blocking(move || stt::Custom::new(runtime, &source, stt::fetch_repo))
        .await
        .map_err(|e| e.to_string())??;
    let db = db.0.lock().unwrap();
    let mut all = stt::customs(&db);
    if all.iter().any(|x| x.id() == c.id()) {
        return Err("Already added".into());
    }
    all.push(c);
    stt::save(&db, &all).map_err(|e| e.to_string())
}

/// Full checksum of an installed model; async so hashing gigabytes stays off the main thread.
#[tauri::command]
async fn verify_model(dir: tauri::State<'_, ModelsDir>, db: tauri::State<'_, Db>, id: String) -> Result<bool, String> {
    let root = dir.0.clone();
    let curated = models::MODELS.iter().find(|m| m.id == id);
    let custom = stt::customs(&db.0.lock().unwrap()).into_iter().find(|c| c.id() == id);
    if curated.is_none() && custom.is_none() {
        return Err("unknown model".into());
    }
    tauri::async_runtime::spawn_blocking(move || match (&custom, curated) {
        (Some(c), _) => c.verify(&root),
        (_, Some(m)) => models::verify(&root, m),
        _ => unreachable!(),
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_models_dir(dir: tauri::State<ModelsDir>) {
    let _ = std::process::Command::new("open").arg(&dir.0).spawn();
}

#[tauri::command]
fn get_setting(db: tauri::State<Db>, key: String) -> Option<String> {
    settings::get(&db.0.lock().unwrap(), &key)
}

#[tauri::command]
fn set_setting(app: AppHandle, db: tauri::State<Db>, key: String, value: String) -> Result<(), String> {
    settings::set(&db.0.lock().unwrap(), &key, &value).map_err(|e| e.to_string())?;
    if key == "dictation_enabled" || key == "dictation_hotkey" {
        sync_hotkey(&app);
    }
    Ok(())
}

#[derive(Serialize)]
struct HotkeyStatus {
    key: String,
    error: Option<String>,
}

#[tauri::command]
fn hotkey_status(app: AppHandle, err: tauri::State<HotkeyError>) -> HotkeyStatus {
    HotkeyStatus { key: hotkey(&app), error: err.0.lock().unwrap().clone() }
}

#[tauri::command]
fn request_permission(name: String) {
    permissions::request(&name)
}

#[tauri::command]
fn permission_granted(name: String) -> bool {
    permissions::granted(&name)
}

#[tauri::command]
fn open_privacy_settings(name: String) {
    let pane = match name.as_str() {
        "Microphone" => "Privacy_Microphone",
        "Accessibility" => "Privacy_Accessibility",
        _ => "Privacy_ScreenCapture",
    };
    let _ = std::process::Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
        .spawn();
}

fn create_pill(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("index.html#pill".into()))
        .inner_size(PILL_SIZE.0, PILL_SIZE.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        // The pill must never take focus: the paste goes to whichever app had it.
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    w.set_ignore_cursor_events(true)
}

/// Bottom-center of the display the cursor is on, above the Dock.
fn place_pill(app: &AppHandle, w: &WebviewWindow) {
    // Not `monitor_from_point`: on macOS it takes points while `cursor_position` returns pixels.
    let under_cursor = app.cursor_position().ok().and_then(|c| {
        app.available_monitors().ok()?.into_iter().find(|m| {
            let (p, s) = (m.position(), m.size());
            (p.x as f64..p.x as f64 + s.width as f64).contains(&c.x)
                && (p.y as f64..p.y as f64 + s.height as f64).contains(&c.y)
        })
    });
    let Some(mon) = under_cursor.or_else(|| app.primary_monitor().ok().flatten()) else { return };
    let (area, scale) = (mon.work_area(), mon.scale_factor());
    let x = area.position.x as f64 + (area.size.width as f64 - PILL_SIZE.0 * scale) / 2.0;
    let y = (area.position.y + area.size.height as i32) as f64 - (PILL_SIZE.1 + PILL_MARGIN) * scale;
    let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
}

fn emit_pill(app: &AppHandle, state: &Pill) {
    let _ = app.emit_to("pill", "pill", state);
}

fn show_pill(app: &AppHandle, state: Pill) {
    app.state::<PillEpoch>().0.fetch_add(1, Ordering::Relaxed);
    emit_pill(app, &state); // before show(), so the window never flashes the previous state
    let Some(w) = app.get_webview_window("pill") else { return };
    if !w.is_visible().unwrap_or(false) {
        place_pill(app, &w);
        let _ = w.show();
    }
}

fn hide_pill_after(app: &AppHandle, ms: u64) {
    let (app, shown) = (app.clone(), app.state::<PillEpoch>().0.load(Ordering::Relaxed));
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if app.state::<PillEpoch>().0.load(Ordering::Relaxed) == shown {
            if let Some(w) = app.get_webview_window("pill") {
                let _ = w.hide();
            }
        }
    });
}

fn show_error(app: &AppHandle, message: impl Into<String>) {
    show_pill(app, Pill::Error { message: message.into() });
    hide_pill_after(app, 3000);
}

fn setting(app: &AppHandle, key: &str) -> Option<String> {
    settings::get(&app.state::<Db>().0.lock().unwrap(), key)
}

fn hotkey(app: &AppHandle) -> String {
    setting(app, "dictation_hotkey").filter(|k| !k.trim().is_empty()).unwrap_or_else(|| hotkey::DEFAULT.into())
}

fn toggle_mode(app: &AppHandle) -> bool {
    setting(app, "dictation_mode").as_deref() == Some("toggle")
}

/// Runs `f` on the selected speech model, loading it first if needed. The selection is read on
/// every call, so a change in Settings applies to the next dictation without a restart.
pub(crate) fn with_engine<R>(app: &AppHandle, f: impl FnOnce(&mut dyn dictation::Transcriber) -> R) -> Result<R, String> {
    let (id, customs) = {
        let state = app.state::<Db>();
        let db = state.0.lock().unwrap();
        let id = settings::get(&db, "model_stt")
            .unwrap_or_else(|| models::MODELS.iter().find(|m| m.task == "stt").unwrap().id.to_string());
        (id, stt::customs(&db))
    };
    let dir = app.state::<ModelsDir>().0.clone();
    let load = || {
        let (runtime, path) = stt::resolve(&id, &dir, &customs)?;
        dictation::load(runtime, &path)
    };
    app.state::<dictation::Engine>().with(&id, load, f)
}

fn press(app: &AppHandle) {
    let st = app.state::<Dictating>();
    if st.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    if !permissions::granted("Microphone") {
        permissions::request("Microphone");
        st.busy.store(false, Ordering::SeqCst);
        return show_error(app, "Allow the microphone in System Settings");
    }
    let a = app.clone();
    match audio::Recording::start(move |level| emit_pill(&a, &Pill::Listening { level })) {
        Ok(rec) => {
            *st.rec.lock().unwrap() = Some(rec);
            show_pill(app, Pill::Listening { level: 0.0 });
            // Load the model while the user is still talking.
            let a = app.clone();
            std::thread::spawn(move || {
                let _ = with_engine(&a, |_| ());
            });
        }
        Err(e) => {
            st.busy.store(false, Ordering::SeqCst);
            show_error(app, e);
        }
    }
}

fn release(app: &AppHandle) {
    let Some(rec) = app.state::<Dictating>().rec.lock().unwrap().take() else { return };
    let app = app.clone();
    std::thread::spawn(move || finish(&app, rec));
}

fn finish(app: &AppHandle, rec: audio::Recording) {
    // Clears `busy` however this ends, a panic in the model included, so the hotkey never goes dead.
    struct Reset<'a>(&'a AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let st = app.state::<Dictating>();
    let _reset = Reset(&st.busy);

    let result = rec.stop().and_then(|clip| {
        show_pill(app, Pill::Transcribing);
        match with_engine(app, |t| dictation::dictate(&clip, t))?? {
            dictation::Heard::TooShort => Ok(Pill::Idle { hotkey: hotkey(app), toggle: toggle_mode(app) }),
            dictation::Heard::Silence => Err("Didn't catch that".to_string()),
            dictation::Heard::Text(text) => {
                let rules = setting(app, "dictation_cleanup").as_deref() != Some("false");
                let text = cleanup::clean(&text, rules, &cleanup::dictionary(setting(app, "dictionary")));
                if text.is_empty() {
                    return Err("Didn't catch that".to_string()); // it was all fillers
                }
                paste_into_focused_app(app, &text)?;
                Ok(Pill::Done { words: text.split_whitespace().count() })
            }
        }
    });
    match result {
        Ok(pill) => {
            let linger = if matches!(pill, Pill::Done { .. }) { 1200 } else { 2500 };
            show_pill(app, pill);
            hide_pill_after(app, linger);
        }
        Err(message) => show_error(app, message),
    }
}

fn paste_into_focused_app(app: &AppHandle, text: &str) -> Result<(), String> {
    if !permissions::granted("Accessibility") {
        // Without it the Cmd-V is silently dropped. Keep the text on the clipboard rather than lose it.
        permissions::request("Accessibility");
        paste::copy_only(text)?;
        return Err("Copied. Allow Accessibility to paste".into());
    }
    if setting(app, "paste_method").as_deref() == Some("keystrokes") {
        return paste::type_text(text);
    }
    paste::paste_text(text, paste::delay(setting(app, "paste_delay_ms")))
}

/// The hotkey is held only while Dictation is enabled, so Meetings-only users keep the shortcut.
fn sync_hotkey(app: &AppHandle) {
    let keys = app.global_shortcut();
    let _ = keys.unregister_all();
    let enabled = setting(app, "dictation_enabled").as_deref() == Some("true");
    let hotkey = hotkey(app);
    let error = enabled
        .then(|| {
            keys.on_shortcut(hotkey.as_str(), |app, _, event| {
                // Read per event, so switching mode needs no re-registering.
                let recording = app.state::<Dictating>().rec.lock().unwrap().is_some();
                match hotkey::act(toggle_mode(app), event.state() == ShortcutState::Pressed, recording) {
                    hotkey::Act::Start => press(app),
                    hotkey::Act::Stop => release(app),
                    hotkey::Act::Nothing => {}
                }
            })
            .err()
            .map(|e| format!("{hotkey} could not be registered ({e}). Another app may be using it."))
        })
        .flatten();
    *app.state::<HotkeyError>().0.lock().unwrap() = error;
}

fn open_window(app: &AppHandle, label: &str, title: &str, size: (f64, f64)) {
    open_window_at(app, label, label, title, size)
}

/// Like `open_window`, but a new window starts on `hash` (the router's route, plus any argument after a slash).
fn open_window_at(app: &AppHandle, label: &str, hash: &str, title: &str, size: (f64, f64)) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_focus();
        return;
    }
    let url = WebviewUrl::App(format!("index.html#{hash}").into());
    if let Ok(w) = WebviewWindowBuilder::new(app, label, url)
        .title(title)
        .inner_size(size.0, size.1)
        .build()
    {
        let _ = w.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            let dir = app.path().data_dir()?.join("Myna");
            std::fs::create_dir_all(&dir)?;
            let db = settings::open(&dir.join("myna.db"))?;
            let onboarded = settings::get(&db, "onboarded").is_some();
            app.manage(Db(Mutex::new(db)));
            let models_dir = dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            models::clean_partials(&models_dir);
            app.manage(ModelsDir(models_dir));
            app.manage(Arc::new(models::Jobs::default()));
            app.manage(dictation::Engine::default());
            app.manage(Dictating::default());
            app.manage(HotkeyError(Mutex::new(None)));
            app.manage(PillEpoch::default());
            create_pill(app.handle())?;
            sync_hotkey(app.handle());

            popover::create(app.handle())?;
            TrayIconBuilder::with_id(meetings::TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                        popover::toggle(tray.app_handle(), rect);
                    }
                })
                .build(app)?;
            meetings::setup(app, &dir)?;

            if !onboarded {
                open_window(app.handle(), "onboarding", "Welcome to Myna", (520.0, 460.0));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_setting,
            set_setting,
            hotkey_status,
            request_permission,
            permission_granted,
            open_privacy_settings,
            list_models,
            download_model,
            cancel_model,
            remove_model,
            add_custom_model,
            verify_model,
            reveal_models_dir,
            popover::popover_view,
            popover::popover_fit,
            popover::popover_hide,
            popover::popover_go,
            meetings::meeting_toggle,
            meetings::list_meetings,
            meetings::get_meeting,
            meetings::retry_transcription,
            meetings::export_meeting,
            meetings::list_folders,
            meetings::create_folder,
            meetings::rename_folder,
            meetings::delete_folder,
            meetings::move_meeting
        ])
        .build(tauri::generate_context!())
        .expect("error building Myna")
        .run(|app, event| match event {
            // Tray app: closing every window must not quit.
            RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
            RunEvent::Exit => meetings::shutdown(app),
            _ => {}
        });
}
