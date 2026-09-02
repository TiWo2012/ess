//! Rendering: host list, footer, and the add/edit-host popup.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Field, Mode};
use crate::edit::focus_border;

pub fn draw(frame: &mut Frame, app: &mut App) {
    draw_list(frame, app);
    if app.mode == Mode::AddHost {
        draw_form(frame, app);
    }
}

fn draw_list(frame: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());

    // Header block with host count in the top-right corner.
    let count = app.hosts.len();
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(Line::from(" SSH Manager ").alignment(Alignment::Left))
            .title(
                Line::from(format!(
                    " {count} host{} ",
                    if count == 1 { "" } else { "s" }
                ))
                .alignment(Alignment::Right),
            ),
        header,
    );

    // Host list (stateful: ListState keeps selection and scroll offset).
    if app.hosts.is_empty() {
        frame.render_widget(
            Paragraph::new("No hosts yet — press a to add one.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            body,
        );
    } else {
        let items: Vec<ListItem> = app
            .hosts
            .iter()
            .map(|h| {
                let has_pw = !h.password.is_empty();
                let pw = if has_pw {
                    "password saved"
                } else {
                    "no password"
                };
                let pw_style = if has_pw {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(h.label(), Style::default().fg(Color::White)),
                    Span::styled(format!("  [{pw}]"), pw_style),
                ]))
            })
            .collect();
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Hosts "))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▸ "),
            body,
            &mut app.list,
        );
    }

    // Footer: transient status message or key hints.
    let (text, style) = match &app.status {
        Some(msg) => (
            Line::from(msg.clone()).alignment(Alignment::Center),
            Style::default().fg(Color::Yellow),
        ),
        None => (
            Line::from(
                " Enter connect · j/k move · g/G top/bottom · a add · e edit · d delete · q quit ",
            )
            .alignment(Alignment::Center),
            Style::default().fg(Color::DarkGray),
        ),
    };
    frame.render_widget(Paragraph::new(text).style(style), footer);
}

fn draw_form(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    // Never exceed the available area (tiny/zero-width terminals must not panic).
    let w = area.width.clamp(24, 52).min(area.width);
    let h = 15.min(area.height);
    let popup = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(if app.edit_index.is_some() {
                " Edit host "
            } else {
                " Add host "
            })
            .title_bottom(
                Line::from(" Enter: save · Esc: cancel · Tab: next field ")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray)),
            ),
        popup,
    );

    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };
    let [hostname_area, user_area, port_area, password_area, hint_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(inner);

    // Host name field.
    let focused = app.form_field == Field::Hostname;
    app.form_hostname.render(
        frame,
        hostname_area,
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(focused))
            .title(" Host name "),
        focused,
    );

    // User field.
    let focused = app.form_field == Field::User;
    app.form_user.render(
        frame,
        user_area,
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(focused))
            .title(" User (empty = local user) "),
        focused,
    );

    // Port field.
    let focused = app.form_field == Field::Port;
    app.form_port.render(
        frame,
        port_area,
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(focused))
            .title(" Port (default 22) "),
        focused,
    );

    // Password field.
    let focused = app.form_field == Field::Password;
    app.form_password.render(
        frame,
        password_area,
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(focused))
            .title(" Password "),
        focused,
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " password sent automatically on connect; stored in the OS keyring ",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        hint_area,
    );
}
