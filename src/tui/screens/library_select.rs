use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::{theme, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibChoice {
    Movies,
    Series,
    Both,
}

#[derive(Debug)]
pub struct LibrarySelectState {
    pub choice: LibChoice,
}

impl Default for LibrarySelectState {
    fn default() -> Self {
        Self {
            choice: LibChoice::Both,
        }
    }
}

impl LibrarySelectState {
    pub fn next(&mut self) {
        self.choice = match self.choice {
            LibChoice::Movies => LibChoice::Series,
            LibChoice::Series => LibChoice::Both,
            LibChoice::Both => LibChoice::Movies,
        };
    }
    pub fn prev(&mut self) {
        self.choice = match self.choice {
            LibChoice::Movies => LibChoice::Both,
            LibChoice::Series => LibChoice::Movies,
            LibChoice::Both => LibChoice::Series,
        };
    }
}

pub fn render(f: &mut Frame, area: Rect, st: &LibrarySelectState) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    widgets::header(f, main[0], "Choose library to scan", "");

    let inner = widgets::bordered_block("Library");
    let inner_area = inner.inner(main[1]);
    f.render_widget(inner, main[1]);

    let lines: Vec<Line> = [
        (LibChoice::Movies, "[M]ovies"),
        (LibChoice::Series, "[S]eries"),
        (LibChoice::Both, "[B]oth"),
    ]
    .iter()
    .map(|(c, label)| {
        let selected = *c == st.choice;
        let style = if selected {
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let prefix = if selected { "  ▸ " } else { "    " };
        Line::from(vec![Span::raw(prefix), Span::styled(*label, style)])
    })
    .collect();

    let p = Paragraph::new(lines);
    f.render_widget(p, inner_area);

    widgets::footer(
        f,
        main[2],
        &[
            ("↑↓", "select"),
            ("Enter", "scan"),
            ("k", "settings"),
            ("q", "quit"),
        ],
    );
}
