//! Tauri commands exposed to the webview (see `src/services/bridge.ts`).

pub mod files;
pub mod fonts;
pub mod layout;
pub mod pdf;
pub mod project;
pub mod system;

/// Commands return `Result<T, String>` so errors reach the UI as messages.
pub type CmdResult<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Read a raw (binary) invoke body.
pub(crate) fn raw_body(request: &tauri::ipc::Request<'_>) -> CmdResult<Vec<u8>> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        tauri::ipc::InvokeBody::Json(_) => Err("expected a binary payload".into()),
    }
}

/// Read a header from a raw invoke (percent-decoded UTF-8).
pub(crate) fn header(request: &tauri::ipc::Request<'_>, name: &str) -> Option<String> {
    let value = request.headers().get(name)?.to_str().ok()?;
    Some(percent_decode(value))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_percent_headers() {
        assert_eq!(super::percent_decode("My%20Book%C3%A9.pdf"), "My Booké.pdf");
    }
}
