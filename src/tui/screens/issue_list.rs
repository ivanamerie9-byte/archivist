use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::domain::{Issue, LibraryKind, MatchCandidate, ScanReport, Title, TmdbMatch};
use crate::tui::{theme, widgets};

#[derive(Debug, Clone)]
pub enum EntryKind {
    /// Restore folder.jpg / desktop.ini / .ico for this title's root.
    RestoreTitle { title_idx: usize },
    /// Restore folder.jpg / desktop.ini / .ico for one season folder.
    RestoreSeason { title_idx: usize, season: u8 },
    /// Wrap a single orphan video (and sidecars) into a `Title (Year)\` folder.
    WrapOrphanVideo { issue_idx: usize },
    /// A folder whose name didn't parse into title+year (and has no clear video).
    UnparseableTitle { issue_idx: usize },
}

#[derive(Debug, Clone)]
pub enum MatchState {
    Pending,
    AutoMatched(TmdbMatch),
    NeedsChoice(Vec<MatchCandidate>),
    Skipped,
    Unresolvable(String),
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub label: String,
    pub library: LibraryKind,
    pub kind: EntryKind,
    pub checked: bool,
    pub state: MatchState,
}

#[derive(Debug)]
pub struct IssueListState {
    pub entries: Vec<Entry>,
    pub cursor: usize,
}

impl IssueListState {
    pub fn from_report(report: &ScanReport) -> Self {
        let mut entries: Vec<Entry> = Vec::new();
        let mut seen_title_root: std::collections::HashSet<usize> = Default::default();
        let mut seen_title_season: std::collections::HashSet<(usize, u8)> = Default::default();

        for (issue_idx, issue) in report.issues.iter().enumerate() {
            match issue {
                Issue::MissingFolderJpg { title_idx, season }
                | Issue::MissingDesktopIni { title_idx, season }
                | Issue::MissingIco { title_idx, season }
                | Issue::MissingSystemAttr { title_idx, season } => {
                    let title = &report.titles[*title_idx];
                    match season {
                        Some(n) => {
                            if seen_title_season.insert((*title_idx, *n)) {
                                entries.push(Entry {
                                    label: format!(
                                        "[{}] {} · Season {}",
                                        title.library.label(),
                                        folder_basename(title),
                                        n
                                    ),
                                    library: title.library,
                                    kind: EntryKind::RestoreSeason {
                                        title_idx: *title_idx,
                                        season: *n,
                                    },
                                    checked: true,
                                    state: MatchState::Pending,
                                });
                            }
                        }
                        None => {
                            if seen_title_root.insert(*title_idx) {
                                entries.push(Entry {
                                    label: format!(
                                        "[{}] {}",
                                        title.library.label(),
                                        folder_basename(title)
                                    ),
                                    library: title.library,
                                    kind: EntryKind::RestoreTitle {
                                        title_idx: *title_idx,
                                    },
                                    checked: true,
                                    state: MatchState::Pending,
                                });
                            }
                        }
                    }
                }
                Issue::UnparseableFolder { path, library } => {
                    entries.push(Entry {
                        label: format!(
                            "[{}] (unparsed) {}",
                            library.label(),
                            path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                        ),
                        library: *library,
                        kind: EntryKind::UnparseableTitle { issue_idx },
                        checked: true,
                        state: MatchState::Pending,
                    });
                }
                Issue::OrphanVideo { path, library, .. } => {
                    entries.push(Entry {
                        label: format!(
                            "[{}] (orphan) {}",
                            library.label(),
                            path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                        ),
                        library: *library,
                        kind: EntryKind::WrapOrphanVideo { issue_idx },
                        checked: true,
                        state: MatchState::Pending,
                    });
                }
            }
        }

        Self { entries, cursor: 0 }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
    }
    pub fn page_up(&mut self, page: usize) {
        self.cursor = self.cursor.saturating_sub(page);
    }
    pub fn page_down(&mut self, page: usize) {
        self.cursor = (self.cursor + page).min(self.entries.len().saturating_sub(1));
    }
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }
    pub fn cursor_end(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
    }
    pub fn toggle(&mut self) {
        if let Some(e) = self.entries.get_mut(self.cursor) {
            e.checked = !e.checked;
        }
    }
    pub fn toggle_all(&mut self) {
        let any_unchecked = self.entries.iter().any(|e| !e.checked);
        for e in &mut self.entries {
            e.checked = any_unchecked;
        }
    }

    pub fn checked_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| if e.checked { Some(i) } else { None })
            .collect()
    }
}

fn folder_basename(t: &Title) -> String {
    t.folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

pub fn render(f: &mut Frame, area: Rect, st: &IssueListState) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let count_total = st.entries.len();
    let count_checked = st.entries.iter().filter(|e| e.checked).count();
    let cursor_pos = if count_total == 0 { 0 } else { st.cursor + 1 };
    let header = format!("{count_checked}/{count_total} selected · row {cursor_pos}/{count_total}");
    widgets::header(f, main[0], "Issues found", &header);

    let block = widgets::bordered_block("Items");
    let inner = block.inner(main[1]);
    f.render_widget(block, main[1]);

    let visible = inner.height as usize;
    let scroll_top = st.cursor.saturating_sub(visible.saturating_sub(1) / 2);
    let scroll_top = scroll_top.min(st.entries.len().saturating_sub(visible));

    let mut lines: Vec<Line> = Vec::new();
    for (i, e) in st.entries.iter().enumerate().skip(scroll_top).take(visible) {
        let cursor = if i == st.cursor { "▸ " } else { "  " };
        let check = if e.checked { "[x] " } else { "[ ] " };
        let state_label = match &e.state {
            MatchState::Pending => Span::raw(""),
            MatchState::AutoMatched(m) => Span::styled(
                format!("  → {} ({})", m.canonical_title, m.year),
                theme::ok_style(),
            ),
            MatchState::NeedsChoice(c) => Span::styled(
                format!("  ? {} candidates", c.len()),
                Style::default().fg(theme::ACCENT),
            ),
            MatchState::Skipped => Span::styled("  (skipped)", theme::dim_style()),
            MatchState::Unresolvable(err) => Span::styled(format!("  ✗ {err}"), theme::err_style()),
        };
        let style = if i == st.cursor {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        lines.push(Line::from(vec![
            Span::raw(cursor),
            Span::raw(check),
            Span::styled(e.label.clone(), style),
            state_label,
        ]));
    }
    let p = Paragraph::new(lines);
    f.render_widget(p, inner);

    widgets::footer(
        f,
        main[2],
        &[
            ("↑↓", "move"),
            ("PgUp/PgDn", "page"),
            ("Home/End", "jump"),
            ("Space", "toggle"),
            ("a", "all"),
            ("Enter", "apply"),
            ("k", "settings"),
            ("Esc", "back"),
        ],
    );
}
