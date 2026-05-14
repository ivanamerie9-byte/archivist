use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("create_dir_all {}", path.display()))
}

/// Find sidecar files that share a base name with `video` (different
/// extensions like `.srt`, `.ass`, `.ac3`). Case-insensitive on the stem.
pub fn find_sidecars(video: &Path) -> Vec<PathBuf> {
    let Some(parent) = video.parent() else {
        return Vec::new();
    };
    let Some(stem) = video.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let stem_lower = stem.to_lowercase();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == video {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(other_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let other_lower = other_stem.to_lowercase();
        if other_lower == stem_lower || other_lower.starts_with(&format!("{}.", stem_lower)) {
            // also catches `Mickey.17.2025.eng.srt` matching `Mickey.17.2025.mkv`
            if SIDECAR_EXTS.iter().any(|e| {
                path.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case(e))
                    .unwrap_or(false)
            }) {
                out.push(path);
            }
        }
    }
    out
}

const SIDECAR_EXTS: &[&str] = &[
    "srt", "ass", "ssa", "sub", "idx", "vtt", "ac3", "dts", "mka", "nfo",
];
