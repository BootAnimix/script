# Changelog

## [3.0.0] - 2026-10-19

### Added
- **Shared Rust Core (`bootanimix-core`)**:
  - Typed `AnimationDescriptor` parser and deterministic serializer supporting `p`, `c`, `f` (fade-out), `$SYSTEM`, `dynamic_colors`, progress flag, `#RRGGBB` backgrounds, and `CLOCK1`/`CLOCK2` coordinates.
  - Natural numeric frame ordering (`order_frame_names`, `scan_directory_frames`) independent of filesystem iteration order.
  - Safe archive engine (`inspect_zip_file`, `write_stored_bootanimation_zip`, `safe_extract_zip`) that enforces `ZIP_STORED` (`compression == 0`), reopens generated archives to verify invariants, commits outputs transactionally, and blocks path traversal / zip bombs.
  - Structured validator (`validate_archive_file`, `validate_project`) with stable diagnostic codes (`BA-ARC-*`, `BA-DSC-*`, `BA-FRM-*`, `BA-TRM-*`, `BA-PRF-*`).
  - Frame-sequence import, optional FFmpeg video import (`import_video_with_ffmpeg`), pixel-verified lossless PNG optimization, and `trim.txt` bounding-box generation.
  - Accurate playback scheduler (`build_playback_schedule`) modeling repeats, infinite loops (`COUNT=0`), frame-based pauses, and fade-out parts.
  - First-class Magisk module exporter (`export_magisk_module`) with two-layer archive verification and configurable overlay paths (`system/media/bootanimation.zip`, `system/product/media/bootanimation.zip`, etc.).
- **CLI (`bootanimix`)**: Static-argument commands (`inspect`, `validate`, `import`, `build`, `preview`, `export magisk`, `info`, `doctor`) with `--json` output.
- **TUI (`bootanimix-tui`)**: Persistent Ratatui terminal workspace with 5 panes, command bar autocomplete, and background worker threads.
- **GUI (`apps/gui`)**: Tauri 2 + Svelte 5 + TypeScript visual editor featuring an asset browser, live preview canvas, filmstrip timeline, properties inspector, interactive diagnostics, undo/redo history, and reviewed export dialog. The desktop shell is a real Tauri app: the Rust core owns the session state and the export path, so "Verify & Write Package" now writes a real `ZIP_STORED` archive (or Magisk module) instead of only logging a result.
- **Release automation (`.github/workflows/release.yml`)**: Pushing a `v*` tag builds smoke-tested `bootanimix` and `bootanimix-tui` release binaries for Linux (x86_64), Windows (x86_64), and macOS (arm64 + x86_64) and publishes them to a GitHub Release with SHA-256 checksums, together with native **BootAnimix Studio** installers (NSIS/MSI, `.app`/`.dmg`, `.deb`/`.rpm`/`.AppImage`) built on per-OS runners. macOS bundles are ad-hoc signed so Gatekeeper does not flag them as damaged.

### Fixed
- `bootanimix-tui` now handles `--help`/`-h` and `--version`/`-V` instead of treating them as a project path and opening the interactive TUI.

### Removed
- Legacy 2.x Python tooling (`main.py`, `pyproject.toml`, `requirements.txt`). The Rust core and its three interfaces now cover every workflow the old script provided; the audit of that code lives on in `docs/audit-2x.md` and its history remains in Git.

### Fixed (from 2.x)
- Fixed destructive `shutil.rmtree` and unconditional `img_path.unlink()` in legacy `main.py` that could delete user source files.
- Fixed path-traversal vulnerability in ZIP extraction.
- Fixed `desc.txt` generator omitting the final trailing newline.

## [2.x / 3.1.0-python] - 2025-01-31
- Initial Python + Rich + MoviePy interactive script (`main.py`).
