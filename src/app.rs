//! Application state and keyboard handling.

use crate::edit::EditValue;
use crate::secrets::{self, Secrets};
use crate::storage::HostFile;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// `user@host` or a plain hostname — anything `ssh` accepts as a target.
    pub hostname: String,
    /// 0 = use ssh's default port (22).
    pub port: u16,
    /// Loaded from the OS keyring; only stored in plaintext JSON while the
    /// keyring is unavailable (see `secrets.rs`). Never serialized.
    pub password: String,
}

impl Host {
    /// Human-friendly label like `user@host:2222`; also the keyring key.
    pub fn label(&self) -> String {
        secrets::host_key(&self.hostname, self.port)
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
    secrets: Secrets,
}

impl App {
    pub fn new(file: HostFile, secrets: Secrets) -> Self {
        let stored = match file.load() {
            Ok(hosts) => hosts,
            Err(e) => {
                eprintln!("warning: could not load hosts file: {e:#}");
                Vec::new()
            }
        };

        let mut hosts = Vec::with_capacity(stored.len());
        let mut migrate: Vec<(String, String)> = Vec::new();
        for s in &stored {
            let key = secrets::host_key(&s.hostname, s.port);
            let legacy = s.password.clone().unwrap_or_default();
            let password = match secrets.get(&key) {
                Ok(Some(pw)) => pw,
                // Entry not in keyring yet: if the JSON has a legacy plaintext
                // password, queue it for migration into the keyring.
                Ok(None) if !legacy.is_empty() => {
                    migrate.push((key, legacy.clone()));
                    legacy
                }
                Ok(None) => String::new(),
                // Keyring unreachable → keep the plaintext fallback for now.
                Err(()) => legacy,
            };
            hosts.push(Host {
                hostname: s.hostname.clone(),
                port: s.port,
                password,
            });
        }

        // One-time migration: legacy plaintext passwords → OS keyring, then
        // rewrite the file so passwords no longer sit on disk.
        if secrets.available() && !migrate.is_empty() {
            let all_ok = migrate.iter().all(|(key, pw)| secrets.set(key, pw).is_ok());
            if all_ok {
                if let Err(e) = file.save(&hosts, false) {
                    eprintln!("warning: could not rewrite hosts.json: {e:#}");
                }
            }
        }

        let status = if secrets.available() {
            None
        } else {
            Some("keyring unavailable — passwords stored in plaintext (hosts.json)".into())
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
            status,
            should_quit: false,
            pending_connect: None,
            file,
            secrets,
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
        // Remove the password from the OS keyring too.
        if self.secrets.available() {
            let _ = self.secrets.delete(&removed.label());
        }
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
            password: password.clone(),
        };
        let key = secrets::host_key(&host.hostname, host.port);

        let warn = match self.edit_index {
            Some(i) => {
                let old_key = self.hosts[i].label();
                self.hosts[i] = host;
                self.sync_secret(Some(&old_key), &key, &password)
            }
            None => {
                self.hosts.push(host);
                self.list.select_last();
                self.sync_secret(None, &key, &password)
            }
        };
        let was_edit = self.edit_index.is_some();
        self.edit_index = None;
        self.mode = Mode::List;
        self.status = Some(match warn {
            Some(w) => w,
            None => format!("{} {hostname}", if was_edit { "Updated" } else { "Added" }),
        });
        self.save();
    }

    /// Push a host's password to the OS keyring (handling renames), falling
    /// back to plaintext JSON storage when the keyring is unavailable.
    /// Returns a warning to show as the status when fallback engaged.
    fn sync_secret(&mut self, old_key: Option<&str>, key: &str, password: &str) -> Option<String> {
        if !self.secrets.available() {
            return None;
        }
        if let Some(old) = old_key {
            if old != key && self.secrets.delete(old).is_err() {
                self.secrets.mark_unavailable();
                return Some("keyring write failed — password stored in plaintext".into());
            }
        }
        if self.secrets.set(key, password).is_err() {
            self.secrets.mark_unavailable();
            return Some("keyring write failed — password stored in plaintext".into());
        }
        None
    }

