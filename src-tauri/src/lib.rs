mod meetings;
mod models;
mod permissions;
mod settings;

use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    ActivationPolicy, AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
};

struct Db(Mutex<rusqlite::Connection>);
struct ModelsDir(PathBuf);

#[derive(Serialize)]
struct ModelRow {
    id: &'static str,
    task: &'static str,
    name: &'static str,
    desc: &'static str,
    size: u64,
    status: models::Status,
    /// Bytes fetched so far while a download runs.
    downloading: Option<u64>,
    error: Option<String>,
}

#[derive(Serialize)]
struct ModelList {
    dir: String,
    models: Vec<ModelRow>,
}

#[tauri::command]
fn list_models(dir: tauri::State<ModelsDir>, jobs: tauri::State<models::Jobs>) -> ModelList {
    let jobs = jobs.lock().unwrap();
    let models = models::MODELS
        .iter()
        .map(|m| {
            let job = jobs.get(m.id);
            let result = job.and_then(|j| j.result.lock().unwrap().clone());
            ModelRow {
                id: m.id,
                task: m.task,
                name: m.name,
                desc: m.desc,
                size: m.files.iter().map(|f| f.size).sum(),
                status: models::status(&dir.0, m),
                downloading: job.filter(|_| result.is_none()).map(|j| j.done.load(Ordering::Relaxed)),
                error: result.and_then(|r| r.err()),
            }
        })
        .collect();
    ModelList { dir: dir.0.display().to_string(), models }
}

#[tauri::command]
fn download_model(
    dir: tauri::State<ModelsDir>,
    jobs: tauri::State<Arc<models::Jobs>>,
    id: String,
) -> Result<(), String> {
    let m = models::MODELS.iter().find(|m| m.id == id).ok_or("unknown model")?;
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
        let res = models::install(&root, m.id, m.files, models::http_open, &job);
        match res {
            Ok(()) | Err(models::Stop::Cancelled) => {
                jobs.lock().unwrap().remove(m.id);
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

#[tauri::command]
fn remove_model(dir: tauri::State<ModelsDir>, jobs: tauri::State<Arc<models::Jobs>>, id: String) {
    jobs.lock().unwrap().remove(&id);
    models::remove(&dir.0, &id);
}

/// Full checksum of an installed model; async so hashing gigabytes stays off the main thread.
#[tauri::command]
async fn verify_model(dir: tauri::State<'_, ModelsDir>, id: String) -> Result<bool, String> {
    let root = dir.0.clone();
    let m = models::MODELS.iter().find(|m| m.id == id).ok_or("unknown model")?;
    tauri::async_runtime::spawn_blocking(move || models::verify(&root, m))
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
fn set_setting(db: tauri::State<Db>, key: String, value: String) -> Result<(), String> {
    settings::set(&db.0.lock().unwrap(), &key, &value).map_err(|e| e.to_string())
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

fn open_window(app: &AppHandle, label: &str, title: &str, size: (f64, f64)) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.set_focus();
        return;
    }
    let url = WebviewUrl::App(format!("index.html#{label}").into());
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

            let record = MenuItem::with_id(app, "record", "Start meeting recording", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &record,
                    &MenuItem::with_id(app, "meetings", "Meetings…", true, None::<&str>)?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "Quit Myna", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::with_id(meetings::TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "record" => {
                        let app = app.clone();
                        // Starting and stopping block on audio hardware; keep them off the main thread.
                        std::thread::spawn(move || {
                            if let Err(e) = meetings::toggle(&app) {
                                meetings::alert(&e);
                            }
                        });
                    }
                    "meetings" => open_window(app, "meetings", "Meetings", (1000.0, 680.0)),
                    "settings" => open_window(app, "settings", "Myna Settings", (800.0, 560.0)),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            meetings::setup(app, &dir, menu, record)?;

            if !onboarded {
                open_window(app.handle(), "onboarding", "Welcome to Myna", (520.0, 460.0));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_setting,
            set_setting,
            request_permission,
            permission_granted,
            open_privacy_settings,
            list_models,
            download_model,
            cancel_model,
            remove_model,
            verify_model,
            reveal_models_dir,
            meetings::meeting_toggle,
            meetings::list_meetings,
            meetings::get_meeting,
            meetings::retry_transcription,
            meetings::export_meeting
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
