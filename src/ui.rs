use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use logview::app::App;
use logview::parse::LogLine;

fn level_style(level: Option<&str>) -> Style {
    match level {
        Some("ERROR") | Some("FATAL") => {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        }
        Some("WARN") => Style::default().fg(Color::Yellow),
        Some("DEBUG") | Some("TRACE") => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn line_to_ratatui<'a>(line: &'a LogLine, show_source: bool) -> Line<'a> {
    let mut spans = Vec::new();
    if show_source {
        spans.push(Span::styled(
            format!("{:<12} ", truncate(&line.source, 12)),
            Style::default().fg(Color::Cyan),
        ));
    }
    if let Some(level) = &line.level {
        spans.push(Span::styled(
            format!("{level:<5} "),
            level_style(Some(level)),
        ));
    }
    spans.push(Span::styled(
        line.message.clone(),
        level_style(line.level.as_deref()),
    ));
    Line::from(spans)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

pub fn render(frame: &mut Frame, app: &App, multi_source: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    render_log_area(frame, chunks[0], app, multi_source);
    render_status_bar(frame, chunks[1], app);
}

fn render_log_area(frame: &mut Frame, area: Rect, app: &App, multi_source: bool) {
    let filtered = app.filtered();
    let height = area.height as usize;
    let total = filtered.len();
    let end = total.saturating_sub(app.scroll_up);
    let start = end.saturating_sub(height);

    let lines: Vec<Line> = filtered[start..end]
        .iter()
        .map(|l| line_to_ratatui(l, multi_source))
        .collect();

    let title = if app.is_following() {
        "logview (following)"
    } else {
        "logview (scrolled — G to resume following)"
    };
    let block = Block::default().borders(Borders::TOP).title(title);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let text = if app.editing_filter {
        format!("/{}", app.filter_pattern)
    } else {
        let filter_desc = if app.filter_pattern.is_empty() {
            "no filter".to_string()
        } else {
            format!("filter: {}", app.filter_pattern)
        };
        let level_desc = app
            .level_floor
            .map(|l| format!("level>={l}"))
            .unwrap_or_else(|| "all levels".to_string());
        format!(
            "{} | {} | {} lines shown | / filter  w/e/a level  j/k or ↑/↓ scroll  G bottom  g top  q quit",
            filter_desc,
            level_desc,
            app.filtered().len()
        )
    };
    frame.render_widget(Paragraph::new(text), area);
}
