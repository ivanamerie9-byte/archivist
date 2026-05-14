use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirRole {
    /// A title's root folder (movie or series).
    TitleRoot,
    /// A `Season N` folder under a series.
    SeasonDir,
    /// The library's `_covers` directory.
    CoversDir,
    Unknown,
}

const VIDEO_EXTS: &[&str] = &[
    "mkv", "mp4", "avi", "m4v", "mov", "ts", "m2ts", "webm", "mpg", "mpeg", "wmv",
];

pub fn is_video_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.iter().any(|v| v.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

static SEASON_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^Season\s+(\d{1,3})$").unwrap());
static SEASON_RE_S: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^S(\d{1,3})$").unwrap());

pub fn detect_season_number(folder_name: &str) -> Option<u8> {
    if let Some(c) = SEASON_RE.captures(folder_name) {
        return c.get(1).and_then(|m| m.as_str().parse().ok());
    }
    if let Some(c) = SEASON_RE_S.captures(folder_name) {
        return c.get(1).and_then(|m| m.as_str().parse().ok());
    }
    None
}

pub fn classify_dir_role(p: &Path) -> DirRole {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.eq_ignore_ascii_case("_covers") {
        return DirRole::CoversDir;
    }
    if detect_season_number(name).is_some() {
        return DirRole::SeasonDir;
    }
    DirRole::TitleRoot
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn season_full() {
        assert_eq!(detect_season_number("Season 1"), Some(1));
        assert_eq!(detect_season_number("season 12"), Some(12));
    }

    #[test]
    fn season_short() {
        assert_eq!(detect_season_number("S01"), Some(1));
        assert_eq!(detect_season_number("S2"), Some(2));
    }

    #[test]
    fn season_negative() {
        assert_eq!(detect_season_number("Season"), None);
        assert_eq!(detect_season_number("Special"), None);
    }

    #[test]
    fn video_ext() {
        assert!(is_video_file(&PathBuf::from("a.mkv")));
        assert!(is_video_file(&PathBuf::from("a.AVI")));
        assert!(!is_video_file(&PathBuf::from("a.srt")));
        assert!(!is_video_file(&PathBuf::from("a")));
    }
}
