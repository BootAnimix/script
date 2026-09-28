use crate::archive::{inspect_zip_file, is_safe_archive_entry_path, ArchiveInspection};
use crate::error::CoreError;
use crate::frames::{order_frame_names, FrameFormat};
use crate::manifest::ProjectManifest;
use crate::model::{AnimationDescriptor, PlaybackMode, RepeatCount, TrimRect};
use image::{ColorType, GenericImageView};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOutcome {
    Valid,
    ValidWithWarnings,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file_or_part: Option<String>,
    pub line: Option<usize>,
    pub remediation: String,
    pub blocks_export: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub total_frames: usize,
    pub total_image_bytes: u64,
    pub archive_size_bytes: u64,
    pub per_part_frames: BTreeMap<String, usize>,
    pub finite_duration_seconds: Option<f64>,
    pub has_infinite_loop: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub outcome: ValidationOutcome,
    pub diagnostics: Vec<Diagnostic>,
    pub performance: PerformanceSummary,
    pub descriptor: Option<AnimationDescriptor>,
}

impl ValidationReport {
    pub fn from_parts(
        diagnostics: Vec<Diagnostic>,
        performance: PerformanceSummary,
        descriptor: Option<AnimationDescriptor>,
    ) -> Self {
        let has_error = diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error || d.blocks_export);
        let has_warning = diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Warning);

        let outcome = if has_error {
            ValidationOutcome::Invalid
        } else if has_warning {
            ValidationOutcome::ValidWithWarnings
        } else {
            ValidationOutcome::Valid
        };

        Self {
            outcome,
            diagnostics,
            performance,
            descriptor,
        }
    }
}

