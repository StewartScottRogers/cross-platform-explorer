//! System-tray icon + quick-access menu + show/hide + close-to-tray (CPE-1272, epic CPE-713).
//!
//! Tauri-v2 tray wiring only — the *model* is `cpe_server::tray_quick::QuickAccess` (CPE-946): a pure,
//! bounded list of pinned + recent folders with its own persistence (`load`/`save` to the app data dir).
//! This module renders that model's `items()` into a native tray menu, keeps the menu in sync as the user
//! opens folders (`note_folder`), and handles the tray's click/menu events:
//!
//! - **Left-click the tray icon** → toggle the main window (show + unminimize + focus when hidden).
//! - **A quick-access entry** → show/focus the window and emit `tray://open-folder` so the frontend
//!   navigates there (reusing the existing navigation path).
//! - **Show/Hide Window** → same toggle as a left-click.
//! - **Quit** → exit the app (always available, regardless of the close-to-tray setting).
//!
//! Close-to-tray (window close hides instead of quitting) is opt-in via a Settings toggle
//! (`cpe.closeToTray`), read from `settings.json` on each close — **default off** so a plain close still
//! quits, never surprising the user. Desktop-only: this whole module is gated out on mobile, which has no
//! system tray.

use std::sync::Mutex;

use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use cpe_server::tray_quick::QuickAccess;

/// How many recent folders the tray remembers (pinned entries are unbounded).
const MAX_RECENT: usize = 8;

/// The tray icon's stable id, so `note_folder` can re-fetch it to swap the menu — and so the window
/// CloseRequested handler can check the tray actually exists before hiding-to-tray (never trap the user
/// with a hidden window and no tray Quit if `setup` failed to create the icon).
pub const TRAY_ID: &str = "main";

/// Menu-item id prefix carrying a folder path to open (`tray-open::<path>`).
const OPEN_PREFIX: &str = "tray-open::";
/// Menu-item id: toggle the main window's visibility.
const TOGGLE_ID: &str = "tray-toggle";
/// Menu-item id: quit the app.
const QUIT_ID: &str = "tray-quit";
/// Menu-item id: the disabled placeholder shown when there are no quick-access entries.
const EMPTY_ID: &str = "tray-empty";

/// Event the frontend listens for to navigate to a tray-chosen folder.
const OPEN_EVENT: &str = "tray://open-folder";

/// The tray's live quick-access state (the persisted model, held in memory while the app runs).
pub struct TrayState(pub Mutex<QuickAccess>);

/// Build the tray menu from the current quick-access model: pinned-then-recent entries (each opening its
/// folder), then Show/Hide and Quit. An empty model shows a single disabled placeholder so the section is
/// never blank.
fn build_menu(app: &AppHandle, qa: &QuickAccess) -> tauri::Result<Menu<Wry>> {
    let mut owned: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();

    let entries = qa.items();
    if entries.is_empty() {
        owned.push(Box::new(MenuItem::with_id(
            app,
            EMPTY_ID,
            "No recent folders",
            false,
            None::<&str>,
        )?));
    } else {
        for e in entries {
            let text = if e.label.trim().is_empty() { e.path.as_str() } else { e.label.as_str() };
            let id = format!("{OPEN_PREFIX}{}", e.path);
            owned.push(Box::new(MenuItem::with_id(app, id, text, true, None::<&str>)?));
        }
    }

    owned.push(Box::new(PredefinedMenuItem::separator(app)?));
    owned.push(Box::new(MenuItem::with_id(app, TOGGLE_ID, "Show/Hide Window", true, None::<&str>)?));
    owned.push(Box::new(MenuItem::with_id(
        app,
        QUIT_ID,
        "Quit Cross-Platform Explorer",
        true,
        None::<&str>,
    )?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// Toggle the main window: hide it if visible, otherwise show + unminimize + focus it.
fn toggle_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            show_and_focus(&win);
        }
    }
}

/// Bring the main window to the foreground (used by both "show" and "open a folder").
fn show_and_focus(win: &tauri::WebviewWindow) {
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();
}

