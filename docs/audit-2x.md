# BootAnimix 2.x Audit & 3.0 Notes (May 2025)

## What is currently in `main.py` (2.x / "3.1.0" in pyproject.toml)

Looked through `main.py` from the Jan 2025 revamp (`273f585`). While it works for basic video-to-JPG experiments, there are several nasty bugs and design problems that break real boot animations or destroy user files:

1. **Destructive cleanup on error or completion:**
   - `handle_vid2jpg` runs `shutil.rmtree(output_dir, ignore_errors=True)` in the `except` block. If `output_dir` already existed and had user files in it, a failed video decode wipes the whole folder.
   - `handle_pic2jpg` calls `img_path.unlink()` immediately after saving `.jpg`, permanently deleting the original `.png`/`.webp` source frames in place.
   - `handle_zip2vid` extracts into `Path(zip_file).stem`, flattens section folders in place, and then runs `shutil.rmtree(folder)` at the end.

2. **Unsafe ZIP extraction (`handle_uncompress` & `handle_zip2vid`):**
   - Calls `zipf.extractall(...)` directly with no check for `../` path traversal, absolute paths, or zip bombs.

3. **Fragile `desc.txt` handling:**
   - `get_section_names` splits lines after line 0 and blindly takes `line.split()[3]`. Comment lines (`#`), blank lines, `dynamic_colors`, or `f` parts with extra fields either crash with `IndexError` or return the wrong token (`FADE` count instead of `PATH`!).
   - `handle_sort` appends `\n` + sections to `desc.txt` using `"a"` mode, so sorting twice corrupts `desc.txt` instead of regenerating it cleanly.
   - `handle_vid2jpg` writes `f"{clip.w} {clip.h} {int(fps)}"` without a trailing newline.

4. **Format & image issues vs AOSP `FORMAT.md`:**
   - Everything in `main.py` forces `.jpg` (`vid2jpg`, `pic2jpg`, `sort`, `zip2vid`), whereas AOSP `FORMAT.md` treats PNG as the primary frame format and recommends lossless PNG / `ZIP_STORED`.
   - `pic2jpg` converts RGBA images to RGB with `.convert("RGB")`, silently flattening alpha to black.
   - `detect_prefix` grabs `list(folder.glob("*.jpg"))[0]`, which depends on filesystem iteration order and fails if files aren't uniformly named.

5. **Architecture:**
   - Everything is coupled to interactive `rich.prompt.Prompt.ask` loops inside a single 400-line script. You can't run it non-interactively in scripts, test functions in isolation, or share logic with a proper TUI or GUI.

## Plan for 3.0

Instead of piling more prompts onto `main.py`, I want to build BootAnimix 3.0 properly:
- Move core logic (descriptor parser/serializer, frame ordering, safe ZIP inspection/packing, validator) into a shared **Rust** library (`crates/core`).
- Keep source assets strictly read-only unless an explicit transformation is requested into a build/staging area.
- Build three clean interfaces on top of the same Rust core:
  - `cli` (static flags with `clap`)
  - `tui` (persistent terminal UI with `ratatui`)
  - `gui` (desktop visual editor with Tauri + Svelte)

## Task Checklist

| Milestone | Status | Notes |
| :--- | :--- | :--- |
| Phase 0: Audit 2.x & document AOSP format support | In progress | `main.py` audited; writing `docs/format-support.md` next |
| Phase 1: Rust core model, `desc.txt` parser & frame ordering | Not started | Need lossless round-trip and `f` part / clock / color support |
| Phase 2: Safe ZIP engine (strict `STORE` check, safe extract) | Not started | Must reopen archive and verify compression method == 0 |
| Phase 3: Validator & structured diagnostics | Not started | Stable diagnostic codes + golden fixtures |
| Phase 4: Import & build pipeline (frames + FFmpeg) | Not started | Optional FFmpeg without `moviepy` memory blowup |
| Phase 5: Playback scheduler & preview engine | Not started | Model `p`, `c`, `f`, repeats, pauses accurately |
| Phase 6: CLI (`bootanimix`) | Not started | `inspect`, `validate`, `import`, `build`, `preview`, `export` |
| Phase 7: TUI (`ratatui`) | Not started | Interactive workspace using `bootanimix-core` |
| Phase 8: GUI (Tauri 2 + Svelte) | Not started | Timeline, preview canvas, inspector, undo/redo |
| Phase 9: Magisk module exporter | Not started | `module.prop` + `system/media/bootanimation.zip` |
| Phase 10: Lossless optimization & `trim.txt` | Not started | Explicit opt-in only |
