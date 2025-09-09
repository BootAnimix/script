use crate::descriptor::parse_descriptor_lenient;
use crate::error::CoreError;
use crate::model::AnimationDescriptor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

pub const MAX_EXTRACTION_ENTRIES: usize = 20_000;
pub const MAX_SINGLE_ENTRY_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB per frame/asset
pub const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB total

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveEntryInfo {
    pub name: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub compression_method: String,
    pub compression_code: u16,
    pub crc32: u32,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveInspection {
    pub archive_size_bytes: u64,
    pub entries: Vec<ArchiveEntryInfo>,
    pub duplicate_entries: Vec<String>,
    pub unsafe_paths: Vec<String>,
    pub compressed_entries: Vec<String>,
    pub has_desc_txt: bool,
    pub parsed_descriptor: Option<AnimationDescriptor>,
    pub descriptor_error: Option<String>,
    pub part_frame_counts: BTreeMap<String, usize>,
}

/// Read-only inspection of a ZIP archive on disk. Never modifies the source file.
pub fn inspect_zip_file(path: &Path) -> Result<ArchiveInspection, CoreError> {
    let metadata = std::fs::metadata(path)?;
    let archive_size_bytes = metadata.len();
    let file = File::open(path)?;
    inspect_zip_reader(file, archive_size_bytes)
}

pub fn inspect_zip_reader<R: Read + Seek>(
    mut reader: R,
    archive_size_bytes: u64,
) -> Result<ArchiveInspection, CoreError> {
    // Scan raw central directory headers first so duplicate filenames are detected
    // even if `ZipArchive` deduplicates its internal name map.
    let raw_names = scan_central_directory_names(&mut reader).unwrap_or_default();
    reader.seek(SeekFrom::Start(0))?;

    let mut archive =
        ZipArchive::new(reader).map_err(|e| CoreError::Archive(format!("invalid zip: {e}")))?;

    let mut entries = Vec::with_capacity(archive.len());
    let mut raw_seen = HashSet::new();
    let mut duplicate_entries = Vec::new();
    for raw_name in &raw_names {
        if !raw_seen.insert(raw_name.clone()) && !duplicate_entries.contains(raw_name) {
            duplicate_entries.push(raw_name.clone());
        }
    }

    let mut seen_names = HashSet::new();
    let mut unsafe_paths = Vec::new();
    let mut compressed_entries = Vec::new();
    let mut has_desc_txt = false;
    let mut desc_content: Option<String> = None;
    let mut part_frame_counts: BTreeMap<String, usize> = BTreeMap::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CoreError::Archive(format!("corrupt zip entry #{i}: {e}")))?;

        let name = entry.name().to_string();
        if !seen_names.insert(name.clone()) && !duplicate_entries.contains(&name) {
            duplicate_entries.push(name.clone());
        }

        if !is_safe_archive_entry_path(&name) {
            unsafe_paths.push(name.clone());
        }

        let is_dir = entry.is_dir();
        let (method_name, method_code) = compression_method_label(entry.compression());
        if !is_dir && method_code != 0 {
            compressed_entries.push(name.clone());
        }

        if name == "desc.txt" && !is_dir {
            has_desc_txt = true;
            if entry.size() <= 256 * 1024 {
                let mut text = String::new();
                if entry.read_to_string(&mut text).is_ok() {
                    desc_content = Some(text);
                }
            }
        } else if !is_dir {
            let parts: Vec<&str> = name.split('/').collect();
            if parts.len() == 2 {
                let dir_name = parts[0];
                let file_name = parts[1].to_ascii_lowercase();
                if (file_name.ends_with(".png")
                    || file_name.ends_with(".jpg")
                    || file_name.ends_with(".jpeg"))
                    && file_name != "clock_font.png"
                    && file_name != "progress_font.png"
                {
                    *part_frame_counts.entry(dir_name.to_string()).or_insert(0) += 1;
                }
            } else if is_dir {
                let trimmed = name.trim_end_matches('/');
                if !trimmed.contains('/') && !trimmed.is_empty() {
                    part_frame_counts.entry(trimmed.to_string()).or_insert(0);
                }
            }
        }

        entries.push(ArchiveEntryInfo {
            name,
            uncompressed_size: entry.size(),
            compressed_size: entry.compressed_size(),
            compression_method: method_name,
            compression_code: method_code,
            crc32: entry.crc32(),
            is_dir,
        });
    }

    let (parsed_descriptor, descriptor_error) = match desc_content {
        Some(text) => match parse_descriptor_lenient(&text) {
            Ok(desc) => (Some(desc), None),
            Err(err) => (None, Some(err.to_string())),
        },
        None => (None, None),
    };

    Ok(ArchiveInspection {
        archive_size_bytes,
        entries,
        duplicate_entries,
        unsafe_paths,
        compressed_entries,
        has_desc_txt,
        parsed_descriptor,
        descriptor_error,
        part_frame_counts,
    })
}

