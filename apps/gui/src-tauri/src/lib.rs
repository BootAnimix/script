use bootanimix_core::{
    build_playback_schedule, build_project_to_archive, export_magisk_module, validate_project,
    AnimationDescriptor, EditableProjectSession, FrameAssetRef, MagiskExportReport,
    MagiskExportSettings, PartDeclaration, PlaybackMode, PlaybackSchedule, ProjectManifest,
    ProjectPart, RepeatCount, RgbColor, ScheduleOptions, TransformSettings, ValidationReport,
    CURRENT_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub project_dir: Option<String>,
    pub manifest: ProjectManifest,
    pub validation: ValidationReport,
    pub schedule: PlaybackSchedule,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
}

pub struct GuiBackendState {
    pub project_dir: Option<PathBuf>,
    pub session: EditableProjectSession,
}

impl Default for GuiBackendState {
    fn default() -> Self {
        Self::new_default()
    }
}

impl GuiBackendState {
    pub fn new_default() -> Self {
        let initial = ProjectManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: "pixel_monochrome_boot".to_string(),
            descriptor: AnimationDescriptor {
                width: 1080,
                height: 2400,
                fps: 60,
                show_progress: true,
                dynamic_colors: None,
                parts: vec![
                    PartDeclaration {
                        mode: PlaybackMode::PlayInterruptible,
                        repeat: RepeatCount::Finite(1),
                        pause_frames: 6,
                        path: "part0".to_string(),
                        background_color: Some(RgbColor::new(8, 10, 14)),
                        clock: None,
                    },
                    PartDeclaration {
                        mode: PlaybackMode::FadeOut(12),
                        repeat: RepeatCount::Infinite,
                        pause_frames: 0,
                        path: "part1".to_string(),
                        background_color: Some(RgbColor::new(8, 10, 14)),
                        clock: None,
                    },
                ],
                preserved_directives: vec![],
            },
            parts: vec![
                ProjectPart {
                    name: "part0".to_string(),
                    frames: vec![FrameAssetRef {
                        relative_path: "assets/part0/00000.png".to_string(),
                        trim: None,
                    }],
                    audio_relative_path: None,
                },
                ProjectPart {
                    name: "part1".to_string(),
                    frames: vec![FrameAssetRef {
                        relative_path: "assets/part1/00000.png".to_string(),
                        trim: None,
                    }],
                    audio_relative_path: None,
                },
            ],
            transforms: TransformSettings::default(),
            magisk: MagiskExportSettings::default(),
            source_archive: None,
            import_warnings: vec![],
        };
        Self {
            project_dir: None,
            session: EditableProjectSession::new(initial),
        }
    }

    pub fn snapshot(&self, selected_part_only: Option<usize>) -> Result<WorkspaceSnapshot, String> {
        let validation = if let Some(dir) = &self.project_dir {
            validate_project(&self.session.current, dir).map_err(|e| e.to_string())?
        } else {
            bootanimix_core::ValidationReport::from_parts(
                vec![],
                bootanimix_core::PerformanceSummary {
                    total_frames: self
                        .session
                        .current
                        .parts
                        .iter()
                        .map(|p| p.frames.len())
                        .sum(),
                    total_image_bytes: 0,
                    archive_size_bytes: 0,
                    per_part_frames: self
                        .session
                        .current
                        .parts
                        .iter()
                        .map(|p| (p.name.clone(), p.frames.len()))
                        .collect(),
                    finite_duration_seconds: None,
                    has_infinite_loop: true,
                },
                Some(self.session.current.descriptor.clone()),
            )
        };
        let schedule = build_playback_schedule(
            &self.session.current,
            ScheduleOptions {
                max_window_ticks: 240,
                selected_part_only,
                simulated_boot_exit_tick: None,
            },
        );
        Ok(WorkspaceSnapshot {
            project_dir: self.project_dir.as_ref().map(|p| p.display().to_string()),
            manifest: self.session.current.clone(),
            validation,
            schedule,
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
            dirty: self.session.dirty,
        })
    }
}

pub struct ManagedGuiState(pub Mutex<GuiBackendState>);

impl Default for ManagedGuiState {
    fn default() -> Self {
        Self(Mutex::new(GuiBackendState::new_default()))
    }
}

