//! Persistence: hosts are stored as a JSON file on disk.
//!
//! SECURITY NOTE: passwords are written in plaintext JSON. That is acceptable
//! for this early "add hosts and passwords" stage, but should be replaced with
//! the OS keyring (`keyring` crate) before real use. The file lives outside
//! this git repo, in the user data dir.

use std::{env, fs, io, path::PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::app::Host;

pub struct HostFile {
    path: PathBuf,
}

impl HostFile {
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

    pub fn load(&self) -> Result<Vec<Host>> {
        match fs::read_to_string(&self.path) {
            Ok(json) => serde_json::from_str(&json).context("parsing hosts.json"),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).context("reading hosts.json"),
        }
    }

    pub fn save(&self, hosts: &[Host]) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).context("creating data directory")?;
        }
        let json = serde_json::to_string_pretty(hosts)?;
        fs::write(&self.path, json).context("writing hosts.json")
    }
}