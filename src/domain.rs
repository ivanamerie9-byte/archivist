use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LibraryKind {
    Movies,
    Series,
}

impl LibraryKind {
    pub fn label(self) -> &'static str {
        match self {
            LibraryKind::Movies => "Movies",
            LibraryKind::Series => "Series",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Library {
    pub kind: LibraryKind,
    pub root: PathBuf,
    pub covers_dir: PathBuf,
}

impl Library {
    pub fn new(kind: LibraryKind, root: PathBuf) -> Self {
        let covers_dir = root.join("_covers");
        Self {
            kind,
            root,
            covers_dir,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameConfidence {
    High,
    Medium,
    Low,
    Unparseable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedName {
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u8>,
    pub edition: Option<String>,
    pub confidence: NameConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Movie,
    Tv,
}

impl MediaKind {
    pub fn from_library(kind: LibraryKind) -> Self {
        match kind {
            LibraryKind::Movies => MediaKind::Movie,
            LibraryKind::Series => MediaKind::Tv,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TmdbMatch {
    pub id: u64,
    /// Reserved: future apply paths may need to disambiguate movie vs TV
    /// (e.g. when fetching season images by id).
    #[allow(dead_code)]
    pub kind: MediaKind,
    pub canonical_title: String,
    pub year: u16,
    pub poster_path: Option<String>,
}

impl TmdbMatch {
    pub fn canonical_folder_name(&self) -> String {
        format!("{} ({})", sanitize_folder(&self.canonical_title), self.year)
    }
}

#[derive(Debug, Clone)]
pub struct SeasonDir {
    pub number: u8,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Title {
    pub library: LibraryKind,
    pub folder: PathBuf,
    pub parsed: ParsedName,
    /// Reserved for future use: persisted TMDB match between scan and apply
    /// when a title was already resolved earlier in the session.
    #[allow(dead_code)]
    pub tmdb: Option<TmdbMatch>,
    pub seasons: Vec<SeasonDir>,
}

/// Problems the scanner detected. `title_idx` refers to a slot in
/// `ScanReport::titles`; `path` is used when the issue is not bound to a
/// recognised title.
#[derive(Debug, Clone)]
pub enum Issue {
    MissingFolderJpg {
        title_idx: usize,
        season: Option<u8>,
    },
    MissingDesktopIni {
        title_idx: usize,
        season: Option<u8>,
    },
    MissingIco {
        title_idx: usize,
        season: Option<u8>,
    },
    MissingSystemAttr {
        title_idx: usize,
        season: Option<u8>,
    },
    UnparseableFolder {
        path: PathBuf,
        library: LibraryKind,
    },
    OrphanVideo {
        path: PathBuf,
        library: LibraryKind,
        sidecars: Vec<PathBuf>,
    },
}

#[derive(Debug, Clone)]
pub struct MatchCandidate {
    pub tmdb: TmdbMatch,
    pub overview: String,
    #[allow(dead_code)]
    pub auto_score: f32,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub titles: Vec<Title>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone)]
pub struct ActionPlan {
    pub items: Vec<PlannedAction>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub enum PlannedAction {
    /// Download `poster_url` once and materialise the full Windows
    /// folder-icon kit for `dir`: `folder.jpg`, the `_covers/<...>.jpg` backup,
    /// the `_covers/<...>.ico`, the `desktop.ini` pointing at the ico, and the
    /// `+SYSTEM` flag on the directory plus `+HIDDEN +SYSTEM` on the ini.
    RestoreCover {
        label: String,
        dir: PathBuf,
        jpg_backup: PathBuf,
        ico_path: PathBuf,
        poster_url: String,
    },
    /// Create `new_dir` and move `video` (+ its sidecars) into it.
    WrapVideoIntoFolder {
        label: String,
        video: PathBuf,
        new_dir: PathBuf,
        sidecars: Vec<PathBuf>,
    },
    /// Create `new_parent` and move `src_dir` inside it as-is. Optionally rename
    /// the moved directory (top-level only, contents are never touched).
    WrapDirectoryIntoFolder {
        label: String,
        src_dir: PathBuf,
        new_parent: PathBuf,
        rename_to: Option<String>,
    },
    /// Rename a title folder in place. Used when the TMDB-canonical name
    /// differs from the on-disk basename (e.g. scene-style names that already
    /// have valid contents). Contents are never touched.
    RenameTitleFolder {
        label: String,
        src: PathBuf,
        dst: PathBuf,
    },
    /// Move a scene-style season folder (e.g. `Arcane.S02.2160p.WEB-DL...\`)
    /// into an existing series folder as `Season N\`. The folder's contents
    /// are never modified. Cover for the new season is left to a follow-up
    /// scan (so it picks up MissingFolderJpg/season=N issue automatically).
    AdoptAsSeason {
        label: String,
        src_dir: PathBuf,
        series_root: PathBuf,
        season: u8,
    },
}

impl PlannedAction {
    pub fn label(&self) -> &str {
        match self {
            PlannedAction::RestoreCover { label, .. }
            | PlannedAction::WrapVideoIntoFolder { label, .. }
            | PlannedAction::WrapDirectoryIntoFolder { label, .. }
            | PlannedAction::RenameTitleFolder { label, .. }
            | PlannedAction::AdoptAsSeason { label, .. } => label,
        }
    }
}

#[derive(Debug, Default)]
pub struct ExecutionReport {
    pub successes: Vec<String>,
    pub failures: Vec<(String, String)>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub current: u64,
    pub total: u64,
    pub message: String,
}

/// Replace characters that are illegal in Windows file/folder names. Mirrors
/// the existing HTML/Python tool: replaces ``<>:"/\\|?*`` with ``-``.
pub fn sanitize_folder(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push('-'),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_folder_replaces_illegal_chars() {
        assert_eq!(sanitize_folder("Pokemon: The Show"), "Pokemon- The Show");
        assert_eq!(sanitize_folder("foo/bar?baz*qux"), "foo-bar-baz-qux");
        assert_eq!(sanitize_folder("trailing... "), "trailing");
    }

    #[test]
    fn canonical_folder_name_format() {
        let m = TmdbMatch {
            id: 1,
            kind: MediaKind::Movie,
            canonical_title: "Blade Runner".into(),
            year: 1982,
            poster_path: None,
        };
        assert_eq!(m.canonical_folder_name(), "Blade Runner (1982)");
    }
}
