use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::theme;

pub fn header(f: &mut Frame, area: Rect, title: &str, hint: &str) {
    let line = Line::from(vec![
        Span::styled(title, theme::title_style()),
        Span::raw("  "),
        Span::styled(hint, theme::dim_style()),
    ]);
    let p = Paragraph::new(line);
    f.render_widget(p, area);
}

pub fn footer(f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let mut spans = Vec::new();
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(format!("[{key}]"), theme::title_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, Style::default().fg(theme::TEXT)));
    }
    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}

pub fn bordered_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(format!(" {title} "), theme::title_style()))
}
