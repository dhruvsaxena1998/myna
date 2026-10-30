//! Update checks against the release feed in tauri.conf.json. The pending update is held between the
//! check and the install so About can show the version before anything downloads.
use std::sync::Mutex;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// The newer version the feed offers, or None when this build is current.
#[tauri::command(async)]
pub async fn update_check(app: AppHandle, pending: State<'_, Pending>) -> Result<Option<String>, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let version = update.as_ref().map(|u| u.version.clone());
    *pending.0.lock().unwrap() = update;
    Ok(version)
}

/// Downloads and installs the update found by `update_check`, then restarts into it.
#[tauri::command(async)]
pub async fn update_install(app: AppHandle, pending: State<'_, Pending>) -> Result<(), String> {
    let update = pending.0.lock().unwrap().take().ok_or("Check for updates first.")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart()
}
