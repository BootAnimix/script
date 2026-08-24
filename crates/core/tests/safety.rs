use bootanimix_core::{
    build_project_to_archive, encode_solid_png, import_frame_sequence_dir, safe_extract_zip,
    validate_archive_file, ProjectManifest, ValidationOutcome,
};
use std::fs::File;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[test]
fn failed_build_never_destroys_existing_valid_output_or_source_assets() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("source_assets");
    std::fs::create_dir_all(&src_dir).unwrap();

    let png = encode_solid_png(64, 64, [10, 20, 30, 255]).unwrap();
    let src_file = src_dir.join("000.png");
    std::fs::write(&src_file, &png).unwrap();

    let proj_dir = dir.path().join("proj");
    let mut manifest = import_frame_sequence_dir(&src_dir, &proj_dir, 30).unwrap();

    let out_zip = dir.path().join("bootanimation.zip");
    build_project_to_archive(&manifest, &proj_dir, &out_zip).unwrap();
    let valid_zip_bytes = std::fs::read(&out_zip).unwrap();

    // Now corrupt a project reference so the next build fails validation
    manifest.parts[0].frames.push(bootanimix_core::FrameAssetRef {
        relative_path: "assets/part0/nonexistent_missing_frame.png".to_string(),
        trim: None,
    });
    let res = build_project_to_archive(&manifest, &proj_dir, &out_zip);
    assert!(res.is_err());

    // Previous valid output must still be intact and valid!
    assert_eq!(std::fs::read(&out_zip).unwrap(), valid_zip_bytes);
    assert_eq!(
        validate_archive_file(&out_zip).unwrap().outcome,
        ValidationOutcome::Valid
    );
    // Original source asset must still be intact!
    assert_eq!(std::fs::read(&src_file).unwrap(), png);
}

#[test]
fn negative_tests_reject_animated_gif_and_malicious_zip_paths_and_corrupt_manifests() {
    let dir = tempfile::tempdir().unwrap();

    // 1. Animated GIF in frame folder rejected with clear message
    let gif_dir = dir.path().join("gif_folder");
    std::fs::create_dir_all(&gif_dir).unwrap();
    std::fs::write(gif_dir.join("anim.gif"), b"GIF89a").unwrap();
    let gif_err =
        import_frame_sequence_dir(&gif_dir, &dir.path().join("out_gif"), 30).unwrap_err();
    assert!(gif_err.to_string().contains(".gif"));

    // 2. Malicious ZIP path rejected before writing any file outside staging
    let evil_zip = dir.path().join("evil.zip");
    {
        let f = File::create(&evil_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("../../pwned.txt", opts).unwrap();
        zw.write_all(b"evil").unwrap();
        zw.finish().unwrap();
    }
    let extract_res = safe_extract_zip(&evil_zip, &dir.path().join("extract_target"));
    assert!(extract_res.is_err());
    assert!(!dir.path().join("pwned.txt").exists());

    // 3. Invalid / unknown schema_version in manifest rejected
    let bad_manifest = r#"{"schema_version": 999, "name": "future"}"#;
    assert!(ProjectManifest::from_json_str(bad_manifest).is_err());
}
