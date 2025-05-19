# Compatibility & Device Notes

Android's `bootanimation` binary (`frameworks/base/cmds/bootanimation`) has a deceptively simple format, but OEM customizations and subtle rules in `BootAnimation.cpp` cause real-world failures when tools cut corners.

## 1. Archive & Storage Rules

- **`ZIP_STORED` (Method `0`) is mandatory for `bootanimation.zip`:**
  - `BootAnimation.cpp` maps the archive into memory (`FileMap`) and reads frame entries directly. Any entry compressed with `DEFLATE` (method `8`) is skipped or causes the animation to abort back to the stock Android shine logo.
  - Note: When packaging a **Magisk module**, the outer `module.zip` can be standard DEFLATE or Stored, because Magisk extracts `module.zip` into `/data/adb/modules/<id>/` during installation. However, the inner `system/media/bootanimation.zip` placed inside the module **must** remain uncompressed (`compression_method == 0` for every entry).

- **Entry Ordering & Paths:**
  - `desc.txt` should be at the root of `bootanimation.zip` (no top-level wrapper folder like `my_anim/desc.txt`).
  - Part frames are loaded in the order they appear / sort within their part directory. Using zero-padded sequential names (`00000.png`, `00001.png`, ...) and writing entries in deterministic sorted order avoids playback glitches.
  - Archive paths must always use forward slashes (`/`) even when built on Windows.

## 2. Image Formats (PNG vs JPEG)

- **PNG (Recommended Default):**
  - Official AOSP `FORMAT.md` specifies PNG frames.
  - 8-bit RGB or RGBA PNGs work reliably across stock AOSP, LineageOS, Pixel, and custom ROMs.
  - 16-bit per channel PNGs or unusual interlaced PNGs can fail or render improperly on some older Skia/OpenGL ES decoders; warn when encountered.
- **JPEG:**
  - Supported by AOSP's decoder in practice and used by many older animations (`main.py` in 2.x only output `.jpg`), but introduces lossy compression artifacts on flat logos/text and does not support transparency.
  - BootAnimix 3.0 keeps source frames in their original format by default and uses PNG as the default when generating new frames.

## 3. Part Types & Timing (`p`, `c`, `f`, `$SYSTEM`)

- `p` (play unless interrupted): Loops `COUNT` times (`0` = infinite until `service.bootanim.exit` is set), then pauses `PAUSE` frames. If boot completes mid-part, playback stops immediately (or moves to remaining `c` parts).
- `c` (play to completion): Plays all frames and repeats even after the system signals boot completion. Caution: setting `c 0 ...` (infinite loop on a `c` part) can soft-hang the boot screen on some builds because the animation refuses to exit! The validator flags `c` parts with `COUNT == 0` as a high-severity warning.
- `f` (fade out when interrupted): Syntax is `f COUNT PAUSE PATH FADE [#RGBHEX [CLOCK1 [CLOCK2]]]`. Notice `FADE` is an extra integer field right after `PATH` that does **not** exist on `p` or `c` lines.

## 4. System Overlay Paths (Magisk)

While AOSP checks `/system/media/bootanimation.zip` first and `/oem/media/bootanimation.zip` second, real devices vary:
- **AOSP / LineageOS / most custom ROMs:** `/system/media/bootanimation.zip`
- **Google Pixel / modern Treble devices:** Often `/system/product/media/bootanimation.zip` (mounted at `/product/media/bootanimation.zip`)
- **Some OEMs (OnePlus / Xiaomi / Nothing):** May use `/system/product/media/bootanimation.zip` or `/system/system_ext/media/bootanimation.zip`. Samsung OneUI uses proprietary `.qmg` files and does **not** play `bootanimation.zip` without a custom bootanimation binary.

Because of this, the Magisk module exporter defaults to `system/media/bootanimation.zip` (or `system/product/media/bootanimation.zip` when selected) and allows overriding the target path inside the module.
