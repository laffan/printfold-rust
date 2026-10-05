//! Reading the system clipboard for "New file from clipboard".
//!
//! Desktop uses arboard (no permission prompt). On iPadOS the UI reads the
//! clipboard through WebKit's async clipboard API instead, which shows the
//! system paste prompt; this command then reports `unsupported`.

use serde::Serialize;

use super::files::PickedFile;
use super::CmdResult;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClipboardContent {
    /// Files copied in Finder / Explorer.
    Files { files: Vec<PickedFile> },
    /// Image data, encoded as PNG (base64).
    Image { png: String },
    Text { text: String },
    Empty,
    /// iPadOS: read through WebKit instead.
    #[cfg_attr(desktop, allow(dead_code))]
    Unsupported,
}

#[tauri::command]
pub fn clipboard_read() -> CmdResult<ClipboardContent> {
    read()
}

#[cfg(desktop)]
fn read() -> CmdResult<ClipboardContent> {
    use base64::Engine as _;

    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;

    // Copied files come first: Finder also puts the files' icons on the
    // pasteboard as an image, which must not win.
    if let Ok(paths) = clipboard.get().file_list() {
        let files: Vec<PickedFile> = paths.iter().filter(|p| p.is_file()).filter_map(|p| super::files::read_picked(p).ok()).collect();
        if !files.is_empty() {
            return Ok(ClipboardContent::Files { files });
        }
    }

    let text = clipboard.get_text().ok().filter(|t| !t.trim().is_empty());
    // Text wins over an image, because apps such as Pages and Word also put a
    // rendered image of copied text on the pasteboard — unless the "text" is
    // just the address of a copied image.
    let text_is_link = text.as_deref().map(|t| {
        let t = t.trim();
        !t.contains(char::is_whitespace) && (t.starts_with("http://") || t.starts_with("https://") || t.starts_with("file://"))
    });
    if let (Some(text), Some(false)) = (&text, text_is_link) {
        return Ok(ClipboardContent::Text { text: text.clone() });
    }
    if let Ok(image) = clipboard.get_image() {
        let png = encode_png(image.width as u32, image.height as u32, &image.bytes)?;
        return Ok(ClipboardContent::Image { png: base64::engine::general_purpose::STANDARD.encode(png) });
    }
    Ok(match text {
        Some(text) => ClipboardContent::Text { text },
        None => ClipboardContent::Empty,
    })
}

#[cfg(desktop)]
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> CmdResult<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(mobile)]
fn read() -> CmdResult<ClipboardContent> {
    Ok(ClipboardContent::Unsupported)
}

#[cfg(all(test, desktop))]
mod tests {
    #[test]
    fn encodes_rgba_png() {
        let png = super::encode_png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }
}
