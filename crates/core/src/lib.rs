pub mod descriptor;
pub mod error;
pub mod frames;
pub mod manifest;
pub mod model;

pub use descriptor::{parse_descriptor, parse_descriptor_lenient, serialize_descriptor};
pub use error::{CoreError, ParseDiagnostic};
pub use frames::{
    normalized_frame_name, order_frame_names, scan_directory_frames, FrameFormat,
    FrameSequenceReport, OrderedFrame,
};
pub use manifest::{
    ensure_safe_relative_path, FrameAssetRef, MagiskExportSettings, ProjectManifest, ProjectPart,
    ResizeMode, TransformSettings, CURRENT_SCHEMA_VERSION,
};
pub use model::*;
