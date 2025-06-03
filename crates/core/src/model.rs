use serde::{Deserialize, Serialize};
use std::fmt;

/// RGB hex color stored as 3 bytes (e.g. `#RRGGBB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn parse_hex(input: &str) -> Option<Self> {
        let hex = input.strip_prefix('#')?;
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }
}

impl fmt::Display for RgbColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// Repeat policy for a boot animation part.
/// `0` in `desc.txt` means loop indefinitely until boot finishes, NOT zero plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "count", rename_all = "snake_case")]
pub enum RepeatCount {
    Infinite,
    Finite(u32),
}

impl RepeatCount {
    pub fn from_raw(count: u32) -> Self {
        if count == 0 {
            Self::Infinite
        } else {
            Self::Finite(count)
        }
    }

    pub fn to_raw(self) -> u32 {
        match self {
            Self::Infinite => 0,
            Self::Finite(n) => n,
        }
    }
}

/// Playback mode of a part in `desc.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "fade_frames", rename_all = "snake_case")]
pub enum PlaybackMode {
    /// `p` — Play unless interrupted by the end of boot.
    PlayInterruptible,
    /// `c` — Play to completion no matter what.
    PlayToCompletion,
    /// `f` — Like `p`, but fade out over `fade_frames` frames when interrupted.
    FadeOut(u32),
    /// `$SYSTEM` — Load `/system/media/bootanimation.zip` and play that.
    SystemFallback,
}

/// Clock coordinate (`c`, positive offset, or negative offset from right/top edge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockCoord {
    Center,
    Offset(i32),
    NegZero,
}

impl ClockCoord {
    pub fn parse_token(token: &str) -> Option<Self> {
        if token == "c" {
            return Some(Self::Center);
        }
        if token == "-0" {
            return Some(Self::NegZero);
        }
        let val: i32 = token.parse().ok()?;
        Some(Self::Offset(val))
    }
}

impl fmt::Display for ClockCoord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Center => write!(f, "c"),
            Self::NegZero => write!(f, "-0"),
            Self::Offset(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockPosition {
    /// When only `CLOCK1` is present in `desc.txt`, x is implicit center and `CLOCK1` is y.
    pub x: Option<ClockCoord>,
    pub y: ClockCoord,
}

/// Optional `dynamic_colors PATH #RGB1 #RGB2 #RGB3 #RGB4` line in `desc.txt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicColors {
    pub part_path: String,
    pub start_colors: [RgbColor; 4],
}

/// Trim geometry (`WxH+X+Y`) from `trim.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimRect {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

impl TrimRect {
    pub fn parse_line(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        let (wh, xy) = trimmed.split_once('+')?;
        let (w_str, h_str) = wh.split_once('x')?;
        let (x_str, y_str) = xy.split_once('+')?;
        let width: u32 = w_str.parse().ok()?;
        let height: u32 = h_str.parse().ok()?;
        let x: u32 = x_str.parse().ok()?;
        let y: u32 = y_str.parse().ok()?;
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            width,
            height,
            x,
            y,
        })
    }
}

impl fmt::Display for TrimRect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}+{}+{}", self.width, self.height, self.x, self.y)
    }
}

/// A single part declaration in `desc.txt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartDeclaration {
    pub mode: PlaybackMode,
    pub repeat: RepeatCount,
    /// Number of frames (NOT seconds) to pause on the final frame after this part ends.
    pub pause_frames: u32,
    pub path: String,
    pub background_color: Option<RgbColor>,
    pub clock: Option<ClockPosition>,
}

/// Preserved unknown/unrecognized directive line when safe preservation is requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownDirective {
    pub line_number: usize,
    pub raw_line: String,
}

/// Parsed representation of `desc.txt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationDescriptor {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub show_progress: bool,
    pub dynamic_colors: Option<DynamicColors>,
    pub parts: Vec<PartDeclaration>,
    #[serde(default)]
    pub preserved_directives: Vec<UnknownDirective>,
}
