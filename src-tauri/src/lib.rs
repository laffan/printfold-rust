//! PrintFold — Tauri application entry.
//!
//! The webview hosts the editor UI; this crate wires it to the
//! `printfold-core` engine and to the platform (dialogs, files, fonts,
//! `.printfold` file associations).

mod commands;
mod platform;
mod state;

use std::path::PathBuf;

use tauri::{Emitter, Manager};

use state::AppState;

/// Route a `.printfold` the OS asked us to open to the UI (or queue it
/// until the UI has mounted and called `take_pending_opens`).
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
fn deliver_open(app: &tauri::AppHandle, path: PathBuf) {
    let is_project = path.extension().map(|e| e.eq_ignore_ascii_case(platform::PROJECT_EXT)).unwrap_or(false);
    if !is_project {
        return;
    }
    let state = app.state::<AppState>();
    let ready = *state.webview_ready.lock().unwrap();
    if ready {
        let _ = app.emit("open-project", path.to_string_lossy().to_string());
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_focus();
        }
    } else {
        state.pending_opens.lock().unwrap().push(path);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            // Windows/Linux pass a double-clicked file as an argument.
            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
            for arg in std::env::args().skip(1) {
                let p = PathBuf::from(&arg);
                if p.extension().map(|e| e.eq_ignore_ascii_case(platform::PROJECT_EXT)).unwrap_or(false) && p.exists() {
                    app.state::<AppState>().pending_opens.lock().unwrap().push(p);
                }
            }
            // Scan system fonts off the main thread; engine calls wait for it.
            let handle = app.handle().clone();
            std::thread::spawn(move || handle.state::<AppState>().engine.load_system_fonts());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::platform_info,
            commands::system::take_pending_opens,
            commands::system::recents_list,
            commands::system::recents_add,
            commands::system::recents_remove,
            commands::fonts::fonts_list,
            commands::fonts::fonts_variants,
            commands::fonts::fonts_register,
            commands::fonts::fonts_unregister,
            commands::layout::reflow,
            commands::layout::clear_measurement_cache,
            commands::files::file_put,
            commands::files::file_rename,
            commands::files::files_retain,
            commands::files::pick_files,
            commands::files::save_file,
            commands::project::project_new,
            commands::project::project_open_dialog,
            commands::project::project_open_path,
            commands::project::project_save,
            commands::project::project_close,
            commands::pdf::pdf_prerender_plan,
            commands::pdf::pdf_put_prerendered,
            commands::pdf::pdf_clear_prerendered,
            commands::pdf::pdf_generate,
            commands::pdf::pdf_test_page,
        ])
        .build(tauri::generate_context!())
        .expect("error while building PrintFold");

    app.run(|handle, event| {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let tauri::RunEvent::Opened { urls } = &event {
            for url in urls {
                if let Ok(path) = url.to_file_path() {
                    deliver_open(handle, path);
                }
            }
        }
        let _ = (handle, event);
    });
}
