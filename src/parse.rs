use chrono::{DateTime, Utc};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    pub source: String,
    pub raw: String,
    pub seq: u64,
    pub timestamp: Option<DateTime<Utc>>,
    pub level: Option<String>,
    pub message: String,
}

fn bracket_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // env_logger's default format: `[2026-08-23T14:32:10Z INFO  actix_server::builder] message`
    RE.get_or_init(|| {
        Regex::new(r"^\[(?P<ts>[^\s\]]+)\s+(?P<level>TRACE|DEBUG|INFO|WARN|ERROR)\s*(?:[^\]]*)\]\s?(?P<msg>.*)$").unwrap()
    })
}

fn plain_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `2026-08-23T14:32:10.201Z LOG:  message` / `2026-08-23 14:32:10 INFO message`
    // — the timestamp is matched explicitly (rather than `\S+`) because the
    // date and time can be space-separated, and a bare `\S+` would only
    // grab the date half and then fail to find a level keyword next.
    RE.get_or_init(|| {
        Regex::new(r"^(?P<ts>\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\s+(?P<level>TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|LOG|FATAL)\b:?\s*(?P<msg>.*)$").unwrap()
    })
}

fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // `2026-08-23` with no time, or with a space instead of `T`.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

fn normalize_level(s: &str) -> String {
    match s.to_ascii_uppercase().as_str() {
        "WARNING" => "WARN".to_string(),
        "LOG" => "INFO".to_string(),
        other => other.to_string(),
    }
}

fn parse_json(raw: &str) -> Option<(Option<DateTime<Utc>>, Option<String>, String)> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let obj = value.as_object()?;

    let ts = ["timestamp", "time", "ts", "@timestamp"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .and_then(parse_timestamp);

    let level = ["level", "lvl", "severity"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .map(normalize_level);

    let message = ["message", "msg"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| raw.to_string());

    Some((ts, level, message))
}

/// Parses one line of a log file into structured fields. Falls back
/// gracefully at every stage: unparseable JSON falls through to plain-text
/// regexes, and a line matching neither still becomes a `LogLine` with
/// `timestamp: None, level: None, message: raw` — nothing is ever dropped
/// for being unparseable, it's just less filterable by level/time.
pub fn parse_line(source: &str, seq: u64, raw: &str) -> LogLine {
    if let Some((timestamp, level, message)) = parse_json(raw) {
        return LogLine {
            source: source.to_string(),
            raw: raw.to_string(),
            seq,
            timestamp,
            level,
            message,
        };
    }

    if let Some(caps) = bracket_re().captures(raw) {
        let ts = caps.name("ts").and_then(|m| parse_timestamp(m.as_str()));
        let level = caps.name("level").map(|m| normalize_level(m.as_str()));
        let message = caps
            .name("msg")
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        return LogLine {
            source: source.to_string(),
            raw: raw.to_string(),
            seq,
            timestamp: ts,
            level,
            message,
        };
    }

    if let Some(caps) = plain_re().captures(raw) {
        let ts = caps.name("ts").and_then(|m| parse_timestamp(m.as_str()));
        let level = caps.name("level").map(|m| normalize_level(m.as_str()));
        let message = caps
            .name("msg")
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        return LogLine {
            source: source.to_string(),
            raw: raw.to_string(),
            seq,
            timestamp: ts,
            level,
            message,
        };
    }

    LogLine {
        source: source.to_string(),
        raw: raw.to_string(),
        seq,
        timestamp: None,
        level: None,
        message: raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env_logger_bracket_format() {
        let line = parse_line(
            "app.log",
            0,
            "[2026-08-23T14:32:10Z INFO  actix_server::builder] starting service",
        );
        assert_eq!(line.level.as_deref(), Some("INFO"));
        assert_eq!(line.message, "starting service");
        assert!(line.timestamp.is_some());
    }

    #[test]
    fn parses_json_lines_with_common_field_names() {
        let line = parse_line(
            "app.log",
            0,
            r#"{"timestamp":"2026-08-23T14:32:10Z","level":"error","msg":"boom"}"#,
        );
        assert_eq!(line.level.as_deref(), Some("ERROR"));
        assert_eq!(line.message, "boom");
        assert!(line.timestamp.is_some());
    }

    #[test]
    fn json_takes_priority_over_plain_text_regexes() {
        // This line would also loosely match plain_re if JSON parsing were
        // skipped, so this pins the intended precedence.
        let line = parse_line(
            "app.log",
            0,
            r#"{"level":"warn","message":"disk almost full"}"#,
        );
        assert_eq!(line.level.as_deref(), Some("WARN"));
        assert_eq!(line.message, "disk almost full");
    }

    #[test]
    fn unparseable_line_is_kept_verbatim_as_message() {
        let line = parse_line("app.log", 0, "just some free-text output, no structure");
        assert_eq!(line.level, None);
        assert_eq!(line.timestamp, None);
        assert_eq!(line.message, "just some free-text output, no structure");
    }

    #[test]
    fn warning_and_log_levels_normalize() {
        let l1 = parse_line("a", 0, "[2026-08-23T14:32:10Z WARN  x] msg");
        assert_eq!(l1.level.as_deref(), Some("WARN"));
        let l2 = parse_line(
            "a",
            0,
            "2026-08-23 14:32:10 LOG:  postgres startup complete",
        );
        assert_eq!(l2.level.as_deref(), Some("INFO"));
    }
}
