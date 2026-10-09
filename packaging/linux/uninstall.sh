#!/bin/sh
# Removes what install.sh added; captures you saved are kept.
set -eu
data="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$HOME/.local/bin/shotoshot" "$data/applications/shotoshot.desktop" \
    "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/shotoshot.desktop"
for size in 32 48 256; do rm -f "$data/icons/hicolor/${size}x${size}/apps/shotoshot.png"; done
# Leftovers of the former name (before 0.2.0).
rm -f "$HOME/.local/bin/shotshot" "$data/applications/shotshot.desktop" \
    "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/shotshot.desktop"
for size in 32 48 256; do rm -f "$data/icons/hicolor/${size}x${size}/apps/shotshot.png"; done
echo "Shotoshot removed."
