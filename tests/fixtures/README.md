# Test Fixtures & Provenance

All test fixtures used by BootAnimix's unit and integration test suites are **100% synthetic** and generated deterministically during test execution (or stored as small plain-text descriptor samples in `tests/fixtures/`). No third-party copyrighted boot animations or external media files are bundled in the repository.

## Synthetic Fixture Inventory

1. **Minimal valid animation (`minimal_valid`)** — 64x64 @ 30fps, 1 part (`p 1 0 part0`), 2 solid-color RGBA PNG frames stored with `ZIP_STORED`.
2. **Multi-part finite & infinite loop (`multi_part_loops`)** — 3 parts (`p 1 0 part0`, `f 0 10 part1 5 #112233 c -24`, `c 1 6 part2`).
3. **Trimmed frames (`trimmed_part`)** — 64x64 canvas with 32x32+16+16 trimmed PNG frames and matching `trim.txt`.
4. **Optional metadata (`metadata_clock_progress`)** — Includes `clock_font.png`, `progress_font.png`, `dynamic_colors`, and `part0/audio.wav`.
5. **Preserved unknown directive (`unknown_vendor_directive`)** — Includes a non-standard vendor directive line tested via `parse_descriptor_lenient`.
6. **Malformed descriptors (`malformed_descriptors`)** — Zero dimensions, missing `FADE` field on `f` part, invalid hex colors, and `../` part paths.
7. **Corrupt PNG (`corrupt_png`)** — Truncated/invalid PNG payload inside `part0/00000.png` (`BA-FRM-001`).
8. **Missing part directory (`missing_part_dir`)** — `desc.txt` references `part1` which has no entries in the archive (`BA-DSC-005`).
9. **Duplicate archive entries (`duplicate_entries`)** — Raw central directory containing two `part0/00000.png` entries (`BA-ARC-003`).
10. **Deflated inner entry (`deflated_entry`)** — Archive built with `CompressionMethod::Deflated` (`BA-ARC-001`).
11. **Unsafe path traversal (`unsafe_traversal`)** — Archive containing `../escape.png` (`BA-ARC-002`).

## Licensing

All synthetic fixtures and test generators are licensed under the repository's MIT License.
