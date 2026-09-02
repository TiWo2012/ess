//! Application state and keyboard handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};

use crate::edit::EditValue;
use crate::storage::HostFile;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    /// `user@host` or a plain hostname — anything `ssh` accepts as a target.
    pub hostname: String,
    /// 0 = use ssh's default port (22).
    #[serde(default)]
    pub port: u16,
    /// Stored in plaintext JSON for now — see `storage.rs` for the security note.
    pub password: String,
}

impl Host {
    /// Human-friendly label like `user@host:2222`.
    pub fn label(&self) -> String {
        if self.port != 0 {
            format!("{}:{}", self.hostname, self.port)
        } else {
            self.hostname.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    List,
    AddHost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Hostname,
    Port,
    Password,
}

pub struct App {
    pub mode: Mode,
    pub hosts: Vec<Host>,
    /// Selection state for the host list (ListState also handles scrolling).
    pub list: ListState,
    /// Which form field is focused while in `Mode::AddHost` / edit mode.
    pub form_field: Field,
    pub form_hostname: EditValue,
    pub form_port: EditValue,
    pub form_password: EditValue,
    /// `Some(i)` while the form is open to *edit* host `i` (None = adding).
    pub edit_index: Option<usize>,
    /// Transient hint/error shown in the footer.
    pub status: Option<String>,
    pub should_quit: bool,
    /// Set by Enter in list mode; the main loop runs the ssh session.
    pending_connect: Option<usize>,
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
            form_port: EditValue::new(false).numeric(),
            form_password: EditValue::new(true),
            edit_index: None,
            status: None,
            should_quit: false,
            pending_connect: None,
            file,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::List => self.on_list_key(key),
            Mode::AddHost => self.on_form_key(key),
        }
    }

    /// Called by the main loop: after `Enter` on a host, take the request.
    pub fn take_connect_request(&mut self) -> Option<usize> {
        self.pending_connect.take()
    }

    pub fn set_status(&mut self, msg: String) {
        self.status = Some(msg);
    }

    // ---- list mode --------------------------------------------------------

    fn on_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Enter => {
                if let Some(i) = self.list.selected() {
                    self.pending_connect = Some(i);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.list.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.list.select_previous(),
            KeyCode::Home | KeyCode::Char('g') => self.list.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.list.select_last(),
            KeyCode::Char('a') => self.open_form(None),
            KeyCode::Char('e') => {
                if let Some(i) = self.list.selected() {
                    self.open_form(Some(i));
                }
            }
            KeyCode::Char('d') => self.delete_selected(),
            _ => {}
        }
    }

    fn open_form(&mut self, edit: Option<usize>) {
        match edit.and_then(|i| self.hosts.get(i)) {
            Some(h) => {
                let port = if h.port != 0 {
                    h.port.to_string()
                } else {
                    String::new()
                };
                self.form_hostname = EditValue::from(&h.hostname, false);
                self.form_port = EditValue::from(&port, false).numeric();
                self.form_password = EditValue::from(&h.password, true);
            }
            None => {
                self.form_hostname.clear();
                self.form_port.clear();
                self.form_password.clear();
            }
        }
        self.edit_index = edit;
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
        self.status = Some(format!("Deleted {}", removed.label()));
        self.save();
    }

    // ---- form mode --------------------------------------------------------

    fn on_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_form(),
            KeyCode::Enter => self.submit_form(),
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
            Field::Port => &mut self.form_port,
            Field::Password => &mut self.form_password,
        }
    }

    fn focus_next(&mut self) {
        self.form_field = match self.form_field {
            Field::Hostname => Field::Port,
            Field::Port => Field::Password,
            Field::Password => Field::Hostname,
        };
    }

    fn focus_prev(&mut self) {
        self.focus_next();
    }

    fn cancel_form(&mut self) {
        self.edit_index = None;
        self.mode = Mode::List;
        self.status = Some("Cancelled".into());
    }

    fn submit_form(&mut self) {
        let hostname = self.form_hostname.value().trim().to_string();
        if hostname.is_empty() {
            self.status = Some("Host name is required".into());
            return;
        }
        let port = match self.form_port.value().parse::<u16>() {
            Ok(p) => p,
            Err(_) if self.form_port.value().is_empty() => 0,
            Err(_) => {
                self.status = Some("Port must be a number (1-65535)".into());
                return;
            }
        };
        let password = self.form_password.value().to_string();
        let host = Host {
            hostname: hostname.clone(),
            port,
            password,
        };

        match self.edit_index {
            Some(i) => {
                self.hosts[i] = host;
                self.status = Some(format!("Updated {hostname}"));
            }
            None => {
                self.hosts.push(host);
                self.list.select_last();
                self.status = Some(format!("Added {hostname}"));
            }
        }
        self.edit_index = None;
        self.mode = Mode::List;
        self.save();
    }

    // ---- persistence ------------------------------------------------------

    fn save(&mut self) {
        if let Err(e) = self.file.save(&self.hosts) {
            self.status = Some(format!("save failed: {e:#}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::HostFile;

    fn temp_file() -> HostFile {
        let path = std::env::temp_dir().join("ess-app-test-hosts.json");
        let _ = std::fs::remove_file(&path); // fresh state per test run
        HostFile::new(path)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn add_host_roundtrip() {
        let mut app = App::new(temp_file());
        app.on_key(key(KeyCode::Char('a')));
        assert_eq!(app.mode, Mode::AddHost);
        for c in "web1".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Tab)); // -> port
        app.on_key(key(KeyCode::Tab)); // -> password
        for c in "s3cret!".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.hosts.len(), 1);
        assert_eq!(app.hosts[0].hostname, "web1");
        assert_eq!(app.hosts[0].password, "s3cret!");
    }

    #[test]
    fn enter_requests_connect_for_selected() {
        let mut app = App::new(temp_file());
        app.hosts.push(Host {
            hostname: "db".into(),
            port: 5432,
            password: String::new(),
        });
        app.list.select_first();
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.take_connect_request(), Some(0));
        assert_eq!(app.take_connect_request(), None);
    }

    #[test]
    fn edit_updates_in_place() {
        let mut app = App::new(temp_file());
        app.hosts.push(Host {
            hostname: "old".into(),
            port: 0,
            password: String::new(),
        });
        app.list.select_first();
        app.on_key(key(KeyCode::Char('e')));
        assert_eq!(app.edit_index, Some(0));
        // cursor is at end; type "er" to make "older"
        for c in "er".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Tab)); // skip port
        app.on_key(key(KeyCode::Tab)); // skip password
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.hosts.len(), 1, "edit must not add a host");
        assert_eq!(app.hosts[0].hostname, "older");
    }

    #[test]
    fn empty_hostname_rejected() {
        let mut app = App::new(temp_file());
        app.on_key(key(KeyCode::Char('a')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::AddHost, "form stays open");
        assert!(app.hosts.is_empty());
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::List);
    }

    #[test]
    fn numeric_port_field_rejects_letters() {
        let mut app = App::new(temp_file());
        app.on_key(key(KeyCode::Char('a')));
        app.on_key(key(KeyCode::Char('h'))); // hostname
        app.on_key(key(KeyCode::Tab)); // -> port
        app.on_key(key(KeyCode::Char('x'))); // rejected: not a digit
        app.on_key(key(KeyCode::Char('2')));
        app.on_key(key(KeyCode::Char('2')));
        app.on_key(key(KeyCode::Tab)); // -> password
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.hosts[0].hostname, "h");
        assert_eq!(app.hosts[0].port, 22);
    }
}
