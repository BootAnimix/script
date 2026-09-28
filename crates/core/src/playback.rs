use crate::error::CoreError;
use crate::manifest::ProjectManifest;
use crate::model::{PlaybackMode, RepeatCount, RgbColor, TrimRect};
use crate::pipeline::detect_ffmpeg;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledFrame {
    pub timeline_tick: usize,
    pub timestamp_seconds: f64,
    pub part_index: usize,
    pub part_name: String,
    pub loop_iteration: u32,
    pub frame_index_in_part: usize,
    pub relative_path: String,
    pub is_pause_hold: bool,
    pub fade_opacity: f32,
    pub background_color: RgbColor,
    pub trim: Option<TrimRect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSchedule {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub is_infinite: bool,
    /// Only populated when `is_infinite == false`. Never invents a fake finite duration for infinite loops.
    pub finite_duration_seconds: Option<f64>,
    pub window_capped: bool,
    pub frames: Vec<ScheduledFrame>,
    pub fidelity_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleOptions {
    /// Maximum number of timeline ticks to materialize in the preview window (e.g. 600 ticks).
    pub max_window_ticks: usize,
    /// Optional filter to preview only a single part index.
    pub selected_part_only: Option<usize>,
    /// Optional simulated tick at which `service.bootanim.exit` fires (to preview `p`/`f`/`c` interruption).
    pub simulated_boot_exit_tick: Option<usize>,
}

impl Default for ScheduleOptions {
    fn default() -> Self {
        Self {
            max_window_ticks: 600,
            selected_part_only: None,
            simulated_boot_exit_tick: None,
        }
    }
}

/// Build a deterministic frame playback schedule from a `ProjectManifest`.
/// Does not load any image buffers into RAM — schedules lightweight asset references and timestamps.
pub fn build_playback_schedule(
    manifest: &ProjectManifest,
    options: ScheduleOptions,
) -> PlaybackSchedule {
    let fps = manifest.descriptor.fps.max(1);
    let mut frames = Vec::new();
    let mut fidelity_warnings = Vec::new();
    let mut is_infinite = false;
    let mut window_capped = false;

    if manifest.descriptor.show_progress {
        fidelity_warnings.push(
            "Progress percentage ('p' in header) depends on Android boot progress properties and is approximated in desktop preview."
                .to_string(),
        );
    }
    if manifest.descriptor.dynamic_colors.is_some() {
        fidelity_warnings.push(
            "dynamic_colors end-colors read Android system properties (persist.bootanim.color1..4) at runtime; desktop preview shows unmasked source textures."
                .to_string(),
        );
    }

    let mut current_tick = 0usize;
    let mut first_f_interrupted = false;

    for (part_idx, decl) in manifest.descriptor.parts.iter().enumerate() {
        if let Some(sel) = options.selected_part_only {
            if sel != part_idx {
                continue;
            }
        }

        if decl.mode == PlaybackMode::SystemFallback {
            fidelity_warnings.push(
                "Part '$SYSTEM' references /system/media/bootanimation.zip on an Android device and cannot be previewed on desktop."
                    .to_string(),
            );
            continue;
        }

        if decl.clock.is_some() {
            fidelity_warnings.push(format!(
                "Part '{}' includes watch clock coordinates; desktop preview discloses clock placement metadata rather than rendering device time.",
                decl.path
            ));
        }

        let Some(proj_part) = manifest.parts.iter().find(|p| p.name == decl.path) else {
            continue;
        };
        if proj_part.audio_relative_path.is_some() {
            fidelity_warnings.push(format!(
                "Part '{}' contains audio.wav; note that many Android OEM targets mute boot audio.",
                decl.path
            ));
        }
        if proj_part.frames.is_empty() {
            continue;
        }

        if decl.repeat == RepeatCount::Infinite {
            is_infinite = true;
        }

        let bg = decl.background_color.unwrap_or(RgbColor::new(0, 0, 0));
        let mut loop_iter = 0u32;

        loop {
            if let RepeatCount::Finite(max_loops) = decl.repeat {
                if loop_iter >= max_loops {
                    break;
                }
            }

            let boot_exited = options
                .simulated_boot_exit_tick
                .map(|exit_t| current_tick >= exit_t)
                .unwrap_or(false);

            if boot_exited {
                match decl.mode {
                    PlaybackMode::PlayToCompletion => {}
                    PlaybackMode::FadeOut(fade_len)
                        if !first_f_interrupted && fade_len > 0 && loop_iter > 0 =>
                    {
                        // Interrupted right at a loop boundary while already playing this `f` part
                    }
                    _ => break,
                }
            }

            // Emit active frames in the part
            for (f_idx, frame_ref) in proj_part.frames.iter().enumerate() {
                if current_tick >= options.max_window_ticks {
                    window_capped = true;
                    break;
                }

                let interrupted_now = options
                    .simulated_boot_exit_tick
                    .map(|exit_t| current_tick >= exit_t)
                    .unwrap_or(false);

                if interrupted_now {
                    match decl.mode {
                        PlaybackMode::PlayInterruptible => break,
                        PlaybackMode::PlayToCompletion => {}
                        PlaybackMode::FadeOut(fade_len) => {
                            if first_f_interrupted || fade_len == 0 {
                                break;
                            }
                            first_f_interrupted = true;
                            for fade_step in 0..(fade_len as usize) {
                                if current_tick >= options.max_window_ticks {
                                    window_capped = true;
                                    break;
                                }
                                let f_ref =
                                    &proj_part.frames[(f_idx + fade_step) % proj_part.frames.len()];
                                let opacity =
                                    1.0 - ((fade_step + 1) as f32 / (fade_len as f32 + 1.0));
                                frames.push(ScheduledFrame {
                                    timeline_tick: current_tick,
                                    timestamp_seconds: current_tick as f64 / fps as f64,
                                    part_index: part_idx,
                                    part_name: decl.path.clone(),
                                    loop_iteration: loop_iter,
                                    frame_index_in_part: (f_idx + fade_step)
                                        % proj_part.frames.len(),
                                    relative_path: f_ref.relative_path.clone(),
                                    is_pause_hold: false,
                                    fade_opacity: opacity,
                                    background_color: bg,
                                    trim: f_ref.trim,
                                });
                                current_tick += 1;
                            }
                            break;
                        }
                        PlaybackMode::SystemFallback => break,
                    }
                }

                frames.push(ScheduledFrame {
                    timeline_tick: current_tick,
                    timestamp_seconds: current_tick as f64 / fps as f64,
                    part_index: part_idx,
                    part_name: decl.path.clone(),
                    loop_iteration: loop_iter,
                    frame_index_in_part: f_idx,
                    relative_path: frame_ref.relative_path.clone(),
                    is_pause_hold: false,
                    fade_opacity: 1.0,
                    background_color: bg,
                    trim: frame_ref.trim,
                });
                current_tick += 1;
            }

            if window_capped {
                break;
            }

            // Emit pause hold frames (holding the final frame of the part for `pause_frames` ticks)
            if let Some(last_frame) = proj_part.frames.last() {
                let last_idx = proj_part.frames.len() - 1;
                for _ in 0..decl.pause_frames {
                    if current_tick >= options.max_window_ticks {
                        window_capped = true;
                        break;
                    }
                    frames.push(ScheduledFrame {
                        timeline_tick: current_tick,
                        timestamp_seconds: current_tick as f64 / fps as f64,
                        part_index: part_idx,
                        part_name: decl.path.clone(),
                        loop_iteration: loop_iter,
                        frame_index_in_part: last_idx,
                        relative_path: last_frame.relative_path.clone(),
                        is_pause_hold: true,
                        fade_opacity: 1.0,
                        background_color: bg,
                        trim: last_frame.trim,
                    });
                    current_tick += 1;
                }
            }

            if window_capped {
                break;
            }

            loop_iter = loop_iter.saturating_add(1);
        }

        if window_capped {
            break;
        }
    }

    let finite_duration_seconds = if is_infinite {
        None
    } else {
        Some(frames.len() as f64 / fps as f64)
    };

    PlaybackSchedule {
        width: manifest.descriptor.width,
        height: manifest.descriptor.height,
        fps,
        is_infinite,
        finite_duration_seconds,
        window_capped,
        frames,
        fidelity_warnings,
    }
}

/// Render a preview MP4 video from a `ProjectManifest` using FFmpeg's `concat` demuxer
/// over the scheduled preview window. Never claims to be the authoritative bootanimation artifact.
pub fn export_preview_video_with_ffmpeg(
    manifest: &ProjectManifest,
    project_dir: &Path,
    output_mp4: &Path,
    max_window_ticks: usize,
    ffmpeg_bin: Option<&str>,
) -> Result<PlaybackSchedule, CoreError> {
    let ff = detect_ffmpeg(ffmpeg_bin);
    if !ff.available {
        return Err(CoreError::Ffmpeg(
            "ffmpeg is required to export a preview MP4 video".to_string(),
        ));
    }

    let schedule = build_playback_schedule(
        manifest,
        ScheduleOptions {
            max_window_ticks: max_window_ticks.max(1),
            selected_part_only: None,
            simulated_boot_exit_tick: None,
        },
    );

    if schedule.frames.is_empty() {
        return Err(CoreError::Pipeline(
            "playback schedule produced 0 frames to render".to_string(),
        ));
    }

    let temp_dir = tempfile::tempdir()?;
    let concat_list_path = temp_dir.path().join("concat.txt");
    let frame_duration = 1.0 / (schedule.fps as f64);
    let mut concat_body = String::new();

    for sf in &schedule.frames {
        let abs_frame =
            ProjectManifest::resolve_asset_path(project_dir, &sf.relative_path)?.canonicalize()?;
        // Strip Windows `\\?\` verbatim prefix if present so FFmpeg concat accepts the path
        let clean_path = abs_frame
            .display()
            .to_string()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/");
        concat_body.push_str(&format!("file '{}'\n", clean_path.replace('\'', "'\\''")));
        concat_body.push_str(&format!("duration {frame_duration:.6}\n"));
    }
    if let Some(last) = schedule.frames.last() {
        let abs_last = ProjectManifest::resolve_asset_path(project_dir, &last.relative_path)?
            .canonicalize()?;
        let clean_path = abs_last
            .display()
            .to_string()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/");
        concat_body.push_str(&format!("file '{}'\n", clean_path.replace('\'', "'\\''")));
    }

    std::fs::write(&concat_list_path, concat_body)?;

    if let Some(parent) = output_mp4.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let output = Command::new(&ff.binary_path)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
        ])
        .arg(&concat_list_path)
        .args([
            "-vf",
            &format!("fps={},format=yuv420p", schedule.fps),
            "-c:v",
            "libx264",
        ])
        .arg(output_mp4)
        .output()
        .map_err(|e| CoreError::Ffmpeg(format!("failed to invoke ffmpeg: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CoreError::Ffmpeg(format!(
            "ffmpeg preview export failed: {}",
            stderr.trim()
        )));
    }

    Ok(schedule)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        FrameAssetRef, MagiskExportSettings, ProjectPart, TransformSettings, CURRENT_SCHEMA_VERSION,
    };
    use crate::model::{AnimationDescriptor, PartDeclaration};

    fn sample_manifest(repeat_part1: RepeatCount, mode_part1: PlaybackMode) -> ProjectManifest {
        ProjectManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: "sched_test".to_string(),
            descriptor: AnimationDescriptor {
                width: 100,
                height: 100,
                fps: 10,
                show_progress: true,
                dynamic_colors: None,
                parts: vec![
                    PartDeclaration {
                        mode: PlaybackMode::PlayInterruptible,
                        repeat: RepeatCount::Finite(2),
                        pause_frames: 1,
                        path: "part0".to_string(),
                        background_color: None,
                        clock: None,
                    },
                    PartDeclaration {
                        mode: mode_part1,
                        repeat: repeat_part1,
                        pause_frames: 0,
                        path: "part1".to_string(),
                        background_color: Some(RgbColor::new(16, 32, 48)),
                        clock: None,
                    },
                ],
                preserved_directives: vec![],
            },
            parts: vec![
                ProjectPart {
                    name: "part0".to_string(),
                    frames: vec![
                        FrameAssetRef {
                            relative_path: "assets/part0/00.png".to_string(),
                            trim: None,
                        },
                        FrameAssetRef {
                            relative_path: "assets/part0/01.png".to_string(),
                            trim: None,
                        },
                    ],
                    audio_relative_path: None,
                },
                ProjectPart {
                    name: "part1".to_string(),
                    frames: vec![FrameAssetRef {
                        relative_path: "assets/part1/00.png".to_string(),
                        trim: None,
                    }],
                    audio_relative_path: None,
                },
            ],
            transforms: TransformSettings::default(),
            magisk: MagiskExportSettings::default(),
            source_archive: None,
            import_warnings: vec![],
        }
    }

    #[test]
    fn finite_schedule_computes_repeats_and_pause_holds_accurately() {
        let manifest = sample_manifest(RepeatCount::Finite(3), PlaybackMode::PlayToCompletion);
        let sched = build_playback_schedule(&manifest, ScheduleOptions::default());

        // part0: 2 loops * (2 frames + 1 pause frame) = 6 ticks
        // part1: 3 loops * (1 frame + 0 pause) = 3 ticks -> total 9 ticks @ 10fps = 0.9s
        assert!(!sched.is_infinite);
        assert_eq!(sched.frames.len(), 9);
        assert_eq!(sched.finite_duration_seconds, Some(0.9));
        assert!(sched.frames[2].is_pause_hold);
        assert!(sched.frames[5].is_pause_hold);
        assert!(!sched.fidelity_warnings.is_empty());
    }

    #[test]
    fn infinite_loop_does_not_invent_finite_duration_and_caps_at_window() {
        let manifest = sample_manifest(RepeatCount::Infinite, PlaybackMode::PlayInterruptible);
        let sched = build_playback_schedule(
            &manifest,
            ScheduleOptions {
                max_window_ticks: 25,
                selected_part_only: None,
                simulated_boot_exit_tick: None,
            },
        );
        assert!(sched.is_infinite);
        assert_eq!(sched.finite_duration_seconds, None);
        assert!(sched.window_capped);
        assert_eq!(sched.frames.len(), 25);
    }

    #[test]
    fn fade_out_part_fades_when_interrupted() {
        let manifest = sample_manifest(RepeatCount::Infinite, PlaybackMode::FadeOut(3));
        let sched = build_playback_schedule(
            &manifest,
            ScheduleOptions {
                max_window_ticks: 50,
                selected_part_only: None,
                simulated_boot_exit_tick: Some(7),
            },
        );
        // part0 takes ticks 0..6, part1 starts at tick 6, interrupted at tick 7 -> 3 fade frames (ticks 7, 8, 9)
        assert_eq!(sched.frames.len(), 10);
        assert!(sched.frames[7].fade_opacity < 1.0);
        assert!(sched.frames[9].fade_opacity < sched.frames[7].fade_opacity);
    }
}
