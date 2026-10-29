#!/usr/bin/env bash
# Builds wt-monitor, installs it to ~/.local/bin and enables the systemd user service.
# Does NOT touch Claude Code settings; for agent notifications run ./install-hooks.py.
set -euo pipefail
cd "$(dirname "$0")"
root="${1:-$HOME/dev}"; port="${2:-8765}"

cargo build --release
install -Dm755 target/release/wt-monitor "$HOME/.local/bin/wt-monitor.new"
mv "$HOME/.local/bin/wt-monitor.new" "$HOME/.local/bin/wt-monitor"

mkdir -p "$HOME/.config/systemd/user"
sed -e "s|%h/dev 8765|$root $port|" contrib/wt-monitor.service > "$HOME/.config/systemd/user/wt-monitor.service"
systemctl --user daemon-reload
systemctl --user enable wt-monitor.service
systemctl --user restart wt-monitor.service
echo "wt-monitor running at http://127.0.0.1:$port (watching $root)"
