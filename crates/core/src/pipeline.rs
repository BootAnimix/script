use crate::archive::{
    inspect_zip_file, safe_extract_zip, write_stored_bootanimation_zip, ArchiveBuildEntry,
};
use crate::descriptor::{parse_descriptor_lenient, serialize_descriptor};
use crate::error::CoreError;
use crate::frames::{normalized_frame_name, scan_directory_frames, FrameFormat};
use crate::manifest::{
    FrameAssetRef, MagiskExportSettings, ProjectManifest, ProjectPart, ResizeMode,
    TransformSettings, CURRENT_SCHEMA_VERSION,
};
use crate::model::{
    AnimationDescriptor, PartDeclaration, PlaybackMode, RepeatCount, RgbColor, TrimRect,
};
use crate::validator::{
    validate_archive_file, validate_project, ValidationOutcome, ValidationReport,
};
use image::codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageEncoder, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfmpegInfo {
    pub available: bool,
    pub binary_path: String,
    pub version_line: Option<String>,
}

/// Detect whether `ffmpeg` is installed and inspect its version banner safely via `Command::new`.
pub fn detect_ffmpeg(custom_binary: Option<&str>) -> FfmpegInfo {
    let bin = custom_binary.unwrap_or("ffmpeg");
    match Command::new(bin).arg("-version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let first_line = stdout.lines().next().map(|s| s.trim().to_string());
            FfmpegInfo {
                available: true,
                binary_path: bin.to_string(),
                version_line: first_line,
            }
        }
        _ => FfmpegInfo {
            available: false,
            binary_path: bin.to_string(),
            version_line: None,
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoImportOptions {
    pub target_fps: u32,
    pub dimensions: Option<(u32, u32)>,
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
    pub output_format: FrameFormat,
}

impl Default for VideoImportOptions {
    fn default() -> Self {
        Self {
            target_fps: 30,
            dimensions: None,
            start_seconds: None,
            end_seconds: None,
            output_format: FrameFormat::Png,
        }
    }
}

/// Import a frame sequence directory into a new BootAnimix project folder without modifying source files.
pub fn import_frame_sequence_dir(
    source_dir: &Path,
    project_dir: &Path,
    fps: u32,
) -> Result<ProjectManifest, CoreError> {
    if !source_dir.exists() || !source_dir.is_dir() {
        return Err(CoreError::Pipeline(format!(
            "source directory '{}' does not exist",
            source_dir.display()
        )));
    }

    // Explicitly reject animated GIF / WebP files rather than silently importing only frame 0.
    for entry in std::fs::read_dir(source_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                let lower = ext.to_ascii_lowercase();
                if lower == "gif" || lower == "webp" {
                    return Err(CoreError::Pipeline(format!(
                        "animated or unsupported image format '.{lower}' in '{}'; extract frames explicitly or use video import",
                        entry.path().display()
                    )));
                }
            }
        }
    }

    let (ordered_paths, seq_report) = scan_directory_frames(source_dir)?;
    let first_bytes = std::fs::read(&ordered_paths[0])?;
    let first_img = image::load_from_memory(&first_bytes)
        .map_err(|e| CoreError::Pipeline(format!("failed to decode first frame: {e}")))?;
    let (width, height) = first_img.dimensions();

    std::fs::create_dir_all(project_dir.join("assets").join("part0"))?;
    let mut frame_refs = Vec::with_capacity(ordered_paths.len());

    for src_path in &ordered_paths {
        let file_name = src_path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CoreError::Pipeline("invalid source filename".to_string()))?;
        let rel = format!("assets/part0/{file_name}");
        let dest = project_dir.join(&rel);
        std::fs::copy(src_path, &dest)?;
        frame_refs.push(FrameAssetRef {
            relative_path: rel,
            trim: None,
        });
    }

    let project_name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("bootanimation_project")
        .to_string();

    let manifest = ProjectManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: project_name,
        descriptor: AnimationDescriptor {
            width,
            height,
            fps: fps.max(1),
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
            frames: frame_refs,
            audio_relative_path: None,
        }],
        transforms: TransformSettings::default(),
        magisk: MagiskExportSettings::default(),
        source_archive: None,
        import_warnings: seq_report.warnings,
    };

    manifest.save_to_file(&project_dir.join("project.bootanimix.json"))?;
    Ok(manifest)
}

