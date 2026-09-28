use crate::error::CoreError;
use crate::manifest::{FrameAssetRef, ProjectManifest, ProjectPart};
use crate::model::{PartDeclaration, PlaybackMode, RepeatCount, RgbColor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub label: String,
    pub manifest: ProjectManifest,
}

/// Undo/Redo session wrapper around a lightweight `ProjectManifest`.
/// Never duplicates raw image buffers in memory — only snapshots the JSON-serializable manifest metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditableProjectSession {
    pub current: ProjectManifest,
    pub undo_stack: Vec<HistoryEntry>,
    pub redo_stack: Vec<HistoryEntry>,
    pub dirty: bool,
}

impl EditableProjectSession {
    pub fn new(initial: ProjectManifest) -> Self {
        Self {
            current: initial,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
        }
    }

    fn record_before_edit(&mut self, label: impl Into<String>) {
        self.undo_stack.push(HistoryEntry {
            label: label.into(),
            manifest: self.current.clone(),
        });
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> Option<String> {
        let entry = self.undo_stack.pop()?;
        let label = entry.label.clone();
        self.redo_stack.push(HistoryEntry {
            label: label.clone(),
            manifest: self.current.clone(),
        });
        self.current = entry.manifest;
        self.dirty = true;
        Some(label)
    }

    pub fn redo(&mut self) -> Option<String> {
        let entry = self.redo_stack.pop()?;
        let label = entry.label.clone();
        self.undo_stack.push(HistoryEntry {
            label: label.clone(),
            manifest: self.current.clone(),
        });
        self.current = entry.manifest;
        self.dirty = true;
        Some(label)
    }

    pub fn set_dimensions_and_fps(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        show_progress: bool,
    ) -> Result<(), CoreError> {
        if width == 0 || height == 0 || fps == 0 {
            return Err(CoreError::InvalidDescriptor(
                "width, height, and fps must be > 0".to_string(),
            ));
        }
        self.record_before_edit("Update header dimensions/FPS");
        self.current.descriptor.width = width;
        self.current.descriptor.height = height;
        self.current.descriptor.fps = fps;
        self.current.descriptor.show_progress = show_progress;
        Ok(())
    }

    pub fn update_part_properties(
        &mut self,
        part_index: usize,
        mode: PlaybackMode,
        repeat: RepeatCount,
        pause_frames: u32,
        background_color: Option<RgbColor>,
    ) -> Result<(), CoreError> {
        if part_index >= self.current.descriptor.parts.len() {
            return Err(CoreError::InvalidDescriptor(format!(
                "part index {part_index} out of bounds"
            )));
        }
        self.record_before_edit(format!(
            "Edit part '{}'",
            self.current.descriptor.parts[part_index].path
        ));
        let decl = &mut self.current.descriptor.parts[part_index];
        decl.mode = mode;
        decl.repeat = repeat;
        decl.pause_frames = pause_frames;
        decl.background_color = background_color;
        Ok(())
    }

    pub fn add_part(&mut self, name: &str, mode: PlaybackMode) -> Result<(), CoreError> {
        let trimmed = name.trim();
        if trimmed.is_empty()
            || self
                .current
                .descriptor
                .parts
                .iter()
                .any(|p| p.path == trimmed)
        {
            return Err(CoreError::InvalidDescriptor(format!(
                "part name '{trimmed}' is empty or already exists"
            )));
        }
        self.record_before_edit(format!("Add part '{trimmed}'"));
        self.current.descriptor.parts.push(PartDeclaration {
            mode,
            repeat: RepeatCount::Finite(1),
            pause_frames: 0,
            path: trimmed.to_string(),
            background_color: None,
            clock: None,
        });
        self.current.parts.push(ProjectPart {
            name: trimmed.to_string(),
            frames: Vec::new(),
            audio_relative_path: None,
        });
        Ok(())
    }

    pub fn remove_part(&mut self, part_index: usize) -> Result<(), CoreError> {
        if self.current.descriptor.parts.len() <= 1 {
            return Err(CoreError::InvalidDescriptor(
                "cannot remove the last remaining part from project".to_string(),
            ));
        }
        if part_index >= self.current.descriptor.parts.len() {
            return Err(CoreError::InvalidDescriptor(
                "part index out of bounds".to_string(),
            ));
        }
        let path_name = self.current.descriptor.parts[part_index].path.clone();
        self.record_before_edit(format!("Remove part '{path_name}'"));
        self.current.descriptor.parts.remove(part_index);
        self.current.parts.retain(|p| p.name != path_name);
        Ok(())
    }

    pub fn reorder_frame(
        &mut self,
        part_index: usize,
        from_index: usize,
        to_index: usize,
    ) -> Result<(), CoreError> {
        let Some(part) = self.current.parts.get(part_index) else {
            return Err(CoreError::InvalidDescriptor("part not found".to_string()));
        };
        if from_index >= part.frames.len() || to_index >= part.frames.len() {
            return Err(CoreError::InvalidDescriptor(
                "frame index out of bounds".to_string(),
            ));
        }
        if from_index == to_index {
            return Ok(());
        }
        self.record_before_edit(format!(
            "Reorder frame #{from_index} -> #{to_index} in '{}'",
            part.name
        ));
        let part_mut = &mut self.current.parts[part_index];
        let item = part_mut.frames.remove(from_index);
        part_mut.frames.insert(to_index, item);
        Ok(())
    }

    pub fn duplicate_frame(
        &mut self,
        part_index: usize,
        frame_index: usize,
    ) -> Result<(), CoreError> {
        let Some(part) = self.current.parts.get(part_index) else {
            return Err(CoreError::InvalidDescriptor("part not found".to_string()));
        };
        if frame_index >= part.frames.len() {
            return Err(CoreError::InvalidDescriptor(
                "frame index out of bounds".to_string(),
            ));
        }
        self.record_before_edit(format!("Duplicate frame #{frame_index} in '{}'", part.name));
        let part_mut = &mut self.current.parts[part_index];
        let clone_ref: FrameAssetRef = part_mut.frames[frame_index].clone();
        part_mut.frames.insert(frame_index + 1, clone_ref);
        Ok(())
    }

    pub fn remove_frame(&mut self, part_index: usize, frame_index: usize) -> Result<(), CoreError> {
        let Some(part) = self.current.parts.get(part_index) else {
            return Err(CoreError::InvalidDescriptor("part not found".to_string()));
        };
        if part.frames.len() <= 1 {
            return Err(CoreError::InvalidDescriptor(
                "cannot remove the last remaining frame in a part".to_string(),
            ));
        }
        if frame_index >= part.frames.len() {
            return Err(CoreError::InvalidDescriptor(
                "frame index out of bounds".to_string(),
            ));
        }
        self.record_before_edit(format!("Remove frame #{frame_index} in '{}'", part.name));
        self.current.parts[part_index].frames.remove(frame_index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{MagiskExportSettings, TransformSettings, CURRENT_SCHEMA_VERSION};
    use crate::model::AnimationDescriptor;

    #[test]
    fn undo_and_redo_restore_exact_project_state() {
        let initial = ProjectManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: "edit_test".to_string(),
            descriptor: AnimationDescriptor {
                width: 1080,
                height: 2400,
                fps: 60,
                show_progress: false,
                dynamic_colors: None,
                parts: vec![PartDeclaration {
                    mode: PlaybackMode::PlayInterruptible,
                    repeat: RepeatCount::Finite(1),
                    pause_frames: 0,
                    path: "part0".to_string(),
                    background_color: None,
                    clock: None,
                }],
                preserved_directives: vec![],
            },
            parts: vec![ProjectPart {
                name: "part0".to_string(),
                frames: vec![
                    FrameAssetRef {
                        relative_path: "assets/part0/000.png".to_string(),
                        trim: None,
                    },
                    FrameAssetRef {
                        relative_path: "assets/part0/001.png".to_string(),
                        trim: None,
                    },
                ],
                audio_relative_path: None,
            }],
            transforms: TransformSettings::default(),
            magisk: MagiskExportSettings::default(),
            source_archive: None,
            import_warnings: vec![],
        };

        let mut session = EditableProjectSession::new(initial.clone());
        session.reorder_frame(0, 0, 1).unwrap();
        assert_eq!(
            session.current.parts[0].frames[0].relative_path,
            "assets/part0/001.png"
        );

        session.duplicate_frame(0, 1).unwrap();
        assert_eq!(session.current.parts[0].frames.len(), 3);

        session.undo().unwrap();
        assert_eq!(session.current.parts[0].frames.len(), 2);

        session.undo().unwrap();
        assert_eq!(session.current, initial);

        session.redo().unwrap();
        assert_eq!(
            session.current.parts[0].frames[0].relative_path,
            "assets/part0/001.png"
        );
    }
}
