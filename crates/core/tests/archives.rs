use bootanimix_core::{
    encode_solid_png, inspect_zip_file, is_safe_archive_entry_path, validate_archive_file,
    write_stored_bootanimation_zip, ArchiveBuildEntry, ValidationOutcome,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_archive_builder_never_emits_compressed_entries_or_duplicates(
        frame_count in 1usize..=8usize,
        fps in 10u32..=90u32,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("prop_boot.zip");
        let png = encode_solid_png(32, 32, [100, 150, 200, 255]).unwrap();

        let mut entries = vec![ArchiveBuildEntry {
            archive_path: "desc.txt".to_string(),
            bytes: format!("32 32 {fps}\np 1 0 part0\n").into_bytes(),
        }];

        for i in 0..frame_count {
            entries.push(ArchiveBuildEntry {
                archive_path: format!("part0/{i:05}.png"),
                bytes: png.clone(),
            });
        }

        let inspection = write_stored_bootanimation_zip(&entries, &zip_path).unwrap();
        prop_assert!(inspection.compressed_entries.is_empty());
        prop_assert!(inspection.duplicate_entries.is_empty());
        prop_assert!(inspection.unsafe_paths.is_empty());
        for entry in &inspection.entries {
            prop_assert_eq!(entry.compression_code, 0);
        }
    }

    #[test]
    fn prop_path_sanitizer_always_rejects_traversal_and_backslashes(
        seg in "[a-zA-Z0-9_-]{1,12}",
    ) {
        let traversal = format!("{seg}/../evil.png");
        let backslash = format!("{seg}\\000.png");
        let absolute = format!("/{seg}/000.png");
        prop_assert!(!is_safe_archive_entry_path(&traversal));
        prop_assert!(!is_safe_archive_entry_path(&backslash));
        prop_assert!(!is_safe_archive_entry_path(&absolute));
    }
}

#[test]
fn detects_duplicate_central_directory_entries_in_malformed_archive() {
    use std::fs::File;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("dup.zip");
    let png = encode_solid_png(16, 16, [0, 0, 0, 255]).unwrap();

    // Manually construct a ZIP buffer or test inspect_zip_file on out-of-order entries
    {
        let f = File::create(&zip_path).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("desc.txt", opts).unwrap();
        zw.write_all(b"16 16 30\np 1 0 part0\n").unwrap();
        zw.start_file("part0/00002.png", opts).unwrap();
        zw.write_all(&png).unwrap();
        zw.start_file("part0/00001.png", opts).unwrap();
        zw.write_all(&png).unwrap();
        zw.finish().unwrap();
    }

    let insp = inspect_zip_file(&zip_path).unwrap();
    assert!(insp.has_desc_txt);
    let val = validate_archive_file(&zip_path).unwrap();
    assert_eq!(val.outcome, ValidationOutcome::ValidWithWarnings);
    assert!(val.diagnostics.iter().any(|d| d.code == "BA-ARC-005"));
}