/// Validate a `bootanimation.zip` archive on disk.
pub fn validate_archive_file(path: &Path) -> Result<ValidationReport, CoreError> {
    let inspection = inspect_zip_file(path)?;
    let mut diagnostics = Vec::new();

    for unsafe_path in &inspection.unsafe_paths {
        diagnostics.push(Diagnostic {
            code: "BA-ARC-002".to_string(),
            severity: DiagnosticSeverity::Error,
            message: format!("Archive entry has unsafe path: '{unsafe_path}'"),
            file_or_part: Some(unsafe_path.clone()),
            line: None,
            remediation:
                "Remove parent-directory ('..'), absolute, or backslash paths from the archive."
                    .to_string(),
            blocks_export: true,
        });
    }

    for dup in &inspection.duplicate_entries {
        diagnostics.push(Diagnostic {
            code: "BA-ARC-003".to_string(),
            severity: DiagnosticSeverity::Error,
            message: format!("Duplicate archive entry: '{dup}'"),
            file_or_part: Some(dup.clone()),
            line: None,
            remediation: "Ensure every file in bootanimation.zip has a unique path.".to_string(),
            blocks_export: true,
        });
    }

    if !inspection.compressed_entries.is_empty() {
        diagnostics.push(Diagnostic {
            code: "BA-ARC-001".to_string(),
            severity: DiagnosticSeverity::Error,
            message: format!(
                "{} archive entries use compression instead of ZIP_STORED (0), e.g. '{}'",
                inspection.compressed_entries.len(),
                inspection.compressed_entries[0]
            ),
            file_or_part: Some(inspection.compressed_entries[0].clone()),
            line: None,
            remediation: "Repackage bootanimation.zip using ZIP_STORED (compression method 0) for all entries.".to_string(),
            blocks_export: true,
        });
    }

    if !inspection.has_desc_txt {
        diagnostics.push(Diagnostic {
            code: "BA-ARC-004".to_string(),
            severity: DiagnosticSeverity::Error,
            message: "Missing required 'desc.txt' at root of archive".to_string(),
            file_or_part: Some("desc.txt".to_string()),
            line: None,
            remediation: "Place 'desc.txt' at the root level of bootanimation.zip.".to_string(),
            blocks_export: true,
        });
    }

    if let Some(err_msg) = &inspection.descriptor_error {
        diagnostics.push(Diagnostic {
            code: "BA-DSC-001".to_string(),
            severity: DiagnosticSeverity::Error,
            message: format!("Failed to parse desc.txt: {err_msg}"),
            file_or_part: Some("desc.txt".to_string()),
            line: extract_line_from_error(err_msg),
            remediation: "Fix the syntax error in desc.txt according to AOSP FORMAT.md."
                .to_string(),
            blocks_export: true,
        });
    }

    let file = File::open(path)?;
    let mut zip = ZipArchive::new(file).map_err(|e| CoreError::Archive(e.to_string()))?;

    let mut part_entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut trim_entries: BTreeMap<String, String> = BTreeMap::new();
    let mut total_image_bytes: u64 = 0;
    let mut has_clock_font = false;
    let mut has_progress_font = false;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| CoreError::Archive(e.to_string()))?;
        let name = entry.name().to_string();
        if entry.is_dir() || !is_safe_archive_entry_path(&name) {
            continue;
        }
        if name == "clock_font.png" {
            has_clock_font = true;
            continue;
        }
        if name == "progress_font.png" {
            has_progress_font = true;
            continue;
        }

        let segs: Vec<&str> = name.split('/').collect();
        if segs.len() == 2 {
            let part_name = segs[0].to_string();
            let file_name = segs[1];
            if file_name == "trim.txt" {
                let mut text = String::new();
                let _ = entry.read_to_string(&mut text);
                trim_entries.insert(part_name, text);
            } else if file_name == "audio.wav" {
                // recognized optional audio
            } else {
                part_entries
                    .entry(part_name)
                    .or_default()
                    .push(file_name.to_string());
                total_image_bytes = total_image_bytes.saturating_add(entry.size());
            }
        }
    }

    if let Some(desc) = &inspection.parsed_descriptor {
        validate_descriptor_semantics(
            desc,
            &part_entries,
            &trim_entries,
            has_clock_font,
            has_progress_font,
            &mut diagnostics,
        );

        // Inspect actual frame images inside each declared part
        for part in &desc.parts {
            if part.mode == PlaybackMode::SystemFallback {
                continue;
            }
            if let Some(files) = part_entries.get(&part.path) {
                match order_frame_names(files) {
                    Ok(seq_report) => {
                        if seq_report.inconsistent_padding {
                            diagnostics.push(Diagnostic {
                                code: "BA-FRM-004".to_string(),
                                severity: DiagnosticSeverity::Warning,
                                message: format!(
                                    "Part '{}' has inconsistent zero-padding in frame filenames",
                                    part.path
                                ),
                                file_or_part: Some(part.path.clone()),
                                line: None,
                                remediation: "Use consistent zero-padded frame names (e.g. 00000.png, 00001.png).".to_string(),
                                blocks_export: false,
                            });
                        }
                        if seq_report.has_numbering_gaps {
                            diagnostics.push(Diagnostic {
                                code: "BA-FRM-004".to_string(),
                                severity: DiagnosticSeverity::Warning,
                                message: format!(
                                    "Part '{}' has gaps in frame numbering sequence",
                                    part.path
                                ),
                                file_or_part: Some(part.path.clone()),
                                line: None,
                                remediation: "Verify no frames were accidentally dropped or renumber sequentially.".to_string(),
                                blocks_export: false,
                            });
                        }
                    }
                    Err(err) => {
                        diagnostics.push(Diagnostic {
                            code: "BA-FRM-002".to_string(),
                            severity: DiagnosticSeverity::Error,
                            message: format!("Frame sequence error in '{}': {err}", part.path),
                            file_or_part: Some(part.path.clone()),
                            line: None,
                            remediation: "Remove unsupported files or duplicate frame numbers from the part directory.".to_string(),
                            blocks_export: true,
                        });
                    }
                }

                // Checkarchive entry order vs numeric frame order
                let mut prev_name: Option<String> = None;
                for file_name in files {
                    if let Some(prev) = &prev_name {
                        if file_name < prev {
                            diagnostics.push(Diagnostic {
                                code: "BA-ARC-005".to_string(),
                                severity: DiagnosticSeverity::Warning,
                                message: format!(
                                    "Archive entries in '{}' are not stored in lexicographical order ('{}' after '{}')",
                                    part.path, file_name, prev
                                ),
                                file_or_part: Some(part.path.clone()),
                                line: None,
                                remediation: "Rebuild the archive so frame entries are written in sorted order.".to_string(),
                                blocks_export: false,
                            });
                            break;
                        }
                    }
                    prev_name = Some(file_name.clone());
                }

                // Decode and validate each frame image in this part
                let trim_rects = trim_entries
                    .get(&part.path)
                    .map(|t| parse_trim_lines(t))
                    .unwrap_or_default();

                for (idx, file_name) in files.iter().enumerate() {
                    let entry_path = format!("{}/{}", part.path, file_name);
                    if let Ok(mut zf) = zip.by_name(&entry_path) {
                        let mut buf = Vec::new();
                        if zf.read_to_end(&mut buf).is_ok() {
                            let expected_dims = trim_rects
                                .get(idx)
                                .and_then(|r| *r)
                                .map(|r| (r.width, r.height))
                                .unwrap_or((desc.width, desc.height));
                            validate_image_bytes(
                                &buf,
                                &entry_path,
                                expected_dims,
                                &mut diagnostics,
                            );
                        }
                    }
                }
            }
        }
    }

    let perf = compute_performance_summary(&inspection, total_image_bytes, &mut diagnostics);

    Ok(ValidationReport::from_parts(
        diagnostics,
        perf,
        inspection.parsed_descriptor,
    ))
}