/// Import an existing `bootanimation.zip` archive into a local editable project directory.
pub fn import_archive_to_project(
    archive_path: &Path,
    project_dir: &Path,
) -> Result<ProjectManifest, CoreError> {
    let inspection = inspect_zip_file(archive_path)?;
    if !inspection.has_desc_txt {
        return Err(CoreError::Pipeline(
            "archive does not contain desc.txt at root".to_string(),
        ));
    }

    let assets_dir = project_dir.join("assets");
    safe_extract_zip(archive_path, &assets_dir)?;

    let desc_text = std::fs::read_to_string(assets_dir.join("desc.txt"))?;
    let descriptor = parse_descriptor_lenient(&desc_text)?;

    let mut project_parts = Vec::new();
    let mut warnings = Vec::new();
    if !inspection.compressed_entries.is_empty() {
        warnings.push(format!(
            "source archive had {} compressed entries; next build will normalize to ZIP_STORED",
            inspection.compressed_entries.len()
        ));
    }

    for part in &descriptor.parts {
        if part.mode == PlaybackMode::SystemFallback {
            continue;
        }
        let part_dir = assets_dir.join(&part.path);
        if !part_dir.exists() {
            warnings.push(format!("declared part directory '{}' is missing", part.path));
            continue;
        }

        let (ordered_paths, seq_report) = scan_directory_frames(&part_dir)?;
        warnings.extend(seq_report.warnings);

        let trim_path = part_dir.join("trim.txt");
        let trims: Vec<Option<TrimRect>> = if trim_path.exists() {
            std::fs::read_to_string(&trim_path)?
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(TrimRect::parse_line)
                .collect()
        } else {
            Vec::new()
        };

        let mut frame_refs = Vec::with_capacity(ordered_paths.len());
        for (idx, frame_path) in ordered_paths.iter().enumerate() {
            let file_name = frame_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("frame.png");
            frame_refs.push(FrameAssetRef {
                relative_path: format!("assets/{}/{}", part.path, file_name),
                trim: trims.get(idx).copied().flatten(),
            });
        }

        let audio_rel = if part_dir.join("audio.wav").exists() {
            Some(format!("assets/{}/audio.wav", part.path))
        } else {
            None
        };

        // Copy optional fonts to project root if present
        for font_name in ["clock_font.png", "progress_font.png"] {
            let src_font = assets_dir.join(font_name);
            if src_font.exists() {
                let _ = std::fs::copy(&src_font, project_dir.join(font_name));
            }
        }

        project_parts.push(ProjectPart {
            name: part.path.clone(),
            frames: frame_refs,
            audio_relative_path: audio_rel,
        });
    }

    let project_name = archive_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported_animation")
        .to_string();

    let manifest = ProjectManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: project_name,
        descriptor,
        parts: project_parts,
        transforms: TransformSettings::default(),
        magisk: MagiskExportSettings::default(),
        source_archive: Some(archive_path.display().to_string()),
        import_warnings: warnings,
    };

    manifest.save_to_file(&project_dir.join("project.bootanimix.json"))?;
    Ok(manifest)
}

