//! Text reflow.

use printfold_core::flow::{reflow as run_reflow, FlowRequest, FlowResult, Measurer};
use printfold_core::model::ProjectSnapshot;
use tauri::State;

use super::CmdResult;
use crate::state::AppState;

/// Lay the concatenated markdown out across pages, preserving static pages
/// from `project.signatures`.
#[tauri::command]
pub async fn reflow(state: State<'_, AppState>, markdown: String, project: ProjectSnapshot) -> CmdResult<FlowResult> {
    let image_sizes = state.files.lock().unwrap().image_sizes();
    let request = FlowRequest { markdown, project, image_sizes };
    Ok(state.engine.with(|e| {
        let mut m = Measurer::new(&mut e.fonts, &mut e.cache);
        run_reflow(&mut m, &request)
    }))
}

/// Clear cached text widths (fonts changed).
#[tauri::command]
pub fn clear_measurement_cache(state: State<'_, AppState>) {
    state.engine.with_now(|e| e.cache.clear());
}
