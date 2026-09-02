#!/usr/bin/env python3
"""Integration test: drives the ess TUI through a real pty with
synchronized waits, exercising: refused connection, key-auth session,
and password-auth via the askpass mechanism.

Scenario 3 needs a throwaway local user for real password auth:
    sudo useradd -m ess_test && echo 'ess_test:ess_test_pw' | sudo chpasswd
"""
import fcntl, json, os, pty, re, select, struct, subprocess, sys, termios, time

HOME = os.path.expanduser("~")
HOSTS = os.path.join(HOME, ".local/share/ess/hosts.json")
BIN = os.path.join(os.path.dirname(os.path.abspath(__file__)), "target", "debug", "ess")

PROMPT = r"❯|\$|# "


def set_winsz(fd, rows=24, cols=80):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def note(ok, desc):
    print(("PASS" if ok else "FAIL") + " | " + desc)
    return bool(ok)


class Session:
    def __init__(self, hosts):
        os.makedirs(os.path.dirname(HOSTS), exist_ok=True)
        with open(HOSTS, "w") as f:
            json.dump(hosts, f)
        self.master, slave = pty.openpty()
        set_winsz(self.master)
        env = dict(os.environ, TERM="xterm-256color")
        self.proc = subprocess.Popen(
            [BIN], stdin=slave, stdout=slave, stderr=slave, env=env, close_fds=True)
        os.close(slave)
        self.buf = b""

    def read_until(self, pattern, timeout=10, interval=0.03):
        pat = re.compile(pattern if isinstance(pattern, bytes) else pattern.encode())
        deadline = time.time() + timeout
        while time.time() < deadline:
            r, _, _ = select.select([self.master], [], [], interval)
            if r:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.buf += data
            if pat.search(self.buf):
                return True
        return False

    def text(self):
        return self.buf.decode("utf-8", errors="replace")

    def send(self, s):
        os.write(self.master, s.encode() if isinstance(s, str) else s)

    def finish(self, label):
        self.send("q")
        try:
            self.proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        ok = self.proc.returncode == 0
        print(("PASS" if ok else "FAIL") + f" | {label} clean exit ({self.proc.returncode})")
        os.close(self.master)
        return ok


results = []

# --- scenario 1: connection refused (deterministic failure path) ------------
s = Session([{"hostname": "localhost", "port": 1, "password": ""}])
ok = note(s.read_until("SSH Manager", 5), "tui up")
s.send("\r")
ok &= note(s.read_until(r"[Rr]efused", 8), "ssh prints refused")
ok &= note(s.read_until(r"ssh exited \(localhost:1\) with code", 8),
           "returns to TUI with status")
ok &= s.finish("refused-connection")
results.append(("refused-connection", ok))

# --- scenario 2: key-auth full session ------------------------------------
s = Session([{"hostname": "localhost", "port": 0, "password": ""}])
ok = note(s.read_until("SSH Manager", 5), "tui up")
s.send("\r")
ok &= note(s.read_until(PROMPT, 10), "key-auth shell prompt")
s.send("echo HI_FROM_SSH\r")
ok &= note(s.read_until("HI_FROM_SSH", 5), "command executed in session")
s.send("exit\r")
ok &= note(s.read_until(r"Session ended \(localhost\)", 8), "session-ended status")
ok &= s.finish("key-auth-session")
results.append(("key-auth-session", ok))

# --- scenario 3: password auth via askpass --------------------------------
if subprocess.run(["id", "ess_test"], capture_output=True).returncode != 0:
    print("SKIP password-auth-askpass (create user `ess_test` first)")
else:
    s = Session([{"hostname": "ess_test@localhost", "port": 0,
                  "password": "ess_test_pw"}])
    ok = note(s.read_until("SSH Manager", 5), "tui up")
    s.send("\r")
    ok &= note(s.read_until(PROMPT, 12), "password-auth shell prompt")
    ok &= note("password:" not in s.text(),
               "no interactive password prompt (askpass did the work)")
    s.send("whoami\r")
    ok &= note(s.read_until("ess_test", 5), "logged in as ess_test")
    s.send("exit\r")
    ok &= note(s.read_until(r"Session ended \(ess_test@localhost\)", 8),
               "session-ended status")
    ok &= s.finish("password-auth-askpass")
    results.append(("password-auth-askpass", ok))

print("\n=== SUMMARY ===")
for name, ok in results:
    print(("PASS" if ok else "FAIL"), name)
sys.exit(0 if all(ok for _, ok in results) else 1)