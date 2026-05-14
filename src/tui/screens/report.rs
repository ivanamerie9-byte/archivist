use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use crate::domain::ExecutionReport;
use crate::tui::{theme, widgets};

#[derive(Debug)]
pub struct ReportState {
    pub report: ExecutionReport,
}

pub fn render(f: &mut Frame, area: Rect, st: &ReportState) {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let summary = format!(
        "{} succeeded · {} failed · {} skipped",
        st.report.successes.len(),
        st.report.failures.len(),
        st.report.skipped.len()
    );
    widgets::header(f, main[0], "Report", &summary);

    let block = widgets::bordered_block("Details");
    let inner = block.inner(main[1]);
    f.render_widget(block, main[1]);

    let mut lines: Vec<Line> = Vec::new();
    for s in &st.report.successes {
        lines.push(Line::from(vec![
            Span::styled("✓ ", theme::ok_style()),
            Span::styled(s.clone(), Style::default().fg(theme::TEXT)),
        ]));
    }
    for (label, err) in &st.report.failures {
        lines.push(Line::from(vec![
            Span::styled("✗ ", theme::err_style()),
            Span::styled(label.clone(), Style::default().fg(theme::TEXT)),
        ]));
        // Error message goes on its own indented line so it doesn't get
        // truncated by the terminal width.
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(err.clone(), theme::dim_style()),
        ]));
    }
    for s in &st.report.skipped {
        lines.push(Line::from(vec![
            Span::styled("· ", theme::dim_style()),
            Span::styled(s.clone(), theme::dim_style()),
        ]));
    }

    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(p, inner);

    widgets::footer(
        f,
        main[2],
        &[("Enter", "back to library select"), ("q", "quit")],
    );
}
