//! Running real ssh sessions from the TUI.
//!
//! Strategy: ratatui only *renders*; it can't host an interactive ssh session.
//! So before connecting we leave raw mode + the alternate screen, spawn the
//! system `ssh` client on the real terminal, wait for it to finish, then
//! re-enter the TUI. This gives us a full-featured ssh client (host keys,
//! agent forwarding, control characters, resize, ...) for free.
//!
//! Stored passwords are fed non-interactively through OpenSSH's askpass
//! mechanism (`SSH_ASKPASS_REQUIRE=force`, OpenSSH >= 8.4 which is ancient by
//! now). If ssh is older or the password is empty, ssh just falls back to its
//! normal interactive prompt — no harm done.

use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::{Child, Command, ExitStatus},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};

use crate::app::Host;

const ASKPASS_SHELL: &str = "/bin/sh";

/// Leave the TUI: exit raw mode and the alternate screen and clear the screen
/// so ssh output starts clean.
fn leave_terminal() -> Result<()> {
    use crossterm::{
        cursor::{MoveTo, Show},
        execute,
        terminal::{disable_raw_mode, Clear, ClearType, LeaveAlternateScreen},
    };

    disable_raw_mode().context("disable_raw_mode")?;
    let mut out = io::stdout();
    execute!(
        out,
        LeaveAlternateScreen,
        Clear(ClearType::All),
        MoveTo(0, 0),
        // The TUI ran with the cursor hidden (`Hide`); make sure the ssh
        // session gets a visible cursor.
        Show
    )
    .context("leaving alternate screen")?;
    out.flush().context("flush stdout")?;
    Ok(())
}

/// Re-enter the TUI after an ssh session.
fn enter_terminal() -> Result<()> {
    use crossterm::{
        execute,
        terminal::{enable_raw_mode, EnterAlternateScreen},
    };

    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen).context("entering alternate screen")?;
    out.flush().context("flush stdout")?;
    enable_raw_mode().context("enable_raw_mode")?;
    // Discard any input the ssh session left behind in the tty queue, so
    // stray keystrokes can't leak back into the TUI as phantom events.
    drain_input();
    Ok(())
}

/// Drain pending input until the tty queue has been quiet for a while (the
/// last of ssh's keystrokes can still be in flight for a few ms after it
/// exits). Cap the wait so we never delay the user noticeably.
fn drain_input() {
    use std::time::Instant;

    use crossterm::event::{poll, read};

    let deadline = Instant::now() + Duration::from_millis(500);
    let quiet_after = Duration::from_millis(25);
    let mut last_event = Instant::now();
    while Instant::now() < deadline {
        if poll(Duration::from_millis(5)).unwrap_or(false) {
            let _ = read();
            last_event = Instant::now();
        } else if last_event.elapsed() >= quiet_after {
            break;
        }
    }
}

/// Run `ssh` against `host`, blocking until the session ends, then restore the
/// TUI. Returns the exit status of ssh.
pub fn connect_session(host: &Host) -> Result<ExitStatus> {
    leave_terminal().context("leaving TUI")?;

    let ask = if host.password.is_empty() {
        None
    } else {
        Some(Askpass::new(&host.password).context("creating askpass helper")?)
    };
    let mut cmd = ssh_command(host, ask.as_ref());
    let spawned = cmd.spawn().context("spawning ssh");
    let mut child: Child = match spawned {
        Ok(c) => c,
        Err(e) => {
            drop(ask);
            enter_terminal().context("re-entering TUI")?;
            return Err(e);
        }
    };

    let exited = child.wait();
    drop(ask); // delete the temp password files before re-entering the TUI
    enter_terminal().context("re-entering TUI")?;
    exited.context("waiting for ssh")
}

/// Build the `ssh` command line.
fn ssh_command(host: &Host, ask: Option<&Askpass>) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-o",
        "StrictHostKeyChecking=accept-new", // remember new host keys silently
        "-o",
        "CheckHostIP=no",
        "-o",
        "PreferredAuthentications=publickey,keyboard-interactive,password",
    ]);
    if host.port != 0 && host.port != 22 {
        cmd.arg("-p").arg(host.port.to_string());
    }
    if let Some(ask) = ask {
        cmd.env("SSH_ASKPASS", ask.script_path())
            .env("SSH_ASKPASS_REQUIRE", "force");
    }
    cmd.arg(&host.hostname);
    cmd
}

/// Temp script + password file that the askpass mechanism uses to answer ssh's
/// password prompt. Cleans itself up via `Drop`.
struct Askpass {
    dir: PathBuf,
    script: PathBuf,
}

impl Askpass {
    fn new(password: &str) -> Result<Self> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("ess-askpass-{}-{}", std::process::id(), ts));
        fs::create_dir(&dir).context("creating askpass dir")?;

        let password_file = dir.join("password");
        let script = dir.join("askpass.sh");
        write_secret(&password_file, password)?;
        fs::write(
            &script,
            format!("#!{ASKPASS_SHELL}\ncat '{}'\n", password_file.display()),
        )
        .context("writing askpass script")?;
        set_executable(&script)?;

        Ok(Self { dir, script })
    }

    fn script_path(&self) -> &std::path::Path {
        &self.script
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Write a file with 0600 permissions (the password must not be world-readable).
#[cfg(unix)]
fn write_secret(path: &std::path::Path, contents: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("creating secret file")?;
    f.write_all(contents.as_bytes()).context("writing secret")?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret(path: &std::path::Path, contents: &str) -> Result<()> {
    fs::write(path, contents).context("writing secret file")
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)
        .context("stat askpass script")?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(path, perms).context("chmod askpass script")
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
