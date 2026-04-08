pub mod archive;
pub mod descriptor;
pub mod error;
pub mod frames;
pub mod magisk;
pub mod manifest;
pub mod model;
pub mod pipeline;
pub mod playback;
pub mod validator;

pub use archive::{
    inspect_zip_file, inspect_zip_reader, is_safe_archive_entry_path, safe_extract_zip,
    write_stored_bootanimation_zip, ArchiveBuildEntry, ArchiveEntryInfo, ArchiveInspection,
};
pub use descriptor::{parse_descriptor, parse_descriptor_lenient, serialize_descriptor};
pub use error::{CoreError, ParseDiagnostic};
pub use frames::{
    normalized_frame_name, order_frame_names, scan_directory_frames, FrameFormat,
    FrameSequenceReport, OrderedFrame,
};
pub use magisk::{
    export_magisk_module, format_module_prop, validate_magisk_settings, MagiskExportReport,
};
pub use manifest::{
    ensure_safe_relative_path, FrameAssetRef, MagiskExportSettings, ProjectManifest, ProjectPart,
    ResizeMode, TransformSettings, CURRENT_SCHEMA_VERSION,
};
pub use model::*;
pub use pipeline::{
    build_project_to_archive, compute_trimmed_subimage, detect_ffmpeg, import_archive_to_project,
    import_frame_sequence_dir, import_video_with_ffmpeg, BuildReport, FfmpegInfo,
    VideoImportOptions,
};
pub use playback::{
    build_playback_schedule, export_preview_video_with_ffmpeg, PlaybackSchedule, ScheduleOptions,
    ScheduledFrame,
};
pub use validator::{
    encode_solid_png, validate_archive_file, validate_project, Diagnostic, DiagnosticSeverity,
    PerformanceSummary, ValidationOutcome, ValidationReport,
};
