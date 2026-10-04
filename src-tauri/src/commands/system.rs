//! Platform info, recents and OS file-open hand-off.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};

use super::CmdResult;
use crate::platform::{self, RecentEntry};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: &'static str,
    pub mobile: bool,
}

#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    PlatformInfo { os: std::env::consts::OS, mobile: cfg!(any(target_os = "ios", target_os = "android")) }
}

/// Called once the UI is mounted: returns `.printfold` files the OS asked
/// us to open during launch; later opens arrive as `open-project` events.
#[tauri::command]
pub fn take_pending_opens(state: State<'_, AppState>) -> Vec<String> {
    *state.webview_ready.lock().unwrap() = true;
    let paths: Vec<PathBuf> = std::mem::take(&mut *state.pending_opens.lock().unwrap());
    paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

#[tauri::command]
pub fn recents_list(app: AppHandle) -> Vec<RecentEntry> {
    platform::list_recents(&app)
}

#[tauri::command]
pub fn recents_add(app: AppHandle, path: String, name: String) -> CmdResult<()> {
    platform::add_recent(&app, std::path::Path::new(&path), &name);
    Ok(())
}

#[tauri::command]
pub fn recents_remove(app: AppHandle, path: String) -> CmdResult<()> {
    platform::remove_recent(&app, &path);
    Ok(())
}