/// Import a video file into a BootAnimix project using FFmpeg without constructing shell strings.
pub fn import_video_with_ffmpeg(
    video_path: &Path,
    project_dir: &Path,
    options: &VideoImportOptions,
    ffmpeg_bin: Option<&str>,
) -> Result<ProjectManifest, CoreError> {
    if !video_path.exists() {
        return Err(CoreError::Pipeline(format!(
            "video file '{}' not found",
            video_path.display()
        )));
    }
    let info = detect_ffmpeg(ffmpeg_bin);
    if !info.available {
        return Err(CoreError::Ffmpeg(
            "ffmpeg executable not found in PATH; install FFmpeg to import video files".to_string(),
        ));
    }
    if options.target_fps == 0 || options.target_fps > 240 {
        return Err(CoreError::Pipeline(format!(
            "invalid target_fps {} for video import",
            options.target_fps
        )));
    }

    let frames_dir = project_dir.join("assets").join("part0");
    std::fs::create_dir_all(&frames_dir)?;

    let mut cmd = Command::new(&info.binary_path);
    cmd.arg("-hide_banner").arg("-loglevel").arg("error").arg("-y");

    if let Some(start) = options.start_seconds {
        if start >= 0.0 {
            cmd.arg("-ss").arg(format!("{start:.3}"));
        }
    }
    if let Some(end) = options.end_seconds {
        if end > 0.0 {
            cmd.arg("-to").arg(format!("{end:.3}"));
        }
    }

    cmd.arg("-i").arg(video_path);

    let mut vf_chain = vec![format!("fps={}", options.target_fps)];
    if let Some((w, h)) = options.dimensions {
        vf_chain.push(format!("scale={w}:{h}:flags=lanczos"));
    }
    cmd.arg("-vf").arg(vf_chain.join(","));

    let out_pattern = frames_dir.join(format!(
        "%05d.{}",
        options.output_format.canonical_extension()
    ));
    cmd.arg(&out_pattern);

    let output = cmd
        .output()
        .map_err(|e| CoreError::Ffmpeg(format!("failed to spawn ffmpeg: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CoreError::Ffmpeg(format!(
            "ffmpeg exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let (ordered_paths, seq_report) = scan_directory_frames(&frames_dir)?;
    let first_bytes = std::fs::read(&ordered_paths[0])?;
    let first_img = image::load_from_memory(&first_bytes)
        .map_err(|e| CoreError::Pipeline(format!("failed to decode extracted frame: {e}")))?;
    let (width, height) = first_img.dimensions();

    let frame_refs: Vec<FrameAssetRef> = ordered_paths
        .iter()
        .filter_map(|p| {
            p.file_name().and_then(|s| s.to_str()).map(|name| FrameAssetRef {
                relative_path: format!("assets/part0/{name}"),
                trim: None,
            })
        })
        .collect();

    let manifest = ProjectManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: video_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video_import")
            .to_string(),
        descriptor: AnimationDescriptor {
            width,
            height,
            fps: options.target_fps,
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
            frames: frame_refs,
            audio_relative_path: None,
        }],
        transforms: TransformSettings::default(),
        magisk: MagiskExportSettings::default(),
        source_archive: None,
        import_warnings: seq_report.warnings,
    };

    manifest.save_to_file(&project_dir.join("project.bootanimix.json"))?;
    Ok(manifest)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildReport {
    pub output_path: String,
    pub output_size_bytes: u64,
    pub original_image_bytes: u64,
    pub final_image_bytes: u64,
    pub frame_count: usize,
    pub part_count: usize,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub transformations_applied: Vec<String>,
    pub validation: ValidationReport,
    pub invariants_verified: bool,
}

/// Build a validated, uncompressed (`ZIP_STORED`) `bootanimation.zip` from a `ProjectManifest`.
/// Source files are never modified; any requested transformations are computed in memory/staging.
pub fn build_project_to_archive(
    manifest: &ProjectManifest,
    project_dir: &Path,
    dest_zip: &Path,
) -> Result<BuildReport, CoreError> {
    let pre_validation = validate_project(manifest, project_dir)?;
    if pre_validation.outcome == ValidationOutcome::Invalid {
        let blocking: Vec<String> = pre_validation
            .diagnostics
            .iter()
            .filter(|d| d.blocks_export)
            .map(|d| format!("[{}] {}", d.code, d.message))
            .collect();
        return Err(CoreError::Pipeline(format!(
            "project validation failed before build: {}",
            blocking.join("; ")
        )));
    }

    let mut effective_desc = manifest.descriptor.clone();
    let mut transformations_applied = Vec::new();
    if let Some((rw, rh, mode)) = manifest.transforms.resize {
        effective_desc.width = rw;
        effective_desc.height = rh;
        transformations_applied.push(format!("resize:{rw}x{rh}:{mode:?}"));
    }
    if let Some(bg) = manifest.transforms.flatten_alpha_to {
        transformations_applied.push(format!("flatten_alpha:{bg}"));
    }
    if let Some(fmt) = manifest.transforms.convert_format {
        transformations_applied.push(format!("convert_format:{fmt:?}"));
    }
    if let Some(fps) = manifest.transforms.target_fps {
        effective_desc.fps = fps.max(1);
        transformations_applied.push(format!("target_fps:{fps}"));
    }
    if manifest.transforms.lossless_optimize_png {
        transformations_applied.push("lossless_optimize_png".to_string());
    }
    if manifest.transforms.generate_trim_txt {
        transformations_applied.push("generate_trim_txt".to_string());
    }

    let desc_bytes = serialize_descriptor(&effective_desc)?.into_bytes();
    let mut archive_entries = vec![ArchiveBuildEntry {
        archive_path: "desc.txt".to_string(),
        bytes: desc_bytes,
    }];

    for font_name in ["clock_font.png", "progress_font.png"] {
        let font_path = project_dir.join(font_name);
        if font_path.exists() {
            archive_entries.push(ArchiveBuildEntry {
                archive_path: font_name.to_string(),
                bytes: std::fs::read(font_path)?,
            });
        }
    }

    let mut original_image_bytes: u64 = 0;
    let mut final_image_bytes: u64 = 0;
    let mut total_frames = 0usize;

    for part in &manifest.parts {
        let total_part_frames = part.frames.len();
        let mut trim_lines: Vec<String> = Vec::new();
        let mut has_any_trim = manifest.transforms.generate_trim_txt;

        let part_bg = effective_desc
            .parts
            .iter()
            .find(|p| p.path == part.name)
            .and_then(|p| p.background_color)
            .unwrap_or(RgbColor::new(0, 0, 0));

        for (idx, frame_ref) in part.frames.iter().enumerate() {
            let src_path = ProjectManifest::resolve_asset_path(project_dir, &frame_ref.relative_path)?;
            let src_bytes = std::fs::read(&src_path)?;
            original_image_bytes = original_image_bytes.saturating_add(src_bytes.len() as u64);

            let src_ext = src_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("png");
            let src_format = FrameFormat::from_extension(src_ext).unwrap_or(FrameFormat::Png);
            let target_format = manifest.transforms.convert_format.unwrap_or(src_format);

            let needs_pixel_op = manifest.transforms.resize.is_some()
                || manifest.transforms.flatten_alpha_to.is_some()
                || manifest.transforms.convert_format.is_some()
                || manifest.transforms.lossless_optimize_png
                || manifest.transforms.generate_trim_txt;

            let (out_bytes, computed_trim) = if !needs_pixel_op {
                // Preserve exact source bytes when no transformation is requested!
                (src_bytes, frame_ref.trim)
            } else {
                process_frame_Pipeline(
                    &src_bytes,
                    &manifest.transforms,
                    part_bg,
                    target_format,
                    frame_ref.trim,
                )?
            };

            if let Some(t) = computed_trim {
                has_any_trim = true;
                trim_lines.push(t.to_string());
            } else {
                trim_lines.push(format!(
                    "{}x{}+0+0",
                    effective_desc.width, effective_desc.height
                ));
            }

            final_image_bytes = final_image_bytes.saturating_add(out_bytes.len() as u64);
            let norm_name = normalized_frame_name(idx, total_part_frames, target_format);
            archive_entries.push(ArchiveBuildEntry {
                archive_path: format!("{}/{}", part.name, norm_name),
                bytes: out_bytes,
            });
            total_frames += 1;
        }

        if has_any_trim && !trim_lines.is_empty() {
            let mut trim_body = trim_lines.join("\n");
            trim_body.push('\n');
            archive_entries.push(ArchiveBuildEntry {
                archive_path: format!("{}/trim.txt", part.name),
                bytes: trim_body.into_bytes(),
            });
        }

        if let Some(audio_rel) = &part.audio_relative_path {
            let audio_path = ProjectManifest::resolve_asset_path(project_dir, audio_rel)?;
            if audio_path.exists() {
                archive_entries.push(ArchiveBuildEntry {
                    archive_path: format!("{}/audio.wav", part.name),
                    bytes: std::fs::read(audio_path)?,
                });
            }
        }
    }

    let inspection = write_stored_bootanimation_zip(&archive_entries, dest_zip)?;
    let post_validation = validate_archive_file(dest_zip)?;

    let invariants_verified = inspection.compressed_entries.is_empty()
        && inspection.has_desc_txt
        && inspection.unsafe_paths.is_empty()
        && inspection.duplicate_entries.is_empty()
        && post_validation.outcome != ValidationOutcome::Invalid;

    if !invariants_verified {
        return Err(CoreError::Pipeline(
            "built archive failed post-build validation invariants".to_string(),
        ));
    }

    Ok(BuildReport {
        output_path: dest_zip.display().to_string(),
        output_size_bytes: inspection.archive_size_bytes,
        original_image_bytes,
        final_image_bytes,
        frame_count: total_frames,
        part_count: manifest.parts.len(),
        width: effective_desc.width,
        height: effective_desc.height,
        fps: effective_desc.fps,
        transformations_applied,
        validation: post_validation,
        invariants_verified,
    })
}

#[allow(non_snake_case)]
fn process_frame_Pipeline(
    src_bytes: &[u8],
    transforms: &TransformSettings,
    part_bg: RgbColor,
    target_format: FrameFormat,
    existing_trim: Option<TrimRect>,
) -> Result<(Vec<u8>, Option<TrimRect>), CoreError> {
    let decoded = image::load_from_memory(src_bytes)
        .map_err(|e| CoreError::Pipeline(format!("image decode error: {e}")))?;
    let mut rgba = decoded.to_rgba8();

    if let Some((target_w, target_h, mode)) = transforms.resize {
        rgba = apply_resize(&rgba, target_w, target_h, mode, transforms.flatten_alpha_to);
    }

    if let Some(bg) = transforms.flatten_alpha_to {
        flatten_alpha_in_place(&mut rgba, bg);
    }

    let mut computed_trim = existing_trim;
    if transforms.generate_trim_txt {
        let (trimmed_img, rect) = compute_trimmed_subimage(&rgba, part_bg);
        rgba = trimmed_img;
        computed_trim = Some(rect);
    }

    let is_pure_lossless_png = transforms.resize.is_none()
        && transforms.flatten_alpha_to.is_none()
        && !transforms.generate_trim_txt
        && target_format == FrameFormat::Png;

    let encoded = match target_format {
        FrameFormat::Png => {
            let mut out = Vec::new();
            let compression = if transforms.lossless_optimize_png {
                CompressionType::Best
            } else {
                CompressionType::Default
            };
            let encoder = PngEncoder::new_with_quality(&mut out, compression, PngFilterType::Adaptive);
            encoder
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| CoreError::Pipeline(format!("png encode error: {e}")))?;

            if is_pure_lossless_png {
                let redecoded = image::load_from_memory(&out)
                    .map_err(|e| CoreError::Pipeline(format!("png verify error: {e}")))?
                    .to_rgba8();
                if redecoded.as_raw() != rgba.as_raw() {
                    return Err(CoreError::Pipeline(
                        "lossless PNG verification failed: decoded pixels differed!".to_string(),
                    ));
                }
            }
            out
        }
        FrameFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();
            let mut cursor = std::io::Cursor::new(Vec::new());
            rgb.write_to(&mut cursor, image::ImageFormat::Jpeg)
                .map_err(|e| CoreError::Pipeline(format!("jpeg encode error: {e}")))?;
            cursor.into_inner()
        }
    };

    Ok((encoded, computed_trim))
}

