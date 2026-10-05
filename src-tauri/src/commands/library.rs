//! Project library: the folder of `.printfold` files shown in the project
//! browser (iPadOS: the app's Documents folder; macOS: ~/Documents/PrintFold),
//! plus file operations on its entries.

use std::path::{Path, PathBuf};

use printfold_core::project_file::read_thumbnail;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, FileAccessMode};

use super::files::into_path;
use super::project::{bind, ProjectLocation};
use super::{err, header, raw_body, CmdResult};
use crate::platform::{self, PROJECT_EXT};
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub path: String,
    /// Human-readable location ("On My iPad › PrintFold", "~/Documents/PrintFold").
    pub display: String,
    /// "Show in Finder" is available.
    pub can_reveal: bool,
    /// Deleting moves projects to the Trash (macOS) instead of removing them.
    pub uses_trash: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub path: String,
    /// File name without the extension.
    pub name: String,
    pub file_name: String,
    /// Last modification, ms since the epoch.
    pub modified: f64,
    pub size: u64,
    /// False for recent projects opened from another folder (macOS).
    pub in_library: bool,
}

fn library_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    platform::library_dir(app).ok_or_else(|| "The projects folder is unavailable".to_string())
}

fn is_project(path: &Path) -> bool {
    path.extension().map(|e| e.eq_ignore_ascii_case(PROJECT_EXT)).unwrap_or(false)
}

fn entry_for(path: &Path, in_library: bool) -> Option<LibraryEntry> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    Some(LibraryEntry {
        path: path.to_string_lossy().into_owned(),
        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        file_name: platform::file_name(path),
        modified,
        size: meta.len(),
        in_library,
    })
}

/// A file name from user input: no path separators, no extension, not empty.
fn clean_stem(name: &str) -> CmdResult<String> {
    let trimmed = name.trim();
    let stem = trimmed
        .strip_suffix(&format!(".{PROJECT_EXT}"))
        .unwrap_or(trimmed)
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c })
        .collect::<String>();
    let stem = stem.trim().trim_start_matches('.').to_string();
    if stem.is_empty() {
        return Err("Enter a project name".into());
    }
    Ok(stem)
}

fn location(path: &Path) -> ProjectLocation {
    ProjectLocation { name: platform::file_name(path), path: path.to_string_lossy().into_owned() }
}

#[tauri::command]
pub fn library_info(app: AppHandle) -> CmdResult<LibraryInfo> {
    let dir = library_dir(&app)?;
    let display = if cfg!(target_os = "ios") {
        "On My iPad › PrintFold".to_string()
    } else {
        let full = dir.to_string_lossy().into_owned();
        match std::env::var("HOME") {
            Ok(home) if !home.is_empty() && full.starts_with(&home) => format!("~{}", &full[home.len()..]),
            _ => full,
        }
    };
    Ok(LibraryInfo {
        path: dir.to_string_lossy().into_owned(),
        display,
        can_reveal: cfg!(desktop),
        uses_trash: cfg!(target_os = "macos"),
    })
}

