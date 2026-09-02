//! Basic SSH host manager with keyboard-first navigation.
//!
//! - list hosts, navigate with j/k or arrows
//! - `a` opens a form to add a host (hostname + password)
//! - `d` deletes the selected host, `q`/Esc quits
//!
//! Hosts are persisted as JSON — see `storage.rs` for the security note.

mod app;
mod edit;
mod ssh;
mod storage;
mod ui;

use std::time::Duration;

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Mode};
use storage::HostFile;

fn main() -> anyhow::Result<()> {
    let mut terminal = setup_terminal().context("terminal setup failed")?;
    let result = run(&mut terminal);
    restore_terminal(terminal).context("terminal restore failed")?;
    result
}

/// Enter raw mode and the alternate screen, returning a `Terminal` handle.
fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode().context("enable_raw_mode failed")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("EnterAlternateScreen failed")?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

/// Restore the terminal to a sane state, even on error paths.
fn restore_terminal(
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<()> {
    disable_raw_mode().context("disable_raw_mode failed")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("LeaveAlternateScreen failed")?;
    terminal.show_cursor().context("show_cursor failed")?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
    let path = HostFile::default_path().context("could not resolve hosts data path")?;
    let mut app = App::new(HostFile::new(path));
    terminal.hide_cursor().context("hide_cursor failed")?;

    while !app.should_quit {
        terminal
            .draw(|frame| ui::draw(frame, &mut app))
            .context("draw failed")?;

        // Show the terminal cursor only while editing the add-host form.
        match app.mode {
            Mode::AddHost => terminal.show_cursor().context("show_cursor failed")?,
            Mode::List => terminal.hide_cursor().context("hide_cursor failed")?,
        }

        if event::poll(Duration::from_millis(100)).context("event poll failed")? {
            if let Event::Key(key) = event::read().context("event read failed")? {
                // Ignore key release events; accept press and (key-repeat) events.
                if key.kind != KeyEventKind::Release {
                    app.on_key(key);
                }
            }
        }

        // Enter pressed on a host → run the real ssh session (blocks until it exits).
        if let Some(idx) = app.take_connect_request() {
            if let Some(host) = app.hosts.get(idx).cloned() {
                let result = ssh::connect_session(&host);
                app.set_status(match result {
                    Ok(st) if st.success() => {
                        format!("Session ended ({})", host.label())
                    }
                    Ok(st) => format!(
                        "ssh exited ({}) with code {}",
                        host.label(),
                        st.code().unwrap_or(-1)
                    ),
                    Err(e) => format!("connect failed: {e:#}"),
                });
                // The alternate screen was replaced by the ssh session; draw an
                // empty frame so the next frame repaints everything.
                // (Deliberately NOT `Terminal::clear()`, which asks the terminal
                // for its cursor position — terminals that don't answer that
                // query would make us hang here.)
                terminal.draw(|_frame| {})?;
            }
        }
    }
    Ok(())
}
