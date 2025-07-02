use crate::error::CoreError;
use crate::model::{
    AnimationDescriptor, ClockCoord, ClockPosition, DynamicColors, PartDeclaration, PlaybackMode,
    RepeatCount, RgbColor,
};

/// Parse `desc.txt` in strict mode (rejects unrecognized directives).
pub fn parse_descriptor(input: &str) -> Result<AnimationDescriptor, CoreError> {
    parse_descriptor_with_options(input, false)
}

/// Parse `desc.txt`, optionally preserving unknown vendor directives on lines after the header.
pub fn parse_descriptor_lenient(input: &str) -> Result<AnimationDescriptor, CoreError> {
    parse_descriptor_with_options(input, true)
}

fn parse_descriptor_with_options(
    input: &str,
    preserve_unknown: bool,
) -> Result<AnimationDescriptor, CoreError> {
    let mut header: Option<(u32, u32, u32, bool)> = None;
    let mut dynamic_colors: Option<DynamicColors> = None;
    let mut parts: Vec<PartDeclaration> = Vec::new();
    let mut preserved_directives: Vec<crate::model::UnknownDirective> = Vec::new();

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if header.is_none() {
            header = Some(parse_header_tokens(&tokens, line_no)?);
            continue;
        }

        if tokens[0] == "dynamic_colors" {
            if dynamic_colors.is_some() {
                return Err(CoreError::DescriptorParse {
                    line: line_no,
                    message: "duplicate dynamic_colors declaration".to_string(),
                });
            }
            if !parts.is_empty() {
                return Err(CoreError::DescriptorParse {
                    line: line_no,
                    message: "dynamic_colors must appear before part declarations".to_string(),
                });
            }
            dynamic_colors = Some(parse_dynamic_colors(&tokens, line_no)?);
            continue;
        }

        match parse_part_tokens(&tokens, line_no) {
            Ok(part) => parts.push(part),
            Err(err) => {
                let is_known_prefix = matches!(tokens[0], "p" | "c" | "f" | "$SYSTEM");
                if preserve_unknown && !is_known_prefix {
                    preserved_directives.push(crate::model::UnknownDirective {
                        line_number: line_no,
                        raw_line: line.to_string(),
                    });
                } else {
                    return Err(err);
                }
            }
        }
    }

    let (width, height, fps, show_progress) = header.ok_or_else(|| CoreError::DescriptorParse {
        line: 1,
        message: "descriptor is empty or missing header line (WIDTH HEIGHT FPS)".to_string(),
    })?;

    Ok(AnimationDescriptor {
        width,
        height,
        fps,
        show_progress,
        dynamic_colors,
        parts,
        preserved_directives,
    })
}

fn parse_header_tokens(tokens: &[&str], line_no: usize) -> Result<(u32, u32, u32, bool), CoreError> {
    if tokens.len() < 3 || tokens.len() > 4 {
        return Err(CoreError::DescriptorParse {
            line: line_no,
            message: format!(
                "header must have 3 or 4 fields (WIDTH HEIGHT FPS [p]), found {}",
                tokens.len()
            ),
        });
    }

    let width = parse_positive_u32(tokens[0], "WIDTH", line_no)?;
    let height = parse_positive_u32(tokens[1], "HEIGHT", line_no)?;
    let fps = parse_positive_u32(tokens[2], "FPS", line_no)?;

    let show_progress = if tokens.len() == 4 {
        if tokens[3] == "p" {
            true
        } else {
            return Err(CoreError::DescriptorParse {
                line: line_no,
                message: format!("invalid header progress flag '{}' (expected 'p')", tokens[3]),
            });
        }
    } else {
        false
    };

    Ok((width, height, fps, show_progress))
}

fn parse_dynamic_colors(tokens: &[&str], line_no: usize) -> Result<DynamicColors, CoreError> {
    if tokens.len() != 6 {
        return Err(CoreError::DescriptorParse {
            line: line_no,
            message: "dynamic_colors requires PATH and 4 #RRGGBB colors".to_string(),
        });
    }
    let part_path = validate_part_path(tokens[1], line_no)?;
    let mut colors = [RgbColor::new(0, 0, 0); 4];
    for (i, token) in tokens[2..6].iter().enumerate() {
        colors[i] = RgbColor::parse_hex(token).ok_or_else(|| CoreError::DescriptorParse {
            line: line_no,
            message: format!("invalid hex color '{}' in dynamic_colors", token),
        })?;
    }
    Ok(DynamicColors {
        part_path,
        start_colors: colors,
    })
}

