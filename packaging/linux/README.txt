Shotoshot for Linux (preview)

Install:   sh install.sh      (current user only, no root)
Remove:    sh uninstall.sh

Capture:   bind a keyboard shortcut in your desktop settings to
           ~/.local/bin/shotoshot --capture
           Wayland does not let applications own global shortcuts.
Instant:   ~/.local/bin/shotoshot --instant saves the screen to your Desktop.
Tray:      the Shotoshot icon starts at login; click it to capture.
           GNOME needs the AppIndicator extension to show it.

Requires a desktop with the XDG screenshot portal (GNOME, KDE, wlroots
compositors such as labwc or Sway, most X11 desktops).
Not yet on Linux: presentation mode, crop-size fields, Google Lens, live capture.

License and source code: see ABOUT.txt.
