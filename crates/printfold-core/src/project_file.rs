//! `.printfold` project files: a ZIP archive containing
//!
//! ```text
//! project.json        manifest (settings, file list, concatenation order)
//! text/*.md           markdown sources
//! images/*            images
//! fonts/*             uploaded fonts
//! static/*.json       per-page state, items and backgrounds
//! ```
//!
//! The layout is byte-compatible with files written by the TypeScript app
//! (manifest version 2.1.0), and legacy `printfold.json` manifests,
//! root-level files and item-only static page records are still read.

use std::io::{Cursor, Read, Write};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use zip::write::SimpleFileOptions;

use crate::model::{
    all_pages, new_id, FillConfig, FontOptions, HeaderFooterOptions, LayoutOptions, OutputOptions, PageItem,
    PageState, ProjectFile, Signature,
};

pub const MANIFEST_FILENAME: &str = "project.json";
pub const LEGACY_MANIFEST_FILENAME: &str = "printfold.json";
pub const FORMAT_VERSION: &str = "2.1.0";

#[derive(Debug, thiserror::Error)]
pub enum ProjectFileError {
    #[error("not a valid project archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid project data: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub path: String,
    #[serde(default)]
    pub last_modified: f64,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectManifest {
    pub version: String,
    pub name: String,
    pub project_id: String,
    pub main_document: Option<String>,
    pub measurement_unit: Option<String>,
    pub output_options: OutputOptions,
    pub layout_options: LayoutOptions,
    pub font_options: FontOptions,
    pub header_footer: HeaderFooterOptions,
    pub blank_pages: Vec<u32>,
    pub files: Vec<ManifestFile>,
    pub file_order: Vec<String>,
}

/// Saved state of one page (`static/*.json`).
#[skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticPageData {
    pub page_number: u32,
    /// Legacy records (items only) default to `static`.
    #[serde(default = "static_state")]
    pub page_state: PageState,
    #[serde(default)]
    pub items: Vec<PageItem>,
    pub background_fill: Option<FillConfig>,
    pub custom_background_image_id: Option<String>,
}

fn static_state() -> PageState {
    PageState::Static
}

/// What the app hands over to be saved.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectExport {
    pub id: String,
    pub name: String,
    pub main_document: Option<String>,
    pub measurement_unit: Option<String>,
    pub output_options: OutputOptions,
    pub layout_options: LayoutOptions,
    pub font_options: FontOptions,
    pub header_footer: HeaderFooterOptions,
    pub blank_pages: Vec<u32>,
    pub signatures: Vec<Signature>,
    /// File metadata. Markdown content is inline; binary content may be
    /// inline base64 or supplied by the `binary` lookup in [`export_project`].
    pub files: Vec<ProjectFile>,
}

/// The decoded contents of a project archive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectImport {
    pub manifest: Option<ProjectManifest>,
    /// Files in concatenation order; binary content is base64.
    pub files: Vec<ProjectFile>,
    pub static_pages: Vec<StaticPageData>,
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()
}

/// File type from an extension (`md`, images, fonts).
pub fn file_type_for_extension(ext: &str) -> &'static str {
    match ext {
        "md" => "markdown",
        "png" | "jpg" | "jpeg" | "webp" | "gif" => "image",
        "ttf" | "otf" | "woff" => "font",
        _ => "unknown",
    }
}

fn page_record_name(page_number: u32) -> String {
    // Stable per page (the original mixed in a timestamp; any name works
    // because the page number is stored inside the record).
    format!("page{page_number}-{:x}", page_number.wrapping_mul(2_654_435_761))
}

/// Pages worth saving: any non-text state, or items / backgrounds.
pub fn collect_static_pages(signatures: &[Signature]) -> Vec<StaticPageData> {
    let mut out: Vec<StaticPageData> = Vec::new();
    for page in all_pages(signatures) {
        let worth = page.page_state != PageState::Text
            || page.has_items()
            || page.background_fill.is_some()
            || page.custom_background_image_id.is_some();
        if !worth {
            continue;
        }
        let record = StaticPageData {
            page_number: page.page_number,
            page_state: page.page_state,
            items: page.items().to_vec(),
            background_fill: page.background_fill.clone(),
            custom_background_image_id: page.custom_background_image_id.clone(),
        };
        match out.iter_mut().find(|r| r.page_number == page.page_number) {
            Some(existing) => *existing = record,
            None => out.push(record),
        }
    }
    out
}

