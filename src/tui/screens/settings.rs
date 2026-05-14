use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::config::Config;
use crate::tui::{theme, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    ApiKey,
    MoviesRoot,
    SeriesRoot,
}

#[derive(Debug)]
pub struct SettingsState {
    pub api_key: String,
    pub movies_root: String,
    pub series_root: String,
    pub focus: SettingsField,
    pub mask_key: bool,
    pub status: Option<(bool, String)>,
    pub testing: bool,
}

impl SettingsState {
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            api_key: cfg.tmdb_api_key.clone(),
            movies_root: cfg.libraries.movies.root.display().to_string(),
            series_root: cfg.libraries.series.root.display().to_string(),
            focus: SettingsField::ApiKey,
            mask_key: true,
            status: None,
            testing: false,
        }
    }

    pub fn cycle_focus(&mut self, forward: bool) {
        self.focus = match (self.focus, forward) {
            (SettingsField::ApiKey, true) => SettingsField::MoviesRoot,
            (SettingsField::MoviesRoot, true) => SettingsField::SeriesRoot,
            (SettingsField::SeriesRoot, true) => SettingsField::ApiKey,
            (SettingsField::ApiKey, false) => SettingsField::SeriesRoot,
            (SettingsField::MoviesRoot, false) => SettingsField::ApiKey,
            (SettingsField::SeriesRoot, false) => SettingsField::MoviesRoot,
        };
    }

    pub fn current_buf_mut(&mut self) -> &mut String {
        match self.focus {
            SettingsField::ApiKey => &mut self.api_key,
            SettingsField::MoviesRoot => &mut self.movies_root,
            SettingsField::SeriesRoot => &mut self.series_root,
        }
    }

    pub fn apply_to_config(&self, cfg: &mut Config) {
        cfg.tmdb_api_key = self.api_key.trim().to_string();
        cfg.libraries.movies.root = self.movies_root.trim().into();
        cfg.libraries.series.root = self.series_root.trim().into();
    }
}

pub fn render(f: &mut Frame, area: Rect, st: &SettingsState) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    widgets::header(
        f,
        main[0],
        "Settings",
        "Configure TMDB API key and library paths",
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main[1]);

    render_fields(f, body[0], st);
    render_guide(f, body[1]);

    let mut hints: Vec<(&str, &str)> = vec![
        ("Tab", "next field"),
        ("type", "edit"),
        ("Ctrl+M", "show/hide key"),
        ("Ctrl+T", "test connection"),
        ("Ctrl+S", "save"),
        ("Esc", "back to Library Select"),
    ];
    if st.testing {
        hints.push(("⏳", "testing..."));
    }
    widgets::footer(f, main[2], &hints);
}

fn render_fields(f: &mut Frame, area: Rect, st: &SettingsState) {
    let inner = widgets::bordered_block("Fields");
    let inner_area = inner.inner(area);
    f.render_widget(inner, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner_area);

    field_row(
        f,
        rows[0],
        "TMDB API Key",
        &masked(st),
        st.focus == SettingsField::ApiKey,
    );
    field_row(
        f,
        rows[1],
        "Movies root",
        &st.movies_root,
        st.focus == SettingsField::MoviesRoot,
    );
    field_row(
        f,
        rows[2],
        "Series root",
        &st.series_root,
        st.focus == SettingsField::SeriesRoot,
    );

    if let Some((ok, msg)) = &st.status {
        let style = if *ok {
            theme::ok_style()
        } else {
            theme::err_style()
        };
        let prefix = if *ok { "✓ " } else { "✗ " };
        let p = Paragraph::new(Line::from(vec![Span::styled(
            format!("{prefix}{msg}"),
            style,
        )]));
        f.render_widget(p, rows[3]);
    }
}

fn masked(st: &SettingsState) -> String {
    if st.mask_key && !st.api_key.is_empty() {
        let n = st.api_key.chars().count();
        "*".repeat(n.min(40))
    } else {
        st.api_key.clone()
    }
}

fn field_row(f: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let label_style = Style::default().fg(theme::TEXT_DIM);
    let value_style = if focused {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };
    let prefix = if focused { "▸ " } else { "  " };
    let lines = vec![
        Line::from(vec![Span::raw(prefix), Span::styled(label, label_style)]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                if value.is_empty() {
                    "(empty)".into()
                } else {
                    value.to_string()
                },
                value_style,
            ),
        ]),
    ];
    let p = Paragraph::new(lines);
    f.render_widget(p, area);
}

fn render_guide(f: &mut Frame, area: Rect) {
    let inner = widgets::bordered_block("How to get a TMDB API key");
    let inner_area = inner.inner(area);
    f.render_widget(inner, area);

    let text: Vec<Line> = [
        "1. Sign up at https://www.themoviedb.org/signup",
        "2. Verify your email",
        "3. Open Settings → API:",
        "   https://www.themoviedb.org/settings/api",
        "4. Click \"Create\" → \"Developer\"",
        "5. Fill the form (any non-commercial use is fine)",
        "6. Copy \"API Key (v3 auth)\" into the field on the left",
        "",
        "It's free and takes ~3 minutes.",
    ]
    .iter()
    .map(|s| Line::from(Span::styled(*s, Style::default().fg(theme::TEXT_DIM))))
    .collect();

    let p = Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(p, inner_area);
}
