pub mod attrs;
pub mod classify;

use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::domain::{
    Issue, Library, LibraryKind, ParsedName, ProgressEvent, ScanReport, SeasonDir, Title,
};
use crate::parser::parse_release_name;
use crate::scanner::attrs::has_system_attr;
use crate::scanner::classify::{classify_dir_role, is_video_file, DirRole};
use crate::util::fs_ext::find_sidecars;

pub async fn scan(
    libraries: Vec<Library>,
    progress: UnboundedSender<ProgressEvent>,
    cancel: CancellationToken,
) -> Result<ScanReport> {
    tokio::task::spawn_blocking(move || scan_blocking(libraries, progress, cancel)).await?
}

fn scan_blocking(
    libraries: Vec<Library>,
    progress: UnboundedSender<ProgressEvent>,
    cancel: CancellationToken,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    for lib in libraries {
        if cancel.is_cancelled() {
            break;
        }
        if !lib.root.is_dir() {
            tracing::warn!(root = %lib.root.display(), "library root missing, skipping");
            continue;
        }
        let _ = progress.send(ProgressEvent {
            current: 0,
            total: 0,
            message: format!("Scanning {}", lib.root.display()),
        });
        scan_library(&lib, &mut report, &progress, &cancel)?;
    }
    Ok(report)
}

fn scan_library(
    lib: &Library,
    report: &mut ScanReport,
    progress: &UnboundedSender<ProgressEvent>,
    cancel: &CancellationToken,
) -> Result<()> {
    let entries = match std::fs::read_dir(&lib.root) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(root = %lib.root.display(), error = %e, "read_dir failed");
            return Ok(());
        }
    };

    let mut top_level: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    top_level.sort();
    let total = top_level.len() as u64;

    for (i, path) in top_level.into_iter().enumerate() {
        if cancel.is_cancelled() {
            break;
        }
        let current = (i + 1) as u64;

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.eq_ignore_ascii_case("_covers")
            || name.eq_ignore_ascii_case("System Volume Information")
        {
            continue;
        }

        let _ = progress.send(ProgressEvent {
            current,
            total,
            message: format!("[{}] {}", lib.kind.label(), name),
        });

        if path.is_file() {
            // Orphan video at the library root.
            if is_video_file(&path) {
                // Skip if a sibling .parts file is present — that means an
                // active download owns this file. We never touch downloads in
                // progress.
                if has_parts_neighbour(&path) {
                    tracing::info!(
                        file = %path.display(),
                        "skipping orphan video — active download (.parts) sibling present"
                    );
                    continue;
                }
                let sidecars = find_sidecars(&path);
                report.issues.push(Issue::OrphanVideo {
                    path: path.clone(),
                    library: lib.kind,
                    sidecars,
                });
            }
            // .parts, .ico, desktop.ini etc. at the library root: ignore
            // silently. They are user / downloader artifacts, not our problem.
            continue;
        }
        if !path.is_dir() {
            continue;
        }

        scan_title_dir(&path, lib, report);
    }
    Ok(())
}

fn scan_title_dir(dir: &Path, lib: &Library, report: &mut ScanReport) {
    let basename = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let parsed = parse_release_name(&basename);

    let mut title = Title {
        library: lib.kind,
        folder: dir.to_path_buf(),
        parsed: parsed.clone(),
        tmdb: None,
        seasons: Vec::new(),
    };

    // For series, discover season subdirectories.
    if lib.kind == LibraryKind::Series {
        title.seasons = discover_seasons(dir);
    }

    let title_idx = report.titles.len();
    let folder = dir.to_path_buf();
    let needs_review = matches!(
        parsed,
        ParsedName {
            confidence: crate::domain::NameConfidence::Unparseable,
            ..
        }
    );

    if needs_review {
        report.issues.push(Issue::UnparseableFolder {
            path: folder.clone(),
            library: lib.kind,
        });
    }

    // Check artifacts at the title folder level.
    check_artifacts(&folder, lib, title_idx, None, report);

    // For each season, check artifacts there too.
    for season in &title.seasons {
        check_artifacts(&season.path, lib, title_idx, Some(season.number), report);
    }

    report.titles.push(title);
}

