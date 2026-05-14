use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Gauge, Paragraph},
    Frame,
};

use crate::tui::{theme, widgets};

#[derive(Debug, Default)]
pub struct ApplyingState {
    pub current: u64,
    pub total: u64,
    pub message: String,
    pub log_tail: Vec<String>,
}

impl ApplyingState {
    pub fn push(&mut self, current: u64, total: u64, message: String) {
        self.current = current;
        self.total = total;
        self.message = message.clone();
        self.log_tail.push(message);
        if self.log_tail.len() > 200 {
            let drop = self.log_tail.len() - 200;
            self.log_tail.drain(..drop);
        }
    }
}

pub fn render(f: &mut Frame, area: Rect, st: &ApplyingState) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    widgets::header(f, main[0], "Applying", &st.message);

    let pct = if st.total > 0 {
        ((st.current as f64 / st.total as f64) * 100.0).clamp(0.0, 100.0) as u16
    } else {
        0
    };
    let gauge = Gauge::default()
        .block(widgets::bordered_block("Progress"))
        .gauge_style(ratatui::style::Style::default().fg(theme::ACCENT))
        .percent(pct)
        .label(format!("{}/{}", st.current, st.total));
    f.render_widget(gauge, main[1]);

    let log_block = widgets::bordered_block("Log");
    let log_area = log_block.inner(main[2]);
    f.render_widget(log_block, main[2]);
    let lines: Vec<ratatui::text::Line> = st
        .log_tail
        .iter()
        .rev()
        .take(log_area.height as usize)
        .rev()
        .map(|s| ratatui::text::Line::raw(s.clone()))
        .collect();
    f.render_widget(Paragraph::new(lines), log_area);

    widgets::footer(f, main[3], &[("Esc", "cancel")]);
}