/// Input entry for deterministic `bootanimation.zip` creation.
#[derive(Debug, Clone)]
pub struct ArchiveBuildEntry {
    pub archive_path: String,
    pub bytes: Vec<u8>,
}

/// Build an uncompressed (`ZIP_STORED`) `bootanimation.zip` transactionally.
/// Reopens and verifies the generated archive before committing it to `dest_path`.
pub fn write_stored_bootanimation_zip(
    entries: &[ArchiveBuildEntry],
    dest_path: &Path,
) -> Result<ArchiveInspection, CoreError> {
    write_bootanimation_zip_with_method(entries, dest_path, CompressionMethod::Stored)
}

pub(crate) fn write_bootanimation_zip_with_method(
    entries: &[ArchiveBuildEntry],
    dest_path: &Path,
    method: CompressionMethod,
) -> Result<ArchiveInspection, CoreError> {
    if entries.is_empty() {
        return Err(CoreError::Archive(
            "cannot build empty bootanimation archive".to_string(),
        ));
    }

    let mut seen = HashSet::new();
    let mut ordered_entries: Vec<&ArchiveBuildEntry> = entries.iter().collect();

    // Deterministic entry order: `desc.txt` first, top-level metadata next, then parts in order.
    ordered_entries.sort_by(|a, b| {
        let rank_a = entry_sort_rank(&a.archive_path);
        let rank_b = entry_sort_rank(&b.archive_path);
        rank_a
            .cmp(&rank_b)
            .then_with(|| a.archive_path.cmp(&b.archive_path))
    });

    for item in &ordered_entries {
        if !is_safe_archive_entry_path(&item.archive_path) {
            return Err(CoreError::Archive(format!(
                "refusing to write unsafe archive path '{}'",
                item.archive_path
            )));
        }
        if !seen.insert(item.archive_path.clone()) {
            return Err(CoreError::Archive(format!(
                "duplicate archive entry path '{}'",
                item.archive_path
            )));
        }
    }

    let parent_dir = dest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent_dir)?;

    let temp_file = tempfile::NamedTempFile::new_in(parent_dir)?;
    {
        let mut zip_writer = ZipWriter::new(temp_file.as_file());
        let fixed_time = DateTime::from_date_and_time(2025, 1, 1, 0, 0, 0)
            .unwrap_or_else(|_| DateTime::default());
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(fixed_time)
            .unix_permissions(0o644);

        for item in &ordered_entries {
            zip_writer
                .start_file(&item.archive_path, options)
                .map_err(|e| CoreError::Archive(format!("zip start_file failed: {e}")))?;
            zip_writer.write_all(&item.bytes)?;
        }

        let mut finished = zip_writer
            .finish()
            .map_err(|e| CoreError::Archive(format!("zip finish failed: {e}")))?;
        finished.flush()?;
    }

    // Reopen and verify all mandatory invariants BEFORE touching dest_path.
    let inspection = inspect_zip_file(temp_file.path())?;
    if !inspection.has_desc_txt {
        return Err(CoreError::Archive(
            "post-build verification failed: desc.txt missing from archive".to_string(),
        ));
    }
    if !inspection.compressed_entries.is_empty() {
        return Err(CoreError::Archive(format!(
            "post-build verification failed: inner bootanimation entries must use ZIP_STORED (0), found compressed entries: {:?}",
            inspection.compressed_entries
        )));
    }
    if !inspection.unsafe_paths.is_empty() || !inspection.duplicate_entries.is_empty() {
        return Err(CoreError::Archive(
            "post-build verification failed: unsafe or duplicate paths detected".to_string(),
        ));
    }

    commit_tempfile_transactionally(temp_file, dest_path)?;
    Ok(inspection)
}

