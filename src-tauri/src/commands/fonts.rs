//! Font listing and custom-font registration.

use base64::Engine as _;
use printfold_core::fonts::FamilyVariants;
use tauri::State;

use super::CmdResult;
use crate::state::AppState;

/// Installed font families (for the Styles font pickers).
#[tauri::command]
pub async fn fonts_list(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    Ok(state.engine.with(|e| e.fonts.system_families()))
}

/// Which real faces (regular/bold/italic/bold italic) a family has.
#[tauri::command]
pub async fn fonts_variants(state: State<'_, AppState>, family: String) -> CmdResult<FamilyVariants> {
    Ok(state.engine.with(|e| e.fonts.family_variants(&family)))
}

/// Register an uploaded font (base64 TTF/OTF/WOFF) under `family`.
#[tauri::command]
pub fn fonts_register(state: State<'_, AppState>, family: String, content: String) -> CmdResult<bool> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(content.trim()).map_err(super::err)?;
    Ok(state.engine.with_now(|e| e.fonts.register_custom_font(&family, bytes)))
}

#[tauri::command]
pub fn fonts_unregister(state: State<'_, AppState>, family: String) {
    state.engine.with_now(|e| e.fonts.unregister_custom_font(&family));
}
