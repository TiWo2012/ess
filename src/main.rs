use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};

fn main() -> anyhow::Result<()> {
    let mut terminal = setup_terminal().context("failed to set up terminal")?;
    let res = run(&mut terminal);
    restore_terminal(terminal).context("failed to restore terminal")?;
    res
}

/// Enter raw mode and the alternate screen, returning a `Terminal` handle.
fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("enable_raw_mode failed")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("EnterAlternateScreen failed")?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

/// Restore the terminal to a sane state, even on error/panic paths.
fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode().context("disable_raw_mode failed")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).context("LeaveAlternateScreen failed")?;
    terminal.show_cursor().context("show_cursor failed")?;
    Ok(())
}

/// The main loop: draw a frame, then poll for input until it's time to quit.
fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
    let tick_rate = Duration::from_millis(250);
    let last_tick = Instant::now();
    let mut ticks = 0u32;
    let done = false;

    loop {
        terminal.draw(|frame| ui(frame, ticks, done))?;

        if event::poll(tick_rate).context("event poll failed")? {
            if let Event::Key(key) = event::read().context("event read failed")? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        // quit on q or Esc
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        _ => {}
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            ticks = ticks.wrapping_add(1);
        }
    }
}

/// Render one frame.
fn ui(frame: &mut Frame, ticks: u32, _done: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(3), // counter paragraph
            Constraint::Length(1), // gauge
            Constraint::Min(3),    // list fills the rest
        ])
        .split(frame.area());

    // Title
    frame.render_widget(
        Paragraph::new("Basic ratatui + crossterm demo — press q or Esc to quit")
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    // Live counter
    let counter_line = Line::from(vec![
        Span::styled("Ticks: ", Style::default().fg(Color::Yellow)),
        Span::styled(format!("{ticks}"), Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(
        Paragraph::new(counter_line).block(Block::default().borders(Borders::ALL)),
        chunks[1],
    );

    // Progress gauge (ticks wraps around)
    let percent = (ticks % 100) as f64;
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(Color::Green))
            .percent(percent as u16),
        chunks[2],
    );

    // Simple list
    let items: Vec<ListItem> = (0..10)
        .map(|i| {
            ListItem::new(format!("Item {i} — tick {ticks}"))
                .style(Style::default().fg(Color::White))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("List")),
        chunks[3],
    );
}