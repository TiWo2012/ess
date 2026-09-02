#!/usr/bin/env bash
# Live-keyring end-to-end test: drives ess against a real gnome-keyring
# Secret Service inside an isolated D-Bus session.
#
# Needs: gnome-keyring installed; sudo to create a temporary ssh user.
# Creates ess_test, runs live_keyring_test.py, removes ess_test again.
set -euo pipefail
cd "$(dirname "$0")"

if ! id ess_test >/dev/null 2>&1; then
    sudo useradd -m -s /bin/bash ess_test
    echo 'ess_test:ess_test_pw' | sudo chpasswd
    created_user=1
fi
trap 'if [ "${created_user:-0}" = 1 ]; then sudo pkill -u ess_test >/dev/null 2>&1 || true; sleep 0.5; sudo userdel -r ess_test >/dev/null 2>&1 || true; fi' EXIT

rm -rf ~/.local/share/keyrings
GKDIR=$(mktemp -d)
dbus-run-session -- bash -c "
    mkdir -p '$GKDIR/keyrings' '$GKDIR/ess'
    export XDG_DATA_HOME='$GKDIR'
    echo -n testpw | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1
    sleep 2
    python3 live_keyring_test.py
"
rm -rf "$GKDIR"