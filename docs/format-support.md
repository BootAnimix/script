# Boot Animation Format Support (`FORMAT.md` Reference)

Reference consulted: AOSP `platform/frameworks/base/+/master/cmds/bootanimation/FORMAT.md` (and Magisk module guide at `https://topjohnwu.github.io/Magisk/guides.html`).

## Status Categories

1. **Supported & Tested**: Parsed, validated, editable, previewed, and exported with automated tests.
2. **Parsed & Preserved**: Recognized by parser and preserved on round-trip, but not directly editable in basic UI controls.
3. **Parsed / Unsupported for Export**: Recognized so we can warn clearly rather than silently corrupting the archive.
4. **Invalid Input**: Rejected with a structured diagnostic and line/entry context.
5. **Requires Investigation**: Vendor-specific or hardware-dependent behavior.

## Format Feature Matrix (Initial Baseline — May 2025)

| Feature | AOSP Reference | Current 2.x (`main.py`) | 3.0 Target Policy | Compatibility / Notes |
| :--- | :--- | :--- | :--- | :--- |
| `bootanimation.zip` stored mode (`compression == 0`) | Mandatory (`zip -0qry`) | Uses `ZIP_STORED` in `compress`, unverified after write | **Supported & Tested** (verified by reopening archive) | Compressed entries fail to play on AOSP `BootAnimation.cpp`. |
| Header `WIDTH HEIGHT FPS` | Mandatory first line | Parsed via `readline().split()`, crashes on `[PROGRESS]` | **Supported & Tested** | Positive integers required. |
| Header `[PROGRESS]` token (`p`) | Optional 4th token on header line | Breaks `map(int, ...)` in 2.x | **Supported & Tested** | Displays progress % on last part if `progress_font.png` present. |
| `dynamic_colors PATH #RGB1 #RGB2 #RGB3 #RGB4` | Optional 2nd line | Treated as section line and crashes in 2.x | **Parsed & Preserved** (initially), then **Supported & Tested** | Uses RGBA channels as masks; end colors come from `persist.bootanim.color1..4`. |
| Part types `p` and `c` | Mandatory part syntax | Supported in `sort` | **Supported & Tested** | `p` interrupts on boot finish; `c` plays to completion. |
| Part type `f` (fade out) | `f COUNT PAUSE PATH FADE [#RGBHEX [CLOCK1 [CLOCK2]]]` | Broken (2.x assumes 4 tokens per line) | **Supported & Tested** | Note `FADE` is the 5th field for `f` parts only! |
| Special part `$SYSTEM` | Loads `/system/media/bootanimation.zip` | Unsupported | **Parsed & Preserved** | Warn in desktop preview since host has no `/system/media`. |
| Repeat `COUNT` (`0` = infinite) | Mandatory per part | Supported | **Supported & Tested** | `0` loops until boot finishes; preview must not invent fake finite duration. |
| `PAUSE` duration in frames | Mandatory per part (in frames, not seconds) | Always hardcoded to `0` in 2.x | **Supported & Tested** | Pausing holds the last frame of the part for `PAUSE` frames. |
| Part `#RGBHEX` background color | Optional `#RRGGBB` per part | Ignored in 2.x | **Supported & Tested** | 6-digit hex color (with optional `#`). |
| `CLOCK1` / `CLOCK2` coordinates | Optional watch clock coordinates (`c`, `n`, `-n`) | Ignored / breaks 2.x | **Supported & Tested** | Requires `clock_font.png` on device; preview discloses approximation. |
| PNG frames in `partN/` | Primary frame format in AOSP | 2.x converts everything to JPG | **Supported & Tested** (Default) | Keep source pixels untouched unless user asks for transform. |
| JPEG frames in `partN/` | Accepted by many AOSP builds, no alpha | Only format supported in 2.x | **Supported & Tested** (with warning if alpha expected) | Never auto-convert PNG to JPG behind the user's back. |
| `trim.txt` (`WxH+X+Y` per line) | Optional per-part file | Ignored in 2.x | **Parsed & Preserved** -> **Supported & Tested** in Phase 10 | Must match frame count in part directory. |
| `audio.wav` per part | Optional per-part WAV audio | Ignored in 2.x | **Parsed & Preserved** | Desktop preview notes audio.wav presence; some OEM ROMs disable boot audio. |
| `clock_font.png` & `progress_font.png` | Optional root archive textures | Ignored in 2.x | **Parsed & Preserved** | 16x6 ASCII glyph grid (top regular, bottom bold). |
