#![allow(clippy::unused_unit)] // tauri_panel! requires `-> ()` on event signatures
mod storage;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{ActivationPolicy, AppHandle, Manager, PhysicalPosition, WindowEvent};
use tauri_nspanel::{tauri_panel, CollectionBehavior, ManagerExt, StyleMask, WebviewWindowExt};

tauri_panel! {
    panel!(PopoverPanel {
        config: { can_become_key_window: true, is_floating_panel: true }
    })
    panel_event!(PopoverEvents {
        window_did_resign_key(notification: &NSNotification) -> ()
    })
}

/// Result of creating the data folder and database: the folder path, or the error to show.
struct Storage(Result<String, String>);

/// When click-away last hid the popover. A click on the menubar icon hides it first, then
/// arrives as a tray click; without this the same click would reopen it.
struct PopoverHiddenAt(Mutex<Option<Instant>>);

#[tauri::command]
fn storage_status(storage: tauri::State<Storage>) -> Result<String, String> {
    storage.0.clone()
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    hide_popover(&app);
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn hide_popover(app: &AppHandle) {
    // AppKit ignores orderOut from the command thread; hop to the main thread.
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(p) = handle.get_webview_panel("popover") {
            p.hide();
        }
    });
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn toggle_popover(app: &AppHandle, icon: tauri::Rect) {
    let Ok(panel) = app.get_webview_panel("popover") else { return };
    if panel.is_visible() {
        panel.hide();
        return;
    }
    let just_hidden = app.state::<PopoverHiddenAt>().0.lock().unwrap()
        .is_some_and(|t| t.elapsed() < Duration::from_millis(250));
    if just_hidden {
        return;
    }
    // Not panel.to_window(): that converts the panel back into a plain window.
    let Some(w) = app.get_webview_window("popover") else { return };
    let scale = w.scale_factor().unwrap_or(2.0);
    let pos = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let width = w.outer_size().map(|s| s.width as f64).unwrap_or(0.0);
    // Centred under the icon, a few points below the menubar, like Control Center.
    let x = pos.x + size.width / 2.0 - width / 2.0;
    let y = pos.y + size.height + 5.0 * scale;
    let _ = w.set_position(PhysicalPosition::new(x, y));
    panel.show_and_make_key();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .manage(PopoverHiddenAt(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![storage_status, open_settings, quit])
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            let dir = app.path().data_dir()?.join("Myna");
            let status = storage::open(&dir).map(|_| dir.display().to_string());
            let failed = status.is_err();
            app.manage(Storage(status));

            let popover = app.get_webview_window("popover").expect("popover window in tauri.conf.json");
            let panel = popover.to_panel::<PopoverPanel>()?;
            panel.set_level(101); // NSPopUpMenuWindowLevel: above the menubar, like a menu
            // Takes keys and clicks without activating Myna or pulling focus from the user's app.
            panel.set_style_mask(StyleMask::empty().nonactivating_panel().value())
                .map_err(|e| format!("popover style: {e:?}"))?;
            // Shows on the current Space, including over full-screen apps.
            panel.set_collection_behavior(
                CollectionBehavior::new().can_join_all_spaces().full_screen_auxiliary().value(),
            );
            let events = PopoverEvents::new();
            let handle = app.handle().clone();
            events.window_did_resign_key(move |_| {
                *handle.state::<PopoverHiddenAt>().0.lock().unwrap() = Some(Instant::now());
                hide_popover(&handle);
            });
            panel.set_event_handler(Some(events.as_ref()));

            let settings_item = MenuItemBuilder::with_id("settings", "Settings…").accelerator("Cmd+,").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Myna").accelerator("Cmd+Q").build(app)?;
            let menu = MenuBuilder::new(app).item(&settings_item).separator().item(&quit_item).build()?;
            TrayIconBuilder::with_id("myna")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("Myna")
                .menu(&menu)
                .show_menu_on_left_click(false) // left click opens the popover; right click the menu
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "settings" => open_settings(app.clone()),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    // On mouse-down, like native menubar items (mouse-up is not delivered reliably here).
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Down, rect, .. } = e {
                        toggle_popover(tray.app_handle(), rect);
                    }
                })
                .build(app)?;

            if failed {
                open_settings(app.handle().clone()); // never fail silently: Settings shows the error
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