pub fn gui_open_project(
    state: &ManagedGuiState,
    project_dir: &Path,
) -> Result<WorkspaceSnapshot, String> {
    let manifest_path = if project_dir.is_dir() {
        project_dir.join("project.bootanimix.json")
    } else {
        project_dir.to_path_buf()
    };
    let root_dir = if project_dir.is_dir() {
        project_dir.to_path_buf()
    } else {
        project_dir
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    };
    let manifest = ProjectManifest::load_from_file(&manifest_path).map_err(|e| e.to_string())?;
    let mut guard = state.0.lock().map_err(|_| "mutex poisoned")?;
    guard.project_dir = Some(root_dir);
    guard.session = EditableProjectSession::new(manifest);
    guard.snapshot(None)
}

pub fn gui_replace_manifest(
    state: &ManagedGuiState,
    manifest: ProjectManifest,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    let mut guard = state.0.lock().map_err(|_| "mutex poisoned")?;
    let prev = guard.session.current.clone();
    guard.session.undo_stack.push(bootanimix_core::HistoryEntry {
        label: "GUI edit".to_string(),
        manifest: prev,
    });
    guard.session.redo_stack.clear();
    guard.session.current = manifest;
    guard.session.dirty = true;
    guard.snapshot(selected_part_only)
}

pub fn gui_undo(
    state: &ManagedGuiState,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    let mut guard = state.0.lock().map_err(|_| "mutex poisoned")?;
    guard.session.undo();
    guard.snapshot(selected_part_only)
}

pub fn gui_redo(
    state: &ManagedGuiState,
    selected_part_only: Option<usize>,
) -> Result<WorkspaceSnapshot, String> {
    let mut guard = state.0.lock().map_err(|_| "mutex poisoned")?;
    guard.session.redo();
    guard.snapshot(selected_part_only)
}

pub fn gui_build_archive(
    state: &ManagedGuiState,
    output_zip: &Path,
) -> Result<bootanimix_core::BuildReport, String> {
    let guard = state.0.lock().map_err(|_| "mutex poisoned")?;
    let dir = guard
        .project_dir
        .as_deref()
        .ok_or_else(|| "no project directory loaded".to_string())?;
    build_project_to_archive(&guard.session.current, dir, output_zip).map_err(|e| e.to_string())
}

pub fn gui_export_magisk(
    state: &ManagedGuiState,
    output_zip: &Path,
) -> Result<MagiskExportReport, String> {
    let guard = state.0.lock().map_err(|_| "mutex poisoned")?;
    let dir = guard
        .project_dir
        .as_deref()
        .ok_or_else(|| "no project directory loaded".to_string())?;
    export_magisk_module(&guard.session.current, dir, output_zip).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootanimix_core::{encode_solid_png, import_frame_sequence_dir};

    #[test]
    fn gui_backend_commands_share_core_and_support_undo_redo_and_build() {
        let dir = tempfile::tempdir().unwrap();
        let frames_dir = dir.path().join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();
        let png = encode_solid_png(64, 64, [30, 60, 90, 255]).unwrap();
        std::fs::write(frames_dir.join("000.png"), &png).unwrap();

        let proj_dir = dir.path().join("proj");
        import_frame_sequence_dir(&frames_dir, &proj_dir, 30).unwrap();

        let state = ManagedGuiState::default();
        let snap1 = gui_open_project(&state, &proj_dir).unwrap();
        assert_eq!(snap1.manifest.descriptor.fps, 30);

        let mut modified = snap1.manifest.clone();
        modified.descriptor.fps = 60;
        let snap2 = gui_replace_manifest(&state, modified, None).unwrap();
        assert_eq!(snap2.manifest.descriptor.fps, 60);
        assert!(snap2.can_undo);

        let snap3 = gui_undo(&state, None).unwrap();
        assert_eq!(snap3.manifest.descriptor.fps, 30);
        assert!(snap3.can_redo);

        let snap4 = gui_redo(&state, None).unwrap();
        assert_eq!(snap4.manifest.descriptor.fps, 60);

        let out_zip = dir.path().join("gui_built.zip");
        let rep = gui_build_archive(&state, &out_zip).unwrap();
        assert!(rep.invariants_verified);
    }
}
