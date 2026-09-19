# logview

A modern `lnav` — C++'s `lnav` (tail multiple log files, live filter, one
powerful TUI) has no serious Rust competitor despite `ratatui` being a
mature, popular library. `logview` covers the tail/filter/multi-source core
of that (not `lnav`'s SQL-query-over-logs feature — see Scope below).

## Usage

```bash
logview /var/log/myapp.log
logview apps/roasted/*.log apps/confide/*.log     # merged, sorted by timestamp where parseable
logview --filter 'ERROR|WARN' --level warn app.log
```

Keys: `/` to type a filter (regex, or plain substring if it doesn't
compile) and Enter to apply, Esc to cancel without changing the current
filter; `w`/`e`/`a` for a quick WARN/ERROR/all level floor; `j`/`k` or
arrows to scroll; `G`/`End` to jump to the bottom and resume following new
lines; `g`/`Home` to jump to the top; `q` to quit.

## What it parses

Each line is tried against, in order: **JSON** (common field names —
`timestamp`/`time`/`ts`, `level`/`lvl`/`severity`, `message`/`msg`), then
**`env_logger`'s bracket format** (`[2026-08-23T14:32:10Z INFO module] msg`
— what every Actix app in `apps/` already logs), then a **plain
`timestamp LEVEL message`** format. A line matching none of these is still
shown, verbatim, just without a parsed level/timestamp — nothing is ever
dropped for being unparseable.

## Scope vs. `lnav`

`lnav`'s standout feature is querying log lines with SQL. Not built here —
that's a genuinely large second project (a query engine over structured
log data), and this v1's value is the multi-file tail/filter/merge core,
which is itself the missing piece in Rust. A reasonable v2, not a v1.

## Status: built and verified — including through a real pty, not just unit tests

- **22 unit tests** (`cargo test --lib`) across every pure module:
  - `parse`: the `env_logger` bracket format, JSON (and that JSON is tried
    *before* the plain-text regexes, pinned by a test), unparseable lines
    kept verbatim, `WARNING`/`LOG` level normalization.
  - `filter`: case-insensitive substring, a deliberately-invalid regex
    (`[abc`) falling back to literal-substring instead of erroring, a valid
    regex used as a regex, and — the one that actually matters — a level
    floor that hides `INFO` under a `WARN` floor but **never** hides a line
    whose level couldn't be parsed.
  - `merge`: lines from two different sources correctly reordered by
    timestamp; lines with no timestamp keep arrival order; mixed
    timestamped/untimestamped lines don't panic or scramble ties.
  - `tail`: a real temp file, written to incrementally — a line with no
    trailing `\n` yet is correctly held back and only surfaces once
    completed; `\r\n` handled; a bounded initial read on a 1000-line file
    via `seed_from_near_end` provably doesn't return all 1000 lines yet
    still ends on the file's actual last line.
  - `app`: the ring buffer actually drops the oldest line once full (not
    just "doesn't crash"), scroll clamps to the real number of *filtered*
    lines (not the full buffer), and — a real bug class this test exists
    to catch — canceling a filter-edit-in-progress correctly leaves the
    *previous* applied filter in effect rather than the half-typed one.
- **Live pty verification**, not just unit tests: launched the actual
  compiled binary against a real log file inside a real pseudo-terminal
  (Python's `pty` module, with an explicit `TIOCSWINSZ` — a freshly forked
  pty starts at 0×0 size, which silently renders nothing and was caught and
  fixed in this same verification pass, not assumed away), then:
  - confirmed the rendered frame actually contains the log content and
    level tags (`INFO event number 1`, `ERROR disk almost full`), not just
    that the process didn't crash;
  - sent real keystrokes (`/ERROR` + Enter) and confirmed the **next
    rendered frame** shows only the `ERROR` line and the status bar reading
    `filter:ERROR | all levels | 1 lines shown` — the filter was proven to
    actually take effect on screen, not just internally;
  - sent `q` and confirmed the process exits on its own (no `SIGKILL`
    needed) both with and without the filter active.

**Not done / deliberately deferred**: SQL-over-logs (see Scope), a
config file for saved per-directory filters (`lnav`'s "saved queries"), and
inotify-based tailing — this polls each file every 200ms instead, which is
simpler and works identically across platforms without an extra
dependency, at the cost of a bounded (small) latency versus a true
filesystem-event push.
