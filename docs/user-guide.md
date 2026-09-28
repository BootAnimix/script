# BootAnimix 3.0 User Guide & Troubleshooting

## 1. Importing & Creating a Project

You can start a project from a directory of numbered PNG/JPG frames, an existing `bootanimation.zip`, or an MP4/WebM video (when FFmpeg is installed):

```bash
# Import a directory of PNG frames at 60 FPS
bootanimix import ./my_frames --output ./my_project --fps 60

# Import an existing bootanimation.zip for inspection and editing
bootanimix import ./bootanimation.zip --output ./my_project

# Import a video file using FFmpeg (explicit dimensions and FPS)
bootanimix import ./intro.mp4 --output ./my_project --fps 60 --dimensions 1080x2400
```

Source files are copied into `<project>/assets/` and referenced by `<project>/project.bootanimix.json`. Original files are never deleted or overwritten.

## 2. Editing Parts, Repeats & Pauses

Each part in `desc.txt` has the form:

```text
TYPE COUNT PAUSE PATH [FADE [#RGBHEX [CLOCK1 [CLOCK2]]]]
```

- **Playback Mode (`TYPE`):**
  - `p`: Plays while the device is booting; stops immediately once Android sets `service.bootanim.exit`.
  - `c`: Plays to completion even if Android finishes booting mid-part.
  - `f`: Plays like `p`, but fades out over `FADE` frames when interrupted.
- **Repeat Count (`COUNT`):**
  - `1`, `2`, ...: Plays a fixed number of times.
  - `0`: Loops **forever** until boot finishes. (Do not combine `c` with `0`, or the boot animation will never exit!)
- **Pause Duration (`PAUSE`):**
  - Measured in **frames** (not seconds!). A pause of `15` at `30 FPS` holds the final frame of the part for `0.5s` before advancing.

## 3. Building & Validating Archives

```bash
# Validate a project or bootanimation.zip (add --json for CI/scripts)
bootanimix validate ./my_project

# Build a verified ZIP_STORED bootanimation.zip
bootanimix build ./my_project --output ./dist/bootanimation.zip

# Optional explicit transformations during build:
bootanimix build ./my_project --output ./dist/bootanimation.zip --optimize-png --generate-trim
```

## 4. Exporting a Flashable Magisk Module

```bash
bootanimix export magisk ./my_project \
  --output ./dist/pixel_boot_module.zip \
  --id bootanimix_custom \
  --name "Custom Boot Animation" \
  --target-path system/product/media/bootanimation.zip
```

---

## Troubleshooting

| Symptom / Diagnostic Code | Cause | Solution |
| :--- | :--- | :--- |
| **`BA-ARC-001`: Compressed entries found** | Archive was created with standard ZIP Deflate (`method 8`). Android's `BootAnimation.cpp` requires uncompressed `ZIP_STORED` (`method 0`). | Import the archive into BootAnimix and run `bootanimix build` to repackage with verified `ZIP_STORED`. |
| **Black screen or stock Android shine logo on boot** | Either `desc.txt` is inside a subfolder (`my_folder/desc.txt`), entries are compressed, or the last line of `desc.txt` had no trailing newline. | Run `bootanimix validate bootanimation.zip` to check root `desc.txt`, frame dimensions, and storage mode. |
| **Magisk module flashes cleanly, but boot animation doesn't change** | Your device reads `/product/media/bootanimation.zip` instead of `/system/media/bootanimation.zip` (common on Pixel / Treble ROMs), or runs Samsung OneUI (`.qmg`). | Re-export the Magisk module with `--target-path system/product/media/bootanimation.zip`. |
| **`BA-DSC-004`: `c` part with `COUNT=0`** | A play-to-completion (`c`) part is configured to loop infinitely (`0`). | Change the part mode to `p` (or `f`), or set a finite repeat count (`1`). |
| **`BA-FRM-003`: Frame dimension mismatch** | One or more PNG/JPG frames do not match the header `WIDTH HEIGHT` (or `trim.txt` `WxH`). | Pass `--resize WIDTHxHEIGHT` during `bootanimix build` or update the header dimensions in the Inspector. |
| **FFmpeg not found (`bootanimix doctor`)** | `ffmpeg` is not installed or not in your system `PATH`. | Install FFmpeg (`winget install Gyan.FFmpeg`, `brew install ffmpeg`, or `apt install ffmpeg`). Note that ZIP inspection, validation, frame import, and builds work without FFmpeg. |
