use bootanimix_core::encode_solid_png;
use std::process::Command;

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootanimix")
}

#[test]
fn cli_end_to_end_import_info_build_inspect_validate_preview_and_magisk() {
    let dir = tempfile::tempdir().unwrap();
    let frames_dir = dir.path().join("frames");
    std::fs::create_dir_all(&frames_dir).unwrap();

    let png0 = encode_solid_png(64, 64, [255, 64, 64, 255]).unwrap();
    let png1 = encode_solid_png(64, 64, [64, 255, 64, 255]).unwrap();
    std::fs::write(frames_dir.join("0000.png"), &png0).unwrap();
    std::fs::write(frames_dir.join("0001.png"), &png1).unwrap();

    // 1. doctor
    let doc_out = Command::new(cli_bin())
        .args(["--json", "doctor"])
        .output()
        .unwrap();
    assert!(doc_out.status.success());

    // 2. import
    let proj_dir = dir.path().join("proj");
    let imp_out = Command::new(cli_bin())
        .args([
            "--json",
            "import",
            frames_dir.to_str().unwrap(),
            "--output",
            proj_dir.to_str().unwrap(),
            "--fps",
            "30",
        ])
        .output()
        .unwrap();
    assert!(imp_out.status.success());

    // 3. info
    let info_out = Command::new(cli_bin())
        .args(["--json", "info", proj_dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(info_out.status.success());

    // 4. build
    let built_zip = dir.path().join("bootanimation.zip");
    let build_out = Command::new(cli_bin())
        .args([
            "--json",
            "build",
            proj_dir.to_str().unwrap(),
            "--output",
            built_zip.to_str().unwrap(),
            "--optimize-png",
        ])
        .output()
        .unwrap();
    assert!(build_out.status.success());
    assert!(built_zip.exists());

    // 5. inspect
    let insp_out = Command::new(cli_bin())
        .args(["--json", "inspect", built_zip.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(insp_out.status.success());
    let insp_json: serde_json::Value = serde_json::from_slice(&insp_out.stdout).unwrap();
    assert_eq!(insp_json["has_desc_txt"], true);
    assert_eq!(insp_json["compressed_entries"].as_array().unwrap().len(), 0);

    // 6. validate (valid archive returns exit code 0)
    let val_out = Command::new(cli_bin())
        .args(["--json", "validate", built_zip.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(val_out.status.success());

    // 7. preview schedule
    let prev_out = Command::new(cli_bin())
        .args([
            "--json",
            "preview",
            proj_dir.to_str().unwrap(),
            "--max-ticks",
            "20",
        ])
        .output()
        .unwrap();
    assert!(prev_out.status.success());

    // 8. export magisk
    let magisk_zip = dir.path().join("magisk_boot.zip");
    let magisk_out = Command::new(cli_bin())
        .args([
            "--json",
            "export",
            "magisk",
            proj_dir.to_str().unwrap(),
            "--output",
            magisk_zip.to_str().unwrap(),
            "--id",
            "bootanimix_test_mod",
        ])
        .output()
        .unwrap();
    assert!(magisk_out.status.success());
    assert!(magisk_zip.exists());
}

#[test]
fn cli_returns_nonzero_on_invalid_input_and_missing_paths() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does_not_exist.zip");
    let out = Command::new(cli_bin())
        .args(["inspect", missing.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