/// Projects in the library folder, plus (desktop) recent projects that live
/// elsewhere, newest first.
#[tauri::command]
pub fn library_list(app: AppHandle) -> CmdResult<Vec<LibraryEntry>> {
    let dir = library_dir(&app)?;
    let mut out: Vec<LibraryEntry> = std::fs::read_dir(&dir)
        .map_err(err)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_project(p))
        .filter_map(|p| entry_for(&p, true))
        .collect();
    if !cfg!(mobile) {
        for recent in platform::read_recents(&app) {
            let path = PathBuf::from(&recent.path);
            if path.parent() == Some(dir.as_path()) || out.iter().any(|e| e.path == recent.path) {
                continue;
            }
            if let Some(entry) = entry_for(&path, false) {
                out.push(entry);
            }
        }
    }
    out.sort_by(|a, b| b.modified.partial_cmp(&a.modified).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// The project's cover thumbnail (PNG), or an empty body if it has none.
#[tauri::command]
pub fn library_thumbnail(path: String) -> Response {
    let png = std::fs::File::open(&path)
        .ok()
        .and_then(|f| read_thumbnail(std::io::BufReader::new(f)))
        .unwrap_or_default();
    Response::new(png)
}

/// Create an empty project in the library and bind it as the auto-save
/// target (the UI writes the initial content right away).
#[tauri::command]
pub fn library_create(app: AppHandle, state: State<'_, AppState>, name: Option<String>) -> CmdResult<ProjectLocation> {
    let stem = clean_stem(name.as_deref().unwrap_or("Untitled")).unwrap_or_else(|_| "Untitled".into());
    let path = platform::unique_path(&library_dir(&app)?, &stem, PROJECT_EXT);
    std::fs::write(&path, []).map_err(err)?;
    bind(&state, &path);
    state.files.lock().unwrap().clear();
    *state.thumbnail.lock().unwrap() = None;
    platform::add_recent(&app, &path, &platform::file_name(&path));
    Ok(location(&path))
}

#[tauri::command]
pub fn library_rename(app: AppHandle, state: State<'_, AppState>, path: String, name: String) -> CmdResult<ProjectLocation> {
    let from = PathBuf::from(&path);
    let dir = from.parent().ok_or("Invalid project path")?;
    let to = dir.join(format!("{}.{PROJECT_EXT}", clean_stem(&name)?));
    if to == from {
        return Ok(location(&from));
    }
    // Case-only renames refer to the same file on case-insensitive volumes.
    let same_file = to.to_string_lossy().to_lowercase() == from.to_string_lossy().to_lowercase();
    if to.exists() && !same_file {
        return Err(format!("A project named \"{}\" already exists", platform::file_name(&to)));
    }
    std::fs::rename(&from, &to).map_err(err)?;
    {
        let mut bound = state.project_path.lock().unwrap();
        if bound.as_deref() == Some(from.as_path()) {
            *bound = Some(to.clone());
        }
    }
    platform::rename_recent(&app, &from, &to);
    Ok(location(&to))
}

/// Copy a project into the library as "<name> copy".
#[tauri::command]
pub fn library_duplicate(app: AppHandle, path: String) -> CmdResult<ProjectLocation> {
    let from = PathBuf::from(&path);
    let stem = from.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
    let to = platform::unique_path(&library_dir(&app)?, &format!("{stem} copy"), PROJECT_EXT);
    std::fs::copy(&from, &to).map_err(err)?;
    Ok(location(&to))
}

/// Delete projects: to the Trash on macOS, permanently elsewhere (the UI
/// confirms first).
#[tauri::command]
pub fn library_delete(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<()> {
    for path in &paths {
        let p = PathBuf::from(path);
        {
            let mut bound = state.project_path.lock().unwrap();
            if bound.as_deref() == Some(p.as_path()) {
                *bound = None;
            }
        }
        if p.exists() {
            remove(&p)?;
        }
        platform::remove_recent(&app, path);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove(path: &Path) -> CmdResult<()> {
    trash::delete(path).map_err(|e| format!("Could not move {} to the Trash: {e}", platform::file_name(path)))
}

#[cfg(not(target_os = "macos"))]
fn remove(path: &Path) -> CmdResult<()> {
    std::fs::remove_file(path).map_err(|e| format!("Could not delete {}: {e}", platform::file_name(path)))
}

/// Forget a recent project that lives outside the library (desktop).
#[tauri::command]
pub fn library_forget(app: AppHandle, path: String) {
    platform::remove_recent(&app, &path);
}

/// Pick `.printfold` files and copy them into the library.
#[tauri::command]
pub async fn library_import(app: AppHandle) -> CmdResult<Vec<ProjectLocation>> {
    let picked: Vec<PathBuf> = if let Some(paths) = platform::e2e_picks() {
        paths
    } else {
        app.dialog()
            .file()
            .set_file_access_mode(FileAccessMode::Copy)
            .add_filter("PrintFold Project", &[PROJECT_EXT])
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(into_path)
            .collect::<CmdResult<_>>()?
    };
    let dir = library_dir(&app)?;
    let mut out = Vec::new();
    for from in picked.iter().filter(|p| is_project(p)) {
        let stem = from.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let to = platform::unique_path(&dir, &stem, PROJECT_EXT);
        std::fs::copy(from, &to).map_err(|e| format!("Could not import {}: {e}", platform::file_name(from)))?;
        out.push(location(&to));
    }
    Ok(out)
}

/// Add a project dropped onto the browser. Raw body = file bytes; header
/// `x-file-name`.
#[tauri::command]
pub fn library_import_bytes(app: AppHandle, request: tauri::ipc::Request<'_>) -> CmdResult<ProjectLocation> {
    let bytes = raw_body(&request)?;
    let name = header(&request, "x-file-name").unwrap_or_else(|| "Untitled".into());
    printfold_core::project_file::import_project(&bytes)
        .map_err(|e| format!("\"{name}\" is not a PrintFold project ({e})"))?;
    let stem = clean_stem(&name).unwrap_or_else(|_| "Untitled".into());
    let to = platform::unique_path(&library_dir(&app)?, &stem, PROJECT_EXT);
    platform::atomic_write(&to, &bytes).map_err(err)?;
    Ok(location(&to))
}

/// Present the system share sheet (iPadOS) / sharing picker (macOS) for a
/// file. `x`, `y` anchor the popover in webview coordinates.
#[tauri::command]
pub async fn library_share(app: AppHandle, window: WebviewWindow, path: String, x: f64, y: f64) -> CmdResult<()> {
    share_path(&app, window, Path::new(&path), x, y).await
}

pub(crate) async fn share_path(app: &AppHandle, window: WebviewWindow, path: &Path, x: f64, y: f64) -> CmdResult<()> {
    use tauri_plugin_sharekit::{Error as ShareError, ShareExt, ShareFileOptions, SharePosition};
    // Share a fresh copy in the temp folder: the iPadOS share sheet copies
    // the file there itself and would otherwise reuse a stale earlier copy.
    let name = platform::file_name(path);
    let tmp = std::env::temp_dir().join(&name);
    if tmp != path {
        let _ = std::fs::remove_file(&tmp);
        std::fs::copy(path, &tmp).map_err(err)?;
    }
    // iPadOS parses a URL; macOS expects a plain path.
    let target = if cfg!(target_os = "ios") {
        tauri::Url::from_file_path(&tmp).map(|u| u.to_string()).map_err(|_| "Invalid file path".to_string())?
    } else {
        tmp.to_string_lossy().into_owned()
    };
    let options = ShareFileOptions {
        mime_type: None,
        title: Some(name),
        position: Some(SharePosition { x, y, preferred_edge: None }),
    };
    match app.share().share_file(window, target, options).await {
        Ok(()) | Err(ShareError::ShareCancelled) => Ok(()),
        Err(e) if e.to_string().to_lowercase().contains("cancel") => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Show a project (or the library folder when `path` is empty) in Finder.
#[tauri::command]
pub fn library_reveal(app: AppHandle, path: Option<String>) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    match path.filter(|p| !p.is_empty()) {
        Some(p) => app.opener().reveal_item_in_dir(p).map_err(err),
        None => app.opener().open_path(library_dir(&app)?.to_string_lossy(), None::<&str>).map_err(err),
    }
}

/// The open project's cover thumbnail (PNG body), embedded on the next save.
#[tauri::command]
pub fn project_set_thumbnail(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<()> {
    let png = raw_body(&request)?;
    *state.thumbnail.lock().unwrap() = (!png.is_empty()).then_some(png);
    Ok(())
}
