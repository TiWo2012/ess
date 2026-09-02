#!/usr/bin/env python3
"""Live keyring integration test. Must run inside a `dbus-run-session` that
has an unlocked gnome-keyring Secret Service (see run_live_keyring_test.sh).

Verifies:
 1. legacy plaintext password in hosts.json is migrated into the keyring and
    the file is rewritten without it,
 2. an ssh session can use the keyring-stored password (no prompt),
 3. restarting the app with no password in hosts.json still connects,
 4. the keyring holds the password under service "ess" / user <host label>.
"""
import fcntl, json, os, pty, re, select, struct, subprocess, sys, termios, time

# The app resolves hosts.json via XDG_DATA_HOME (or ~/.local/share); make sure
# driver and app agree even when the test overrides XDG_DATA_HOME.
_data_dir = os.environ.get("XDG_DATA_HOME") or os.path.join(
    os.path.expanduser("~"), ".local", "share")
HOSTS = os.path.join(_data_dir, "ess", "hosts.json")
BIN = os.path.join(os.path.dirname(os.path.abspath(__file__)), "target", "debug", "ess")
PROBE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                     "target", "debug", "examples", "keyring_probe")
PROMPT = r"❯|\$|# "

failures = []


def note(ok, desc):
    print(("PASS" if ok else "FAIL") + " | " + desc)
    if not ok:
        failures.append(desc)


def drive(hosts_entry, whoami_user, label):
    """Run the app, connect to the first host, run whoami, exit, quit."""
    with open(HOSTS, "w") as f:
        json.dump([hosts_entry], f)
    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    p = subprocess.Popen([BIN], stdin=slave, stdout=slave, stderr=slave,
                         env=dict(os.environ, TERM="xterm-256color"))
    os.close(slave)
    buf = b""

    def waitfor(pat, t=12):
        nonlocal buf
        pat = re.compile(pat.encode())
        end = time.time() + t
        while time.time() < end:
            r, _, _ = select.select([master], [], [], 0.03)
            if r:
                try:
                    buf += os.read(master, 65536)
                except OSError:
                    break
            if pat.search(buf):
                return True
        return False

    ok = waitfor("SSH Manager")
    note(ok, f"{label}: TUI up")
    os.write(master, b"\r")
    ok = waitfor(PROMPT)
    note(ok, f"{label}: logged in (shell prompt)")
    ok = "password:" not in buf.decode(errors="replace")
    note(ok, f"{label}: no interactive password prompt")
    os.write(master, f"whoami\r".encode())
    ok = waitfor(whoami_user)
    note(ok, f"{label}: whoami = {whoami_user}")
    os.write(master, b"exit\r")
    ok = waitfor(f"Session ended \\({label}\\)")
    note(ok, f"{label}: session-ended status")
    os.write(master, b"q")
    try:
        p.wait(timeout=3)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
    note(p.returncode == 0, f"{label}: clean exit ({p.returncode})")
    os.close(master)


# ---- 1: migration ----------------------------------------------------------
legacy = {"hostname": "ess_test@localhost", "port": 0, "password": "ess_test_pw"}
drive(legacy, "ess_test", "ess_test@localhost")

with open(HOSTS) as f:
    disk = f.read()
note("ess_test_pw" not in disk, "migration: plaintext removed from hosts.json")
note('"password"' not in disk, "migration: password field gone from hosts.json")

# ---- 4: probe the keyring directly -----------------------------------------
r = subprocess.run([PROBE, "ess", "ess_test@localhost"], capture_output=True, text=True)
note(r.stdout.strip() == "ess_test_pw",
     f"keyring holds password under ess/ess_test@localhost (got: {r.stdout.strip()!r}, err: {r.stderr.strip()!r})")

# ---- 3: restart with a password-less hosts.json ----------------------------
bare = {"hostname": "ess_test@localhost", "port": 0}
drive(bare, "ess_test", "ess_test@localhost")

with open(HOSTS) as f:
    disk = f.read()
note("ess_test_pw" not in disk, "restart: still no plaintext on disk")

print("\n=== SUMMARY ===")
print("ALL PASS" if not failures else f"{len(failures)} FAILURES: {failures}")
sys.exit(0 if not failures else 1)