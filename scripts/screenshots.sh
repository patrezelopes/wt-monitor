#!/usr/bin/env bash
# Regenerates docs/img/*.png from the client-side demo data (never from real projects).
set -euo pipefail
cd "$(dirname "$0")/.."
port="${SHOT_PORT:-8799}"
chrome="$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)"
[ -n "$chrome" ] || { echo "Chrome/Chromium not found" >&2; exit 1; }

target/release/wt-monitor "$HOME" "$port" >/dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null' EXIT
sleep 1.5

shot() { # name, query
  "$chrome" --headless=new --no-sandbox --disable-gpu --hide-scrollbars --window-size=1440,1000 \
    --virtual-time-budget=9000 --screenshot="$PWD/docs/img/$1.png" "http://127.0.0.1:$port/?demo&$2" >/dev/null 2>&1
  echo "docs/img/$1.png"
}
mkdir -p docs/img
shot overview       "mode=sober"
shot overview-light "mode=sober&theme=light"
shot arena          "mode=fun"
shot session        "mode=sober&open=acme/api&tab=agents"
shot terminals      "mode=term&layout=4"
shot priority       "mode=sober&open=lab/ml-experiments&tab=prio"
shot activity       "mode=sober&open=acme/api&tab=activity"
