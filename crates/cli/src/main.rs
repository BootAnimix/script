use bootanimix_core::{
    build_playback_schedule, build_project_to_archive, detect_ffmpeg, export_magisk_module,
    export_preview_video_with_ffmpeg, import_archive_to_project, import_frame_sequence_dir,
    import_video_with_ffmpeg, inspect_zip_file, validate_archive_file, validate_project,
    FrameFormat, ProjectManifest, ResizeMode, RgbColor, ScheduleOptions, ValidationOutcome,
    VideoImportOptions,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "bootanimix",
    version,
    about = "BootAnimix 3.0 — Inspect, validate, import, edit, preview, and package Android boot animations"
)]
struct Cli {
    /// Emit machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Inspect a bootanimation.zip archive without modifying it
    Inspect {
        /// Path to bootanimation.zip
        archive: PathBuf,
    },
    /// Validate a bootanimation.zip archive or project directory
    Validate {
        /// Path to bootanimation.zip or project directory / project.bootanimix.json
        target: PathBuf,
    },
    /// Import a frame folder, existing bootanimation.zip, or video into a BootAnimix project
    Import {
        /// Source directory, bootanimation.zip, or video file
        source: PathBuf,
        /// Destination project directory
        #[arg(short, long)]
        output: PathBuf,
        /// Target FPS for frame-sequence or video import
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// Optional widthxheight for video import (e.g. 1080x2400)
        #[arg(long)]
        dimensions: Option<String>,
        /// Optional start time in seconds for video import
        #[arg(long)]
        start: Option<f64>,
        /// Optional end time in seconds for video import
        #[arg(long)]
        end: Option<f64>,
    },
    /// Build a validated, uncompressed (ZIP_STORED) bootanimation.zip from a project
    Build {
        /// Project directory or path to project.bootanimix.json
        project: PathBuf,
        /// Destination bootanimation.zip path
        #[arg(short, long)]
        output: PathBuf,
        /// Explicit resize WidthxHeight (e.g. 1080x2400)
        #[arg(long)]
        resize: Option<String>,
        /// Resize mode when --resize is used
        #[arg(long, value_enum, default_value_t = CliResizeMode::Contain)]
        resize_mode: CliResizeMode,
        /// Explicitly flatten alpha channel against #RRGGBB background
        #[arg(long)]
        flatten_alpha: Option<String>,
        /// Explicitly convert frames to png or jpg during build
        #[arg(long, value_enum)]
        format: Option<CliFrameFormat>,
        /// Apply verified lossless PNG recompression
        #[arg(long)]
        optimize_png: bool,
        /// Compute and emit trim.txt bounding boxes per part
        #[arg(long)]
        generate_trim: bool,
    },
    /// Generate a playback schedule or MP4 preview video from a project or archive
    Preview {
        /// Project directory or bootanimation.zip
        target: PathBuf,
        /// Optional output MP4 path (requires FFmpeg)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Maximum timeline ticks to schedule in preview window
        #[arg(long, default_value_t = 300)]
        max_ticks: usize,
    },
    /// Export artifacts such as a flashable Magisk module
    Export {
        #[command(subcommand)]
        kind: ExportSubcommand,
    },
    /// Show summary information about a BootAnimix project
    Info {
        /// Project directory or path to project.bootanimix.json
        project: PathBuf,
    },
    /// Check local environment and optional dependencies (such as FFmpeg)
    Doctor,
}