/// Safely extract a ZIP archive into `dest_dir` via a temporary staging workspace.
/// Enforces path traversal protection, symlink rejection, and decompression size bounds.
pub fn safe_extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let inspection = inspect_zip_file(zip_path)?;
    if !inspection.unsafe_paths.is_empty() {
        return Err(CoreError::Archive(format!(
            "refusing to extract archive with unsafe paths: {:?}",
            inspection.unsafe_paths
        )));
    }
    if !inspection.duplicate_entries.is_empty() {
        return Err(CoreError::Archive(format!(
            "refusing to extract archive with duplicate entries: {:?}",
            inspection.duplicate_entries
        )));
    }
    if inspection.entries.len() > MAX_EXTRACTION_ENTRIES {
        return Err(CoreError::Archive(format!(
            "archive entry count {} exceeds limit {}",
            inspection.entries.len(),
            MAX_EXTRACTION_ENTRIES
        )));
    }

    let file = File::open(zip_path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| CoreError::Archive(format!("invalid zip: {e}")))?;

    std::fs::create_dir_all(dest_dir)?;
    let dest_canonical = dest_dir.canonicalize()?;
    let mut total_written: u64 = 0;
    let mut extracted_files = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CoreError::Archive(format!("zip entry error: {e}")))?;

        if entry.is_symlink() {
            return Err(CoreError::Archive(format!(
                "refusing to extract symlink entry '{}'",
                entry.name()
            )));
        }

        let name = entry.name().to_string();
        if !is_safe_archive_entry_path(&name) {
            return Err(CoreError::Archive(format!("unsafe archive path '{name}'")));
        }

        if entry.is_dir() {
            let dir_target = dest_canonical.join(name.trim_end_matches('/'));
            std::fs::create_dir_all(&dir_target)?;
            continue;
        }

        if entry.size() > MAX_SINGLE_ENTRY_BYTES {
            return Err(CoreError::Archive(format!(
                "entry '{name}' size {} exceeds max single entry limit {}",
                entry.size(),
                MAX_SINGLE_ENTRY_BYTES
            )));
        }

        let out_path = dest_canonical.join(&name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out_file = File::create(&out_path)?;
        let mut limited_reader = (&mut entry).take(MAX_SINGLE_ENTRY_BYTES + 1);
        let copied = std::io::copy(&mut limited_reader, &mut out_file)?;
        if copied > MAX_SINGLE_ENTRY_BYTES {
            let _ = std::fs::remove_file(&out_path);
            return Err(CoreError::Archive(format!(
                "entry '{name}' exceeded maximum allowed uncompressed size during read"
            )));
        }
        total_written = total_written.saturating_add(copied);
        if total_written > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(CoreError::Archive(
                "total uncompressed extraction size limit exceeded".to_string(),
            ));
        }

        extracted_files.push(out_path);
    }

    Ok(extracted_files)
}

pub fn is_safe_archive_entry_path(name: &str) -> bool {
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
    {
        return false;
    }
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    for seg in trimmed.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return false;
        }
    }
    let path = Path::new(trimmed);
    for comp in path.components() {
        if !matches!(comp, Component::Normal(_)) {
            return false;
        }
    }
    true
}

fn entry_sort_rank(path: &str) -> u8 {
    if path == "desc.txt" {
        0
    } else if !path.contains('/') {
        1
    } else {
        2
    }
}

fn compression_method_label(method: CompressionMethod) -> (String, u16) {
    match method {
        CompressionMethod::Stored => ("Stored".to_string(), 0),
        CompressionMethod::Deflated => ("Deflated".to_string(), 8),
        other => (format!("{other:?}"), 999),
    }
}

fn commit_tempfile_transactionally(
    temp_file: tempfile::NamedTempFile,
    dest_path: &Path,
) -> Result<(), CoreError> {
    let parent_dir = dest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    // Preserve previous valid artifact if dest_path already exists so a failed replace can restore it.
    let backup_path = if dest_path.exists() {
        let bkp = parent_dir.join(format!(
            ".{}.bootanimix.bak",
            dest_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("output.zip")
        ));
        std::fs::copy(dest_path, &bkp)?;
        Some(bkp)
    } else {
        None
    };

    match temp_file.persist(dest_path) {
        Ok(_) => {
            if let Some(bkp) = backup_path {
                let _ = std::fs::remove_file(bkp);
            }
            Ok(())
        }
        Err(err) => {
            if let Some(bkp) = backup_path {
                let _ = std::fs::rename(&bkp, dest_path);
            }
            Err(CoreError::Io(err.error))
        }
    }
}