/// Serialise a project to `.printfold` bytes.
pub fn export_project(
    project: &ProjectExport,
    binary: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Vec<u8>, ProjectFileError> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let store = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for dir in ["text/", "images/", "static/", "fonts/"] {
        zip.add_directory(dir, store)?;
    }

    let mut manifest_files = Vec::new();
    let mut file_order = Vec::new();
    for file in &project.files {
        let base = base_name(&file.name);
        let (folder, bytes, options) = match file.file_type.as_str() {
            "markdown" => ("text", file.content.as_bytes().to_vec(), deflate),
            kind @ ("image" | "font") => {
                let bytes = binary(&file.id)
                    .or_else(|| base64::engine::general_purpose::STANDARD.decode(&file.content).ok())
                    .unwrap_or_default();
                (if kind == "image" { "images" } else { "fonts" }, bytes, store)
            }
            _ => continue,
        };
        let path = format!("{folder}/{base}");
        zip.start_file(&path, options)?;
        zip.write_all(&bytes)?;
        if file.file_type == "markdown" {
            file_order.push(file.id.clone());
        }
        manifest_files.push(ManifestFile {
            id: file.id.clone(),
            name: file.name.clone(),
            file_type: file.file_type.clone(),
            path,
            last_modified: file.last_modified,
        });
    }

    for record in collect_static_pages(&project.signatures) {
        zip.start_file(format!("static/{}.json", page_record_name(record.page_number)), deflate)?;
        zip.write_all(serde_json::to_string_pretty(&record)?.as_bytes())?;
    }

    let manifest = ProjectManifest {
        version: FORMAT_VERSION.into(),
        name: project.name.clone(),
        project_id: project.id.clone(),
        main_document: project.main_document.clone(),
        measurement_unit: project.measurement_unit.clone(),
        output_options: project.output_options.clone(),
        layout_options: project.layout_options.clone(),
        font_options: project.font_options.clone(),
        header_footer: project.header_footer.clone(),
        blank_pages: project.blank_pages.clone(),
        files: manifest_files,
        file_order,
    };
    zip.start_file(MANIFEST_FILENAME, deflate)?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    Ok(zip.finish()?.into_inner())
}

/// Decode `.printfold` bytes.
pub fn import_project(bytes: &[u8]) -> Result<ProjectImport, ProjectFileError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;

    let mut manifest: Option<ProjectManifest> = None;
    for name in [MANIFEST_FILENAME, LEGACY_MANIFEST_FILENAME] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            manifest = Some(serde_json::from_str(&text)?);
            break;
        }
    }

    let mut files = Vec::new();
    let mut static_pages: Vec<StaticPageData> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let path = entry.name().to_string();
        if entry.is_dir() || path == MANIFEST_FILENAME || path == LEGACY_MANIFEST_FILENAME {
            continue;
        }
        let parts: Vec<&str> = path.split('/').collect();
        let folder = if parts.len() > 1 { Some(parts[0]) } else { None };
        let file_name = *parts.last().unwrap_or(&"");
        let ext = extension(file_name);

        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;

        if folder == Some("static") && ext == "json" {
            match serde_json::from_slice::<StaticPageData>(&data) {
                Ok(record) => match static_pages.iter_mut().find(|r| r.page_number == record.page_number) {
                    Some(existing) => *existing = record,
                    None => static_pages.push(record),
                },
                Err(e) => log_skip(&path, &e.to_string()),
            }
            continue;
        }

        let kind = file_type_for_extension(&ext);
        if kind == "unknown" {
            continue;
        }
        let entry_meta = manifest.as_ref().and_then(|m| {
            m.files.iter().find(|f| f.path == path || f.name == path || f.name == file_name)
        });
        let is_text = kind == "markdown";
        let content = if is_text {
            String::from_utf8_lossy(&data).into_owned()
        } else {
            base64::engine::general_purpose::STANDARD.encode(&data)
        };
        files.push(ProjectFile {
            id: entry_meta.map(|m| m.id.clone()).unwrap_or_else(new_id),
            name: entry_meta.map(|m| m.name.clone()).unwrap_or_else(|| file_name.to_string()),
            file_type: kind.to_string(),
            content,
            is_base64: !is_text,
            last_modified: entry_meta.map(|m| m.last_modified).filter(|v| *v > 0.0).unwrap_or_else(now_ms),
        });
    }

    if let Some(m) = &manifest {
        if !m.file_order.is_empty() {
            let index = |id: &str| m.file_order.iter().position(|o| o == id);
            // Stable sort: ordered files first (by order), the rest after.
            files.sort_by_key(|f| index(&f.id).unwrap_or(usize::MAX));
        }
    }
    static_pages.sort_by_key(|r| r.page_number);

    Ok(ProjectImport { manifest, files, static_pages })
}

