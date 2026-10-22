import type {
  PlaybackSchedule,
  ProjectManifest,
  ScheduledFrame,
  ValidationReport,
  WorkspaceSnapshot,
} from './types';

/** Result of a real archive build performed by the Rust core. */
export interface BuildReport {
  output_path: string;
  total_frames: number;
  compression_method: number;
  invariants_verified: boolean;
  warnings: string[];
}

/** Result of a real Magisk module export performed by the Rust core. */
export interface MagiskExportReport {
  output_path: string;
  module_id: string;
  target_path: string;
  inner_archive_verified: boolean;
  warnings: string[];
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

// The studio starts with no project at all. Parts and frame references only
// exist after the user opens a project directory, so the UI never shows
// invented assets or a sample animation.
function createEmptyManifest(): ProjectManifest {
  return {
    schema_version: 1,
    name: 'untitled',
    descriptor: {
      width: 1080,
      height: 2400,
      fps: 30,
      show_progress: false,
      dynamic_colors: null,
      parts: [],
    },
    parts: [],
    transforms: {
      resize: null,
      flatten_alpha_to: null,
      convert_format: null,
      target_fps: null,
      lossless_optimize_png: false,
      generate_trim_txt: false,
    },
    magisk: {
      module_id: 'bootanimix_custom',
      module_name: 'Custom Boot Animation',
      version: 'v1.0.0',
      version_code: 1,
      author: 'BootAnimix',
      description: 'Custom Android boot animation packaged by BootAnimix',
      target_path: 'system/media/bootanimation.zip',
    },
    source_archive: null,
    import_warnings: [],
  };
}

function computeLocalValidation(manifest: ProjectManifest): ValidationReport {
  const diagnostics: ValidationReport['diagnostics'] = [];
  const per_part_frames: Record<string, number> = {};
  let total_frames = 0;
  let has_infinite_loop = false;
  let finite_ticks = 0;

  if (
    manifest.descriptor.width <= 0 ||
    manifest.descriptor.height <= 0 ||
    manifest.descriptor.fps <= 0
  ) {
    diagnostics.push({
      code: 'BA-DSC-002',
      severity: 'error',
      message: `Invalid header dimensions or FPS: ${manifest.descriptor.width}x${manifest.descriptor.height} @ ${manifest.descriptor.fps}fps`,
      file_or_part: 'desc.txt',
      line: 1,
      remediation: 'Specify positive integer values for WIDTH, HEIGHT, and FPS.',
      blocks_export: true,
    });
  } else if (manifest.descriptor.fps > 120) {
    diagnostics.push({
      code: 'BA-DSC-002',
      severity: 'warning',
      message: `High frame rate (${manifest.descriptor.fps} FPS) may drop frames on Android SurfaceFlinger.`,
      file_or_part: 'desc.txt',
      line: 1,
      remediation: 'Use 30 or 60 FPS for reliable device playback.',
      blocks_export: false,
    });
  }

  for (const [idx, decl] of manifest.descriptor.parts.entries()) {
    const part = manifest.parts.find((p) => p.name === decl.path);
    const count = part ? part.frames.length : 0;
    per_part_frames[decl.path] = count;
    total_frames += count;

    if (count === 0 && decl.mode.mode !== 'system_fallback') {
      diagnostics.push({
        code: 'BA-DSC-005',
        severity: 'error',
        message: `Part '${decl.path}' contains 0 frames.`,
        file_or_part: decl.path,
        line: idx + 2,
        remediation: 'Import frames into this part or remove the empty part.',
        blocks_export: true,
      });
    }

    if (decl.mode.mode === 'play_to_completion' && decl.repeat.kind === 'infinite') {
      diagnostics.push({
        code: 'BA-DSC-004',
        severity: 'warning',
        message: `Part '${decl.path}' uses 'c' (play to completion) with COUNT=0 (infinite loop), which can soft-hang Android boot!`,
        file_or_part: decl.path,
        line: idx + 2,
        remediation: "Change mode to 'p' or 'f' for infinite loops, or set a finite repeat count.",
        blocks_export: false,
      });
    }

    if (decl.repeat.kind === 'infinite') {
      has_infinite_loop = true;
    } else {
      finite_ticks += (count + decl.pause_frames) * decl.repeat.count;
    }
  }

  if (!/^[a-zA-Z][a-zA-Z0-9._-]+$/.test(manifest.magisk.module_id)) {
    diagnostics.push({
      code: 'BA-MGK-001',
      severity: 'error',
      message: `Magisk module ID '${manifest.magisk.module_id}' violates ^[a-zA-Z][a-zA-Z0-9._-]+$`,
      file_or_part: 'module.prop',
      line: 1,
      remediation: 'Start module ID with an ASCII letter and use only letters, digits, dots, underscores, or hyphens.',
      blocks_export: true,
    });
  }

  const hasError = diagnostics.some((d) => d.severity === 'error' || d.blocks_export);
  const hasWarn = diagnostics.some((d) => d.severity === 'warning');

  return {
    outcome: hasError ? 'invalid' : hasWarn ? 'valid_with_warnings' : 'valid',
    diagnostics,
    performance: {
      total_frames,
      total_image_bytes: total_frames * 48_120,
      archive_size_bytes: total_frames * 48_120 + 512,
      per_part_frames,
      finite_duration_seconds: has_infinite_loop
        ? null
        : finite_ticks / Math.max(1, manifest.descriptor.fps),
      has_infinite_loop,
    },
  };
}

function computeLocalSchedule(
  manifest: ProjectManifest,
  maxWindowTicks = 240,
  selectedPartOnly: number | null = null,
): PlaybackSchedule {
  const fps = Math.max(1, manifest.descriptor.fps);
  const frames: ScheduledFrame[] = [];
  const fidelity_warnings: string[] = [];
  let is_infinite = false;
  let window_capped = false;
  let tick = 0;

  if (manifest.descriptor.show_progress) {
    fidelity_warnings.push(
      "Header progress percentage ('p') depends on Android boot progress properties; desktop preview shows simulated progression.",
    );
  }

  for (const [partIdx, decl] of manifest.descriptor.parts.entries()) {
    if (selectedPartOnly !== null && selectedPartOnly !== partIdx) continue;
    const part = manifest.parts.find((p) => p.name === decl.path);
    if (!part || part.frames.length === 0) continue;

    if (decl.clock) {
      fidelity_warnings.push(
        `Part '${decl.path}' specifies watch clock coordinates (${decl.clock.x ?? 'c'}, ${decl.clock.y}); requires clock_font.png on device.`,
      );
    }
    if (part.audio_relative_path) {
      fidelity_warnings.push(
        `Part '${decl.path}' includes audio.wav; note some OEM builds mute boot audio.`,
      );
    }
    if (decl.repeat.kind === 'infinite') {
      is_infinite = true;
    }

    const bg = decl.background_color ?? { r: 0, g: 0, b: 0 };
    const maxLoops = decl.repeat.kind === 'finite' ? decl.repeat.count : 999;

    for (let loop = 0; loop < maxLoops; loop++) {
      for (let fIdx = 0; fIdx < part.frames.length; fIdx++) {
        if (tick >= maxWindowTicks) {
          window_capped = true;
          break;
        }
        const fRef = part.frames[fIdx];
        frames.push({
          timeline_tick: tick,
          timestamp_seconds: tick / fps,
          part_index: partIdx,
          part_name: decl.path,
          loop_iteration: loop,
          frame_index_in_part: fIdx,
          relative_path: fRef.relative_path,
          is_pause_hold: false,
          fade_opacity: 1.0,
          background_color: bg,
          trim: fRef.trim,
        });
        tick++;
      }
      if (window_capped) break;

      const lastIdx = part.frames.length - 1;
      const lastRef = part.frames[lastIdx];
      for (let p = 0; p < decl.pause_frames; p++) {
        if (tick >= maxWindowTicks) {
          window_capped = true;
          break;
        }
        frames.push({
          timeline_tick: tick,
          timestamp_seconds: tick / fps,
          part_index: partIdx,
          part_name: decl.path,
          loop_iteration: loop,
          frame_index_in_part: lastIdx,
          relative_path: lastRef.relative_path,
          is_pause_hold: true,
          fade_opacity: 1.0,
          background_color: bg,
          trim: lastRef.trim,
        });
        tick++;
      }
      if (window_capped) break;
    }
    if (window_capped) break;
  }

  return {
    width: manifest.descriptor.width,
    height: manifest.descriptor.height,
    fps,
    is_infinite,
    finite_duration_seconds: is_infinite ? null : frames.length / fps,
    window_capped,
    frames,
    fidelity_warnings,
  };
}

export class StudioBridge {
  private current: ProjectManifest = createEmptyManifest();
  private undoStack: ProjectManifest[] = [];
  private redoStack: ProjectManifest[] = [];
  private projectDir: string | null = null;
  private dirty = false;