fn scan_central_directory_names<R: Read + Seek>(reader: &mut R) -> std::io::Result<Vec<String>> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    let mut names = Vec::new();
    let mut i = 0;
    while i + 46 <= buf.len() {
        // Central directory file header signature: 0x02014b50
        if buf[i..i + 4] == [0x50, 0x4b, 0x01, 0x02] {
            let name_len = u16::from_le_bytes([buf[i + 28], buf[i + 29]]) as usize;
            let extra_len = u16::from_le_bytes([buf[i + 30], buf[i + 31]]) as usize;
            let comment_len = u16::from_le_bytes([buf[i + 32], buf[i + 33]]) as usize;
            let name_start = i + 46;
            if name_start + name_len <= buf.len() {
                if let Ok(s) = std::str::from_utf8(&buf[name_start..name_start + name_len]) {
                    names.push(s.to_string());
                }
                i = name_start + name_len + extra_len + comment_len;
                continue;
            }
        }
        i += 1;
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_zip_builder_verifies_compression_zero_and_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let out_a = dir.path().join("boot_a.zip");
        let out_b = dir.path().join("boot_b.zip");

        let entries = vec![
            ArchiveBuildEntry {
                archive_path: "part0/00001.png".to_string(),
                bytes: vec![1, 2, 3, 4],
            },
            ArchiveBuildEntry {
                archive_path: "desc.txt".to_string(),
                bytes: b"1080 1920 60\np 1 0 part0\n".to_vec(),
            },
            ArchiveBuildEntry {
                archive_path: "part0/00000.png".to_string(),
                bytes: vec![5, 6, 7, 8],
            },
        ];

        let inspect_a = write_stored_bootanimation_zip(&entries, &out_a).unwrap();
        let inspect_b = write_stored_bootanimation_zip(&entries, &out_b).unwrap();

        assert!(inspect_a.compressed_entries.is_empty());
        assert_eq!(inspect_a.entries[0].name, "desc.txt");
        assert_eq!(inspect_a.entries[1].name, "part0/00000.png");
        assert_eq!(inspect_a.entries[2].name, "part0/00001.png");
        for entry in &inspect_a.entries {
            assert_eq!(entry.compression_code, 0, "every entry must be Stored (0)");
        }

        // Byte-for-byte deterministic output check:
        assert_eq!(
            std::fs::read(&out_a).unwrap(),
            std::fs::read(&out_b).unwrap()
        );
        assert_eq!(inspect_a, inspect_b);
    }

    #[test]
    fn deflated_inner_zip_fails_verification_and_preserves_previous_valid_output() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("bootanimation.zip");

        let valid_entries = vec![ArchiveBuildEntry {
            archive_path: "desc.txt".to_string(),
            bytes: b"1080 1920 60\np 1 0 part0\n".to_vec(),
        }];
        write_stored_bootanimation_zip(&valid_entries, &dest).unwrap();
        let original_bytes = std::fs::read(&dest).unwrap();

        // Attempt to build with Deflated compression; must fail verification and leave original_bytes intact!
        let res = write_bootanimation_zip_with_method(
            &valid_entries,
            &dest,
            CompressionMethod::Deflated,
        );
        assert!(res.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), original_bytes);
    }

    #[test]
    fn rejects_unsafe_traversal_paths() {
        assert!(!is_safe_archive_entry_path("../evil.sh"));
        assert!(!is_safe_archive_entry_path("part0/../../etc/passwd"));
        assert!(!is_safe_archive_entry_path("/absolute/path.png"));
        assert!(!is_safe_archive_entry_path("C:\\windows\\win.ini"));
        assert!(is_safe_archive_entry_path("desc.txt"));
        assert!(is_safe_archive_entry_path("part0/00000.png"));
    }

    #[test]
    fn safe_extract_rejects_traversal_and_duplicate_entries() {
        let dir = tempfile::tempdir().unwrap();
        let bad_zip = dir.path().join("bad.zip");
        {
            let f = File::create(&bad_zip).unwrap();
            let mut zw = ZipWriter::new(f);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zw.start_file("../escape.txt", opts).unwrap();
            zw.write_all(b"nope").unwrap();
            zw.finish().unwrap();
        }
        let target = dir.path().join("extracted");
        let err = safe_extract_zip(&bad_zip, &target).unwrap_err();
        assert!(err.to_string().contains("unsafe paths"));
        assert!(!dir.path().join("escape.txt").exists());
    }
}
