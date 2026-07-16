export interface RgbColor {
  r: number;
  g: number;
  b: number;
}

export type RepeatCount =
  | { kind: 'infinite' }
  | { kind: 'finite'; count: number };

export type PlaybackMode =
  | { mode: 'play_interruptible' }
  | { mode: 'play_to_completion' }
  | { mode: 'fade_out'; fade_frames: number }
  | { mode: 'system_fallback' };

export interface TrimRect {
  width: number;
  height: number;
  x: number;
  y: number;
}

export interface PartDeclaration {
  mode: PlaybackMode;
  repeat: RepeatCount;
  pause_frames: number;
  path: string;
  background_color: RgbColor | null;
  clock: { x: string | number | null; y: string | number } | null;
}

export interface AnimationDescriptor {
  width: number;
  height: number;
  fps: number;
  show_progress: boolean;
  dynamic_colors: {
    part_path: string;
    start_colors: [RgbColor, RgbColor, RgbColor, RgbColor];
  } | null;
  parts: PartDeclaration[];
}

export interface FrameAssetRef {
  relative_path: string;
  trim: TrimRect | null;
}

export interface ProjectPart {
  name: string;
  frames: FrameAssetRef[];
  audio_relative_path: string | null;
}

export interface TransformSettings {
  resize: [number, number, 'stretch' | 'contain' | 'crop'] | null;
  flatten_alpha_to: RgbColor | null;
  convert_format: 'png' | 'jpeg' | null;
  target_fps: number | null;
  lossless_optimize_png: boolean;
  generate_trim_txt: boolean;
}

export interface MagiskExportSettings {
  module_id: string;
  module_name: string;
  version: string;
  version_code: number;
  author: string;
  description: string;
  target_path: string;
}

export interface ProjectManifest {
  schema_version: number;
  name: string;
  descriptor: AnimationDescriptor;
  parts: ProjectPart[];
  transforms: TransformSettings;
  magisk: MagiskExportSettings;
  source_archive: string | null;
  import_warnings: string[];
}

export interface Diagnostic {
  code: string;
  severity: 'info' | 'warning' | 'error';
  message: string;
  file_or_part: string | null;
  line: number | null;
  remediation: string;
  blocks_export: boolean;
}

export interface ValidationReport {
  outcome: 'valid' | 'valid_with_warnings' | 'invalid';
  diagnostics: Diagnostic[];
  performance: {
    total_frames: number;
    total_image_bytes: number;
    archive_size_bytes: number;
    per_part_frames: Record<string, number>;
    finite_duration_seconds: number | null;
    has_infinite_loop: boolean;
  };
}

export interface ScheduledFrame {
  timeline_tick: number;
  timestamp_seconds: number;
  part_index: number;
  part_name: string;
  loop_iteration: number;
  frame_index_in_part: number;
  relative_path: string;
  is_pause_hold: boolean;
  fade_opacity: number;
  background_color: RgbColor;
  trim: TrimRect | null;
}

export interface PlaybackSchedule {
  width: number;
  height: number;
  fps: number;
  is_infinite: boolean;
  finite_duration_seconds: number | null;
  window_capped: boolean;
  frames: ScheduledFrame[];
  fidelity_warnings: string[];
}

export interface WorkspaceSnapshot {
  project_dir: string | null;
  manifest: ProjectManifest;
  validation: ValidationReport;
  schedule: PlaybackSchedule;
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
}
