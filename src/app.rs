//! Application state and keyboard handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};

use crate::edit::EditValue;
use crate::storage::HostFile;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    pub hostname: String,
    /// Stored in plaintext JSON for now — see `storage.rs` for the security note.
    pub password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    List,
    AddHost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Hostname,
    Password,
}

pub struct App {
    pub mode: Mode,
    pub hosts: Vec<Host>,
    /// Selection state for the host list (ListState also handles scrolling).
    pub list: ListState,
    /// Which form field is focused while in `Mode::AddHost`.
    pub form_field: Field,
    pub form_hostname: EditValue,
    pub form_password: EditValue,
    /// Transient hint/error shown in the footer.
    pub status: Option<String>,
    pub should_quit: bool,
    file: HostFile,
}

impl App {
    pub fn new(file: HostFile) -> Self {
        let hosts = match file.load() {
            Ok(hosts) => hosts,
            Err(e) => {
                eprintln!("warning: could not load hosts file: {e:#}");
                Vec::new()
            }
        };
        let mut list = ListState::default();
        if !hosts.is_empty() {
            list.select_first();
        }
        Self {
            mode: Mode::List,
            hosts,
            list,
            form_field: Field::Hostname,
            form_hostname: EditValue::new(false),
            form_password: EditValue::new(true),
            status: None,
            should_quit: false,
            file,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::List => self.on_list_key(key),
            Mode::AddHost => self.on_form_key(key),
        }
    }

    // ---- list mode --------------------------------------------------------

    fn on_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.list.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.list.select_previous(),
            KeyCode::Home | KeyCode::Char('g') => self.list.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.list.select_last(),
            KeyCode::Char('a') => self.open_add_form(),
            KeyCode::Char('d') => self.delete_selected(),
            _ => {}
        }
    }

    fn open_add_form(&mut self) {
        self.form_hostname.clear();
        self.form_password.clear();
        self.form_field = Field::Hostname;
        self.status = None;
        self.mode = Mode::AddHost;
    }

    fn delete_selected(&mut self) {
        if self.hosts.is_empty() {
            return;
        }
        let idx = self.list.selected().unwrap_or(0);
        let removed = self.hosts.remove(idx);
        if self.hosts.is_empty() {
            self.list = ListState::default();
        } else if idx >= self.hosts.len() {
            self.list.select_last();
        } else {
            self.list.select(Some(idx));
        }
        self.status = Some(format!("Deleted {}", removed.hostname));
        self.save();
    }

    // ---- form mode --------------------------------------------------------

    fn on_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_add(),
            KeyCode::Enter => self.submit_add(),
            KeyCode::Tab | KeyCode::Down => self.focus_next(),
            KeyCode::BackTab | KeyCode::Up => self.focus_prev(),
            KeyCode::Left => self.focused().move_left(),
            KeyCode::Right => self.focused().move_right(),
            KeyCode::Home => self.focused().home(),
            KeyCode::End => self.focused().end(),
            KeyCode::Backspace => self.focused().backspace(),
            KeyCode::Delete => self.focused().delete(),
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.focused().insert_char(c)
            }
            _ => {}
        }
    }

    fn focused(&mut self) -> &mut EditValue {
        match self.form_field {
            Field::Hostname => &mut self.form_hostname,
            Field::Password => &mut self.form_password,
        }
    }

    fn focus_next(&mut self) {
        self.form_field = match self.form_field {
            Field::Hostname => Field::Password,
            Field::Password => Field::Hostname,
        };
    }

    fn focus_prev(&mut self) {
        self.focus_next();
    }

    fn cancel_add(&mut self) {
        self.mode = Mode::List;
        self.status = Some("Cancelled".into());
    }

    fn submit_add(&mut self) {
        let hostname = self.form_hostname.value().trim().to_string();
        if hostname.is_empty() {
            self.status = Some("Host name is required".into());
            return;
        }
        let password = self.form_password.value().to_string();
        self.hosts.push(Host { hostname: hostname.clone(), password });
        self.list.select_last();
        self.mode = Mode::List;
        self.status = Some(format!("Added {hostname}"));
        self.save();
    }

    // ---- persistence ------------------------------------------------------

    fn save(&mut self) {
        if let Err(e) = self.file.save(&self.hosts) {
            self.status = Some(format!("save failed: {e:#}"));
        }
    }
}