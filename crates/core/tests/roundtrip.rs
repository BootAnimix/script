use bootanimix_core::{
    build_project_to_archive, encode_solid_png, export_magisk_module, import_archive_to_project,
    import_frame_sequence_dir, validate_archive_file, ValidationOutcome,
};

#[test]
fn full_roundtrip_frames_to_project_to_archive_to_reimported_project_and_magisk() {
    let dir = tempfile::tempdir().unwrap();
    let raw_frames = dir.path().join("raw_frames");
    std::fs::create_dir_all(&raw_frames).unwrap();

    let f0 = encode_solid_png(128, 128, [12, 34, 56, 255]).unwrap();
    let f1 = encode_solid_png(128, 128, [78, 90, 112, 255]).unwrap();
    let f2 = encode_solid_png(128, 128, [200, 100, 50, 255]).unwrap();

    // Write out-of-order names on purpose to verify deterministic numeric ordering
    std::fs::write(raw_frames.join("img_10.png"), &f2).unwrap();
    std::fs::write(raw_frames.join("img_2.png"), &f1).unwrap();
    std::fs::write(raw_frames.join("img_1.png"), &f0).unwrap();

    let proj_a = dir.path().join("proj_a");
    let manifest_a = import_frame_sequence_dir(&raw_frames, &proj_a, 60).unwrap();
    assert_eq!(manifest_a.parts[0].frames.len(), 3);

    let archive_a = dir.path().join("boot_a.zip");
    let report_a = build_project_to_archive(&manifest_a, &proj_a, &archive_a).unwrap();
    assert!(report_a.invariants_verified);
    assert_eq!(report_a.validation.outcome, ValidationOutcome::Valid);

    // Re-import archive_a into proj_b and rebuild archive_b
    let proj_b = dir.path().join("proj_b");
    let manifest_b = import_archive_to_project(&archive_a, &proj_b).unwrap();
    let archive_b = dir.path().join("boot_b.zip");
    let report_b = build_project_to_archive(&manifest_b, &proj_b, &archive_b).unwrap();
    assert!(report_b.invariants_verified);

    // Rebuilds of equivalent unchanged projects produce identical archives
    let archive_b2 = dir.path().join("boot_b2.zip");
    build_project_to_archive(&manifest_b, &proj_b, &archive_b2).unwrap();
    assert_eq!(
        std::fs::read(&archive_b).unwrap(),
        std::fs::read(&archive_b2).unwrap()
    );

    // Export Magisk module and verify both layers
    let magisk_out = dir.path().join("module.zip");
    let magisk_rep = export_magisk_module(&manifest_b, &proj_b, &magisk_out).unwrap();
    assert!(magisk_rep.inner_stored_verified);
    assert_eq!(
        validate_archive_file(&archive_b).unwrap().outcome,
        ValidationOutcome::Valid
    );
}
