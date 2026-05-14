use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};

use crate::domain::MatchCandidate;
use crate::tui::{theme, widgets};

#[derive(Debug)]
pub struct CandidateModalState {
    pub entry_label: String,
    pub candidates: Vec<MatchCandidate>,
    pub cursor: usize,
    pub editing_query: bool,
    pub query_input: String,
    pub remaining_after_this: usize,
}

impl CandidateModalState {
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.candidates.len() {
            self.cursor += 1;
        }
    }
}

pub fn render(f: &mut Frame, area: Rect, st: &CandidateModalState) {
    f.render_widget(Clear, area);

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    widgets::header(
        f,
        main[0],
        "Pick the right title",
        &format!("({} more after this)", st.remaining_after_this),
    );

    // Entry header
    let entry_block = widgets::bordered_block("Folder");
    let entry_inner = entry_block.inner(main[1]);
    f.render_widget(entry_block, main[1]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            st.entry_label.clone(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )))
        .wrap(Wrap { trim: true }),
        entry_inner,
    );

    // Candidates list / retry input
    if st.editing_query {
        let block = widgets::bordered_block("Retry search");
        let inner = block.inner(main[2]);
        f.render_widget(block, main[2]);
        let p = Paragraph::new(Line::from(vec![
            Span::raw("Query: "),
            Span::styled(
                format!("{}_", st.query_input),
                Style::default().fg(theme::ACCENT),
            ),
        ]));
        f.render_widget(p, inner);
        widgets::footer(f, main[3], &[("Enter", "search"), ("Esc", "cancel")]);
    } else {
        let block = widgets::bordered_block("Candidates");
        let inner = block.inner(main[2]);
        f.render_widget(block, main[2]);

        let mut lines: Vec<Line> = Vec::new();
        if st.candidates.is_empty() {
            lines.push(Line::from(Span::styled(
                "No matches. Press [r] to edit search query.",
                theme::dim_style(),
            )));
        }
        for (i, c) in st.candidates.iter().enumerate() {
            let prefix = if i == st.cursor { "▸ " } else { "  " };
            let style = if i == st.cursor {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            let title_line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(
                    format!("{} ({})", c.tmdb.canonical_title, c.tmdb.year),
                    style,
                ),
                Span::styled(format!("   id:{}", c.tmdb.id), theme::dim_style()),
            ]);
            lines.push(title_line);
            let mut overview = c.overview.clone();
            if overview.len() > 200 {
                overview.truncate(200);
                overview.push('…');
            }
            if !overview.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(overview, theme::dim_style()),
                ]));
            }
            lines.push(Line::raw(""));
        }
        let p = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(p, inner);

        widgets::footer(
            f,
            main[3],
            &[
                ("↑↓", "select"),
                ("Enter", "accept"),
                ("s", "skip"),
                ("r", "retry"),
                ("Esc", "skip all"),
            ],
        );
    }
}
