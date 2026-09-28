use bootanimix_core::{
    parse_descriptor, parse_descriptor_lenient, serialize_descriptor, AnimationDescriptor,
    ClockCoord, ClockPosition, DynamicColors, PartDeclaration, PlaybackMode, RepeatCount, RgbColor,
};
use proptest::prelude::*;

fn arb_rgb() -> impl Strategy<Value = RgbColor> {
    (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| RgbColor::new(r, g, b))
}

fn arb_part(idx: usize) -> impl Strategy<Value = PartDeclaration> {
    (
        prop_oneof![
            Just(PlaybackMode::PlayInterruptible),
            Just(PlaybackMode::PlayToCompletion),
            (1u32..=120u32).prop_map(PlaybackMode::FadeOut),
        ],
        (0u32..=20u32).prop_map(RepeatCount::from_raw),
        0u32..=120u32,
        prop::option::of(arb_rgb()),
    )
        .prop_map(
            move |(mode, repeat, pause_frames, background_color)| PartDeclaration {
                mode,
                repeat,
                pause_frames,
                path: format!("part{idx}"),
                background_color,
                clock: None,
            },
        )
}

proptest! {
    #[test]
    fn prop_valid_descriptors_roundtrip_without_semantic_loss(
        width in 16u32..=4096u32,
        height in 16u32..=4096u32,
        fps in 1u32..=120u32,
        show_progress in any::<bool>(),
        p0 in arb_part(0),
        p1 in arb_part(1),
    ) {
        let desc = AnimationDescriptor {
            width,
            height,
            fps,
            show_progress,
            dynamic_colors: None,
            parts: vec![p0, p1],
            preserved_directives: vec![],
        };
        let text = serialize_descriptor(&desc).unwrap();
        prop_assert!(text.ends_with('\n'));
        let reparsed = parse_descriptor(&text).unwrap();
        prop_assert_eq!(desc, reparsed);
    }

    #[test]
    fn prop_arbitrary_strings_never_panic_descriptor_parser(raw in ".*") {
        let _ = parse_descriptor(&raw);
        let _ = parse_descriptor_lenient(&raw);
    }
}

#[test]
fn parses_and_roundtrips_clock_coordinates_and_dynamic_colors() {
    let desc = AnimationDescriptor {
        width: 1080,
        height: 2400,
        fps: 60,
        show_progress: true,
        dynamic_colors: Some(DynamicColors {
            part_path: "part0".to_string(),
            start_colors: [
                RgbColor::new(0x12, 0x34, 0x56),
                RgbColor::new(0x78, 0x9A, 0xBC),
                RgbColor::new(0xDE, 0xF0, 0x11),
                RgbColor::new(0x22, 0x33, 0x44),
            ],
        }),
        parts: vec![
            PartDeclaration {
                mode: PlaybackMode::PlayInterruptible,
                repeat: RepeatCount::Finite(1),
                pause_frames: 0,
                path: "part0".to_string(),
                background_color: Some(RgbColor::new(0, 0, 0)),
                clock: Some(ClockPosition {
                    x: Some(ClockCoord::Center),
                    y: ClockCoord::Offset(-24),
                }),
            },
            PartDeclaration {
                mode: PlaybackMode::SystemFallback,
                repeat: RepeatCount::Finite(1),
                pause_frames: 0,
                path: "$SYSTEM".to_string(),
                background_color: None,
                clock: None,
            },
        ],
        preserved_directives: vec![],
    };

    let text = serialize_descriptor(&desc).unwrap();
    let parsed = parse_descriptor(&text).unwrap();
    assert_eq!(desc, parsed);
}
