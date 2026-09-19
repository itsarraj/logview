use std::collections::VecDeque;

use crate::filter::{passes_level_floor, Filter};
use crate::parse::LogLine;

pub struct App {
    pub lines: VecDeque<LogLine>,
    pub max_lines: usize,
    pub seq_counter: u64,

    pub filter_pattern: String,
    pub filter: Filter,
    pub level_floor: Option<&'static str>,

    /// Lines scrolled up from the bottom. 0 means "following" (new lines
    /// keep the view pinned to the bottom, tail -f style).
    pub scroll_up: usize,

    pub editing_filter: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines.min(1024)),
            max_lines,
            seq_counter: 0,
            filter_pattern: String::new(),
            filter: Filter::None,
            level_floor: None,
            scroll_up: 0,
            editing_filter: false,
            should_quit: false,
        }
    }

    pub fn push_raw_line(&mut self, source: &str, raw: &str) {
        let seq = self.seq_counter;
        self.seq_counter += 1;
        let line = crate::parse::parse_line(source, seq, raw);
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn filtered(&self) -> Vec<&LogLine> {
        self.lines
            .iter()
            .filter(|l| self.filter.matches(l) && passes_level_floor(l, self.level_floor))
            .collect()
    }

    pub fn is_following(&self) -> bool {
        self.scroll_up == 0
    }

    pub fn scroll_up_one(&mut self) {
        let max = self.filtered().len().saturating_sub(1);
        self.scroll_up = (self.scroll_up + 1).min(max);
    }

    pub fn scroll_down_one(&mut self) {
        self.scroll_up = self.scroll_up.saturating_sub(1);
    }

    pub fn jump_to_bottom(&mut self) {
        self.scroll_up = 0;
    }

    pub fn jump_to_top(&mut self) {
        self.scroll_up = self.filtered().len().saturating_sub(1);
    }

    pub fn start_editing_filter(&mut self) {
        self.editing_filter = true;
    }

    pub fn apply_filter_input(&mut self, c: char) {
        self.filter_pattern.push(c);
    }

    pub fn backspace_filter_input(&mut self) {
        self.filter_pattern.pop();
    }

    pub fn confirm_filter(&mut self) {
        self.filter = Filter::new(&self.filter_pattern);
        self.editing_filter = false;
    }

    pub fn cancel_filter_edit(&mut self) {
        self.editing_filter = false;
    }

    pub fn set_level_floor(&mut self, floor: Option<&'static str>) {
        self.level_floor = floor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_drops_oldest_once_full() {
        let mut app = App::new(3);
        for i in 0..5 {
            app.push_raw_line("a", &format!("line {i}"));
        }
        assert_eq!(app.lines.len(), 3);
        assert_eq!(app.lines.front().unwrap().message, "line 2");
        assert_eq!(app.lines.back().unwrap().message, "line 4");
    }

    #[test]
    fn scroll_up_clamps_to_available_filtered_lines() {
        let mut app = App::new(10);
        app.push_raw_line("a", "one");
        app.push_raw_line("a", "two");
        for _ in 0..10 {
            app.scroll_up_one();
        }
        assert_eq!(app.scroll_up, 1, "only 2 lines exist, max scroll is 1");
    }

    #[test]
    fn jump_to_bottom_resumes_following() {
        let mut app = App::new(10);
        app.push_raw_line("a", "one");
        app.push_raw_line("a", "two");
        app.scroll_up_one();
        assert!(!app.is_following());
        app.jump_to_bottom();
        assert!(app.is_following());
    }

    #[test]
    fn filter_hides_non_matching_lines() {
        let mut app = App::new(10);
        app.push_raw_line("a", "everything is fine");
        app.push_raw_line("a", "disk is full");
        app.filter_pattern = "full".to_string();
        app.confirm_filter();
        let filtered = app.filtered();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].message, "disk is full");
    }

    #[test]
    fn canceling_filter_edit_keeps_previous_filter_applied() {
        let mut app = App::new(10);
        app.push_raw_line("a", "keep me");
        app.push_raw_line("a", "drop me");
        app.filter_pattern = "keep".to_string();
        app.confirm_filter();

        app.start_editing_filter();
        app.apply_filter_input('x');
        app.apply_filter_input('x');
        app.cancel_filter_edit();

        assert_eq!(
            app.filtered().len(),
            1,
            "the 'keep' filter must still be applied, not 'keepxx'"
        );
    }
}