fn log_skip(path: &str, err: &str) {
    eprintln!("printfold: skipping unreadable page record {path}: {err}");
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PageContent, PageItemType, Spread};

    fn sample() -> ProjectExport {
        let mut static_page = PageContent { page_number: 2, page_state: PageState::Static, ..Default::default() };
        static_page.items = Some(vec![PageItem { id: "i1".into(), item_type: PageItemType::Shape, width: 5.0, ..Default::default() }]);
        let text_page = PageContent { page_number: 1, page_state: PageState::Text, ..Default::default() };
        ProjectExport {
            id: "proj".into(),
            name: "Demo".into(),
            main_document: Some("f1".into()),
            measurement_unit: Some("cm".into()),
            signatures: vec![Signature {
                id: "s".into(),
                signature_number: 1,
                page_count: 4,
                spreads: vec![
                    Spread { id: "a".into(), spread_number: 1, verso: None, recto: Some(text_page), ..Default::default() },
                    Spread { id: "b".into(), spread_number: 2, verso: Some(static_page), recto: None, ..Default::default() },
                ],
                ..Default::default()
            }],
            files: vec![
                ProjectFile { id: "f2".into(), name: "b.md".into(), file_type: "markdown".into(), content: "# B".into(), ..Default::default() },
                ProjectFile { id: "f1".into(), name: "a.md".into(), file_type: "markdown".into(), content: "# A".into(), ..Default::default() },
                ProjectFile { id: "img".into(), name: "pic.png".into(), file_type: "image".into(), content: String::new(), is_base64: true, ..Default::default() },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn round_trip() {
        let bytes = export_project(&sample(), &|id| (id == "img").then(|| vec![1, 2, 3])).unwrap();
        let imported = import_project(&bytes).unwrap();
        let m = imported.manifest.unwrap();
        assert_eq!(m.version, FORMAT_VERSION);
        assert_eq!(m.name, "Demo");
        assert_eq!(m.measurement_unit.as_deref(), Some("cm"));
        assert_eq!(m.file_order, vec!["f2", "f1"]);
        let names: Vec<&str> = imported.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names[..2], ["b.md", "a.md"]);
        let img = imported.files.iter().find(|f| f.id == "img").unwrap();
        assert_eq!(img.content, "AQID");
        assert_eq!(imported.static_pages.len(), 1);
        assert_eq!(imported.static_pages[0].page_number, 2);
        assert_eq!(imported.static_pages[0].items[0].id, "i1");
    }

    #[test]
    fn reads_legacy_records() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        zip.start_file("printfold.json", opts).unwrap();
        zip.write_all(br#"{"version":"1.0","name":"Old","projectId":"x","mainDocument":null,"outputOptions":{"sheetSize":"a4","bookletSize":"half","pagesPerSignature":8,"orientation":"portrait","fillAvailableSpace":false,"showFoldMarks":true},"layoutOptions":{"margins":{"top":1,"bottom":2,"inner":3,"outer":4}},"fontOptions":{},"headerFooter":{},"blankPages":[],"files":[],"fileOrder":[]}"#).unwrap();
        zip.start_file("chapter.md", opts).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.start_file("static/p.json", opts).unwrap();
        zip.write_all(br#"{"pageNumber":3,"items":[]}"#).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let imported = import_project(&bytes).unwrap();
        let m = imported.manifest.unwrap();
        assert_eq!(m.output_options.sheet_size, "a4");
        assert_eq!(m.layout_options.margins.outer, 4.0);
        assert_eq!(m.layout_options.line_height, 1.5, "missing fields get defaults");
        assert_eq!(m.font_options.footnote.font_size, 9.0);
        assert_eq!(imported.files[0].content, "hello");
        assert_eq!(imported.static_pages[0].page_state, PageState::Static);
    }
}
