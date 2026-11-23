#!/bin/sh
# Collects hardware and environment facts without sending MIDI or printing.
set -u
cd "$(dirname "$0")/.."

echo "== version =="
./scripts/rust.sh run -q -p lazy-arranger-cli -- --help >/dev/null 2>&1 && echo "cli ok"
echo
echo "== MIDI ports =="
./scripts/rust.sh run -q -p lazy-arranger-cli -- ports 2>&1 | head -40
echo
echo "== installed plugins =="
./scripts/rust.sh run -q -p lazy-arranger-cli -- plugin-scan 2>&1 | head -30
echo
echo "== printers =="
lpstat -p 2>&1 | head -20 || true
echo
echo "== audio devices =="
echo "Open Settings > Find audio inputs / Find audio outputs in the app."