/// Handle a tray menu click.
fn on_menu(app: &AppHandle, id: &str) {
    match id {
        TOGGLE_ID => toggle_window(app),
        QUIT_ID => app.exit(0),
        _ if id.starts_with(OPEN_PREFIX) => {
            let path = &id[OPEN_PREFIX.len()..];
            if let Some(win) = app.get_webview_window("main") {
                show_and_focus(&win);
            }
            // Let the frontend perform the actual navigation via its existing path.
            let _ = app.emit(OPEN_EVENT, path);
        }
        _ => {} // EMPTY_ID / separators / unknown — nothing to do.
    }
}

/// Rebuild + swap the tray menu to reflect the current quick-access state. A no-op if the tray icon or
/// its state isn't present (e.g. `setup` failed to create the tray), so this never panics.
fn rebuild_menu(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let Some(state) = app.try_state::<TrayState>() else { return };
    let qa = { state.0.lock().unwrap().clone() };
    if let Ok(menu) = build_menu(app, &qa) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Record that a folder was opened: move it to the front of recents, persist, and refresh the tray menu.
/// Called from the `tray_note_folder` command, which the frontend fires on every folder navigation.
/// Degrades to a no-op if the tray state was never managed (i.e. `setup` failed) — `try_state` instead of
/// `state`, so a failed tray never turns navigation into a panic.
pub fn note_folder(app: &AppHandle, path: &str, label: &str) -> Result<(), String> {
    let Some(state) = app.try_state::<TrayState>() else { return Ok(()) };
    let ctx = crate::server_ctx::TauriCtx::new(app);
    {
        let mut qa = state.0.lock().unwrap();
        qa.touch(path, label);
        cpe_server::tray_quick::save(&ctx, &qa)?;
    }
    rebuild_menu(app);
    Ok(())
}

/// The pure hide-to-tray decision for a window close: hide (stay tray-resident) ONLY when a tray icon is
/// actually present AND the user opted into close-to-tray. If the tray failed to create, this is always
/// `false` so the close falls through to a normal quit — the user is never trapped with a hidden window
/// and no tray Quit (the CPE-1272 review fix).
pub fn should_hide_to_tray(tray_present: bool, close_to_tray: bool) -> bool {
    tray_present && close_to_tray
}

/// Whether a window-close should hide-to-tray instead of quitting. Reads the persisted Settings flag
/// (`cpe.closeToTray`) fresh on each close; **default false** so a plain close still quits.
pub fn close_to_tray_enabled(app: &AppHandle) -> bool {
    let ctx = crate::server_ctx::TauriCtx::new(app);
    let doc = cpe_server::settings::load(&ctx).unwrap_or_else(|_| "{}".to_string());
    serde_json::from_str::<serde_json::Value>(&doc)
        .ok()
        .and_then(|v| v.get("cpe.closeToTray").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

/// Create the tray icon (reusing the bundled app icon), attach its menu, and wire click/menu events.
/// Called once from `setup()`. Loads the persisted quick-access state and registers it as managed state.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let ctx = crate::server_ctx::TauriCtx::new(handle);
    let qa = cpe_server::tray_quick::load(&ctx, MAX_RECENT);
    let menu = build_menu(handle, &qa)?;
    app.manage(TrayState(Mutex::new(qa)));

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Cross-Platform Explorer")
        .menu(&menu)
        // Left-click toggles the window; the menu still opens on right-click.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        });

    // Reuse the already-bundled app icon (no new bundle resource → CPE-1271 guard stays green).
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::should_hide_to_tray;

    #[test]
    fn hide_to_tray_requires_both_a_tray_and_the_opt_in() {
        // The happy path: tray exists and the user opted in → hide (stay resident).
        assert!(should_hide_to_tray(true, true));

        // Never hide without the opt-in (default off) — a plain close quits.
        assert!(!should_hide_to_tray(true, false));

        // The review fix: even with close-to-tray ON, a MISSING tray must NOT hide, or the window is
        // stranded with no way back and no tray Quit.
        assert!(!should_hide_to_tray(false, true));

        // No tray, no opt-in → plainly a normal close.
        assert!(!should_hide_to_tray(false, false));
    }
}
