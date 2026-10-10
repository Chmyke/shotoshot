#!/bin/sh
# Removes what install.sh added; captures and editable images you saved are kept.
set -eu
data="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$HOME/.local/bin/shotoshot" "$data/applications/shotoshot.desktop" \
    "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/shotoshot.desktop"
for size in 32 48 256; do rm -f "$data/icons/hicolor/${size}x${size}/apps/shotoshot.png"; done
# The .shoto file type and its entry, then the caches that still list them.
rm -f "$data/applications/shotoshot-open.desktop" "$data/mime/packages/shotoshot.xml"
if command -v update-mime-database >/dev/null 2>&1 && [ -d "$data/mime" ]; then
    update-mime-database "$data/mime" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1 && [ -d "$data/applications" ]; then
    update-desktop-database "$data/applications" >/dev/null 2>&1 || true
fi
# Leftovers of the former name (before 0.2.0).
rm -f "$HOME/.local/bin/shotshot" "$data/applications/shotshot.desktop" \
    "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/shotshot.desktop"
for size in 32 48 256; do rm -f "$data/icons/hicolor/${size}x${size}/apps/shotshot.png"; done
echo "Shotoshot removed."