#[derive(Subcommand, Debug)]
enum ExportSubcommand {
    /// Package a project into a flashable Magisk module ZIP with a verified inner bootanimation.zip
    Magisk {
        /// Project directory or path to project.bootanimix.json
        project: PathBuf,
        /// Destination Magisk module ZIP path
        #[arg(short, long)]
        output: PathBuf,
        /// Override Magisk module ID (^[a-zA-Z][a-zA-Z0-9._-]+$)
        #[arg(long)]
        id: Option<String>,
        /// Override Magisk module display name
        #[arg(long)]
        name: Option<String>,
        /// Override system overlay target path (e.g. system/product/media/bootanimation.zip)
        #[arg(long)]
        target_path: Option<String>,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CliResizeMode {
    Stretch,
    Contain,
    Crop,
}

impl From<CliResizeMode> for ResizeMode {
    fn from(v: CliResizeMode) -> Self {
        match v {
            CliResizeMode::Stretch => ResizeMode::Stretch,
            CliResizeMode::Contain => ResizeMode::Contain,
            CliResizeMode::Crop => ResizeMode::Crop,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CliFrameFormat {
    Png,
    Jpg,
}

impl From<CliFrameFormat> for FrameFormat {
    fn from(v: CliFrameFormat) -> Self {
        match v {
            CliFrameFormat::Png => FrameFormat::Png,
            CliFrameFormat::Jpg => FrameFormat::Jpeg,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run_cli(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_cli(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Commands::Inspect { archive } => {
            let info = inspect_zip_file(&archive).map_err(|e| e.to_string())?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&info).unwrap());
            } else {
                println!("Archive: {}", archive.display());
                println!("Size:    {} bytes", info.archive_size_bytes);
                println!("Entries: {}", info.entries.len());
                println!("desc.txt present: {}", info.has_desc_txt);
                if let Some(desc) = &info.parsed_descriptor {
                    println!(
                        "Header:  {}x{} @ {} FPS (progress={})",
                        desc.width, desc.height, desc.fps, desc.show_progress
                    );
                    println!("Parts:   {}", desc.parts.len());
                    for p in &desc.parts {
                        let count = info.part_frame_counts.get(&p.path).copied().unwrap_or(0);
                        println!(
                            "  - {} (mode={:?}, repeat={:?}, pause={}f, frames={})",
                            p.path, p.mode, p.repeat, p.pause_frames, count
                        );
                    }
                }
                if !info.compressed_entries.is_empty() {
                    println!(
                        "WARNING: {} compressed entries found (must be Stored/0)!",
                        info.compressed_entries.len()
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Validate { target } => {
            let report = if target.is_dir()
                || target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|n| n.ends_with(".json"))
                    .unwrap_or(false)
            {
                let (manifest, proj_dir) = load_project_and_dir(&target)?;
                validate_project(&manifest, &proj_dir).map_err(|e| e.to_string())?
            } else {
                validate_archive_file(&target).map_err(|e| e.to_string())?
            };

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("Outcome: {:?}", report.outcome);
                println!(
                    "Frames:  {} ({} image bytes)",
                    report.performance.total_frames, report.performance.total_image_bytes
                );
                for d in &report.diagnostics {
                    println!(
                        "[{:?}] {} ({}): {} -> {}",
                        d.severity,
                        d.code,
                        d.file_or_part.as_deref().unwrap_or("-"),
                        d.message,
                        d.remediation
                    );
                }
            }

            if report.outcome == ValidationOutcome::Invalid {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Commands::Import {
            source,
            output,
            fps,
            dimensions,
            start,
            end,
        } => {
            if !source.exists() {
                return Err(format!("source path '{}' does not exist", source.display()));
            }
            let manifest = if source.is_dir() {
                import_frame_sequence_dir(&source, &output, fps).map_err(|e| e.to_string())?
            } else {
                let ext = source
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ext == "zip" {
                    import_archive_to_project(&source, &output).map_err(|e| e.to_string())?
                } else {
                    let parsed_dims = match dimensions {
                        Some(ref s) => Some(parse_dimensions_arg(s)?),
                        None => None,
                    };
                    let opts = VideoImportOptions {
                        target_fps: fps,
                        dimensions: parsed_dims,
                        start_seconds: start,
                        end_seconds: end,
                        output_format: FrameFormat::Png,
                    };
                    import_video_with_ffmpeg(&source, &output, &opts, None)
                        .map_err(|e| e.to_string())?
                }
            };

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&manifest).unwrap());
            } else {
                println!(
                    "Imported project '{}' into {}",
                    manifest.name,
                    output.display()
                );
                println!(
                    "Resolution: {}x{} @ {} FPS ({} parts)",
                    manifest.descriptor.width,
                    manifest.descriptor.height,
                    manifest.descriptor.fps,
                    manifest.parts.len()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Build {
            project,
            output,
            resize,
            resize_mode,
            flatten_alpha,
            format,
            optimize_png,
            generate_trim,
        } => {
            let (mut manifest, proj_dir) = load_project_and_dir(&project)?;
            if let Some(dim_str) = resize {
                let (w, h) = parse_dimensions_arg(&dim_str)?;
                manifest.transforms.resize = Some((w, h, resize_mode.into()));
            }
            if let Some(hex) = flatten_alpha {
                let color = RgbColor::parse_hex(&hex)
                    .ok_or_else(|| format!("invalid hex color '{hex}' (expected #RRGGBB)"))?;
                manifest.transforms.flatten_alpha_to = Some(color);
            }
            if let Some(fmt) = format {
                manifest.transforms.convert_format = Some(fmt.into());
            }
            if optimize_png {
                manifest.transforms.lossless_optimize_png = true;
            }
            if generate_trim {
                manifest.transforms.generate_trim_txt = true;
            }

            let report = build_project_to_archive(&manifest, &proj_dir, &output)
                .map_err(|e| e.to_string())?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("Built archive: {}", report.output_path);
                println!(
                    "Frames: {} across {} parts ({} bytes, ZIP_STORED verified={})",
                    report.frame_count,
                    report.part_count,
                    report.output_size_bytes,
                    report.invariants_verified
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Preview {
            target,
            output,
            max_ticks,
        } => {
            let temp_proj;
            let (manifest, proj_dir) = if target
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("zip"))
                .unwrap_or(false)
            {
                temp_proj = tempfile::tempdir().map_err(|e| e.to_string())?;
                let m = import_archive_to_project(&target, temp_proj.path())
                    .map_err(|e| e.to_string())?;
                (m, temp_proj.path().to_path_buf())
            } else {
                load_project_and_dir(&target)?
            };

            let schedule = if let Some(mp4_path) = output {
                export_preview_video_with_ffmpeg(&manifest, &proj_dir, &mp4_path, max_ticks, None)
                    .map_err(|e| e.to_string())?
            } else {
                build_playback_schedule(
                    &manifest,
                    ScheduleOptions {
                        max_window_ticks: max_ticks,
                        selected_part_only: None,
                        simulated_boot_exit_tick: None,
                    },
                )
            };

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&schedule).unwrap());
            } else {
                println!(
                    "Preview schedule: {}x{} @ {} FPS ({} ticks materialized, infinite={})",
                    schedule.width,
                    schedule.height,
                    schedule.fps,
                    schedule.frames.len(),
                    schedule.is_infinite
                );
                for warn in &schedule.fidelity_warnings {
                    println!("Note: {warn}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Export { kind } => match kind {
            ExportSubcommand::Magisk {
                project,
                output,
                id,
                name,
                target_path,
            } => {
                let (mut manifest, proj_dir) = load_project_and_dir(&project)?;
                if let Some(id_val) = id {
                    manifest.magisk.module_id = id_val;
                }
                if let Some(name_val) = name {
                    manifest.magisk.module_name = name_val;
                }
                if let Some(tp_val) = target_path {
                    manifest.magisk.target_path = tp_val;
                }
                let report = export_magisk_module(&manifest, &proj_dir, &output)
                    .map_err(|e| e.to_string())?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&report).unwrap());
                } else {
                    println!("Exported Magisk module: {}", report.module_zip_path);
                    println!("Module ID:              {}", report.module_id);
                    println!("Overlay target:         /{}", report.target_overlay_path);
                    println!("Inner Stored verified:  {}", report.inner_stored_verified);
                }
                Ok(ExitCode::SUCCESS)
            }
        },
        Commands::Info { project } => {
            let (manifest, proj_dir) = load_project_and_dir(&project)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&manifest).unwrap());
            } else {
                println!("Project:    {} ({})", manifest.name, proj_dir.display());
                println!(
                    "Resolution: {}x{} @ {} FPS",
                    manifest.descriptor.width, manifest.descriptor.height, manifest.descriptor.fps
                );
                println!("Parts:      {}", manifest.parts.len());
                for p in &manifest.parts {
                    println!("  - {}: {} frames", p.name, p.frames.len());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Commands::Doctor => {
            let ff = detect_ffmpeg(None);
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&ff).unwrap());
            } else {
                println!("BootAnimix 3.0 Doctor");
                println!(
                    "Core schema version: {}",
                    bootanimix_core::CURRENT_SCHEMA_VERSION
                );
                if ff.available {
                    println!(
                        "FFmpeg:              available ({})",
                        ff.version_line.as_deref().unwrap_or(&ff.binary_path)
                    );
                } else {
                    println!(
                        "FFmpeg:              NOT FOUND (video import/preview MP4 disabled; archive/frame workflows available)"
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn load_project_and_dir(path: &Path) -> Result<(ProjectManifest, PathBuf), String> {
    if path.is_dir() {
        let file = path.join("project.bootanimix.json");
        let manifest = ProjectManifest::load_from_file(&file).map_err(|e| e.to_string())?;
        Ok((manifest, path.to_path_buf()))
    } else {
        let manifest = ProjectManifest::load_from_file(path).map_err(|e| e.to_string())?;
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Ok((manifest, dir))
    }
}

fn parse_dimensions_arg(raw: &str) -> Result<(u32, u32), String> {
    let (w_s, h_s) = raw
        .split_once('x')
        .or_else(|| raw.split_once('X'))
        .ok_or_else(|| {
            format!("invalid dimensions '{raw}' (expected WIDTHxHEIGHT, e.g. 1080x2400)")
        })?;
    let w: u32 = w_s
        .parse()
        .map_err(|_| format!("invalid width in '{raw}'"))?;
    let h: u32 = h_s
        .parse()
        .map_err(|_| format!("invalid height in '{raw}'"))?;
    if w == 0 || h == 0 {
        return Err("dimensions must be > 0".to_string());
    }
    Ok((w, h))
}
