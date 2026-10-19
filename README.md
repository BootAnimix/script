# BootAnimix 3.0

**BootAnimix 3.0** is a local-first, cross-platform Android boot-animation inspection, validation, editing, preview, and packaging suite built around a shared Rust core (`bootanimix-core`) with three first-class interfaces:

1. **CLI (`bootanimix`)** — Automation-friendly static-argument command-line interface with human and `--json` output.
2. **TUI (`bootanimix-tui`)** — Persistent interactive terminal workspace powered by Ratatui.
3. **GUI (`apps/gui`)** — Visual desktop studio built with Tauri 2, Svelte 5, and TypeScript featuring an asset browser, playback preview canvas, filmstrip timeline, properties inspector, interactive diagnostics, undo/redo history, and Magisk module export.

---

## Core Engineering Guarantees

- **Mandatory `ZIP_STORED` (`0`) Verification:** Every generated `bootanimation.zip` (and the inner archive inside exported Magisk modules) is written using `ZIP_STORED` (method `0`), then reopened and inspected to verify that no compressed entries exist before committing transactionally.
- **Non-Destructive Asset Handling:** Source images and existing valid output archives are never silently deleted, overwritten on failure, resized, or converted to JPEG behind your back.
- **Specification-Backed `desc.txt` & `trim.txt` Support:** Handles `p`, `c`, `f` (fade-out), `$SYSTEM`, `dynamic_colors`, `#RRGGBB` backgrounds, watch `CLOCK1`/`CLOCK2` coordinates, and `trim.txt` (`WxH+X+Y`) bounding boxes per [AOSP `FORMAT.md`](https://android.googlesource.com/platform/frameworks/base/+/master/cmds/bootanimation/FORMAT.md).

---

## Supported Platforms & Prerequisites

- **Operating Systems:** Windows, Linux, and macOS.
- **Rust Toolchain:** `rustc` / `cargo` (1.80+ recommended).
- **Node.js:** v20+ / npm (for building the Svelte 5 GUI frontend).
- **FFmpeg (Optional):** Used only for video import (`import <video.mp4>`) and MP4 preview export (`preview --output <out.mp4>`). Core ZIP inspection, validation, frame-sequence import, editing, and packaging do **not** require FFmpeg.

---

## Quick Start & Development Setup

```bash
# Build and run all Rust unit, integration, and property tests
cargo test --workspace

# Run the CLI doctor check
cargo run -p bootanimix-cli -- doctor

# Launch the persistent Ratatui TUI workspace
cargo run -p bootanimix-tui -- ./path/to/project

# Type-check and build the Svelte + TypeScript GUI studio
cd apps/gui
npm install
npm run check
npm run build
```

---

## CLI Usage Examples

```bash
# Inspect an existing bootanimation.zip without modifying it
bootanimix inspect bootanimation.zip

# Validate an archive or project directory (emit machine-readable JSON with --json)
bootanimix validate bootanimation.zip
bootanimix --json validate ./my_project

# Import a numbered PNG/JPG frame folder or video into a BootAnimix project
bootanimix import ./frames_dir --output ./my_project --fps 60
bootanimix import ./intro.mp4 --output ./my_project --fps 60 --dimensions 1080x2400

# Build a verified ZIP_STORED bootanimation.zip (with optional lossless PNG & trim.txt)
bootanimix build ./my_project --output ./dist/bootanimation.zip --optimize-png --generate-trim

# Export a flashable Magisk module with two-layer archive verification
bootanimix export magisk ./my_project \
  --output ./dist/magisk_bootanim.zip \
  --id bootanimix_custom \
  --target-path system/product/media/bootanimation.zip
```

---

## Known Limitations & Compatibility Notes

- **No Physical Android Device / Emulator Attached in CI:** Desktop validation proves structural, image, and `ZIP_STORED` invariants against AOSP `FORMAT.md`, but cannot guarantee that every OEM's customized `BootAnimation` binary renders identical timing or memory behavior on hardware.
- **OEM-Specific Formats & Paths:** Samsung OneUI uses proprietary `.qmg` boot animations and ignores `bootanimation.zip` on stock kernels. Google Pixel and modern Treble devices typically load `/product/media/bootanimation.zip` (use `--target-path system/product/media/bootanimation.zip` when exporting a Magisk module).
- **Desktop Preview Approximations:** Watch clock glyphs (`clock_font.png`), boot progress (`progress_font.png`), and `dynamic_colors` end-color system properties (`persist.bootanim.color1..4`) depend on Android runtime properties and are disclosed with fidelity warnings in the preview engine.

---

## Documentation

- [Architecture Overview](docs/architecture.md)
- [AOSP Format Support Matrix](docs/format-support.md)
- [Compatibility & Magisk Overlay Notes](docs/compatibility.md)
- [User Guide & Troubleshooting](docs/user-guide.md)
- [2.x Baseline Audit Notes](docs/audit-2x.md)
- [Test Fixture Provenance](tests/fixtures/README.md)
- [Changelog](CHANGELOG.md)

## License

Released under the [MIT License](LICENSE).