fn parse_part_tokens(tokens: &[&str], line_no: usize) -> Result<PartDeclaration, CoreError> {
    let kind = tokens[0];
    if kind == "$SYSTEM" {
        if tokens.len() != 1 {
            return Err(CoreError::DescriptorParse {
                line: line_no,
                message: "$SYSTEM declaration does not take additional arguments".to_string(),
            });
        }
        return Ok(PartDeclaration {
            mode: PlaybackMode::SystemFallback,
            repeat: RepeatCount::Finite(1),
            pause_frames: 0,
            path: "$SYSTEM".to_string(),
            background_color: None,
            clock: None,
        });
    }

    if tokens.len() < 4 {
        return Err(CoreError::DescriptorParse {
            line: line_no,
            message: format!(
                "part declaration requires at least 4 fields (TYPE COUNT PAUSE PATH), found {}",
                tokens.len()
            ),
        });
    }

    let count_raw = parse_u32(tokens[1], "COUNT", line_no)?;
    let pause_frames = parse_u32(tokens[2], "PAUSE", line_no)?;
    let path = validate_part_path(tokens[3], line_no)?;

    let (mode, tail_start) = match kind {
        "p" => (PlaybackMode::PlayInterruptible, 4),
        "c" => (PlaybackMode::PlayToCompletion, 4),
        "f" => {
            if tokens.len() < 5 {
                return Err(CoreError::DescriptorParse {
                    line: line_no,
                    message: "part type 'f' requires FADE frame count after PATH".to_string(),
                });
            }
            let fade_frames = parse_u32(tokens[4], "FADE", line_no)?;
            (PlaybackMode::FadeOut(fade_frames), 5)
        }
        other => {
            return Err(CoreError::DescriptorParse {
                line: line_no,
                message: format!(
                    "unrecognized part type '{}' (expected 'p', 'c', 'f', or '$SYSTEM')",
                    other
                ),
            });
        }
    };

    let tail = &tokens[tail_start..];
    let (background_color, clock) = parse_optional_tail(tail, line_no)?;

    Ok(PartDeclaration {
        mode,
        repeat: RepeatCount::from_raw(count_raw),
        pause_frames,
        path,
        background_color,
        clock,
    })
}

fn parse_optional_tail(
    tail: &[&str],
    line_no: usize,
) -> Result<(Option<RgbColor>, Option<ClockPosition>), CoreError> {
    if tail.is_empty() {
        return Ok((None, None));
    }

    let (bg_color, clock_tokens) = if tail[0].starts_with('#') {
        let color = RgbColor::parse_hex(tail[0]).ok_or_else(|| CoreError::DescriptorParse {
            line: line_no,
            message: format!("invalid background RGB hex '{}'", tail[0]),
        })?;
        (Some(color), &tail[1..])
    } else {
        (None, tail)
    };

    let clock = match clock_tokens.len() {
        0 => None,
        1 => {
            let y = ClockCoord::parse_token(clock_tokens[0]).ok_or_else(|| {
                CoreError::DescriptorParse {
                    line: line_no,
                    message: format!("invalid CLOCK1 coordinate '{}'", clock_tokens[0]),
                }
            })?;
            Some(ClockPosition { x: None, y })
        }
        2 => {
            let x = ClockCoord::parse_token(clock_tokens[0]).ok_or_else(|| {
                CoreError::DescriptorParse {
                    line: line_no,
                    message: format!("invalid CLOCK1 coordinate '{}'", clock_tokens[0]),
                }
            })?;
            let y = ClockCoord::parse_token(clock_tokens[1]).ok_or_else(|| {
                CoreError::DescriptorParse {
                    line: line_no,
                    message: format!("invalid CLOCK2 coordinate '{}'", clock_tokens[1]),
                }
            })?;
            Some(ClockPosition { x: Some(x), y })
        }
        _ => {
            return Err(CoreError::DescriptorParse {
                line: line_no,
                message: format!(
                    "too many trailing tokens in part declaration: {:?}",
                    clock_tokens
                ),
            });
        }
    };

    Ok((bg_color, clock))
}

fn validate_part_path(raw: &str, line_no: usize) -> Result<String, CoreError> {
    let trimmed = raw.trim_matches('/');
    if trimmed.is_empty()
        || raw.starts_with('/')
        || raw.contains('\\')
        || trimmed.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return Err(CoreError::DescriptorParse {
            line: line_no,
            message: format!("invalid or unsafe part directory path '{}'", raw),
        });
    }
    Ok(trimmed.to_string())
}

fn parse_positive_u32(token: &str, field: &str, line_no: usize) -> Result<u32, CoreError> {
    let val = parse_u32(token, field, line_no)?;
    if val == 0 || val > 16384 {
        return Err(CoreError::DescriptorParse {
            line: line_no,
            message: format!("{} must be between 1 and 16384, got {}", field, val),
        });
    }
    Ok(val)
}

fn parse_u32(token: &str, field: &str, line_no: usize) -> Result<u32, CoreError> {
    token.parse::<u32>().map_err(|_| CoreError::DescriptorParse {
        line: line_no,
        message: format!("invalid integer for {}: '{}'", field, token),
    })
}

