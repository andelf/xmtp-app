mod app;
mod event;
mod format;
mod ipc;
mod ui;

use std::io;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use crossterm::event::EventStream;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use crossterm::{event::Event, execute};
use futures_util::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use xmtp_config::load_config;

use crate::app::App;
use crate::event::AppEvent;
use crate::ipc::Runtime;

#[derive(Debug, Parser)]
#[command(name = "xmtp-tui")]
struct Cli {
    #[arg(long, default_value = "./data")]
    data_dir: PathBuf,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;
    let result = run_app(&mut terminal, cli.data_dir).await;
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    data_dir: PathBuf,
) -> anyhow::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (mut app, initial_effects) = App::new();
    if let Ok(config) = load_config(&data_dir.join("config.json")) {
        app.xmtp_env = Some(config.xmtp_env);
    }
    let mut runtime = Runtime::new(data_dir, tx.clone());
    let daemon_ready = match runtime.ensure_ready().await {
        Ok(()) => true,
        Err(err) => {
        let _ = tx.send(AppEvent::Error(format!("daemon unavailable: {}", err)));
            false
        }
    };
    if daemon_ready {
        runtime.apply_effects(initial_effects).await;
    }

    let mut terminal_events = EventStream::new();
    loop {
        terminal.draw(|frame| ui::render(frame, &app)).context("draw TUI frame")?;

        tokio::select! {
            maybe_event = terminal_events.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        let effects = app.handle_event(AppEvent::Terminal(normalize_terminal_event(event)));
                        runtime.apply_effects(effects).await;
                    }
                    Some(Err(err)) => {
                        let effects = app.handle_event(AppEvent::Error(err.to_string()));
                        runtime.apply_effects(effects).await;
                    }
                    None => break,
                }
            }
            maybe_app_event = rx.recv() => {
                match maybe_app_event {
                    Some(event) => {
                        let effects = app.handle_event(event);
                        runtime.apply_effects(effects).await;
                    }
                    None => break,
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn normalize_terminal_event(event: Event) -> Event {
    event
}
