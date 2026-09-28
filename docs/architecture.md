# BootAnimix 3.0 Architecture

BootAnimix 3.0 is organized around a single shared Rust library (`bootanimix-core`) consumed by three first-class interfaces:

```text
bootanimix/
├── Cargo.toml                  # Rust workspace root (core, cli, tui, gui-backend)
├── crates/
│   ├── core/                   # Shared domain model, parser, archive engine, validator, scheduler
│   ├── cli/                    # Static-argument CLI (`bootanimix`)
│   └── tui/                    # Persistent Ratatui terminal workspace (`bootanimix-tui`)
├── apps/
│   └── gui/                    # Svelte 5 + TypeScript visual editor & Tauri 2 Rust backend
│       ├── src/                # Svelte components, canvas renderer, timeline, inspector
│       └── src-tauri/          # Rust command bridge calling `bootanimix-core`
├── docs/                       # Format matrix, compatibility notes, architecture, guides
├── tests/fixtures/             # Provenance and synthetic fixture documentation
└── main.py                     # Legacy 2.x Python utility (kept with safety fixes for reference)
```

## Core Responsibilities (`crates/core`)

1. **`model.rs` & `descriptor.rs` (`desc.txt` Parser & Serializer)**
   - Parses and serializes AOSP `desc.txt` headers (`WIDTH HEIGHT FPS [p]`), optional `dynamic_colors`, and part declarations (`p`, `c`, `f`, `$SYSTEM`) including `#RRGGBB` backgrounds and watch `CLOCK1`/`CLOCK2` coordinates.
   - Represents infinite loops (`COUNT == 0`) explicitly via `RepeatCount::Infinite` and pause holds in integer frames (`pause_frames: u32`).
   - Always emits a trailing LF (`\n`) so Android's `BootAnimation.cpp` never drops the final declaration.
   - Refuses to overwrite descriptors that contain preserved unknown vendor directives (`preserved_directives`).

2. **`frames.rs` (Deterministic Frame Ordering)**
   - Sorts numbered sequences naturally (`frame_1.png`, `frame_2.png`, `frame_10.png`) independent of OS filesystem traversal order.
   - Detects duplicate logical frame numbers (`01.png` vs `001.png`), mixed prefixes, and numbering gaps.
   - Generates canonical zero-padded names (`00000.png`, `00001.png`, ...) only when building output archives.

3. **`archive.rs` (Safe ZIP Engine & Mandatory `ZIP_STORED` Verification)**
   - `inspect_zip_file`: Read-only inspection of central directory entries, compression methods, CRC32s, duplicate names, and path safety.
   - `write_stored_bootanimation_zip`: Writes entries in deterministic order (`desc.txt` first) with `CompressionMethod::Stored` (`0`), then **reopens the written file** to verify that every entry has compression code `0` before committing transactionally over the destination.
   - `safe_extract_zip`: Rejects `..` traversal, absolute/drive-prefixed paths, backslashes, symlinks, and entries exceeding per-file (64 MiB) or total (2 GiB) decompression bounds.

4. **`validator.rs` (Structured Diagnostics)**
   - Emits stable diagnostic codes (`BA-ARC-001`..`005`, `BA-DSC-001`..`007`, `BA-FRM-001`..`005`, `BA-TRM-001`, `BA-PRF-001`) with severity, file/line context, remediation guidance, and `blocks_export`.

5. **`pipeline.rs` & `magisk.rs` (Import, Explicit Transforms, Build & Magisk Export)**
   - Keeps source assets immutable. Pixel transformations (resize, alpha flatten, format conversion, pixel-verified lossless PNG recompression, and `trim.txt` bounding-box calculation) run only when explicitly enabled.
   - Packages two-layer Magisk modules (`module.prop` + `system/media/bootanimation.zip` or custom overlay target) and verifies both outer and inner ZIP archives independently.

6. **`playback.rs` & `editor.rs` (Preview Scheduler & Undo/Redo Session)**
   - Computes timeline ticks, loop iterations, pause holds, and `f`-part fade-out opacity without loading all decoded images into RAM.
   - `EditableProjectSession` maintains bounded undo/redo stacks over lightweight `ProjectManifest` snapshots.