/// Validate a local `ProjectManifest` and its referenced source files on disk.
pub fn validate_project(
    manifest: &ProjectManifest,
    project_dir: &Path,
) -> Result<ValidationReport, CoreError> {
    let mut diagnostics = Vec::new();
    let mut part_entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut trim_entries: BTreeMap<String, String> = BTreeMap::new();
    let mut total_image_bytes: u64 = 0;
    let mut per_part_frames: BTreeMap<String, usize> = BTreeMap::new();

    for part in &manifest.parts {
        per_part_frames.insert(part.name.clone(), part.frames.len());
        let mut names = Vec::new();
        let mut trim_lines = Vec::new();
        let mut any_trim = false;

        for (idx, frame_ref) in part.frames.iter().enumerate() {
            let file_name = Path::new(&frame_ref.relative_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&frame_ref.relative_path)
                .to_string();
            names.push(file_name);

            if let Some(t) = frame_ref.trim {
                any_trim = true;
                trim_lines.push(t.to_string());
            } else {
                trim_lines.push(format!(
                    "{}x{}+0+0",
                    manifest.descriptor.width, manifest.descriptor.height
                ));
            }

            match ProjectManifest::resolve_asset_path(project_dir, &frame_ref.relative_path) {
                Ok(full_path) => match std::fs::read(&full_path) {
                    Ok(bytes) => {
                        total_image_bytes = total_image_bytes.saturating_add(bytes.len() as u64);
                        let expected_dims = if manifest.transforms.resize.is_some() {
                            manifest
                                .transforms
                                .resize
                                .map(|(w, h, _)| (w, h))
                                .unwrap_or((manifest.descriptor.width, manifest.descriptor.height))
                        } else if let Some(t) = frame_ref.trim {
                            (t.width, t.height)
                        } else {
                            (manifest.descriptor.width, manifest.descriptor.height)
                        };
                        validate_image_bytes(
                            &bytes,
                            &frame_ref.relative_path,
                            expected_dims,
                            &mut diagnostics,
                        );
                    }
                    Err(_) => {
                        diagnostics.push(Diagnostic {
                            code: "BA-DSC-005".to_string(),
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "Missing frame file '{}' (part '{}', frame #{})",
                                frame_ref.relative_path, part.name, idx
                            ),
                            file_or_part: Some(frame_ref.relative_path.clone()),
                            line: None,
                            remediation: "Restore the missing frame file or remove its reference from the part.".to_string(),
                            blocks_export: true,
                        });
                    }
                },
                Err(err) => {
                    diagnostics.push(Diagnostic {
                        code: "BA-ARC-002".to_string(),
                        severity: DiagnosticSeverity::Error,
                        message: format!("Unsafe asset path '{}': {err}", frame_ref.relative_path),
                        file_or_part: Some(frame_ref.relative_path.clone()),
                        line: None,
                        remediation: "Use a relative path inside the project folder.".to_string(),
                        blocks_export: true,
                    });
                }
            }
        }

        if any_trim {
            trim_entries.insert(part.name.clone(), trim_lines.join("\n"));
        }
        part_entries.insert(part.name.clone(), names);
    }

    validate_descriptor_semantics(
        &manifest.descriptor,
        &part_entries,
        &trim_entries,
        project_dir.join("clock_font.png").exists(),
        project_dir.join("progress_font.png").exists(),
        &mut diagnostics,
    );

    let dummy_inspection = ArchiveInspection {
        archive_size_bytes: total_image_bytes,
        entries: vec![],
        duplicate_entries: vec![],
        unsafe_paths: vec![],
        compressed_entries: vec![],
        has_desc_txt: true,
        parsed_descriptor: Some(manifest.descriptor.clone()),
        descriptor_error: None,
        part_frame_counts: per_part_frames,
    };
    let perf = compute_performance_summary(&dummy_inspection, total_image_bytes, &mut diagnostics);

    Ok(ValidationReport::from_parts(
        diagnostics,
        perf,
        Some(manifest.descriptor.clone()),
    ))
}

