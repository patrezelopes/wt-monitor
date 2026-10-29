#!/usr/bin/env bash
# Extracts the <script> from index.html and syntax-checks it with node.
set -euo pipefail
cd "$(dirname "$0")/.."
out="$(mktemp --suffix=.js)"
trap 'rm -f "$out"' EXIT
python3 -c "import re; print(re.search(r'<script>(.*)</script>', open('index.html').read(), re.S).group(1))" > "$out"
node --check "$out"
echo "js ok"