    // ---- persistence ------------------------------------------------------

    fn save(&mut self) {
        // Passwords go to disk only while the keyring is unavailable (fallback).
        let include_passwords = !self.secrets.available();
        if let Err(e) = self.file.save(&self.hosts, include_passwords) {
            self.status = Some(format!("save failed: {e:#}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::HostFile;

    fn temp_file() -> HostFile {
        // Unique path per test — tests run in parallel and share the temp dir.
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        HostFile::new(
            std::env::temp_dir().join(format!("ess-app-test-{}-{n}.json", std::process::id())),
        )
    }

    fn app_with_fake_keyring() -> App {
        App::new(temp_file(), Secrets::fake())
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn add_host_roundtrip() {
        let mut app = app_with_fake_keyring();
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
        let mut app = app_with_fake_keyring();
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
        let mut app = app_with_fake_keyring();
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
        let mut app = app_with_fake_keyring();
        app.on_key(key(KeyCode::Char('a')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::AddHost, "form stays open");
        assert!(app.hosts.is_empty());
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::List);
    }

    #[test]
    fn numeric_port_field_rejects_letters() {
        let mut app = app_with_fake_keyring();
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

    #[test]
    fn passwords_go_to_keyring_not_json() {
        let secrets = Secrets::fake();
        let store = secrets.fake_store();
        let file = temp_file();
        let path = file.path().to_path_buf();
        let mut app = App::new(file, secrets);
        app.on_key(key(KeyCode::Char('a')));
        for c in "web1".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Tab));
        for c in "s3cret!".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        // stored in the keyring, not in the JSON file
        assert_eq!(store.get("web1").as_deref(), Some("s3cret!"));
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(
            !json.contains("s3cret!"),
            "password must not be written to disk"
        );
    }

    #[test]
    fn legacy_passwords_migrate_to_keyring() {
        // Simulate a pre-keyring hosts.json with plaintext passwords.
        let file = temp_file();
        std::fs::write(
            file.path(),
            r#"[{"hostname":"oldbox","port":0,"password":"hunter2"}]"#,
        )
        .unwrap();
        let secrets = Secrets::fake();
        let store = secrets.fake_store();
        let path = file.path().to_path_buf();
        let app = App::new(file, secrets);
        // password readable at runtime…
        assert_eq!(app.hosts[0].password, "hunter2");
        // …but only in the keyring, and the file was rewritten without it
        assert_eq!(store.get("oldbox").as_deref(), Some("hunter2"));
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(
            !json.contains("hunter2"),
            "migrated file must not keep the plaintext"
        );
    }

    #[test]
    fn keyring_down_falls_back_to_plaintext() {
        let secrets = Secrets::fake();
        secrets.make_unavailable();
        let file = temp_file();
        let path = file.path().to_path_buf();
        let mut app = App::new(file, secrets);
        app.on_key(key(KeyCode::Char('a')));
        for c in "web1".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Tab));
        for c in "s3cret!".chars() {
            app.on_key(key(KeyCode::Char(c)));
        }
        app.on_key(key(KeyCode::Enter));
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(
            json.contains("s3cret!"),
            "fallback must keep the password in JSON so it survives restarts"
        );
    }

    #[test]
    fn delete_removes_keyring_entry() {
        let mut app = App::new(temp_file(), Secrets::fake());
        app.hosts.push(Host {
            hostname: "gone".into(),
            port: 0,
            password: "pw".into(),
        });
        app.secrets.set("gone", "pw").unwrap();
        app.list.select_first();
        app.on_key(key(KeyCode::Char('d')));
        assert!(app.hosts.is_empty());
        assert_eq!(app.secrets.get("gone").unwrap(), None);
    }
}
