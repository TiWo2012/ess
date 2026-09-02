//! Persistence: hosts are stored as a JSON file on disk.
//!
//! Passwords normally live in the OS keyring (see `secrets.rs`); the JSON file
//! only carries a `password` field while the keyring is unavailable or for
//! legacy files that predate keyring support. On load, legacy passwords are
//! migrated into the keyring and the file is rewritten without them.

use std::{env, fs, io, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::app::Host;

/// A host as stored on disk. `password` is `Some` only for legacy files or
/// while the OS keyring is unavailable (plaintext fallback).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredHost {
    /// Login username; empty = use ssh's default (local user).
    #[serde(default)]
    pub username: String,
    pub hostname: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

pub struct HostFile {
    path: PathBuf,
}

impl HostFile {
    /// The resolved file path (exposed for tests/tools).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    /// Resolve the data file path: `$XDG_DATA_HOME|$HOME/.local/share|$APPDATA`
    /// + `/ess/hosts.json`.
    pub fn default_path() -> Result<PathBuf> {
        let base = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
            .ok_or_else(|| anyhow!("no HOME/XDG_DATA_HOME/APPDATA found"))?;
        Ok(base.join("ess").join("hosts.json"))
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Vec<StoredHost>> {
        match fs::read_to_string(&self.path) {
            Ok(json) => serde_json::from_str(&json).context("parsing hosts.json"),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).context("reading hosts.json"),
        }
    }

    /// Write hosts. `include_passwords` is true only while the keyring is
    /// unavailable (plaintext fallback); otherwise passwords never touch disk.
    pub fn save(&self, hosts: &[Host], include_passwords: bool) -> Result<()> {
        let stored: Vec<StoredHost> = hosts
            .iter()
            .map(|h| StoredHost {
                username: h.username.clone(),
                hostname: h.hostname.clone(),
                port: h.port,
                password: if include_passwords && !h.password.is_empty() {
                    Some(h.password.clone())
                } else {
                    None
                },
            })
            .collect();
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).context("creating data directory")?;
        }
        let json = serde_json::to_string_pretty(&stored)?;
        fs::write(&self.path, json).context("writing hosts.json")
    }
}
