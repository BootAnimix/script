use crate::error::CoreError;
use crate::frames::FrameFormat;
use crate::model::{AnimationDescriptor, RgbColor, TrimRect};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizeMode {
    Stretch,
    Contain,
    Crop,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformSettings {
    pub resize: Option<(u32, u32, ResizeMode)>,
    pub flatten_alpha_to: Option<RgbColor>,
    pub convert_format: Option<FrameFormat>,
    pub target_fps: Option<u32>,
    #[serde(default)]
    pub lossless_optimize_png: bool,
    #[serde(default)]
    pub generate_trim_txt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameAssetRef {
    /// Relative path inside the project workspace to the source frame image.
    pub relative_path: String,
    pub trim: Option<TrimRect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPart {
    pub name: String,
    pub frames: Vec<FrameAssetRef>,
    pub audio_relative_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagiskExportSettings {
    pub module_id: String,
    pub module_name: String,
    pub version: String,
    pub version_code: u32,
    pub author: String,
    pub description: String,
    /// Target path inside module, e.g. `system/media/bootanimation.zip` or `system/product/media/bootanimation.zip`.
    pub target_path: String,
}

impl Default for MagiskExportSettings {
    fn default() -> Self {
        Self {
            module_id: "bootanimix_custom".to_string(),
            module_name: "Custom Boot Animation".to_string(),
            version: "v1.0.0".to_string(),
            version_code: 1,
            author: "BootAnimix".to_string(),
            description: "Custom Android boot animation packaged by BootAnimix".to_string(),
            target_path: "system/media/bootanimation.zip".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub name: String,
    pub descriptor: AnimationDescriptor,
    pub parts: Vec<ProjectPart>,
    #[serde(default)]
    pub transforms: TransformSettings,
    #[serde(default)]
    pub magisk: MagiskExportSettings,
    #[serde(default)]
    pub source_archive: Option<String>,
    #[serde(default)]
    pub import_warnings: Vec<String>,
}

impl ProjectManifest {
    pub fn to_json_pretty(&self) -> Result<String, CoreError> {
        self.validate_schema_and_paths()?;
        serde_json::to_string_pretty(self).map_err(|e| CoreError::Manifest(e.to_string()))
    }

    pub fn from_json_str(raw: &str) -> Result<Self, CoreError> {
        let raw_val: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| CoreError::Manifest(e.to_string()))?;

        if let Some(ver) = raw_val.get("schema_version").and_then(|v| v.as_u64()) {
            if ver == 0 || ver > CURRENT_SCHEMA_VERSION as u64 {
                return Err(CoreError::Manifest(format!(
                    "unsupported project schema_version {ver} (supported: 1..={CURRENT_SCHEMA_VERSION})"
                )));
            }
        }

        let manifest: Self =
            serde_json::from_value(raw_val).map_err(|e| CoreError::Manifest(e.to_string()))?;
        manifest.validate_schema_and_paths()?;
        Ok(manifest)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), CoreError> {
        let json = self.to_json_pretty()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, CoreError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json_str(&text)
    }

    pub fn validate_schema_and_paths(&self) -> Result<(), CoreError> {
        if self.schema_version == 0 || self.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(CoreError::Manifest(format!(
                "unsupported schema_version {}",
                self.schema_version
            )));
        }

        for part in &self.parts {
            for frame in &part.frames {
                ensure_safe_relative_path(&frame.relative_path)?;
            }
            if let Some(audio) = &part.audio_relative_path {
                ensure_safe_relative_path(audio)?;
            }
        }
        Ok(())
    }

    /// Resolve a relative asset path safely against the project root directory.
    pub fn resolve_asset_path(project_dir: &Path, relative: &str) -> Result<PathBuf, CoreError> {
        ensure_safe_relative_path(relative)?;
        Ok(project_dir.join(relative))
    }
}

pub fn ensure_safe_relative_path(rel: &str) -> Result<(), CoreError> {
    if rel.trim().is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(CoreError::Manifest(format!(
            "asset path must be non-empty relative path: '{rel}'"
        )));
    }
    let path = Path::new(rel);
    for comp in path.components() {
        match comp {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::Manifest(format!(
                    "unsafe path traversal or root component in '{rel}'"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PartDeclaration, PlaybackMode, RepeatCount};

    #[test]
    fn manifest_round_trips_and_rejects_future_schema() {
        let manifest = ProjectManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: "pixel_boot".to_string(),
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
                frames: vec![FrameAssetRef {
                    relative_path: "assets/part0/00000.png".to_string(),
                    trim: None,
                }],
                audio_relative_path: None,
            }],
            transforms: TransformSettings::default(),
            magisk: MagiskExportSettings::default(),
            source_archive: None,
            import_warnings: vec![],
        };

        let json = manifest.to_json_pretty().expect("serialize");
        let loaded = ProjectManifest::from_json_str(&json).expect("deserialize");
        assert_eq!(manifest, loaded);

        let future_json = json.replace("\"schema_version\": 1", "\"schema_version\": 99");
        assert!(ProjectManifest::from_json_str(&future_json).is_err());
    }

    #[test]
    fn rejects_parent_traversal_in_asset_paths() {
        assert!(ensure_safe_relative_path("../etc/passwd").is_err());
        assert!(ensure_safe_relative_path("/abs/path.png").is_err());
        assert!(ensure_safe_relative_path("assets/part0/00000.png").is_ok());
    }
}
