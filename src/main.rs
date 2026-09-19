mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use logview::app::App;
use logview::tail::TailSource;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

#[derive(Parser)]
#[command(
    name = "logview",
    about = "A modern lnav — multi-file structured log tailing/filtering TUI"
)]
struct Cli {
    /// One or more log files to tail together, merged and sorted by
    /// timestamp where one can be parsed.
    files: Vec<PathBuf>,

    /// Start with this filter already applied (same syntax as pressing `/`
    /// in the TUI: a regex, or a plain substring if it doesn't compile).
    #[arg(long)]
    filter: Option<String>,

    /// Only show lines at or above this level (TRACE/DEBUG/INFO/WARN/ERROR).
    /// Lines whose level couldn't be parsed are never hidden by this.
    #[arg(long)]
    level: Option<String>,

    /// How many lines to keep in memory (oldest dropped once exceeded).
    #[arg(long, default_value_t = 20_000)]
    max_lines: usize,

    /// How many bytes from the end of each file to read on startup, so
    /// opening a huge pre-existing file doesn't try to load all of it.
    #[arg(long, default_value_t = 2 * 1024 * 1024)]
    seed_bytes: u64,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.files.is_empty() {
        anyhow::bail!("give at least one log file to tail, e.g. `logview /var/log/myapp.log`");
    }

    let mut app = App::new(cli.max_lines);
    if let Some(pattern) = &cli.filter {
        app.filter_pattern = pattern.clone();
        app.confirm_filter();
    }
    if let Some(level) = &cli.level {
        let leaked: &'static str = Box::leak(level.to_ascii_uppercase().into_boxed_str());
        app.set_level_floor(Some(leaked));
    }
    let multi_source = cli.files.len() > 1;

    let mut sources = Vec::new();
    for path in &cli.files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut source = TailSource::open(path, name)
            .map_err(|e| anyhow::anyhow!("opening {}: {e}", path.display()))?;
        for line in source.seed_from_near_end(cli.seed_bytes)? {
            app.push_raw_line(&source.name, &line);
        }
        sources.push(source);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &mut app, &mut sources, multi_source);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    sources: &mut [TailSource],
    multi_source: bool,
) -> anyhow::Result<()> {
    loop {
        for source in sources.iter_mut() {
            for line in source.poll()? {
                app.push_raw_line(&source.name, &line);
            }
        }

        terminal.draw(|f| ui::render(f, app, multi_source))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                handle_key(app, key.code);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode) {
    if app.editing_filter {
        match code {
            KeyCode::Char(c) => app.apply_filter_input(c),
            KeyCode::Backspace => app.backspace_filter_input(),
            KeyCode::Enter => app.confirm_filter(),
            KeyCode::Esc => app.cancel_filter_edit(),
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('/') => app.start_editing_filter(),
        KeyCode::Char('w') => app.set_level_floor(Some("WARN")),
        KeyCode::Char('e') => app.set_level_floor(Some("ERROR")),
        KeyCode::Char('a') => app.set_level_floor(None),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up_one(),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down_one(),
        KeyCode::Char('G') | KeyCode::End => app.jump_to_bottom(),
        KeyCode::Char('g') | KeyCode::Home => app.jump_to_top(),
        _ => {}
    }
}
