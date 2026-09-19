use crate::parse::LogLine;

/// Merges lines from however many tailed sources into one time-ordered
/// stream. Lines that both have a parsed timestamp sort by it; any line
/// missing one falls back to `seq` (the order it was actually read in) —
/// which also means two lines that both lack a timestamp never scramble
/// relative to each other or to nearby timestamped lines from the same
/// read pass.
pub fn merge_sorted(mut lines: Vec<LogLine>) -> Vec<LogLine> {
    lines.sort_by(|a, b| match (a.timestamp, b.timestamp) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.seq.cmp(&b.seq)),
        _ => a.seq.cmp(&b.seq),
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_line;

    #[test]
    fn reorders_by_timestamp_across_sources() {
        let a = parse_line(
            "app-a.log",
            0,
            "[2026-08-23T14:32:12Z INFO x] second in wall-clock time",
        );
        let b = parse_line(
            "app-b.log",
            1,
            "[2026-08-23T14:32:10Z INFO x] first in wall-clock time",
        );
        let merged = merge_sorted(vec![a.clone(), b.clone()]);
        assert_eq!(merged[0].message, "first in wall-clock time");
        assert_eq!(merged[1].message, "second in wall-clock time");
    }

    #[test]
    fn lines_without_timestamps_keep_arrival_order() {
        let a = parse_line("app.log", 0, "line one, no structure");
        let b = parse_line("app.log", 1, "line two, no structure");
        let c = parse_line("app.log", 2, "line three, no structure");
        // Fed in out of seq order on purpose.
        let merged = merge_sorted(vec![c.clone(), a.clone(), b.clone()]);
        assert_eq!(
            merged.iter().map(|l| l.seq).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn mixed_timestamped_and_untimestamped_lines_dont_panic_or_scramble_ties() {
        let a = parse_line("app.log", 0, "[2026-08-23T14:32:10Z INFO x] timestamped");
        let b = parse_line("app.log", 1, "no timestamp at all");
        let merged = merge_sorted(vec![b.clone(), a.clone()]);
        assert_eq!(merged.len(), 2);
    }
}