fn validate_descriptor_semantics(
    desc: &AnimationDescriptor,
    part_entries: &BTreeMap<String, Vec<String>>,
    trim_entries: &BTreeMap<String, String>,
    has_clock_font: bool,
    has_progress_font: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if desc.width == 0 || desc.height == 0 || desc.fps == 0 {
        diagnostics.push(Diagnostic {
            code: "BA-DSC-002".to_string(),
            severity: DiagnosticSeverity::Error,
            message: format!(
                "Invalid header dimensions or FPS: {}x{} @ {}fps",
                desc.width, desc.height, desc.fps
            ),
            file_or_part: Some("desc.txt".to_string()),
            line: Some(1),
            remediation: "Specify positive integer values for WIDTH, HEIGHT, and FPS.".to_string(),
            blocks_export: true,
        });
    } else if desc.fps > 120 {
        diagnostics.push(Diagnostic {
            code: "BA-DSC-002".to_string(),
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "Unusually high FPS ({}) may drop frames on Android boot surfaceflinger",
                desc.fps
            ),
            file_or_part: Some("desc.txt".to_string()),
            line: Some(1),
            remediation: "Use 30 or 60 FPS for reliable device playback.".to_string(),
            blocks_export: false,
        });
    }

    if desc.show_progress && !has_progress_font {
        diagnostics.push(Diagnostic {
            code: "BA-DSC-006".to_string(),
            severity: DiagnosticSeverity::Warning,
            message: "Header enables progress display ('p'), but 'progress_font.png' is not in the archive".to_string(),
            file_or_part: Some("desc.txt".to_string()),
            line: Some(1),
            remediation: "Include progress_font.png at the archive root or disable progress display.".to_string(),
            blocks_export: false,
        });
    }

    if !desc.preserved_directives.is_empty() {
        for unk in &desc.preserved_directives {
            diagnostics.push(Diagnostic {
                code: "BA-DSC-007".to_string(),
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "Unrecognized directive on line {}: '{}'",
                    unk.line_number, unk.raw_line
                ),
                file_or_part: Some("desc.txt".to_string()),
                line: Some(unk.line_number),
                remediation: "Remove the unsupported directive before rewriting desc.txt."
                    .to_string(),
                blocks_export: true,
            });
        }
    }

    if desc.parts.is_empty() {
        diagnostics.push(Diagnostic {
            code: "BA-DSC-003".to_string(),
            severity: DiagnosticSeverity::Error,
            message: "Descriptor declares no animation parts".to_string(),
            file_or_part: Some("desc.txt".to_string()),
            line: None,
            remediation: "Add at least one part declaration (e.g. 'p 1 0 part0').".to_string(),
            blocks_export: true,
        });
    }

    let declared_paths: HashSet<&str> = desc.parts.iter().map(|p| p.path.as_str()).collect();
    if let Some(dyn_colors) = &desc.dynamic_colors {
        if !declared_paths.contains(dyn_colors.part_path.as_str()) {
            diagnostics.push(Diagnostic {
                code: "BA-DSC-006".to_string(),
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "dynamic_colors references part '{}' which is not declared in desc.txt",
                    dyn_colors.part_path
                ),
                file_or_part: Some("desc.txt".to_string()),
                line: Some(2),
                remediation: "Point dynamic_colors to an existing part name.".to_string(),
                blocks_export: false,
            });
        }
    }

    for part in &desc.parts {
        if part.mode == PlaybackMode::SystemFallback {
            continue;
        }
        if part.mode == PlaybackMode::PlayToCompletion && part.repeat == RepeatCount::Infinite {
            diagnostics.push(Diagnostic {
                code: "BA-DSC-004".to_string(),
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "Part '{}' uses 'c' (play to completion) with COUNT=0 (infinite loop), which can prevent Android from finishing boot",
                    part.path
                ),
                file_or_part: Some(part.path.clone()),
                line: None,
                remediation: "Use 'p 0 ...' for infinite loops that should stop when boot completes, or set a finite COUNT on 'c' parts.".to_string(),
                blocks_export: false,
            });
        }

        if part.clock.is_some() && !has_clock_font {
            diagnostics.push(Diagnostic {
                code: "BA-DSC-006".to_string(),
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "Part '{}' specifies clock coordinates, but 'clock_font.png' is missing",
                    part.path
                ),
                file_or_part: Some(part.path.clone()),
                line: None,
                remediation: "Add clock_font.png at the archive root or remove clock coordinates."
                    .to_string(),
                blocks_export: false,
            });
        }

        match part_entries.get(&part.path) {
            Some(files) if !files.is_empty() => {
                if let Some(trim_text) = trim_entries.get(&part.path) {
                    let parsed_trims = parse_trim_lines(trim_text);
                    if parsed_trims.len() != files.len() {
                        diagnostics.push(Diagnostic {
                            code: "BA-TRM-001".to_string(),
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "Part '{}' trim.txt has {} lines, but part contains {} frames",
                                part.path,
                                parsed_trims.len(),
                                files.len()
                            ),
                            file_or_part: Some(format!("{}/trim.txt", part.path)),
                            line: None,
                            remediation: "Ensure trim.txt has exactly one WxH+X+Y line per frame in the part.".to_string(),
                            blocks_export: true,
                        });
                    }
                    for (idx, maybe_rect) in parsed_trims.iter().enumerate() {
                        match maybe_rect {
                            Some(rect) => {
                                if rect.x.saturating_add(rect.width) > desc.width
                                    || rect.y.saturating_add(rect.height) > desc.height
                                {
                                    diagnostics.push(Diagnostic {
                                        code: "BA-TRM-001".to_string(),
                                        severity: DiagnosticSeverity::Error,
                                        message: format!(
                                            "trim.txt line {} ({}) in '{}' exceeds animation bounds {}x{}",
                                            idx + 1,
                                            rect,
                                            part.path,
                                            desc.width,
                                            desc.height
                                        ),
                                        file_or_part: Some(format!("{}/trim.txt", part.path)),
                                        line: Some(idx + 1),
                                        remediation: "Ensure X+W <= WIDTH and Y+H <= HEIGHT for every trim rectangle.".to_string(),
                                        blocks_export: true,
                                    });
                                }
                            }
                            None => {
                                diagnostics.push(Diagnostic {
                                    code: "BA-TRM-001".to_string(),
                                    severity: DiagnosticSeverity::Error,
                                    message: format!(
                                        "Malformed trim.txt entry on line {} in '{}'",
                                        idx + 1,
                                        part.path
                                    ),
                                    file_or_part: Some(format!("{}/trim.txt", part.path)),
                                    line: Some(idx + 1),
                                    remediation: "Format each trim.txt line as WxH+X+Y (e.g. 500x300+100+200).".to_string(),
                                    blocks_export: true,
                                });
                            }
                        }
                    }
                }
            }
            _ => {
                diagnostics.push(Diagnostic {
                    code: "BA-DSC-005".to_string(),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "Part directory '{}' declared in desc.txt is missing or contains no frame images",
                        part.path
                    ),
                    file_or_part: Some(part.path.clone()),
                    line: None,
                    remediation: "Add frame images to the part directory or remove the unused part from desc.txt.".to_string(),
                    blocks_export: true,
                });
            }
        }
    }
}

