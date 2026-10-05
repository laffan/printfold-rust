//! Filesystem helpers: atomic writes, app directories, recents storage.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

pub const PROJECT_EXT: &str = "printfold";

/// End-to-end test hook: when `PRINTFOLD_E2E_DIR` is set, dialogs are
/// bypassed — new projects and saved files go to that folder, and
/// `PRINTFOLD_E2E_PICK` (paths separated by `|`) answers file pickers.
/// Unset in normal use.
pub fn e2e_dir() -> Option<PathBuf> {
    std::env::var_os("PRINTFOLD_E2E_DIR").map(PathBuf::from)
}

pub fn e2e_picks() -> Option<Vec<PathBuf>> {
    e2e_dir()?;
    let raw = std::env::var("PRINTFOLD_E2E_PICK").ok()?;
    Some(raw.split('|').filter(|s| !s.is_empty()).map(PathBuf::from).collect())
}
const MAX_RECENTS: usize = 10;

/// Write via a temporary sibling and rename, so a crash mid-save can never
/// leave a truncated project file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

pub fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    let has = path.extension().map(|e| e.eq_ignore_ascii_case(ext)).unwrap_or(false);
    if has {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".");
        s.push(ext);
        PathBuf::from(s)
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The folder PrintFold keeps projects in on iPadOS (the app's Documents
/// folder, visible in the Files app as "On My iPad › PrintFold").
pub fn documents_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app.path().document_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// The project library shown in the project browser: the app's Documents
/// folder on iPadOS (visible in the Files app), `~/Documents/PrintFold`
/// elsewhere. The E2E hook folder replaces it in tests.
pub fn library_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    if let Some(dir) = e2e_dir() {
        return Some(dir);
    }
    if cfg!(mobile) {
        return documents_dir(app);
    }
    let dir = app.path().document_dir().ok()?.join("PrintFold");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// A path in `dir` named `stem.ext`, adding " 2", " 3"… if it exists.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let clean: String = stem.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c }).collect();
    let clean = if clean.trim().is_empty() { "Untitled".to_string() } else { clean.trim().to_string() };
    let mut candidate = dir.join(format!("{clean}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{clean} {n}.{ext}"));
        n += 1;
    }
    candidate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub last_opened: f64,
}

fn recents_file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("recents.json"))
}

pub fn read_recents<R: Runtime>(app: &AppHandle<R>) -> Vec<RecentEntry> {
    recents_file(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_recents<R: Runtime>(app: &AppHandle<R>, entries: &[RecentEntry]) {
    if let Some(p) = recents_file(app) {
        if let Ok(json) = serde_json::to_string_pretty(entries) {
            let _ = atomic_write(&p, json.as_bytes());
        }
    }
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

pub fn add_recent<R: Runtime>(app: &AppHandle<R>, path: &Path, name: &str) {
    let mut entries = read_recents(app);
    let key = path.to_string_lossy().to_string();
    match entries.iter_mut().find(|e| e.path == key) {
        Some(e) => {
            e.name = name.to_string();
            e.last_opened = now_ms();
        }
        None => entries.push(RecentEntry {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            path: key,
            last_opened: now_ms(),
        }),
    }
    entries.sort_by(|a, b| b.last_opened.partial_cmp(&a.last_opened).unwrap_or(std::cmp::Ordering::Equal));
    entries.truncate(MAX_RECENTS);
    write_recents(app, &entries);
}

/// Keep a recent entry (and its position) when its file is renamed.
pub fn rename_recent<R: Runtime>(app: &AppHandle<R>, from: &Path, to: &Path) {
    let from = from.to_string_lossy();
    let mut entries = read_recents(app);
    if let Some(e) = entries.iter_mut().find(|e| e.path == from) {
        e.path = to.to_string_lossy().into_owned();
        e.name = file_name(to);
        write_recents(app, &entries);
    }
}

pub fn remove_recent<R: Runtime>(app: &AppHandle<R>, path: &str) {
    let entries: Vec<RecentEntry> = read_recents(app).into_iter().filter(|e| e.path != path).collect();
    write_recents(app, &entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_and_unique_names() {
        let dir = std::env::temp_dir().join(format!("printfold-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.printfold");
        atomic_write(&p, b"one").unwrap();
        atomic_write(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert!(!dir.join("a.printfold.tmp").exists());
        assert_eq!(unique_path(&dir, "a", "printfold"), dir.join("a 2.printfold"));
        assert_eq!(ensure_extension(dir.join("b"), "printfold"), dir.join("b.printfold"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
