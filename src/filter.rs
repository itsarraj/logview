use regex::Regex;

use crate::parse::LogLine;

#[derive(Debug, Clone)]
pub enum Filter {
    None,
    Substring(String),
    Regex(Regex),
}

impl Filter {
    /// Tries to compile `pattern` as a regex; falls back to a plain
    /// case-insensitive substring match if it doesn't compile — so a user
    /// typing `[weird]` or `a.b` as a literal search term gets something
    /// reasonable instead of an error dialog mid-TUI.
    pub fn new(pattern: &str) -> Self {
        if pattern.is_empty() {
            return Filter::None;
        }
        match Regex::new(&format!("(?i){pattern}")) {
            Ok(re) => Filter::Regex(re),
            Err(_) => Filter::Substring(pattern.to_ascii_lowercase()),
        }
    }

    pub fn matches(&self, line: &LogLine) -> bool {
        match self {
            Filter::None => true,
            Filter::Substring(needle) => line.raw.to_ascii_lowercase().contains(needle.as_str()),
            Filter::Regex(re) => re.is_match(&line.raw),
        }
    }
}

/// Optional level floor — e.g. only show WARN and above. `None` means no
/// level filtering (also true for lines whose level couldn't be parsed:
/// they're never hidden by a level filter, only by the text filter, since
/// hiding unparsed lines by an absent level would silently drop exactly the
/// lines a user most needs to see raw).
pub fn level_rank(level: &str) -> u8 {
    match level {
        "TRACE" => 0,
        "DEBUG" => 1,
        "INFO" => 2,
        "WARN" => 3,
        "ERROR" => 4,
        "FATAL" => 5,
        _ => 2,
    }
}

pub fn passes_level_floor(line: &LogLine, floor: Option<&str>) -> bool {
    match (floor, &line.level) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(floor), Some(level)) => level_rank(level) >= level_rank(floor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_line;

    #[test]
    fn substring_filter_is_case_insensitive() {
        let line = parse_line("a", 0, "Connection Refused by upstream");
        let f = Filter::new("refused");
        assert!(f.matches(&line));
    }

    #[test]
    fn invalid_regex_falls_back_to_literal_substring() {
        // `[abc` is an invalid regex (unterminated class) but a
        // perfectly reasonable literal string to search for.
        let line = parse_line("a", 0, "seen: [abc in the raw log line");
        let f = Filter::new("[abc");
        assert!(matches!(f, Filter::Substring(_)));
        assert!(f.matches(&line));
    }

    #[test]
    fn valid_regex_is_used_as_regex() {
        let line = parse_line("a", 0, "user id=482 logged in");
        let f = Filter::new(r"id=\d+");
        assert!(matches!(f, Filter::Regex(_)));
        assert!(f.matches(&line));
        let line2 = parse_line("a", 0, "user id=abc logged in");
        assert!(!f.matches(&line2));
    }

    #[test]
    fn empty_pattern_matches_everything() {
        let f = Filter::new("");
        let line = parse_line("a", 0, "anything at all");
        assert!(f.matches(&line));
    }

    #[test]
    fn level_floor_hides_below_threshold_but_never_hides_unparsed() {
        let info = parse_line("a", 0, "[2026-08-23T14:32:10Z INFO  x] just info");
        let error = parse_line("a", 0, "[2026-08-23T14:32:10Z ERROR x] bad thing");
        let unparsed = parse_line("a", 0, "totally free text");

        assert!(!passes_level_floor(&info, Some("WARN")));
        assert!(passes_level_floor(&error, Some("WARN")));
        assert!(passes_level_floor(&unparsed, Some("WARN")));
    }
}