fn validate_image_bytes(
    bytes: &[u8],
    entry_path: &str,
    expected_dims: (u32, u32),
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ext = Path::new(entry_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if FrameFormat::from_extension(ext).is_none() {
        diagnostics.push(Diagnostic {
            code: "BA-FRM-002".to_string(),
            severity: DiagnosticSeverity::Error,
            message: format!("Unsupported frame file extension in '{entry_path}'"),
            file_or_part: Some(entry_path.to_string()),
            line: None,
            remediation: "Use PNG (.png) or JPEG (.jpg) images for animation frames.".to_string(),
            blocks_export: true,
        });
        return;
    }

    match image::load_from_memory(bytes) {
        Ok(img) => {
            let (w, h) = img.dimensions();
            if (w, h) != expected_dims {
                diagnostics.push(Diagnostic {
                    code: "BA-FRM-003".to_string(),
                    severity: DiagnosticSeverity::Warning,
                    message: format!(
                        "Frame '{entry_path}' dimensions {w}x{h} do not match expected {}x{}",
                        expected_dims.0, expected_dims.1
                    ),
                    file_or_part: Some(entry_path.to_string()),
                    line: None,
                    remediation:
                        "Resize the frame explicitly or update desc.txt / trim.txt dimensions."
                            .to_string(),
                    blocks_export: false,
                });
            }
            let color = img.color();
            if matches!(
                color,
                ColorType::Rgb16 | ColorType::Rgba16 | ColorType::L16 | ColorType::La16
            ) {
                diagnostics.push(Diagnostic {
                    code: "BA-FRM-005".to_string(),
                    severity: DiagnosticSeverity::Warning,
                    message: format!(
                        "Frame '{entry_path}' uses 16-bit channel depth ({color:?}), which some Android boot renderers reject"
                    ),
                    file_or_part: Some(entry_path.to_string()),
                    line: None,
                    remediation: "Convert frames to 8-bit RGB or RGBA PNG.".to_string(),
                    blocks_export: false,
                });
            }
        }
        Err(err) => {
            diagnostics.push(Diagnostic {
                code: "BA-FRM-001".to_string(),
                severity: DiagnosticSeverity::Error,
                message: format!("Corrupt or unreadable image '{entry_path}': {err}"),
                file_or_part: Some(entry_path.to_string()),
                line: None,
                remediation: "Replace or re-export the corrupted frame image.".to_string(),
                blocks_export: true,
            });
        }
    }
}

fn compute_performance_summary(
    inspection: &ArchiveInspection,
    total_image_bytes: u64,
    diagnostics: &mut Vec<Diagnostic>,
) -> PerformanceSummary {
    let total_frames: usize = inspection.part_frame_counts.values().sum();
    let mut has_infinite_loop = false;
    let mut finite_frames_total: u64 = 0;
    let mut fps = 30u32;

    if let Some(desc) = &inspection.parsed_descriptor {
        fps = desc.fps.max(1);
        for part in &desc.parts {
            let part_frames = *inspection.part_frame_counts.get(&part.path).unwrap_or(&0) as u64;
            match part.repeat {
                RepeatCount::Infinite => {
                    has_infinite_loop = true;
                }
                RepeatCount::Finite(n) => {
                    finite_frames_total = finite_frames_total.saturating_add(
                        (part_frames.saturating_add(part.pause_frames as u64))
                            .saturating_mul(n as u64),
                    );
                }
            }
        }
        if desc.width * desc.height > 3840 * 2160 {
            diagnostics.push(Diagnostic {
                code: "BA-PRF-001".to_string(),
                severity: DiagnosticSeverity::Info,
                message: format!(
                    "Resolution {}x{} exceeds 4K UHD and may exhaust boot surface memory",
                    desc.width, desc.height
                ),
                file_or_part: Some("desc.txt".to_string()),
                line: Some(1),
                remediation: "Consider matching the target device panel resolution.".to_string(),
                blocks_export: false,
            });
        }
    }

    if total_image_bytes > 100 * 1024 * 1024 {
        diagnostics.push(Diagnostic {
            code: "BA-PRF-001".to_string(),
            severity: DiagnosticSeverity::Info,
            message: format!(
                "Animation image payload is large ({:.1} MiB across {} frames)",
                total_image_bytes as f64 / (1024.0 * 1024.0),
                total_frames
            ),
            file_or_part: None,
            line: None,
            remediation:
                "Consider lossless PNG optimization or trim.txt if boot playback stutters."
                    .to_string(),
            blocks_export: false,
        });
    }

    let finite_duration_seconds = if has_infinite_loop {
        None
    } else {
        Some(finite_frames_total as f64 / fps as f64)
    };

    PerformanceSummary {
        total_frames,
        total_image_bytes,
        archive_size_bytes: inspection.archive_size_bytes,
        per_part_frames: inspection.part_frame_counts.clone(),
        finite_duration_seconds,
        has_infinite_loop,
    }
}

fn parse_trim_lines(text: &str) -> Vec<Option<TrimRect>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(TrimRect::parse_line)
        .collect()
}