fn apply_resize(
    src: &RgbaImage,
    target_w: u32,
    target_h: u32,
    mode: ResizeMode,
    bg_fill: Option<RgbColor>,
) -> RgbaImage {
    let (sw, sh) = src.dimensions();
    if (sw, sh) == (target_w, target_h) {
        return src.clone();
    }
    match mode {
        ResizeMode::Stretch => image::imageops::resize(src, target_w, target_h, FilterType::Lanczos3),
        ResizeMode::Contain => {
            let scale = f64::min(target_w as f64 / sw as f64, target_h as f64 / sh as f64);
            let nw = ((sw as f64 * scale).round() as u32).max(1);
            let nh = ((sh as f64 * scale).round() as u32).max(1);
            let scaled = image::imageops::resize(src, nw, nh, FilterType::Lanczos3);
            let fill = bg_fill
                .map(|c| Rgba([c.r, c.g, c.b, 255]))
                .unwrap_or(Rgba([0, 0, 0, 255]));
            let mut canvas = RgbaImage::from_pixel(target_w, target_h, fill);
            let ox = ((target_w.saturating_sub(nw)) / 2) as i64;
            let oy = ((target_h.saturating_sub(nh)) / 2) as i64;
            image::imageops::overlay(&mut canvas, &scaled, ox, oy);
            canvas
        }
        ResizeMode::Crop => {
            let scale = f64::max(target_w as f64 / sw as f64, target_h as f64 / sh as f64);
            let nw = ((sw as f64 * scale).round() as u32).max(target_w);
            let nh = ((sh as f64 * scale).round() as u32).max(target_h);
            let scaled = image::imageops::resize(src, nw, nh, FilterType::Lanczos3);
            let cx = (nw.saturating_sub(target_w)) / 2;
            let cy = (nh.saturating_sub(target_h)) / 2;
            image::imageops::crop_imm(&scaled, cx, cy, target_w, target_h).to_image()
        }
    }
}

