//! Where host passwords live: the OS keyring, with a plaintext fallback.
//!
//! Primary storage is the OS credential store via the `keyring` crate
//! (Secret Service on Linux, Keychain on macOS, Credential Manager on
//! Windows). When no keyring daemon is available — headless machines, CI,
//! containers — the app degrades to the old behavior of storing passwords
//! in the JSON file and shows a warning in the footer.
//!
//! The keyring "username" is the host label (`hostname` or `hostname:port`),
//! under the service name `ess`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "ess";

/// Key used for a host in the keyring (and for display).
pub fn host_key(hostname: &str, port: u16) -> String {
    if port != 0 {
        format!("{hostname}:{port}")
    } else {
        hostname.to_string()
    }
}

pub struct Secrets {
    store: Store,
    /// `true` while the OS keyring is in use. Flips to `false` (permanently,
    /// for the session) when a keyring operation fails, switching saves to
    /// the plaintext fallback.
    available: Cell<bool>,
}

enum Store {
    System,
    #[cfg_attr(not(test), allow(dead_code))]
    Fake(Rc<FakeStore>),
}

/// In-memory store for unit tests.
#[derive(Default)]
pub struct FakeStore {
    map: std::cell::RefCell<HashMap<String, String>>,
}

impl FakeStore {
    pub(crate) fn get(&self, key: &str) -> Option<String> {
        self.map.borrow().get(key).cloned()
    }
    pub(crate) fn set(&self, key: &str, password: &str) {
        self.map
            .borrow_mut()
            .insert(key.to_string(), password.to_string());
    }
    pub(crate) fn delete(&self, key: &str) {
        self.map.borrow_mut().remove(key);
    }
}

impl Secrets {
    /// Use the OS keyring. Availability is probed once (lazily) by `keyring`.
    pub fn system() -> Self {
        let available = Entry::store_status().is_ok();
        Self {
            store: Store::System,
            available: Cell::new(available),
        }
    }

    /// In-memory store, always "available" — for tests.
    #[cfg(test)]
    pub fn fake() -> Self {
        Self {
            store: Store::Fake(Rc::new(FakeStore::default())),
            available: Cell::new(true),
        }
    }

    #[cfg(test)]
    pub fn fake_store(&self) -> Rc<FakeStore> {
        match &self.store {
            Store::Fake(f) => f.clone(),
            Store::System => panic!("not a fake store"),
        }
    }

    /// Simulate a dead keyring for fallback tests.
    #[cfg(test)]
    pub fn make_unavailable(&self) {
        self.available.set(false);
    }

    /// `true` while passwords go to the keyring; `false` means the plaintext
    /// JSON fallback is carrying them.
    pub fn available(&self) -> bool {
        self.available.get()
    }

    /// Fetch a password. `Ok(None)` = no stored entry; `Err(())` = the
    /// keyring itself is unreachable.
    pub fn get(&self, key: &str) -> Result<Option<String>, ()> {
        let out = match &self.store {
            Store::System => match Entry::new(SERVICE, key).and_then(|e| e.get_password()) {
                Ok(pw) => Ok(Some(pw)),
                Err(KeyringError::NoEntry) => Ok(None),
                Err(_) => Err(()),
            },
            Store::Fake(f) => Ok(f.get(key)),
        };
        if out.is_err() {
            self.mark_unavailable();
        }
        out
    }

    /// Store a password. An empty password deletes any existing entry.
    pub fn set(&self, key: &str, password: &str) -> Result<(), ()> {
        let out = if password.is_empty() {
            self.delete(key)
        } else {
            match &self.store {
                Store::System => Entry::new(SERVICE, key)
                    .and_then(|e| e.set_password(password))
                    .map_err(|_| ()),
                Store::Fake(f) => {
                    f.set(key, password);
                    Ok(())
                }
            }
        };
        if out.is_err() {
            self.mark_unavailable();
        }
        out
    }

    /// Remove a password. `Ok(())` even when nothing was stored.
    pub fn delete(&self, key: &str) -> Result<(), ()> {
        let out = match &self.store {
            Store::System => match Entry::new(SERVICE, key).and_then(|e| e.delete_credential()) {
                Ok(()) => Ok(()),
                Err(KeyringError::NoEntry) => Ok(()),
                Err(_) => Err(()),
            },
            Store::Fake(f) => {
                f.delete(key);
                Ok(())
            }
        };
        if out.is_err() {
            self.mark_unavailable();
        }
        out
    }

    /// Switch to the plaintext fallback after a keyring failure.
    pub(crate) fn mark_unavailable(&self) {
        self.available.set(false);
    }
}