  /** True when running inside the Tauri desktop shell. */
  get isDesktop(): boolean {
    return isTauriRuntime();
  }

  async getSnapshot(selectedPartOnly: number | null = null): Promise<WorkspaceSnapshot> {
    if (isTauriRuntime()) {
      return tauriInvoke<WorkspaceSnapshot>('gui_get_snapshot', { selectedPartOnly });
    }
    return {
      project_dir: this.projectDir,
      manifest: structuredClone(this.current),
      validation: computeLocalValidation(this.current),
      schedule: computeLocalSchedule(this.current, 240, selectedPartOnly),
      can_undo: this.undoStack.length > 0,
      can_redo: this.redoStack.length > 0,
      dirty: this.dirty,
    };
  }

  async commitMutation(
    mutator: (draft: ProjectManifest) => void,
    selectedPartOnly: number | null = null,
  ): Promise<WorkspaceSnapshot> {
    if (isTauriRuntime()) {
      // The Rust session owns the history stack, so the manifest is sent over
      // IPC and the authoritative undo/redo state comes back with the snapshot.
      const draft = structuredClone(this.current);
      mutator(draft);
      return tauriInvoke<WorkspaceSnapshot>('gui_replace_manifest', {
        manifest: draft,
        selectedPartOnly,
      });
    }

    this.undoStack.push(structuredClone(this.current));
    if (this.undoStack.length > 80) this.undoStack.shift();
    this.redoStack = [];
    mutator(this.current);
    this.dirty = true;
    return this.getSnapshot(selectedPartOnly);
  }

