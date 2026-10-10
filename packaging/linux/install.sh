#!/bin/sh
# Installs Shotoshot for the current user only: no root, nothing outside $HOME.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
mime="${XDG_DATA_HOME:-$HOME/.local/share}/mime"
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
mkdir -p "$bin" "$apps" "$autostart" "$mime/packages"
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
# Editable images (.shoto): the file type, a PNG to every other program, and the hidden
# entry that opens it in Shotoshot on a double-click.
cat > "$mime/packages/shotoshot.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="image/x-shotoshot">
    <comment>Shotoshot editable image</comment>
    <sub-class-of type="image/png"/>
    <icon name="shotoshot"/>
    <glob pattern="*.shoto"/>
  </mime-type>
</mime-info>
EOF
cat > "$apps/shotoshot-open.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Shotoshot
Comment=Edit a Shotoshot editable image
Exec=$bin/shotoshot %f
Icon=shotoshot
MimeType=image/x-shotoshot;
NoDisplay=true
EOF
# The desktop reads both through caches; one without these tools has no such cache.
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$mime" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$apps" >/dev/null 2>&1 || true
fi
echo "Shotoshot installed in $bin."
echo "Bind a keyboard shortcut to: $bin/shotoshot --capture"
echo "(instant capture to the Desktop: $bin/shotoshot --instant)"
if $former; then
    echo "Replaced the former Shotshot install: rebind any shortcut that ran $bin/shotshot."
fi
