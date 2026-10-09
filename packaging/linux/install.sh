#!/bin/sh
# Installs Shotoshot for the current user only: no root, nothing outside $HOME.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
mkdir -p "$bin" "$apps" "$autostart"
# Versions before 0.2.0 were named "shotshot": stop and remove that install.
former=false
if [ -e "$bin/shotshot" ]; then former=true; fi
if command -v pkill >/dev/null 2>&1; then pkill -x -u "$(id -u)" shotshot 2>/dev/null || true; fi
rm -f "$bin/shotshot" "$apps/shotshot.desktop" "$autostart/shotshot.desktop"
for size in 32 48 256; do rm -f "$icons/${size}x${size}/apps/shotshot.png"; done
install -m 755 "$here/shotoshot" "$bin/shotoshot"
for size in 32 48 256; do
    mkdir -p "$icons/${size}x${size}/apps"
    install -m 644 "$here/icons/shotoshot-$size.png" "$icons/${size}x${size}/apps/shotoshot.png"
done
cat > "$apps/shotoshot.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Shotoshot
Comment=Capture, annotate and share a screen area
Exec=$bin/shotoshot --capture
Icon=shotoshot
Categories=Graphics;Utility;
EOF
# The resident shows the notification-area icon at login, as on Windows.
cat > "$autostart/shotoshot.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Shotoshot
Exec=$bin/shotoshot
Icon=shotoshot
NoDisplay=true
EOF
echo "Shotoshot installed in $bin."
echo "Bind a keyboard shortcut to: $bin/shotoshot --capture"
echo "(instant capture to the Desktop: $bin/shotoshot --instant)"
if $former; then
    echo "Replaced the former Shotshot install: rebind any shortcut that ran $bin/shotshot."
fi