/// Serialize an `AnimationDescriptor` into canonical LF-terminated `desc.txt` text.
/// Always ends with a trailing `\n` so Android's parser never drops the last declaration.
pub fn serialize_descriptor(desc: &AnimationDescriptor) -> Result<String, CoreError> {
    if desc.width == 0 || desc.height == 0 || desc.fps == 0 {
        return Err(CoreError::InvalidDescriptor(
            "width, height, and fps must all be > 0".to_string(),
        ));
    }
    if !desc.preserved_directives.is_empty() {
        return Err(CoreError::InvalidDescriptor(
            "refusing to rewrite descriptor with unknown preserved directives".to_string(),
        ));
    }

    let mut out = String::new();
    if desc.show_progress {
        out.push_str(&format!("{} {} {} p\n", desc.width, desc.height, desc.fps));
    } else {
        out.push_str(&format!("{} {} {}\n", desc.width, desc.height, desc.fps));
    }

    if let Some(dyn_colors) = &desc.dynamic_colors {
        out.push_str(&format!(
            "dynamic_colors {} {} {} {} {}\n",
            dyn_colors.part_path,
            dyn_colors.start_colors[0],
            dyn_colors.start_colors[1],
            dyn_colors.start_colors[2],
            dyn_colors.start_colors[3],
        ));
    }

    for part in &desc.parts {
        if part.mode == PlaybackMode::SystemFallback {
            out.push_str("$SYSTEM\n");
            continue;
        }
        let mut line = match part.mode {
            PlaybackMode::PlayInterruptible => format!(
                "p {} {} {}",
                part.repeat.to_raw(),
                part.pause_frames,
                part.path
            ),
            PlaybackMode::PlayToCompletion => format!(
                "c {} {} {}",
                part.repeat.to_raw(),
                part.pause_frames,
                part.path
            ),
            PlaybackMode::FadeOut(fade) => format!(
                "f {} {} {} {}",
                part.repeat.to_raw(),
                part.pause_frames,
                part.path,
                fade
            ),
            PlaybackMode::SystemFallback => unreachable!(),
        };

        if let Some(bg) = part.background_color {
            line.push_str(&format!(" {}", bg));
        }

        if let Some(clock) = &part.clock {
            match &clock.x {
                Some(x) => line.push_str(&format!(" {} {}", x, clock.y)),
                None => line.push_str(&format!(" {}", clock.y)),
            }
        }

        line.push('\n');
        out.push_str(&line);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_and_fade_parts_without_trailing_newline() {
        let raw = "1080 2400 60 p\ndynamic_colors part1 #112233 #445566 #778899 #AABBCC\np 1 0 part0 #000000\nf 0 15 part1 10 #FFFFFF c -24\nc 1 0 part2 16 c";
        let parsed = parse_descriptor(raw).expect("should parse valid desc.txt");
        assert_eq!(parsed.width, 1080);
        assert_eq!(parsed.height, 2400);
        assert_eq!(parsed.fps, 60);
        assert!(parsed.show_progress);
        assert!(parsed.dynamic_colors.is_some());
        assert_eq!(parsed.parts.len(), 3);
        assert_eq!(parsed.parts[1].mode, PlaybackMode::FadeOut(10));
        assert_eq!(parsed.parts[1].repeat, RepeatCount::Infinite);
        assert_eq!(parsed.parts[1].pause_frames, 15);
        assert_eq!(parsed.parts[1].path, "part1");

        let serialized = serialize_descriptor(&parsed).expect("should serialize");
        assert!(serialized.ends_with('\n'));
        let reparsed = parse_descriptor(&serialized).expect("should round-trip");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn rejects_invalid_header_and_unsafe_part_paths() {
        assert!(parse_descriptor("0 1080 60\np 1 0 part0\n").is_err());
        assert!(parse_descriptor("1080 1920 60\np 1 0 ../secret\n").is_err());
        assert!(parse_descriptor("1080 1920 60\nf 1 0 part0\n").is_err());
    }

    #[test]
    fn handles_crlf_comments_and_lenient_unknown_directives() {
        let raw = "# comment header\r\n1080 1920 30\r\n\r\np 1 0 part0\r\nvendor_ext foo bar\r\nc 0 10 part1 #00FF00\r\n";
        assert!(parse_descriptor(raw).is_err());
        let lenient = parse_descriptor_lenient(raw).expect("lenient should capture unknown directive");
        assert_eq!(lenient.parts.len(), 2);
        assert_eq!(lenient.preserved_directives.len(), 1);
        assert_eq!(lenient.preserved_directives[0].line_number, 5);
        assert!(serialize_descriptor(&lenient).is_err());
    }
}
