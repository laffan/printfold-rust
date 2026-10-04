//! PDF generation.

use printfold_core::model::ProjectSnapshot;
use printfold_core::pdf::{generate_pdf, generate_test_page, pages_needing_prerender, FileMeta, PdfContext};
use tauri::ipc::Response;
use tauri::State;

use super::{err, header, raw_body, CmdResult};
use crate::state::AppState;

/// Pages the editor must pre-render (PNG) before `pdf_generate`.
#[tauri::command]
pub fn pdf_pages_to_prerender(project: ProjectSnapshot) -> Vec<u32> {
    pages_needing_prerender(&project)
}

/// Store one pre-rendered page (raw PNG body, `x-page` header).
#[tauri::command]
pub fn pdf_put_prerendered(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<()> {
    let page: u32 = header(&request, "x-page").and_then(|p| p.parse().ok()).ok_or("missing x-page header")?;
    let png = raw_body(&request)?;
    state.prerendered.lock().unwrap().insert(page, png);
    Ok(())
}

#[tauri::command]
pub fn pdf_clear_prerendered(state: State<'_, AppState>) {
    state.prerendered.lock().unwrap().clear();
}

fn run(state: &AppState, project: &ProjectSnapshot, files: &[FileMeta], test_page: bool) -> CmdResult<Vec<u8>> {
    let pre = std::mem::take(&mut *state.prerendered.lock().unwrap());
    let store = &state.files;
    let lookup = |id: &str| store.lock().unwrap().bytes(id).map(|b| (*b).clone());
    let ctx = PdfContext { project, files, pre_rendered: &pre, file_bytes: &lookup };
    state.engine.with(|e| if test_page { generate_test_page(&mut e.fonts, &ctx) } else { generate_pdf(&mut e.fonts, &ctx) }).map_err(err)
}

/// Generate the print PDF (consumes the stored pre-rendered pages).
#[tauri::command]
pub async fn pdf_generate(state: State<'_, AppState>, project: ProjectSnapshot, files: Vec<FileMeta>) -> CmdResult<Response> {
    run(&state, &project, &files, false).map(Response::new)
}

/// Generate the duplex calibration PDF.
#[tauri::command]
pub async fn pdf_test_page(state: State<'_, AppState>, project: ProjectSnapshot) -> CmdResult<Response> {
    run(&state, &project, &[], true).map(Response::new)
}
