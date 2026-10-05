//! Binary file store sync, file picking and saving.

use std::path::PathBuf;

use base64::Engine as _;
use printfold_core::project_file::file_type_for_extension;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FileAccessMode, FilePath};


use super::{err, header, raw_body, CmdResult};
use crate::platform;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct Filter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub content: String,
    pub is_base64: bool,
}

/// Mirror a binary project file (base64) into the native file store.
#[tauri::command]
pub fn file_put(state: State<'_, AppState>, id: String, name: String, file_type: String, content: String) -> CmdResult<()> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(content.trim()).map_err(err)?;
    state.files.lock().unwrap().put(id, name, file_type, bytes);
    Ok(())
}

#[tauri::command]
pub fn file_rename(state: State<'_, AppState>, id: String, name: String) {
    state.files.lock().unwrap().rename(&id, name);
}

/// Drop every stored file whose id is not in `ids`.
#[tauri::command]
pub fn files_retain(state: State<'_, AppState>, ids: Vec<String>) {
    state.files.lock().unwrap().retain(&ids);
}

/// Read a file as a project file: markdown as text, everything else base64.
pub(crate) fn read_picked(path: &std::path::Path) -> CmdResult<PickedFile> {
    let bytes = std::fs::read(path).map_err(err)?;
    let name = platform::file_name(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    let is_text = matches!(ext.as_str(), "md" | "markdown" | "txt");
    let file_type = if is_text { "markdown".to_string() } else { file_type_for_extension(&ext).to_string() };
    Ok(PickedFile {
        name,
        file_type,
        content: if is_text {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        },
        is_base64: !is_text,
    })
}

pub(crate) fn into_path(fp: FilePath) -> CmdResult<PathBuf> {
    fp.into_path().map_err(err)
}

/// Show an open dialog and read the chosen files (markdown as text,
/// everything else base64).
#[tauri::command]
pub async fn pick_files(app: AppHandle, filters: Vec<Filter>, multiple: bool) -> CmdResult<Vec<PickedFile>> {
    let mut dialog = app.dialog().file().set_file_access_mode(FileAccessMode::Copy);
    for f in &filters {
        let exts: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(&f.name, &exts);
    }
    let picked: Vec<PathBuf> = if let Some(paths) = platform::e2e_picks() {
        paths
    } else if multiple {
        dialog.blocking_pick_files().unwrap_or_default().into_iter().map(into_path).collect::<CmdResult<_>>()?
    } else {
        dialog.blocking_pick_file().into_iter().map(into_path).collect::<CmdResult<_>>()?
    };
    let mut out = Vec::new();
    for path in picked {
        out.push(read_picked(&path)?);
    }
    Ok(out)
}

/// Save bytes chosen by the user. Raw body = file content; headers:
/// `x-file-name`, `x-filter-name`, `x-filter-ext` (comma separated), and
/// optionally `x-anchor` ("x,y" in webview coordinates) for the share
/// popover.
///
/// macOS: a save panel. iPadOS: the system share sheet (Save to Files,
/// AirDrop, Mail, Print …) for a temporary copy.
#[tauri::command]
pub async fn save_file(app: AppHandle, window: tauri::WebviewWindow, request: tauri::ipc::Request<'_>) -> CmdResult<bool> {
    let bytes = raw_body(&request)?;
    let name = sanitize_name(&header(&request, "x-file-name").unwrap_or_else(|| "export".into()));
    let filter_name = header(&request, "x-filter-name").unwrap_or_else(|| "File".into());
    let exts: Vec<String> = header(&request, "x-filter-ext")
        .map(|s| s.split(',').map(|e| e.trim().to_string()).filter(|e| !e.is_empty()).collect())
        .unwrap_or_default();
    if let Some(dir) = platform::e2e_dir() {
        platform::atomic_write(&dir.join(&name), &bytes).map_err(err)?;
        return Ok(true);
    }
    if cfg!(mobile) {
        let (x, y) = header(&request, "x-anchor")
            .and_then(|a| a.split_once(',').and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?))))
            .unwrap_or((0.0, 0.0));
        let tmp = std::env::temp_dir().join(&name);
        platform::atomic_write(&tmp, &bytes).map_err(err)?;
        super::library::share_path(&app, window, &tmp, x, y).await?;
        return Ok(true);
    }
    let ext_refs: Vec<&str> = exts.iter().map(String::as_str).collect();
    let mut dialog = app.dialog().file().set_file_name(&name);
    if !ext_refs.is_empty() {
        dialog = dialog.add_filter(&filter_name, &ext_refs);
    }
    let Some(fp) = dialog.blocking_save_file() else { return Ok(false) };
    let mut path = into_path(fp)?;
    if let Some(first) = exts.first() {
        path = platform::ensure_extension(path, first);
    }
    std::fs::write(&path, bytes).map_err(err)?;
    Ok(true)
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c }).collect();
    if cleaned.trim().is_empty() { "export".into() } else { cleaned }
}
