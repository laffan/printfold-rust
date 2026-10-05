//! Project lifecycle: open, auto-save, close (creation lives in `library`).

use std::path::{Path, PathBuf};

use printfold_core::project_file::{export_project_with_thumbnail, import_project, read_thumbnail, ProjectExport, ProjectImport};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FileAccessMode};

use super::files::into_path;
use super::{err, CmdResult};
use crate::platform::{self, PROJECT_EXT};
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLocation {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub name: String,
    pub path: String,
    /// `None` for an empty (freshly created) file.
    pub data: Option<ProjectImport>,
}

pub(crate) fn bind(state: &AppState, path: &Path) {
    *state.project_path.lock().unwrap() = Some(path.to_path_buf());
}

fn read_project(state: &AppState, path: &Path) -> CmdResult<OpenedProject> {
    let bytes = std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let data = if bytes.is_empty() { None } else { Some(import_project(&bytes).map_err(err)?) };
    *state.thumbnail.lock().unwrap() = read_thumbnail(std::io::Cursor::new(&bytes));
    let mut files = state.files.lock().unwrap();
    files.clear();
    if let Some(d) = &data {
        use base64::Engine as _;
        for f in d.files.iter().filter(|f| f.is_base64) {
            if let Ok(b) = base64::engine::general_purpose::STANDARD.decode(&f.content) {
                files.put(f.id.clone(), f.name.clone(), f.file_type.clone(), b);
            }
        }
    }
    drop(files);
    bind(state, path);
    Ok(OpenedProject { name: platform::file_name(path), path: path.to_string_lossy().into_owned(), data })
}

/// On iPadOS, projects opened from elsewhere are copied into the library
/// (Documents) so auto-save always targets a file inside the sandbox.
fn localize(app: &AppHandle, path: PathBuf) -> CmdResult<PathBuf> {
    if !cfg!(target_os = "ios") {
        return Ok(path);
    }
    let dir = platform::library_dir(app).ok_or("Documents folder unavailable")?;
    if path.parent().map(|p| p == dir).unwrap_or(false) {
        return Ok(path);
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
    let target = platform::unique_path(&dir, &stem, PROJECT_EXT);
    // Files shared to PrintFold ("Open in…", AirDrop) arrive as copies in
    // Documents/Inbox: move them into the library instead of duplicating.
    if path.parent().map(|p| p == dir.join("Inbox")).unwrap_or(false) && std::fs::rename(&path, &target).is_ok() {
        return Ok(target);
    }
    std::fs::copy(&path, &target).map_err(|e| {
        format!(
            "PrintFold can't read \"{}\" where it is ({e}). Use Import in the project browser to add a copy.",
            platform::file_name(&path)
        )
    })?;
    Ok(target)
}

#[tauri::command]
pub async fn project_open_dialog(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<OpenedProject>> {
    let picked = app
        .dialog()
        .file()
        .set_file_access_mode(FileAccessMode::Copy)
        .add_filter("PrintFold Project", &[PROJECT_EXT])
        .blocking_pick_file();
    let Some(fp) = picked else { return Ok(None) };
    let path = localize(&app, into_path(fp)?)?;
    let opened = read_project(&state, &path)?;
    platform::add_recent(&app, &path, &opened.name);
    Ok(Some(opened))
}

#[tauri::command]
pub async fn project_open_path(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<OpenedProject> {
    let path = localize(&app, PathBuf::from(path))?;
    let opened = read_project(&state, &path)?;
    platform::add_recent(&app, &path, &opened.name);
    Ok(opened)
}

/// Serialise the project and atomically write it to the bound file.
#[tauri::command]
pub async fn project_save(state: State<'_, AppState>, project: ProjectExport) -> CmdResult<()> {
    let path = state.project_path.lock().unwrap().clone().ok_or("No project file is open")?;
    let bytes = {
        let files = state.files.lock().unwrap();
        let thumbnail = state.thumbnail.lock().unwrap();
        export_project_with_thumbnail(&project, &|id| files.bytes(id).map(|b| (*b).clone()), thumbnail.as_deref())
            .map_err(err)?
    };
    platform::atomic_write(&path, &bytes).map_err(|e| format!("Could not save {}: {e}", path.display()))
}

#[tauri::command]
pub fn project_close(state: State<'_, AppState>) {
    *state.project_path.lock().unwrap() = None;
    *state.thumbnail.lock().unwrap() = None;
}
