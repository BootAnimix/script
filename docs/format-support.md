# Boot Animation Format Support (`FORMAT.md` Reference)

Reference consulted: AOSP `platform/frameworks/base/+/master/cmds/bootanimation/FORMAT.md` and Magisk module specification (`https://topjohnwu.github.io/Magisk/guides.html`).

## Format Feature Matrix (BootAnimix 3.0 — October 2026)

| Feature | AOSP Reference | BootAnimix 3.0 Status | Parser / Serializer | Preview Engine | Validator / Tests | Notes & Compatibility |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `bootanimation.zip` `ZIP_STORED` (`0`) | Mandatory (`zip -0qry`) | **1. Supported & Tested** | Enforced on write; reopened & verified | N/A | `BA-ARC-001`, unit + proptest | Any compressed entry fails build verification and never overwrites previous output. |
| Header `WIDTH HEIGHT FPS` | Mandatory 1st line | **1. Supported & Tested** | Parsed & serialized with trailing `\n` | Sets canvas aspect & tick rate | `BA-DSC-001`, `BA-DSC-002` | Rejects `0` or >`16384` dimensions. |
| Header `[PROGRESS]` (`p`) | Optional 4th token | **1. Supported & Tested** | Parsed & serialized | Simulated % overlay + fidelity warning | `BA-DSC-006` (warns if `progress_font.png` missing) | Requires `progress_font.png` at archive root on device. |
| `dynamic_colors PATH #RGB1..4` | Optional 2nd line | **1. Supported & Tested** | Parsed, validated & round-tripped | Discloses runtime property limitation | `BA-DSC-006`, `descriptors.rs` | End colors come from `persist.bootanim.color1..4` on device. |
| Part modes `p` & `c` | Mandatory per part | **1. Supported & Tested** | Parsed & serialized | Models interruptible vs completion | `BA-DSC-004` (warns on `c 0` infinite loop) | `c 0` soft-hangs boot on many ROMs. |
| Part mode `f` (fade out) | `f COUNT PAUSE PATH FADE ...` | **1. Supported & Tested** | Parsed & serialized (`FADE` after `PATH`) | Models opacity decay on first interrupted `f` part | `descriptor::tests`, `playback::tests` | `FADE=0` behaves like `p`. |
| Special part `$SYSTEM` | `/system/media/bootanimation.zip` | **2. Parsed & Preserved** | Parsed & round-tripped | Emits desktop fidelity warning | `descriptors.rs` | Device-only fallback path. |
| Repeat `COUNT` (`0` = infinite) | Mandatory per part | **1. Supported & Tested** | Typed `RepeatCount::{Infinite, Finite(u32)}` | Never invents fake finite duration | `playback::tests` | Preview uses configurable tick window. |
| `PAUSE` duration in frames | Mandatory per part (in frames) | **1. Supported & Tested** | `pause_frames: u32` | Holds final frame of part for `PAUSE` ticks | `playback::tests` | Explicitly labeled in frames in CLI/TUI/GUI. |
| Part `#RGBHEX` background | Optional `#RRGGBB` | **1. Supported & Tested** | Typed `RgbColor` | Renders part background color | `descriptors.rs` | Used for `trim.txt` bounding-box calculation. |
| `CLOCK1` / `CLOCK2` coords | Optional (`c`, `n`, `-n`) | **1. Supported & Tested** | Typed `ClockPosition` / `ClockCoord` | Emits watch clock fidelity note | `BA-DSC-006`, `descriptors.rs` | Warns if `clock_font.png` is absent. |
| PNG & JPEG frames | PNG primary; JPEG common | **1. Supported & Tested** | Preserves source bytes unless transform requested | Decoded on demand | `BA-FRM-001`..`005` | Lossless PNG optimization verifies decoded RGBA pixels match. |
| `trim.txt` (`WxH+X+Y`) | Optional per-part file | **1. Supported & Tested** | Parsed, validated, and optionally generated | Draws trim bounds on preview canvas | `BA-TRM-001`, `pipeline::tests` | Checks line count == frame count and `X+W <= WIDTH`, `Y+H <= HEIGHT`. |
| `audio.wav` per part | Optional per-part WAV | **2. Parsed & Preserved** | Copied into part during import/build | Discloses OEM boot audio note | `pipeline.rs` | Many OEMs disable boot audio in SurfaceFlinger. |
| Unknown vendor directives | Non-AOSP custom lines | **3. Parsed / Refused for Rewrite** | Captured in `parse_descriptor_lenient` | N/A | `BA-DSC-007` | Refuses destructive `desc.txt` rewrite unless removed. |
