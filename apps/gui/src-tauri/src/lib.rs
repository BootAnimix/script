//! Tauri 2 shell for BootAnimix Studio.
//!
//! This crate owns the desktop window and the IPC surface. All real work is
//! delegated to `bootanimix-core` (through the `bootanimix-gui` session
//! layer), so the same code paths the CLI and TUI exercise are the ones the
//! studio drives.
//!
//! The Svelte frontend calls these commands by name. The names are part of the
//! public contract with `apps/gui/src/lib/bridge.ts`:
//!
//! | command                | arguments                      | returns              |
//! |------------------------|--------------------------------|----------------------|
//! | `gui_get_snapshot`     | `selectedPartOnly`             | `WorkspaceSnapshot`  |
//! | `gui_replace_manifest` | `manifest`, `selectedPartOnly` | `WorkspaceSnapshot`  |
//! | `gui_undo`             | `selectedPartOnly`             | `WorkspaceSnapshot`  |
//! | `gui_redo`             | `selectedPartOnly`             | `WorkspaceSnapshot`  |
//! | `gui_save_project`     | `projectDir`                   | `WorkspaceSnapshot`  |
//! | `gui_open_project`     | `projectDir`                   | `WorkspaceSnapshot`  |
//! | `gui_build_archive`    | `outputPath`                   | `BuildReport`        |
//! | `gui_export_magisk`    | `outputPath`                   | `MagiskExportReport` |

use std::path::PathBuf;

use bootanimix_core::{BuildReport, MagiskExportReport, ProjectManifest};
use bootanimix_gui_lib::{GuiBackendState, ManagedGuiState, WorkspaceSnapshot};
use tauri::State;

/// Shared studio session state managed by Tauri.
type StudioState<'a> = State<'a, ManagedGuiState>;

fn lock<'a>(
    state: &'a StudioState<'_>,
) -> Result<std::sync::MutexGuard<'a, GuiBackendState>, String> {
    state
        .0
        .lock()
        .map_err(|e| format!("studio state lock poisoned: {e}"))
}

/// Current workspace snapshot (manifest, validation, playback schedule).
#[tauri::command]
fn gui_get_snapshot(
    state: StudioState<'_>,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    lock(&state)?.snapshot(selected_part_only)
}

/// Replace the in-memory manifest and return the resulting snapshot.
#[tauri::command]
fn gui_replace_manifest(
    state: StudioState<'_>,
    manifest: ProjectManifest,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    lock(&state)?.replace_manifest(manifest, selected_part_only)
}

/// Undo the last edit and return the resulting snapshot.
#[tauri::command]
fn gui_undo(
    state: StudioState<'_>,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    lock(&state)?.undo(selected_part_only)
}

/// Redo the last undone edit and return the resulting snapshot.
#[tauri::command]
fn gui_redo(
    state: StudioState<'_>,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    lock(&state)?.redo(selected_part_only)
}

/// Persist the manifest to `<projectDir>/project.bootanimix.json`.
#[tauri::command]
fn gui_save_project(
    state: StudioState<'_>,
    project_dir: Option<String>,
) -> Result<WorkspaceSnapshot, String> {
    let mut guard = lock(&state)?;
    let dir = project_dir.map(PathBuf::from);
    guard.save_project(dir.as_deref())?;
    guard.snapshot(None)
}

/// Open a project directory and load `project.bootanimix.json` from it.
#[tauri::command]
fn gui_open_project(
    state: StudioState<'_>,
    project_dir: String,
) -> Result<WorkspaceSnapshot, String> {
    lock(&state)?.open_project(&PathBuf::from(project_dir))
}

/// Build a verified `ZIP_STORED` boot animation from the current project.
#[tauri::command]
fn gui_build_archive(state: StudioState<'_>, output_path: String) -> Result<BuildReport, String> {
    lock(&state)?.build_archive(&PathBuf::from(output_path))
}

/// Export a flashable Magisk module containing the verified inner archive.
#[tauri::command]
fn gui_export_magisk(
    state: StudioState<'_>,
    output_path: String,
) -> Result<MagiskExportReport, String> {
    lock(&state)?.export_magisk(&PathBuf::from(output_path))
}

/// Build and run the desktop studio.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ManagedGuiState::default())
        .invoke_handler(tauri::generate_handler![
            gui_get_snapshot,
            gui_replace_manifest,
            gui_undo,
            gui_redo,
            gui_save_project,
            gui_open_project,
            gui_build_archive,
            gui_export_magisk
        ])
        .run(tauri::generate_context!())
        .expect("error while running BootAnimix Studio");
}
