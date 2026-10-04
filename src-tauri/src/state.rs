//! Shared application state held by Tauri.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

use printfold_core::fonts::{FontRegistry, WidthCache};
use printfold_core::pdf::canvas::image_pixel_size;

/// Binary project files (images, fonts) mirrored from the webview so the
/// engine can embed them without re-sending megabytes of base64 on every
/// save, reflow or export.
#[derive(Default)]
pub struct FileStore {
    entries: HashMap<String, StoredFile>,
}

pub struct StoredFile {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    /// Pixel size for images (computed once on insert).
    pub image_size: Option<(u32, u32)>,
}

impl FileStore {
    pub fn put(&mut self, id: String, name: String, file_type: String, bytes: Vec<u8>) {
        let image_size = if file_type == "image" { image_pixel_size(&bytes) } else { None };
        self.entries.insert(id, StoredFile { name, bytes: Arc::new(bytes), image_size });
    }

    pub fn rename(&mut self, id: &str, name: String) {
        if let Some(e) = self.entries.get_mut(id) {
            e.name = name;
        }
    }

    pub fn retain(&mut self, ids: &[String]) {
        self.entries.retain(|id, _| ids.contains(id));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn bytes(&self, id: &str) -> Option<Arc<Vec<u8>>> {
        self.entries.get(id).map(|e| e.bytes.clone())
    }

    /// Image pixel sizes keyed by file name (for markdown image layout).
    pub fn image_sizes(&self) -> HashMap<String, (f64, f64)> {
        self.entries
            .values()
            .filter_map(|e| e.image_size.map(|(w, h)| (e.name.clone(), (w as f64, h as f64))))
            .collect()
    }
}

/// Font registry + measurement cache. System fonts load on a background
/// thread at startup; `wait_ready` blocks callers until that finishes.
pub struct Engine {
    pub fonts: FontRegistry,
    pub cache: WidthCache,
}

pub struct EngineHandle {
    engine: Mutex<Engine>,
    ready: Mutex<bool>,
    ready_cv: Condvar,
}

impl EngineHandle {
    pub fn new() -> Self {
        Self {
            engine: Mutex::new(Engine { fonts: FontRegistry::new(), cache: WidthCache::default() }),
            ready: Mutex::new(false),
            ready_cv: Condvar::new(),
        }
    }

    /// Scan system fonts (called once from a background thread).
    pub fn load_system_fonts(&self) {
        let mut registry = FontRegistry::new();
        registry.load_system_fonts();
        {
            let mut engine = self.engine.lock().unwrap();
            // Carry over custom fonts registered while scanning.
            let custom = std::mem::replace(&mut engine.fonts, registry);
            engine.fonts.adopt_custom_fonts(custom);
            engine.cache.clear();
        }
        *self.ready.lock().unwrap() = true;
        self.ready_cv.notify_all();
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut Engine) -> T) -> T {
        let mut ready = self.ready.lock().unwrap();
        while !*ready {
            ready = self.ready_cv.wait(ready).unwrap();
        }
        drop(ready);
        let mut engine = self.engine.lock().unwrap();
        f(&mut engine)
    }

    /// Access without waiting for system fonts (custom-font registration).
    pub fn with_now<T>(&self, f: impl FnOnce(&mut Engine) -> T) -> T {
        let mut engine = self.engine.lock().unwrap();
        f(&mut engine)
    }
}

impl Default for EngineHandle {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AppState {
    pub engine: EngineHandle,
    pub files: Mutex<FileStore>,
    /// Absolute path of the open project (auto-save target).
    pub project_path: Mutex<Option<PathBuf>>,
    /// Files the OS asked us to open before the webview was ready.
    pub pending_opens: Mutex<Vec<PathBuf>>,
    pub webview_ready: Mutex<bool>,
    /// Pre-rendered page PNGs for the next PDF generation.
    pub prerendered: Mutex<HashMap<u32, Vec<u8>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            engine: EngineHandle::new(),
            files: Mutex::new(FileStore::default()),
            project_path: Mutex::new(None),
            pending_opens: Mutex::new(Vec::new()),
            webview_ready: Mutex::new(false),
            prerendered: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