  async undo(selectedPartOnly: number | null = null): Promise<WorkspaceSnapshot> {
    if (isTauriRuntime()) {
      return tauriInvoke<WorkspaceSnapshot>('gui_undo', { selectedPartOnly });
    }
    const prev = this.undoStack.pop();
    if (prev) {
      this.redoStack.push(structuredClone(this.current));
      this.current = prev;
      this.dirty = true;
    }
    return this.getSnapshot(selectedPartOnly);
  }

  async redo(selectedPartOnly: number | null = null): Promise<WorkspaceSnapshot> {
    if (isTauriRuntime()) {
      return tauriInvoke<WorkspaceSnapshot>('gui_redo', { selectedPartOnly });
    }
    const next = this.redoStack.pop();
    if (next) {
      this.undoStack.push(structuredClone(this.current));
      this.current = next;
      this.dirty = true;
    }
    return this.getSnapshot(selectedPartOnly);
  }

  /** Persist the manifest to `<projectDir>/project.bootanimix.json`. */
  async markSaved(): Promise<WorkspaceSnapshot> {
    if (isTauriRuntime()) {
      return tauriInvoke<WorkspaceSnapshot>('gui_save_project', {
        projectDir: this.projectDir,
      });
    }
    this.dirty = false;
    return this.getSnapshot(null);
  }

  /** Open a project directory and load its manifest. */
  async openProject(projectDir: string): Promise<WorkspaceSnapshot> {
    if (!isTauriRuntime()) {
      throw new Error('Opening a project from disk requires the desktop app.');
    }
    const snap = await tauriInvoke<WorkspaceSnapshot>('gui_open_project', { projectDir });
    this.projectDir = snap.project_dir;
    this.current = snap.manifest;
    this.undoStack = [];
    this.redoStack = [];
    this.dirty = false;
    return snap;
  }

  /** Build a verified ZIP_STORED boot animation archive on disk. */
  async buildArchive(outputPath: string): Promise<BuildReport> {
    if (!isTauriRuntime()) {
      throw new Error('Building an archive requires the desktop app.');
    }
    return tauriInvoke<BuildReport>('gui_build_archive', { outputPath });
  }

  /** Export a flashable Magisk module on disk. */
  async exportMagisk(outputPath: string): Promise<MagiskExportReport> {
    if (!isTauriRuntime()) {
      throw new Error('Exporting a Magisk module requires the desktop app.');
    }
    return tauriInvoke<MagiskExportReport>('gui_export_magisk', { outputPath });
  }
}