fn check_artifacts(
    dir: &Path,
    lib: &Library,
    title_idx: usize,
    season: Option<u8>,
    report: &mut ScanReport,
) {
    let folder_jpg = dir.join("folder.jpg");
    let desktop_ini = dir.join("desktop.ini");

    if !folder_jpg.is_file() {
        report
            .issues
            .push(Issue::MissingFolderJpg { title_idx, season });
    }
    if !desktop_ini.is_file() {
        report
            .issues
            .push(Issue::MissingDesktopIni { title_idx, season });
    }

    // Trust desktop.ini as the source of truth for the icon path. If the file
    // exists and points at a valid .ico, accept that path even if it doesn't
    // match our default naming convention. Only fall back to guessing names
    // when the ini is missing.
    let ico_ok = if let Some(ico_from_ini) = attrs::read_icon_resource(&desktop_ini) {
        ico_from_ini.is_file()
    } else {
        let canonical = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let candidates: Vec<String> = match season {
            Some(n) => {
                let parent_name = dir
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or(canonical);
                // Try both "Title (Year) - Season N.ico" and the legacy
                // "Title - Season N.ico" (year stripped) variant.
                let mut v = vec![format!("{} - Season {}.ico", parent_name, n)];
                if let Some(stripped) = strip_year_suffix(parent_name) {
                    v.push(format!("{} - Season {}.ico", stripped, n));
                }
                v
            }
            None => vec![format!("{}.ico", canonical)],
        };
        candidates
            .iter()
            .any(|name| lib.covers_dir.join(name).is_file())
    };
    if !ico_ok {
        report.issues.push(Issue::MissingIco { title_idx, season });
    }

    match has_system_attr(dir) {
        Ok(false) => report
            .issues
            .push(Issue::MissingSystemAttr { title_idx, season }),
        Ok(true) => {}
        Err(e) => tracing::warn!(dir = %dir.display(), error = %e, "could not read attrs"),
    }
}

fn discover_seasons(title_dir: &Path) -> Vec<SeasonDir> {
    let Ok(entries) = std::fs::read_dir(title_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(n) = classify::detect_season_number(name) {
            out.push(SeasonDir { number: n, path });
        }
    }
    out.sort_by_key(|s| s.number);
    out
}

/// True if the directory containing `video` also has a `.parts` file —
/// typical signature of an in-progress download (rTorrent, qBittorrent,
/// browsers all use this convention).
fn has_parts_neighbour(video: &Path) -> bool {
    let Some(parent) = video.parent() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return false;
    };
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("parts") {
            return true;
        }
    }
    false
}

/// Strip a trailing " (YYYY)" from a folder name. Used to recognise
/// pre-existing covers that omit the year suffix (e.g. legacy
/// `The Walking Dead - Season 1.ico` vs. our default
/// `The Walking Dead (2010) - Season 1.ico`).
fn strip_year_suffix(name: &str) -> Option<String> {
    let trimmed = name.trim_end();
    if !trimmed.ends_with(')') {
        return None;
    }
    let open = trimmed.rfind('(')?;
    let inside = &trimmed[open + 1..trimmed.len() - 1];
    if inside.len() == 4 && inside.chars().all(|c| c.is_ascii_digit()) {
        Some(trimmed[..open].trim_end().to_string())
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn debug_dir_role(p: &Path) -> DirRole {
    classify_dir_role(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_year() {
        assert_eq!(
            strip_year_suffix("The Walking Dead (2010)").as_deref(),
            Some("The Walking Dead")
        );
        assert_eq!(strip_year_suffix("Foo (12)"), None);
        assert_eq!(strip_year_suffix("No year here"), None);
    }
}
