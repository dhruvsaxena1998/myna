mod addon;
mod ai;
mod apple;
mod cleanup;
mod dictation;
mod download;
mod feedback;
mod fnkey;
mod look;
mod meeting;
mod meeting_session;
mod models;
mod notify;
mod permissions;
mod pill;
mod rules;
mod runtime;
mod session;
mod shortcuts;
mod speakers;
mod storage;
mod trace;
mod update;
mod voice;
mod voz;
mod who;

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{ActivationPolicy, AppHandle, Emitter, Listener, Manager, WindowEvent, Wry};
use meeting_session::MeetingSession;
use session::Session;
use shortcuts::{Do, Shortcuts};
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

/// Result of creating the data folder and database: the folder path, or the error to show.
struct Storage(Result<String, String>);

/// Managed only when the database opened.
struct Data {
    db: Arc<Mutex<Connection>>,
    models_dir: PathBuf,
    macos: u32,
    /// Model id -> cancel flag, for each download in progress.
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The meeting notes shortcut registered with macOS right now.
    meeting_shortcut: Mutex<Option<String>>,
    /// API keys for AI providers: the Keychain.
    keys: Arc<dyn ai::Keys>,
}

/// "Start voice typing" in the menubar menu, which shows the shortcut.
struct VoiceItem(MenuItem<Wry>);

/// "Start meeting notes" (or "Stop recording") in the menubar menu, and the note under it.
struct MeetItem {
    item: MenuItem<Wry>,
    note: MenuItem<Wry>,
}

/// The last meetings under "Recent" in the menubar menu. Tauri menus can't hide an item, so they go in and out.
struct Recent {
    menu: Menu<Wry>,
    shown: Mutex<Vec<tauri::menu::MenuItemKind<Wry>>>,
}

/// "Start voice typing with cleanup": in the menu only while a Fast model is set (`sync_clean_item`).
struct CleanItem {
    item: MenuItem<Wry>,
    menu: Menu<Wry>,
}

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

/// What the Settings window hears about a model: `{ id, state: "progress" | "done" | "canceled" | "failed" | "loaded", ... }`.
/// "failed" also comes from a downloaded model that won't load; "loaded" clears that.
pub(crate) fn model_event(app: &AppHandle, id: &str, state: &str, extra: serde_json::Value) {
    let mut p = serde_json::json!({ "id": id, "state": state });
    if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    let _ = app.emit("model-state", p);
}

