//! Native macOS menu bar. Menu actions are forwarded to the UI as
//! `menu` events carrying the item id (handled in `App.ts`).

use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime};

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_menu = Submenu::with_items(
        app,
        "PrintFold",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About PrintFold"), Some(AboutMetadata::default()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "new-project", "New Project…", true, Some("CmdOrCtrl+N"))?,
            &MenuItem::with_id(app, "open-project", "Open Project…", true, Some("CmdOrCtrl+O"))?,
            &MenuItem::with_id(app, "projects", "Projects…", true, Some("CmdOrCtrl+Shift+O"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "add-files", "Add Files…", true, Some("CmdOrCtrl+Shift+A"))?,
            &MenuItem::with_id(app, "export-pdf", "Export PDF…", true, Some("CmdOrCtrl+E"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "toggle-sidebar", "Toggle Files Sidebar", true, Some("CmdOrCtrl+\\"))?,
            &MenuItem::with_id(app, "show-editor", "Editor", true, Some("CmdOrCtrl+1"))?,
            &MenuItem::with_id(app, "show-preview", "Preview", true, Some("CmdOrCtrl+2"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[&PredefinedMenuItem::minimize(app, None)?, &PredefinedMenuItem::maximize(app, None)?],
    )?;
    Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu])
}

pub fn forward<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let _ = app.emit("menu", id.to_string());
}