fn flatten_alpha_in_place(img: &mut RgbaImage, bg: RgbColor) {
    for p in img.pixels_mut() {
        let a = p[3] as u32;
        let inv = 255 - a;
        p[0] = ((p[0] as u32 * a + bg.r as u32 * inv) / 255) as u8;
        p[1] = ((p[1] as u32 * a + bg.g as u32 * inv) / 255) as u8;
        p[2] = ((p[2] as u32 * a + bg.b as u32 * inv) / 255) as u8;
        p[3] = 255;
    }
}

/// Compute trimmed bounding box against `bg` color according to AOSP `trim.txt` (`WxH+X+Y`).
pub fn compute_trimmed_subimage(img: &RgbaImage, bg: RgbColor) -> (RgbaImage, TrimRect) {
    let (w, h) = img.dimensions();
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;

    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            let is_bg = (p[3] == 0) || (p[0] == bg.r && p[1] == bg.g && p[2] == bg.b);
            if !is_bg {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !found {
        let single = image::imageops::crop_imm(img, 0, 0, 1, 1).to_image();
        return (
            single,
            TrimRect {
                width: 1,
                height: 1,
                x: 0,
                y: 0,
            },
        );
    }

    let tw = max_x - min_x + 1;
    let th = max_y - min_y + 1;
    let sub = image::imageops::crop_imm(img, min_x, min_y, tw, th).to_image();
    (
        sub,
        TrimRect {
            width: tw,
            height: th,
            x: min_x,
            y: min_y,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::encode_solid_png;

    #[test]
    fn imports_frame_sequence_and_builds_stored_archive_without_touching_source() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("raw_frames");
        std::fs::create_dir_all(&src_dir).unwrap();

        let png_a = encode_solid_png(64, 64, [20, 40, 60, 255]).unwrap();
        let png_b = encode_solid_png(64, 64, [80, 100, 120, 255]).unwrap();
        std::fs::write(src_dir.join("frame_2.png"), &png_b).unwrap();
        std::fs::write(src_dir.join("frame_1.png"), &png_a).unwrap();

        let proj_dir = dir.path().join("my_proj");
        let manifest = import_frame_sequence_dir(&src_dir, &proj_dir, 60).unwrap();
        assert_eq!(manifest.parts[0].frames.len(), 2);
        assert!(manifest.parts[0].frames[0]
            .relative_path
            .ends_with("frame_1.png"));

        let out_zip = dir.path().join("bootanimation.zip");
        let report = build_project_to_archive(&manifest, &proj_dir, &out_zip).unwrap();
        assert!(report.invariants_verified);
        assert_eq!(report.frame_count, 2);
        assert_eq!(report.original_image_bytes, report.final_image_bytes);

        // Source files untouched!
        assert_eq!(std::fs::read(src_dir.join("frame_1.png")).unwrap(), png_a);
        assert_eq!(std::fs::read(src_dir.join("frame_2.png")).unwrap(), png_b);
    }

    #[test]
    fn missing_ffmpeg_and_ffmpeg_video_import_work() {
        let dir = tempfile::tempdir().unwrap();
        let fake_vid = dir.path().join("sample.mp4");
        std::fs::write(&fake_vid, b"dummy").unwrap();

        let opts = VideoImportOptions::default();
        let missing_err = import_video_with_ffmpeg(
            &fake_vid,
            &dir.path().join("proj_missing"),
            &opts,
            Some("nonexistent_ffmpeg_binary_xyz"),
        )
        .unwrap_err();
        assert!(missing_err.to_string().contains("not found"));

        let ff = detect_ffmpeg(None);
        if ff.available {
            let synth_mp4 = dir.path().join("synth.mp4");
            let status = Command::new(&ff.binary_path)
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=64x64:r=10:d=0.3",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&synth_mp4)
                .status()
                .unwrap();
            assert!(status.success());

            let vproj = dir.path().join("vproj");
            let vopts = VideoImportOptions {
                target_fps: 10,
                dimensions: Some((64, 64)),
                start_seconds: None,
                end_seconds: None,
                output_format: FrameFormat::Png,
            };
            let manifest = import_video_with_ffmpeg(&synth_mp4, &vproj, &vopts, None).unwrap();
            assert!(!manifest.parts[0].frames.is_empty());
            assert_eq!(manifest.descriptor.width, 64);
            assert_eq!(manifest.descriptor.fps, 10);
        }
    }

    #[test]
    fn explicit_transforms_and_trim_generation_work_and_pass_validator() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("raw");
        std::fs::create_dir_all(&src_dir).unwrap();

        let mut img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 255]));
        for y in 16..32 {
            for x in 20..40 {
                img.put_pixel(x, y, Rgba([255, 128, 0, 255]));
            }
        }
        let mut raw_png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut raw_png, image::ImageFormat::Png).unwrap();
        std::fs::write(src_dir.join("000.png"), raw_png.into_inner()).unwrap();

        let proj_dir = dir.path().join("proj");
        let mut manifest = import_frame_sequence_dir(&src_dir, &proj_dir, 30).unwrap();
        manifest.transforms.lossless_optimize_png = true;
        manifest.transforms.generate_trim_txt = true;

        let out_zip = dir.path().join("trimmed_boot.zip");
            // Verify source files on disk before building
            let report = build_project_to_archive(&manifest, &proj_dir, &out_zip).unwrap();
            assert!(report.invariants_verified);
            assert_eq!(report.validation.outcome, ValidationOutcome::Valid);

            // Round-trip import of the built archive into a fresh project dir
            let reimported_dir = dir.path().join("reimported");
            let reimported = import_archive_to_project(&out_zip, &reimported_dir).unwrap();
            assert_eq!(reimported.descriptor.width, 64);
            assert_eq!(reimported.parts[0].frames.len(), 1);
            assert_eq!(
                reimported.parts[0].frames[0].trim,
                Some(TrimRect {
                    width: 20,
                    height: 16,
                    x: 20,
                    y: 16
                })
            );
    }
}
