#!/usr/bin/env bash
# Re-render static/og.png, favicon-32.png and apple-touch-icon.png from the SVG sources with headless Chrome.
set -euo pipefail
cd "$(dirname "$0")"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
shot() { "$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --default-background-color=00000000 --window-size="$2,$3" --screenshot="$4" "file://$PWD/$1" >/dev/null 2>&1; }
shot og.svg 1200 630 ../../static/og.png
printf '<body style="margin:0"><img src="../../static/favicon.svg" width="32" height="32">' > icon.html
shot icon.html 32 32 ../../static/favicon-32.png
printf '<body style="margin:0;background:#1a2421"><img src="../../static/favicon.svg" width="180" height="180">' > icon.html
shot icon.html 180 180 ../../static/apple-touch-icon.png
rm icon.html
