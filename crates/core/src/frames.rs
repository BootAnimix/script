use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameFormat {
    Png,
    Jpeg,
}

impl FrameFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }

    pub fn canonical_extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedFrame {
    /// Original file path or archive entry name.
    pub source_name: String,
    /// Prefix before trailing digits (e.g. `frame_` in `frame_0001.png`).
    pub prefix: String,
    /// Parsed numeric index if the stem ends with digits.
    pub numeric_index: Option<u64>,
    /// Digit padding width in the original filename.
    pub digit_width: usize,
    /// Detected image format.
    pub format: FrameFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameSequenceReport {
    pub frames: Vec<OrderedFrame>,
    pub has_numbering_gaps: bool,
    pub inconsistent_padding: bool,
    pub mixed_prefixes: bool,
    pub warnings: Vec<String>,
}

/// Deterministic natural/numeric frame ordering.
/// Never relies on filesystem iteration order.
pub fn order_frame_names<I, S>(names: I) -> Result<FrameSequenceReport, CoreError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut parsed_items: Vec<OrderedFrame> = Vec::new();
    let mut exact_seen: HashSet<String> = HashSet::new();

    for raw in names {
        let name = raw.as_ref().trim();
        if name.is_empty() {
            continue;
        }
        let path = Path::new(name);
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CoreError::FrameOrder(format!("invalid frame filename '{name}'")))?;

        if file_name.eq_ignore_ascii_case("trim.txt")
            || file_name.eq_ignore_ascii_case("audio.wav")
            || file_name.eq_ignore_ascii_case("desc.txt")
        {
            continue;
        }

        let lower_key = file_name.to_ascii_lowercase();
        if !exact_seen.insert(lower_key) {
            return Err(CoreError::FrameOrder(format!(
                "duplicate frame filename detected: '{file_name}'"
            )));
        }

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CoreError::FrameOrder(format!("frame '{file_name}' has no extension")))?;

        let format = FrameFormat::from_extension(ext).ok_or_else(|| {
            CoreError::FrameOrder(format!(
                "unsupported frame extension '.{ext}' in '{file_name}' (expected .png or .jpg)"
            ))
        })?;

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CoreError::FrameOrder(format!("invalid stem in '{file_name}'")))?;

        let (prefix, numeric_index, digit_width) = split_stem_number(stem)?;

        parsed_items.push(OrderedFrame {
            source_name: name.to_string(),
            prefix,
            numeric_index,
            digit_width,
            format,
        });
    }

    if parsed_items.is_empty() {
        return Err(CoreError::FrameOrder(
            "no valid frame images (.png/.jpg) found in sequence".to_string(),
        ));
    }

    // Check for duplicate logical frame numbers within the same prefix (e.g. `01.png` and `001.png` or `001.png` and `001.jpg`).
    let mut logical_seen: BTreeMap<(String, u64), String> = BTreeMap::new();
    for item in &parsed_items {
        if let Some(num) = item.numeric_index {
            let key = (item.prefix.clone(), num);
            if let Some(prev) = logical_seen.insert(key, item.source_name.clone()) {
                return Err(CoreError::FrameOrder(format!(
                    "duplicate logical frame index {num} between '{}' and '{}'",
                    prev, item.source_name
                )));
            }
        }
    }

    parsed_items.sort_by(|a, b| match (&a.numeric_index, &b.numeric_index) {
        (Some(na), Some(nb)) => a
            .prefix
            .cmp(&b.prefix)
            .then_with(|| na.cmp(nb))
            .then_with(|| a.source_name.cmp(&b.source_name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.source_name.cmp(&b.source_name),
    });

    let mut warnings = Vec::new();
    let prefixes: HashSet<&str> = parsed_items.iter().map(|f| f.prefix.as_str()).collect();
    let mixed_prefixes = prefixes.len() > 1;
    if mixed_prefixes {
        warnings.push(format!(
            "sequence mixes multiple filename prefixes: {:?}",
            prefixes
        ));
    }

    let widths: HashSet<usize> = parsed_items
        .iter()
        .filter(|f| f.digit_width > 0)
        .map(|f| f.digit_width)
        .collect();
    let inconsistent_padding = widths.len() > 1;
    if inconsistent_padding {
        warnings.push(
            "frames use inconsistent zero-padding widths; unpadded names can sort wrong in raw ZIP readers"
                .to_string(),
        );
    }

    let mut has_numbering_gaps = false;
    for pair in parsed_items.windows(2) {
        if pair[0].prefix == pair[1].prefix {
            if let (Some(a), Some(b)) = (pair[0].numeric_index, pair[1].numeric_index) {
                if b > a + 1 {
                    has_numbering_gaps = true;
                }
            }
        }
    }
    if has_numbering_gaps {
        warnings.push("numbering gaps detected in frame sequence".to_string());
    }

    Ok(FrameSequenceReport {
        frames: parsed_items,
        has_numbering_gaps,
        inconsistent_padding,
        mixed_prefixes,
        warnings,
    })
}

/// Scan a directory on disk and return deterministically ordered frame paths.
pub fn scan_directory_frames(dir: &Path) -> Result<(Vec<PathBuf>, FrameSequenceReport), CoreError> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            if let Some(name) = entry.file_name().to_str() {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                    names.push(name.to_string());
                }
            }
        }
    }
    let report = order_frame_names(&names)?;
    let ordered_paths = report
        .frames
        .iter()
        .map(|f| dir.join(&f.source_name))
        .collect();
    Ok((ordered_paths, report))
}

/// Generate normalized sequential archive filenames (`00000.png`, `00001.png`, ...).
pub fn normalized_frame_name(index: usize, total_frames: usize, format: FrameFormat) -> String {
    let width = std::cmp::max(5, total_frames.saturating_sub(1).to_string().len());
    format!("{index:0width$}.{}", format.canonical_extension(), width = width)
}

fn split_stem_number(stem: &str) -> Result<(String, Option<u64>, usize), CoreError> {
    let digit_start = stem
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(idx, _)| idx);

    match digit_start {
        Some(idx) => {
            let prefix = stem[..idx].to_string();
            let digits_str = &stem[idx..];
            let num = digits_str.parse::<u64>().map_err(|_| {
                CoreError::FrameOrder(format!("frame number overflow in '{stem}'"))
            })?;
            Ok((prefix, Some(num), digits_str.len()))
        }
        None => Ok((stem.to_string(), None, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_frames_numerically_independent_of_input_order() {
        let input = vec!["frame_10.png", "frame_2.png", "frame_1.png", "trim.txt"];
        let report = order_frame_names(input).expect("should order frames");
        let ordered: Vec<&str> = report.frames.iter().map(|f| f.source_name.as_str()).collect();
        assert_eq!(ordered, vec!["frame_1.png", "frame_2.png", "frame_10.png"]);
        assert!(report.inconsistent_padding);
        assert!(report.has_numbering_gaps);
    }

    #[test]
    fn rejects_duplicate_logical_frame_numbers() {
        let input = vec!["001.png", "01.png"];
        let err = order_frame_names(input).unwrap_err();
        assert!(err.to_string().contains("duplicate logical frame index"));
    }

    #[test]
    fn generates_zero_padded_normalized_names() {
        assert_eq!(normalized_frame_name(0, 120, FrameFormat::Png), "00000.png");
        assert_eq!(normalized_frame_name(42, 120, FrameFormat::Jpeg), "00042.jpg");
    }
}