fn extract_line_from_error(msg: &str) -> Option<usize> {
    let marker = "line ";
    let idx = msg.find(marker)?;
    let rest = &msg[idx + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Helper to encode a small solid-color RGBA PNG in memory (useful for tests and fixtures).
pub fn encode_solid_png(width: u32, height: u32, rgba: [u8; 4]) -> Result<Vec<u8>, CoreError> {
    let mut img = image::RgbaImage::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = image::Rgba(rgba);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| CoreError::Archive(format!("png encode error: {e}")))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{write_stored_bootanimation_zip, ArchiveBuildEntry};
    use zip::CompressionMethod;

    #[test]
    fn validates_minimal_valid_archive() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("valid.zip");
        let png = encode_solid_png(64, 64, [255, 0, 0, 255]).unwrap();

        let entries = vec![
            ArchiveBuildEntry {
                archive_path: "desc.txt".to_string(),
                bytes: b"64 64 30\np 1 0 part0\n".to_vec(),
            },
            ArchiveBuildEntry {
                archive_path: "part0/00000.png".to_string(),
                bytes: png.clone(),
            },
            ArchiveBuildEntry {
                archive_path: "part0/00001.png".to_string(),
                bytes: png,
            },
        ];

        write_stored_bootanimation_zip(&entries, &zip_path).unwrap();
        let report = validate_archive_file(&zip_path).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Valid);
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.performance.total_frames, 2);
    }

    #[test]
    fn flags_compressed_entries_and_corrupt_png_and_c_infinite_loop() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("invalid.zip");
        {
            let f = File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated);
            zw.start_file("desc.txt", opts).unwrap();
            use std::io::Write;
            zw.write_all(b"64 64 30\nc 0 0 part0\n").unwrap();
            zw.start_file("part0/00000.png", opts).unwrap();
            zw.write_all(b"not-a-valid-png").unwrap();
            zw.finish().unwrap();
        }

        let report = validate_archive_file(&zip_path).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Invalid);
        let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"BA-ARC-001"),
            "must report compressed entry"
        );
        assert!(
            codes.contains(&"BA-DSC-004"),
            "must warn on c 0 infinite loop"
        );
        assert!(codes.contains(&"BA-FRM-001"), "must report corrupt PNG");
    }

    #[test]
    fn golden_fixtures_cover_trim_missing_dirs_and_fonts() {
        let dir = tempfile::tempdir().unwrap();
        let trimmed_zip = dir.path().join("trimmed.zip");
        let small_png = encode_solid_png(32, 32, [0, 255, 0, 255]).unwrap();
        let font_png = encode_solid_png(160, 144, [255, 255, 255, 255]).unwrap();

        // Valid trimmed animation with progress_font.png and clock_font.png
        let entries = vec![
            ArchiveBuildEntry {
                archive_path: "desc.txt".to_string(),
                bytes: b"64 64 30 p\np 1 5 part0 #000000 c -12\n".to_vec(),
            },
            ArchiveBuildEntry {
                archive_path: "clock_font.png".to_string(),
                bytes: font_png.clone(),
            },
            ArchiveBuildEntry {
                archive_path: "progress_font.png".to_string(),
                bytes: font_png,
            },
            ArchiveBuildEntry {
                archive_path: "part0/trim.txt".to_string(),
                bytes: b"32x32+16+16\n".to_vec(),
            },
            ArchiveBuildEntry {
                archive_path: "part0/00000.png".to_string(),
                bytes: small_png.clone(),
            },
        ];
        write_stored_bootanimation_zip(&entries, &trimmed_zip).unwrap();
        let report = validate_archive_file(&trimmed_zip).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Valid);

        // Invalid trim.txt out of bounds + missing part1 directory
        let bad_trim_zip = dir.path().join("bad_trim.zip");
        let bad_entries = vec![
            ArchiveBuildEntry {
                archive_path: "desc.txt".to_string(),
                bytes: b"64 64 30 p\np 1 0 part0\np 1 0 part1\n".to_vec(),
            },
            ArchiveBuildEntry {
                archive_path: "part0/trim.txt".to_string(),
                bytes: b"32x32+50+50\n".to_vec(),
            },
            ArchiveBuildEntry {
                archive_path: "part0/00000.png".to_string(),
                bytes: small_png,
            },
        ];
        write_stored_bootanimation_zip(&bad_entries, &bad_trim_zip).unwrap();
        let bad_report = validate_archive_file(&bad_trim_zip).unwrap();
        assert_eq!(bad_report.outcome, ValidationOutcome::Invalid);
        let codes: Vec<&str> = bad_report
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert!(codes.contains(&"BA-TRM-001"));
        assert!(codes.contains(&"BA-DSC-005"));
        assert!(codes.contains(&"BA-DSC-006"));
    }
}