#[tauri::command]
fn model_download(app: AppHandle, d: tauri::State<Data>, id: String) -> Result<(), String> {
    let m = models::find(&id).ok_or_else(|| format!("Unknown model {id}"))?;
    if m.files.is_empty() && !m.system && !m.sdk {
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
        } else if m.sdk {
            // shortcut: the SDK's download can't be canceled, so the row shows no Cancel; add one if users ask.
            let pct = |done: f64| progress((done.clamp(0.0, 1.0) * 100.0) as u64);
            let dir = dir.join(&m.id);
            if m.id == "who" { who::download(&dir, pct) } else { voz::download(&dir, pct) }.map(|()| true)
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

/// Returns `task`'s active model afterwards (the first task with the model when there's none): deleting the chosen one
/// falls back to the default, and for meeting notes to voice typing's pick.
#[tauri::command]
async fn model_delete(d: tauri::State<'_, Data>, id: String, task: Option<String>) -> Result<Option<String>, String> {
    let m = models::find(&id).ok_or_else(|| format!("Unknown model {id}"))?;
    if d.downloads.lock().unwrap().contains_key(&id) {
        return Err(format!("{} is downloading. Cancel it first.", m.name));
    }
    let downloading: Vec<String> = d.downloads.lock().unwrap().keys().cloned().collect();
    download::remove(&m, &d.models_dir, &downloading)?;
    let db = d.db.lock().unwrap();
    models::forget(&db, &id)?;
    let tasks = models::list(&db, &d.models_dir, d.macos, &apple::status())?;
    let mine = |t: &models::Task| task.as_deref().map_or_else(|| t.models.iter().any(|x| x.id == id), |task| t.id == task);
    Ok(tasks.into_iter().find(mine).and_then(|t| t.active))
}

/// `MYNA_DEBUG=1` turns on debug aids in the UI (for now: the window size while resizing).
#[tauri::command]
fn debug_on() -> bool {
    std::env::var("MYNA_DEBUG").is_ok_and(|v| !v.is_empty() && v != "0")
}

#[tauri::command]
fn ai_get(d: tauri::State<Data>) -> Result<ai::State, String> {
    ai::get(&d.db.lock().unwrap(), &*d.keys)
}

#[tauri::command]
fn ai_add(d: tauri::State<Data>, kind: String, name: String, url: String, key: String) -> Result<ai::Provider, String> {
    ai::add(&d.db.lock().unwrap(), &*d.keys, &kind, &name, &url, &key)
}

#[tauri::command]
fn ai_remove(app: AppHandle, d: tauri::State<Data>, id: i64) -> Result<Vec<String>, String> {
    let slots = ai::remove(&mut d.db.lock().unwrap(), &*d.keys, id)?;
    sync_clean_item(&app);
    Ok(slots)
}

#[tauri::command]
fn ai_set(app: AppHandle, d: tauri::State<Data>, slot: String, provider: i64, model: String) -> Result<(), String> {
    ai::set(&d.db.lock().unwrap(), &slot, provider, &model)?;
    sync_clean_item(&app);
    Ok(())
}

// The two below call out over the network, so they run off the main thread and never hold the database lock while waiting.
#[tauri::command]
async fn ai_models(d: tauri::State<'_, Data>, id: i64, fresh: bool) -> Result<Vec<ai::Model>, String> {
    let (db, keys) = (d.db.clone(), d.keys.clone());
    tauri::async_runtime::spawn_blocking(move || ai::models(&db, &*keys, id, fresh))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn ai_test(d: tauri::State<'_, Data>, id: i64) -> Result<ai::Check, String> {
    let (db, keys) = (d.db.clone(), d.keys.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let p = ai::provider(&db.lock().unwrap(), &*keys, id)?;
        ai::test(&db, id, &*p)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn ai_test_draft(kind: String, url: String, key: String) -> Result<ai::Check, String> {
    tauri::async_runtime::spawn_blocking(move || ai::test_draft(&kind, &url, &key)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn permissions_state() -> permissions::State {
    permissions::state()
}

/// Asks macOS for `kind` ("mic", "ax" or "audio"); answers once there's a new state to read.
#[tauri::command]
async fn permission_ask(kind: String) {
    match (kind.as_str(), permissions::mic()) {
        ("mic", permissions::Mic::Ask) => permissions::ask_mic(),
        ("mic", _) => permissions::open_pane("Privacy_Microphone"),
        ("audio", _) => permissions::ask_audio(),
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

/// Dictation's language and Tidy up switch, and the words to remember.
#[tauri::command]
fn rules_get(d: tauri::State<Data>) -> Result<serde_json::Value, String> {
    let s = rules::load(&d.db.lock().unwrap())?;
    Ok(serde_json::json!({ "settings": s, "languages": rules::LANGUAGES }))
}

/// Saved settings take effect on the next dictation, which loads them when it starts.
#[tauri::command]
fn rules_set(d: tauri::State<Data>, rules: rules::Settings) -> Result<(), String> {
    rules::save(&d.db.lock().unwrap(), &rules)
}

/// Dictate with cleanup's styles, and the installed apps to bind them to.
#[tauri::command]
fn cleanup_get(d: tauri::State<Data>) -> Result<serde_json::Value, String> {
    let styles = cleanup::styles(&d.db.lock().unwrap())?;
    Ok(serde_json::json!({ "styles": styles, "apps": cleanup::installed_apps() }))
}

/// Takes effect on the next dictation, which reads the styles when cleanup starts. Returns them as saved.
#[tauri::command]
fn cleanup_set(d: tauri::State<Data>, styles: Vec<cleanup::Edit>) -> Result<Vec<cleanup::Style>, String> {
    cleanup::save(&d.db.lock().unwrap(), styles)
}

/// The Words to remember line a style's editor says Myna adds.
#[tauri::command]
fn cleanup_spell(d: tauri::State<Data>) -> Result<String, String> {
    Ok(rules::prompt(&rules::load(&d.db.lock().unwrap())?))
}

/// Stats for nerds: the stored dictation traces, newest first, and one of them opened.
#[tauri::command]
fn traces_list(d: tauri::State<Data>) -> Result<Vec<trace::Row>, String> {
    trace::list(&d.db.lock().unwrap())
}

/// Every trace with its steps, for Stats for nerds' Overview.
#[tauri::command]
fn traces_all(d: tauri::State<Data>) -> Result<Vec<serde_json::Value>, String> {
    trace::all(&d.db.lock().unwrap())
}

#[tauri::command]
fn trace_get(d: tauri::State<Data>, id: i64) -> Result<serde_json::Value, String> {
    trace::get(&d.db.lock().unwrap(), id)
}

/// The dictation Fix it asked for, until the main window takes it. An event alone gets lost when the page
/// isn't listening yet (just reloaded, or waking after being hidden).
static FIX: Mutex<Option<i64>> = Mutex::new(None);

/// "Fix it" on the pill after a 👎: opens Voice typing history with that dictation's What did you mean? box.
#[tauri::command]
fn dictation_fix(app: AppHandle, trace: i64) {
    *FIX.lock().unwrap() = Some(trace);
    open_main(&app);
    let _ = app.emit_to("main", "fix", trace);
}

/// The main window asks on load, on focus and on the "fix" event; the first ask gets it.
#[tauri::command]
fn fix_take() -> Option<i64> {
    FIX.lock().unwrap().take()
}

/// Delete in Voice typing history.
#[tauri::command]
fn trace_delete(d: tauri::State<Data>, id: i64) -> Result<(), String> {
    trace::delete(&d.db.lock().unwrap(), id)
}

/// Clear all, in Voice typing history and Stats for nerds: they read the same traces.
#[tauri::command]
fn traces_clear(d: tauri::State<Data>) -> Result<(), String> {
    trace::clear(&d.db.lock().unwrap())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

/// 👍/👎 on a dictation (`rating` none takes it back); returns the fixes now suggested.
#[tauri::command]
fn feedback_set(d: tauri::State<Data>, trace: i64, rating: Option<String>, corrected: Option<String>) -> Result<Vec<feedback::Fix>, String> {
    let db = d.db.lock().unwrap();
    feedback::set(&db, trace, rating.as_deref(), corrected.as_deref(), now_ms())?;
    feedback::fixes(&db)
}

#[tauri::command]
fn fixes_get(d: tauri::State<Data>) -> Result<Vec<feedback::Fix>, String> {
    feedback::fixes(&d.db.lock().unwrap())
}

/// Apply a suggested fix; returns the fixes left.
#[tauri::command]
fn fix_add(d: tauri::State<Data>, key: String) -> Result<Vec<feedback::Fix>, String> {
    let db = d.db.lock().unwrap();
    feedback::add(&db, &key)?;
    feedback::fixes(&db)
}

/// Not now; returns the fixes left.
#[tauri::command]
fn fix_dismiss(d: tauri::State<Data>, key: String) -> Result<Vec<feedback::Fix>, String> {
    let db = d.db.lock().unwrap();
    feedback::dismiss(&db, &key)?;
    feedback::fixes(&db)
}

/// Export feedback: writes the JSON to Downloads, shows it in Finder, returns its path.
#[tauri::command]
fn feedback_export(d: tauri::State<Data>) -> Result<String, String> {
    let json = feedback::export(&d.db.lock().unwrap(), now_ms())?;
    let downloads = std::path::PathBuf::from(std::env::var("HOME").map_err(|_| "Couldn't find your home folder.")?).join("Downloads");
    let date = std::process::Command::new("/bin/date").arg("+%Y-%m-%d").output().map_err(|e| e.to_string())?;
    let path = feedback::write_export(&downloads, String::from_utf8_lossy(&date.stdout).trim(), &json)?;
    let _ = std::process::Command::new("/usr/bin/open").arg("-R").arg(&path).status();
    Ok(path.display().to_string())
}

/// Copy again and Copy what I said, in Voice typing history.
#[tauri::command]
fn text_copy(text: String) {
    dictation::copy(&text);
}

/// The main window's Settings entry and its ⌘,.
#[tauri::command]
fn settings_open(app: AppHandle) {
    open_window(&app, "settings");
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
    app.state::<Shortcuts>().set_paused(false);
    if let Err(e) = sync_shortcut(&app) {
        voice::set(&d.db.lock().unwrap(), &was)?;
        let _ = sync_shortcut(&app);
        let _ = sync_meeting(&app);
        return Err(e);
    }
    let _ = sync_meeting(&app); // unpaused: meeting notes take their shortcut back; its own page reports a failure
    Ok(())
}

/// Lets go of every shortcut (voice typing's and meeting notes') while Settings records a new one, and takes them back after.
#[tauri::command]
fn voice_pause(app: AppHandle, d: tauri::State<Data>, paused: bool) -> Result<(), String> {
    let v = voice::get(&d.db.lock().unwrap())?;
    let voice = app.state::<Shortcuts>().pause(paused, &v).map(|()| voice_menu(&app, &v));
    sync_meeting(&app).and(voice)
}

#[tauri::command]
fn meeting_get(d: tauri::State<Data>) -> Result<meeting::Notes, String> {
    meeting::get(&d.db.lock().unwrap())
}

/// Saves meeting notes' settings and registers the shortcut at once, like `voice_set`. Turning notes off stops a
/// recording in progress first, and keeps it.
#[tauri::command]
async fn meeting_set(app: AppHandle, d: tauri::State<'_, Data>, notes: meeting::Notes) -> Result<(), String> {
    let was = meeting::get(&d.db.lock().unwrap())?;
    meeting::set(&d.db.lock().unwrap(), &notes)?;
    app.state::<Shortcuts>().set_paused(false); // a recorder saving its pick is done recording
    if let Err(e) = sync_meeting(&app) {
        meeting::set(&d.db.lock().unwrap(), &was)?;
        let _ = sync_meeting(&app);
        let _ = sync_shortcut(&app);
        return Err(e);
    }
    let _ = sync_shortcut(&app); // unpaused: voice typing takes its shortcuts back; Listen reports a failure
    if !notes.on {
        app.state::<Arc<MeetingSession>>().stop();
    }
    Ok(())
}

/// The browser add-on's state for Settings: whether it's connected, and the code to type into it.
#[tauri::command]
fn addon_get(a: tauri::State<addon::Addon>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "connected": a.connected(), "code": a.code()?, "selectors": a.selectors() }))
}

/// Cuts the add-on off and returns the new code.
#[tauri::command]
fn addon_disconnect(a: tauri::State<addon::Addon>) -> Result<String, String> {
    a.disconnect()
}

/// Shows the add-on's folder in Finder: the repo's in dev builds, the app bundle's in release builds.
#[tauri::command]
fn addon_folder(app: AppHandle) -> Result<(), String> {
    let dir = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../extension")
    } else {
        app.path().resource_dir().map_err(|e| e.to_string())?.join("extension")
    };
    std::process::Command::new("open").arg(&dir).spawn().map(drop).map_err(|e| format!("Couldn't open {}: {e}", dir.display()))
}

/// Shows the data folder in Finder.
#[tauri::command]
fn storage_reveal(storage: tauri::State<Storage>) -> Result<(), String> {
    let dir = storage.0.clone()?;
    std::process::Command::new("open").arg(&dir).spawn().map(drop).map_err(|e| format!("Couldn't open {dir}: {e}"))
}

/// Try again on a meeting whose notes failed. Its progress and result come as `meeting-notes` events.
#[tauri::command]
fn meeting_retry(s: tauri::State<Arc<MeetingSession>>, id: i64) -> Result<(), String> {
    s.retry(id)
}

/// Every meeting, newest first, for the Meetings list.
#[tauri::command]
fn meetings_list(d: tauri::State<Data>) -> Result<Vec<meeting::Row>, String> {
    meeting::list(&d.db.lock().unwrap())
}

/// One meeting and its transcript, for its page.
#[tauri::command]
fn meeting_page(d: tauri::State<Data>, id: i64) -> Result<meeting::Page, String> {
    meeting::page(&d.db.lock().unwrap(), id)
}

/// The recording in progress, `{ id, started, source, mic }` (source: the app on the call, once heard), for the Meetings
/// list's live card; none when not recording.
#[tauri::command]
fn meeting_running(s: tauri::State<Arc<MeetingSession>>) -> Option<serde_json::Value> {
    s.running()
}

/// What the add-on reports: Settings hears about its connection here, the Meeting session about the call.
fn on_addon(app: &AppHandle, e: addon::AddonEvent) {
    use addon::AddonEvent as E;
    match e {
        E::Connected | E::SelectorsChanged => {
            let _ = app.emit("addon-status", true);
        }
        E::Disconnected => {
            let _ = app.emit("addon-status", false);
        }
        _ => {}
    }
    app.state::<Arc<MeetingSession>>().addon(e);
}

/// "Start notes" and "Stop" in the Meetings list: what the shortcut does.
#[tauri::command]
fn meeting_toggle(app: AppHandle) {
    toggle_meeting(&app);
}

/// "Open" on the pill's "Notes saved". The meeting page is the main window's (05.4): it listens for `meeting-open`.
#[tauri::command]
fn meeting_open(app: AppHandle, id: i64) {
    open_main(&app);
    let _ = app.emit_to("main", "meeting-open", id);
}

/// Whether the 🌐 key is set to Do Nothing, so fn only starts voice typing.
#[tauri::command]
fn globe_ok() -> bool {
    fnkey::globe_ok()
}

#[tauri::command]
fn globe_open() {
    fnkey::open_keyboard_settings();
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
        let _ = update::remember(&app, &d.db.lock().unwrap()); // the old feed's update no longer applies
        let _ = app.emit("update-channel", &prefs.channel); // the pop-up's "You have Y · Beta"
    }
    Ok(())
}

#[tauri::command]
fn update_state(u: tauri::State<update::Updates>) -> update::State {
    u.state.lock().unwrap().clone()
}

/// The Check for updates button: the update pop-up opens with the result. The error also comes back here.
#[tauri::command]
async fn update_check(app: AppHandle, d: tauri::State<'_, Data>) -> Result<(), String> {
    let p = update::get(&d.db.lock().unwrap(), prerelease(&app))?;
    let r = update::check(&app, update::FEEDS, &p, true).await;
    tell_update(&app, &p.mode, true);
    open_update(&app);
    r
}

#[tauri::command]
fn update_cancel(app: AppHandle) {
    update::cancel(&app);
}

#[tauri::command]
fn update_open(app: AppHandle) {
    open_update(&app);
}

/// Links in What's new open in the browser, never in the pop-up.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!("{url} isn't a web link."));
    }
    std::process::Command::new("open").arg(&url).spawn().map(drop).map_err(|e| format!("Couldn't open {url}: {e}"))
}

#[tauri::command]
async fn update_download(app: AppHandle, d: tauri::State<'_, Data>) -> Result<(), String> {
    let p = update::get(&d.db.lock().unwrap(), prerelease(&app))?;
    update::download(&app, update::FEEDS, &p.channel, false).await
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
    std::thread::spawn(move || {
        // Daily or weekly: the last check's update shows at once; in Check and download it's fetched again.
        let prefs = update::get(&app.state::<Data>().db.lock().unwrap(), prerelease(&app));
        if let Some(p) = prefs.ok().filter(|p| p.mode != "off") {
            let restored = update::restore(&app, &app.state::<Data>().db.lock().unwrap(), &p) == Ok(true);
            if restored && p.mode == "auto" {
                let _ = tauri::async_runtime::block_on(update::download(&app, update::FEEDS, &p.channel, true));
            }
        }
        loop {
            let last = *app.state::<update::Updates>().last.lock().unwrap();
            let prefs = update::get(&app.state::<Data>().db.lock().unwrap(), prerelease(&app));
            if let Some(p) = prefs.ok().filter(|p| update::due(p, last, std::time::SystemTime::now())) {
                let _ = tauri::async_runtime::block_on(update::check(&app, update::FEEDS, &p, false));
                tell_update(&app, &p.mode, false);
            }
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    });
}

/// After a check: saves what it found and sends the notification it earned, if any.
fn tell_update(app: &AppHandle, mode: &str, manual: bool) {
    let s = app.state::<update::Updates>().state.lock().unwrap().clone();
    let d = app.state::<Data>();
    let db = d.db.lock().unwrap();
    let _ = update::remember(app, &db);
    if let Ok(Some((title, body))) = update::tell(&db, mode, &s, manual) {
        notify::send(&title, &body);
    }
}

/// Registers the saved voice typing keys through the Shortcuts module, then sets the menu items to match.
fn sync_shortcut(app: &AppHandle) -> Result<(), String> {
    let v = voice::get(&app.state::<Data>().db.lock().unwrap())?;
    app.state::<Shortcuts>().sync(&v)?;
    voice_menu(app, &v);
    Ok(())
}

fn voice_menu(app: &AppHandle, v: &voice::Voice) {
    if let Some(item) = app.try_state::<VoiceItem>() {
        // A menu can't show fn alone as its shortcut.
        let _ = item.0.set_accelerator((v.key != voice::FN).then_some(v.key.as_str()));
        let _ = item.0.set_enabled(v.on);
    }
    sync_clean_item(app);
}

/// The shortcut to register: none while off, empty, or paused for a recorder in Settings.
fn meeting_want(n: &meeting::Notes, paused: bool) -> Option<String> {
    (n.on && !n.key.is_empty() && !paused).then(|| n.key.clone())
}

/// Registers the meeting notes shortcut while they're on, and sets the menu item and its note to match.
fn sync_meeting(app: &AppHandle) -> Result<(), String> {
    let d = app.state::<Data>();
    let n = meeting::get(&d.db.lock().unwrap())?;
    let want = meeting_want(&n, app.state::<Shortcuts>().paused());
    let mut have = d.meeting_shortcut.lock().unwrap();
    if *have != want {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let gs = app.global_shortcut();
        if let Some(k) = &want {
            gs.register(k.as_str()).map_err(|e| {
                eprintln!("meeting notes: couldn't use {k}: {e}");
                "Another app is using that shortcut. Pick a different one.".to_string()
            })?;
        }
        if let Some(k) = have.as_ref() {
            let _ = gs.unregister(k.as_str());
        }
        *have = want;
    }
    drop(have);
    // The add-on's socket is open only while notes are on.
    if let Some(a) = app.try_state::<addon::Addon>() {
        if n.on {
            if let Err(e) = a.start() {
                eprintln!("browser add-on: {e}");
            }
        } else {
            a.stop();
        }
    }
    if let Some(m) = app.try_state::<MeetItem>() {
        let _ = m.item.set_accelerator((!n.key.is_empty()).then_some(n.key.as_str()));
        let _ = m.item.set_enabled(n.on);
        let _ = m.note.set_text(if n.on { "Records whichever app is using your microphone." } else { "Meeting notes are off. Turn them on in Settings." });
    }
    Ok(())
}

/// The shortcut, or the menu item: start meeting notes, or stop if recording. Off the main thread, since the mic takes a moment.
fn toggle_meeting(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || app.state::<Arc<MeetingSession>>().toggle());
}

/// The transcript as Copy and the AI features may see it: muted lines left out. See `meeting::outgoing`.
#[tauri::command]
fn meeting_text(d: tauri::State<Data>, id: i64) -> Result<String, String> {
    meeting::outgoing(&d.db.lock().unwrap(), id)
}

/// "Include" on a collapsed run of muted lines: keeps them, marked.
#[tauri::command]
fn segments_include(d: tauri::State<Data>, ids: Vec<i64>) -> Result<(), String> {
    meeting::include(&d.db.lock().unwrap(), &ids)
}

/// Puts the last 3 meetings in the menu under "Recent", or takes the section out when there are none.
fn sync_recent(app: &AppHandle) {
    let (Some(r), Some(d)) = (app.try_state::<Recent>(), app.try_state::<Data>()) else { return };
    // Read first: the menu calls wait on the main thread, which mustn't wait on this lock.
    let rows = meeting::list(&d.db.lock().unwrap()).unwrap_or_default();
    let mut shown = r.shown.lock().unwrap();
    for old in shown.drain(..) {
        let _ = r.menu.remove(&old);
    }
    if rows.is_empty() {
        return;
    }
    let now = now_ms();
    let mut items: Vec<tauri::menu::MenuItemKind<Wry>> = vec![];
    if let Ok(h) = MenuItemBuilder::with_id("recent", "Recent").enabled(false).build(app) {
        items.push(tauri::menu::MenuItemKind::MenuItem(h));
    }
    for m in rows.iter().take(3) {
        let label = format!("{}  ·  {}", m.title, meeting::when(m.started, now));
        if let Ok(i) = MenuItemBuilder::with_id(format!("recent-{}", m.id), label).build(app) {
            items.push(tauri::menu::MenuItemKind::MenuItem(i));
        }
    }
    if let Ok(s) = PredefinedMenuItem::separator(app) {
        items.push(tauri::menu::MenuItemKind::Predefined(s));
    }
    // Before "Open Myna", which the update item at the top doesn't move.
    let at = r.menu.items().ok().and_then(|i| i.iter().position(|x| x.id().as_ref() == "open")).unwrap_or(0);
    for (n, i) in items.iter().enumerate() {
        let _ = r.menu.insert(i, at + n);
    }
    *shown = items;
}

/// Puts "Start voice typing with cleanup" in the menu under "Start voice typing" while a Fast model is set, and takes it out when not.
/// Tauri menus can't hide an item, so it goes in and out like the update item.
fn sync_clean_item(app: &AppHandle) {
    let (Some(c), Some(d)) = (app.try_state::<CleanItem>(), app.try_state::<Data>()) else { return };
    // Read first: the menu calls wait on the main thread, which mustn't wait on this lock.
    let (fast, v) = {
        let db = d.db.lock().unwrap();
        (ai::route(&db, "fast").is_ok(), voice::get(&db).ok())
    };
    let shown = c.menu.get("dictate-clean").is_some();
    if fast && !shown {
        let after = c.menu.items().ok().and_then(|i| i.iter().position(|x| x.id().as_ref() == "dictate")).map_or(0, |i| i + 1);
        let _ = c.menu.insert(&c.item, after);
    } else if !fast && shown {
        let _ = c.menu.remove(&c.item);
    }
    if let Some(v) = v {
        let _ = c.item.set_accelerator(v.clean.as_deref().filter(|k| *k != voice::FN));
        let _ = c.item.set_enabled(v.on);
    }
}

/// fn went down or up: a voice typing key if fn is one of the shortcuts, the recorder's pick while Settings records one.
fn on_fn(app: &AppHandle, down: bool) {
    let todo = app.state::<Shortcuts>().dispatch_fn(down);
    act(app, todo);
}

/// Does what the Shortcuts module said.
fn act(app: &AppHandle, todo: Option<Do>) {
    match todo {
        Some(Do::Dictate { pressed, clean }) => app.state::<Arc<Session>>().key(pressed, clean),
        Some(Do::TellSettings) => {
            let _ = app.emit_to("settings", "voice-fn", ());
        }
        None => {}
    }
}

fn open_window(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The main window, from "Open Myna" and the Dock. Myna sits in the Dock only while it's open.
fn open_main(app: &AppHandle) {
    let _ = app.set_activation_policy(ActivationPolicy::Regular);
    open_window(app, "main");
}

/// The update pop-up, from About, the manual check, the notification and the menubar item.
/// Never on its own from an automatic check.
fn open_update(app: &AppHandle) {
    open_window(app, "update");
    let _ = app.emit_to("update", "update-open", ()); // the page moves focus to its main button
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![pill::pill_hit, dictation_fix, fix_take, debug_on, ai_get, ai_add, ai_remove, ai_set, ai_models, ai_test, ai_test_draft, storage_status, storage_reveal, models_list, model_select, model_download, model_cancel, model_delete, permissions_state, permission_ask, stt_unload_get, stt_unload_set, pill_look_get, pill_look_set, look_get, look_set, glass_set, voice_get, voice_set, rules_get, rules_set, cleanup_get, cleanup_set, cleanup_spell, traces_list, traces_all, trace_get, trace_delete, traces_clear, feedback_set, fixes_get, fix_add, fix_dismiss, feedback_export, text_copy, settings_open, voice_pause, meeting_get, meeting_set, meeting_text, segments_include, addon_get, addon_disconnect, addon_folder, meeting_retry, meeting_open, meetings_list, meeting_page, meeting_running, meeting_toggle, globe_ok, globe_open, updates_get, updates_set, update_state, update_check, update_download, update_restart, update_cancel, update_open, open_link])
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::Updates::default())
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            // Dev builds keep their own database, so a migration tried in dev never locks the installed Myna out
            // ("written by a newer Myna"). Models stay shared: they're big, and their layout never changes.
            let base = app.path().data_dir()?;
            let dir = base.join(if cfg!(debug_assertions) { "Myna Dev" } else { "Myna" });
            if cfg!(debug_assertions) && let Err(e) = storage::seed(&base.join("Myna"), &dir) {
                eprintln!("{e}"); // dev starts empty instead
            }
            let models = base.join("Myna/models");
            let status = match storage::open(&dir) {
                Ok(db) => {
                    let db = Arc::new(Mutex::new(db));
                    let world = Box::new(session::Mac(app.handle().clone()));
                    let tap_limit = std::time::Duration::from_secs(voice::TAP_LIMIT_SECS);
                    let second = std::time::Duration::from_secs(1);
                    let keys: Arc<dyn ai::Keys> = Arc::new(ai::Keychain);
                    download::sweep(&models); // before any download can start
                    app.manage(Session::new(world, db.clone(), keys.clone(), models.clone(), models::macos_major(), tap_limit, second));
                    let meetings = meeting::Meetings::new(db.clone(), dir.clone(), Box::new(meeting::Cpal), Box::new(meeting::Tap));
                    app.manage(MeetingSession::new(Box::new(meeting_session::Mac(app.handle().clone())), db.clone(), meetings, second));
                    let handle = app.handle().clone();
                    app.manage(addon::Addon::new(db.clone(), addon::ORIGIN, &addon::PORTS, move |e| on_addon(&handle, e)));
                    app.manage(Data {
                        db,
                        models_dir: models,
                        macos: models::macos_major(),
                        downloads: Default::default(),
                        meeting_shortcut: Default::default(),
                        keys,
                    });
                    app.manage(Shortcuts::new(Box::new(shortcuts::Mac(app.handle().clone()))));
                    app.handle().plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(|app, shortcut, e| {
                                let meeting = app.state::<Data>().meeting_shortcut.lock().unwrap().clone();
                                if meeting.is_some_and(|k| k.parse::<Shortcut>().ok().as_ref() == Some(shortcut)) {
                                    if e.state() == ShortcutState::Pressed {
                                        toggle_meeting(app);
                                    }
                                    return;
                                }
                                let todo = app.state::<Shortcuts>().dispatch(shortcut, e.state() == ShortcutState::Pressed);
                                // Esc is caught only while recording (Session::start), and never when it's a voice typing key.
                                if todo.is_none() && shortcut.key == tauri_plugin_global_shortcut::Code::Escape {
                                    if e.state() == ShortcutState::Pressed {
                                        app.state::<Arc<Session>>().cancel();
                                    }
                                    return;
                                }
                                act(app, todo);
                            })
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
            let open_item = MenuItemBuilder::with_id("open", "Open Myna").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings…").accelerator("Cmd+,").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Myna").accelerator("Cmd+Q").build(app)?;
            // The shortcut and enabled state are set by sync_shortcut below.
            let voice_item = MenuItemBuilder::with_id("dictate", "Start voice typing").enabled(false).build(app)?;
            // Tauri menus have no section headers, so the section is the item and its note under it.
            let meet_item = MenuItemBuilder::with_id("meeting", "Start meeting notes").enabled(false).build(app)?;
            let meet_note = MenuItemBuilder::with_id("meeting-note", "Meeting notes are off. Turn them on in Settings.").enabled(false).build(app)?;
            let menu = MenuBuilder::new(app).item(&voice_item).separator().item(&meet_item).item(&meet_note).separator().item(&open_item).item(&settings_item).separator().item(&quit_item).build()?;
            app.manage(VoiceItem(voice_item));
            app.manage(MeetItem { item: meet_item, note: meet_note });
            // Not in the menu until a Fast model is set; sync_clean_item adds it.
            let clean_item = MenuItemBuilder::with_id("dictate-clean", "Start voice typing with cleanup").enabled(false).build(app)?;
            app.manage(CleanItem { item: clean_item, menu: menu.clone() });
            app.manage(Recent { menu: menu.clone(), shown: Default::default() });
            // Tauri menus can't hide an item, so the update item and its separator go in and out at the top.
            let (update_item, update_sep) = (MenuItemBuilder::with_id("update", "").build(app)?, PredefinedMenuItem::separator(app)?);
            let (h, m) = (app.handle().clone(), menu.clone());
            app.listen_any("update-state", move |_| {
                // Read first: the menu calls wait on the main thread, which mustn't wait on this lock.
                let label = update::menu_label(&h.state::<update::Updates>().state.lock().unwrap());
                match label {
                    Some(label) => {
                        let _ = update_item.set_text(label);
                        if m.get("update").is_none() {
                            let _ = m.insert_items(&[&update_item, &update_sep], 0);
                        }
                    }
                    None => {
                        let _ = m.remove(&update_item);
                        let _ = m.remove(&update_sep);
                    }
                }
            });
            let h = app.handle().clone();
            notify::setup(move || open_update(&h));
            TrayIconBuilder::with_id("myna")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Myna")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "dictate" => app.state::<Arc<Session>>().toggle(false),
                    "dictate-clean" => app.state::<Arc<Session>>().toggle(true),
                    "meeting" => toggle_meeting(app),
                    id if id.starts_with("recent-") => {
                        if let Ok(id) = id["recent-".len()..].parse() {
                            meeting_open(app.clone(), id);
                        }
                    }
                    "open" => open_main(app),
                    "settings" => open_window(app, "settings"),
                    "update" => open_update(app),
                    "quit" => {
                        if let Some(s) = app.try_state::<Arc<MeetingSession>>() {
                            s.stop(); // saved, not cut off
                        }
                        app.exit(0)
                    }
                    _ => {}
                })
                .build(app)?;

            if !failed && let Err(e) = sync_shortcut(app.handle()) {
                eprintln!("dictation: {e}"); // Voice typing still shows the saved shortcut; picking it again says why
            }
            if !failed && let Err(e) = sync_meeting(app.handle()) {
                eprintln!("meeting notes: {e}");
            }
            // Meetings the app died or quit in the middle of: finish and write them up.
            if !failed {
                sync_recent(app.handle());
                app.state::<Arc<MeetingSession>>().resume();
            }
            if failed {
                open_window(app.handle(), "settings"); // never fail silently: Settings shows the error
            } else {
                watch_updates(app.handle().clone());
            }
            Ok(())
        })
        .on_window_event(|w, e| {
            // Closing a window hides it; the app lives on in the menubar, and leaves the Dock with the main window.
            if let (WindowEvent::CloseRequested { api, .. }, "main" | "settings" | "update") = (e, w.label()) {
                api.prevent_close();
                let _ = w.hide();
                if w.label() == "main" {
                    let _ = w.app_handle().set_activation_policy(ActivationPolicy::Accessory);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running Myna")
        // Clicking Myna in the Dock, or opening it again from Finder, while it runs.
        .run(|app, e| {
            if let tauri::RunEvent::Reopen { .. } = e {
                open_main(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meeting_shortcut_is_let_go_while_a_recorder_listens() {
        let n = |on, key: &str| meeting::Notes { on, key: key.into(), keep_teams: false };
        assert_eq!(meeting_want(&n(true, "Alt+Space"), false), Some("Alt+Space".into()));
        assert_eq!(meeting_want(&n(true, "Alt+Space"), true), None);
        assert_eq!((meeting_want(&n(false, "Alt+Space"), false), meeting_want(&n(true, ""), false)), (None, None));
    }
}
