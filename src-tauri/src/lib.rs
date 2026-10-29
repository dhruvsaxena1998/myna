mod permissions;
mod settings;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    ActivationPolicy, AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
};

struct Db(Mutex<rusqlite::Connection>);

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

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "Quit Myna", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => open_window(app, "settings", "Myna Settings", (560.0, 380.0)),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if !onboarded {
                open_window(app.handle(), "onboarding", "Welcome to Myna", (520.0, 460.0));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_setting,
            set_setting,
            request_permission,
            permission_granted,
            open_privacy_settings
        ])
        .build(tauri::generate_context!())
        .expect("error building Myna")
        .run(|_, event| {
            // Tray app: closing every window must not quit.
            if let RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}
