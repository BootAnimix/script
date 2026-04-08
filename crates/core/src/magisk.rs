use crate::archive::{inspect_zip_file, is_safe_archive_entry_path, ArchiveInspection};
use crate::error::CoreError;
use crate::manifest::{MagiskExportSettings, ProjectManifest};
use crate::pipeline::{build_project_to_archive, BuildReport};
use crate::validator::{validate_archive_file, ValidationOutcome};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MagiskExportReport {
    pub module_zip_path: String,
    pub module_zip_size_bytes: u64,
    pub module_id: String,
    pub target_overlay_path: String,
    pub inner_build_report: BuildReport,
    pub outer_entries: Vec<String>,
    pub inner_stored_verified: bool,
    pub install_instructions: Vec<String>,
}

/// Validate Magisk `module.prop` settings according to `https://topjohnwu.github.io/Magisk/guides.html`:
/// - `id` must match `^[a-zA-Z][a-zA-Z0-9._-]+$`
/// - `versionCode` must be an integer
/// - `target_path` must be a safe relative path under `system/` ending with `.zip`
pub fn validate_magisk_settings(settings: &MagiskExportSettings) -> Result<(), CoreError> {
    let id = settings.module_id.trim();
    if id.len() < 2 {
        return Err(CoreError::Pipeline(
            "Magisk module id must be at least 2 characters long (^[a-zA-Z][a-zA-Z0-9._-]+$)"
                .to_string(),
        ));
    }
    let mut chars = id.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(CoreError::Pipeline(format!(
            "Magisk module id '{id}' must start with an ASCII letter"
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
        return Err(CoreError::Pipeline(format!(
            "Magisk module id '{id}' contains invalid characters (allowed: a-zA-Z0-9._-)"
        )));
    }

    if settings.module_name.trim().is_empty() || settings.module_name.contains('\n') {
        return Err(CoreError::Pipeline(
            "Magisk module name must be a non-empty single-line string".to_string(),
        ));
    }
    if settings.version.trim().is_empty() || settings.version.contains('\n') {
        return Err(CoreError::Pipeline(
            "Magisk module version must be a non-empty single-line string".to_string(),
        ));
    }
    if settings.version_code == 0 {
        return Err(CoreError::Pipeline(
            "Magisk module versionCode must be a positive integer".to_string(),
        ));
    }

    let tp = settings.target_path.trim_matches('/');
    if !is_safe_archive_entry_path(tp)
        || !tp.starts_with("system/")
        || !tp.ends_with(".zip")
    {
        return Err(CoreError::Pipeline(format!(
            "Magisk target_path '{}' must be a safe relative path under 'system/' ending in '.zip' (e.g. 'system/media/bootanimation.zip' or 'system/product/media/bootanimation.zip')",
            settings.target_path
        )));
    }

    Ok(())
}

/// Generate a UNIX-LF formatted `module.prop` string.
pub fn format_module_prop(settings: &MagiskExportSettings) -> Result<String, CoreError> {
    validate_magisk_settings(settings)?;
    let clean = |s: &str| s.replace(['\r', '\n'], " ").trim().to_string();
    Ok(format!(
        "id={}\nname={}\nversion={}\nversionCode={}\nauthor={}\ndescription={}\n",
        settings.module_id.trim(),
        clean(&settings.module_name),
        clean(&settings.version),
        settings.version_code,
        clean(&settings.author),
        clean(&settings.description),
    ))
}

/// Build the inner `bootanimation.zip` (strictly `ZIP_STORED`), verify it independently,
/// package it into a flashable Magisk module ZIP alongside `module.prop`, and verify both layers.
pub fn export_magisk_module(
    manifest: &ProjectManifest,
    project_dir: &Path,
    output_module_zip: &Path,
) -> Result<MagiskExportReport, CoreError> {
    validate_magisk_settings(&manifest.magisk)?;
    let module_prop_text = format_module_prop(&manifest.magisk)?;
    let target_overlay_path = manifest.magisk.target_path.trim_matches('/').to_string();

    let staging = tempfile::tempdir()?;
    let inner_zip_path = staging.path().join("bootanimation.zip");

    let inner_build_report = build_project_to_archive(manifest, project_dir, &inner_zip_path)?;
    let inner_bytes = std::fs::read(&inner_zip_path)?;

    let parent_dir = output_module_zip
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent_dir)?;

    let temp_outer = tempfile::NamedTempFile::new_in(parent_dir)?;
    {
        let mut zw = ZipWriter::new(temp_outer.as_file());
        let fixed_time = DateTime::from_date_and_time(2025, 1, 1, 0, 0, 0)
            .unwrap_or_else(|_| DateTime::default());
        let stored_opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(fixed_time)
            .unix_permissions(0o644);

        zw.start_file("module.prop", stored_opts)
            .map_err(|e| CoreError::Archive(format!("failed to write module.prop: {e}")))?;
        zw.write_all(module_prop_text.as_bytes())?;

        zw.start_file(&target_overlay_path, stored_opts)
            .map_err(|e| CoreError::Archive(format!("failed to write inner zip: {e}")))?;
        zw.write_all(&inner_bytes)?;

        let mut finished = zw
            .finish()
            .map_err(|e| CoreError::Archive(format!("failed to finish module zip: {e}")))?;
        finished.flush()?;
    }

    // Independently verify the outer module archive AND extract/verify the inner bootanimation.zip
    let outer_inspection = inspect_zip_file(temp_outer.path())?;
    let outer_names: Vec<String> = outer_inspection.entries.iter().map(|e| e.name.clone()).collect();
    if !outer_names.contains(&"module.prop".to_string())
        || !outer_names.contains(&target_overlay_path)
    {
        return Err(CoreError::Archive(
            "outer Magisk module verification failed: missing module.prop or target bootanimation.zip"
                .to_string(),
        ));
    }

    let verify_inner_path = staging.path().join("verify_inner.zip");
    {
        let f = File::open(temp_outer.path())?;
        let mut za = ZipArchive::new(f).map_err(|e| CoreError::Archive(e.to_string()))?;
        let mut inner_entry = za
            .by_name(&target_overlay_path)
            .map_err(|e| CoreError::Archive(e.to_string()))?;
        let mut extracted_inner = Vec::new();
        inner_entry.read_to_end(&mut extracted_inner)?;
        std::fs::write(&verify_inner_path, &extracted_inner)?;
    }

    let inner_recheck: ArchiveInspection = inspect_zip_file(&verify_inner_path)?;
    let inner_val = validate_archive_file(&verify_inner_path)?;
    let inner_stored_verified = inner_recheck.compressed_entries.is_empty()
        && inner_val.outcome != ValidationOutcome::Invalid;
    if !inner_stored_verified {
        return Err(CoreError::Archive(
            "inner bootanimation.zip inside Magisk module failed storage or validation check"
                .to_string(),
        ));
    }

    let module_zip_size_bytes = std::fs::metadata(temp_outer.path())?.len();
    temp_outer
        .persist(output_module_zip)
        .map_err(|e| CoreError::Io(e.error))?;

    let install_instructions = vec![
        format!("1. Copy '{}' to your Android device.", output_module_zip.display()),
        "2. Open the Magisk app -> Modules -> Install from storage, and select the module ZIP.".to_string(),
        format!(
            "3. Reboot to activate overlay at '/{}'. (If your ROM uses /product/media, re-export with target_path 'system/product/media/bootanimation.zip'.)",
            target_overlay_path
        ),
        format!(
            "4. To remove or recover: disable/remove '{}' in the Magisk app or delete /data/adb/modules/{} from recovery.",
            manifest.magisk.module_name, manifest.magisk.module_id
        ),
    ];

    Ok(MagiskExportReport {
        module_zip_path: output_module_zip.display().to_string(),
        module_zip_size_bytes,
        module_id: manifest.magisk.module_id.clone(),
        target_overlay_path,
        inner_build_report,
        outer_entries: outer_names,
        inner_stored_verified,
        install_instructions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::import_frame_sequence_dir;
    use crate::validator::encode_solid_png;

    #[test]
    fn validates_magisk_id_and_exports_verified_two_layer_module() {
        let mut bad = MagiskExportSettings::default();
        bad.module_id = "1_invalid_start".to_string();
        assert!(validate_magisk_settings(&bad).is_err());

        bad.module_id = "valid_id".to_string();
        bad.target_path = "../evil/bootanimation.zip".to_string();
        assert!(validate_magisk_settings(&bad).is_err());

        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("frames");
        std::fs::create_dir_all(&src_dir).unwrap();
        let png = encode_solid_png(64, 64, [10, 20, 30, 255]).unwrap();
        std::fs::write(src_dir.join("000.png"), &png).unwrap();

        let proj_dir = dir.path().join("proj");
        let mut manifest = import_frame_sequence_dir(&src_dir, &proj_dir, 30).unwrap();
        manifest.magisk.module_id = "bootanimix_pixel".to_string();
        manifest.magisk.target_path = "system/product/media/bootanimation.zip".to_string();

        let mod_zip = dir.path().join("magisk_module.zip");
        let report = export_magisk_module(&manifest, &proj_dir, &mod_zip).unwrap();
        assert!(report.inner_stored_verified);
        assert!(report.outer_entries.contains(&"module.prop".to_string()));
        assert!(report
            .outer_entries
            .contains(&"system/product/media/bootanimation.zip".to_string()));
    }
}
