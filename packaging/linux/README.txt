Shotoshot for Linux

Install:   sh install.sh      (current user only, no root)
Remove:    sh uninstall.sh

Capture:   bind a keyboard shortcut in your desktop settings to
           ~/.local/bin/shotoshot --capture
           Wayland does not let applications own global shortcuts.
Instant:   ~/.local/bin/shotoshot --instant saves the screen to your Desktop.
Save:      Ctrl+S saves a PNG to your Pictures folder.
Editable:  Ctrl+Shift+S saves an editable image (.shoto) there instead. Open it
           later and the capture comes back with every annotation still an
           object; Ctrl+S then rewrites the file, Ctrl+Shift+S saves a PNG.
           It keeps the whole screen as it was, not only the selection: share
           the PNG, keep the .shoto.
Tray:      the Shotoshot icon starts at login; click it to capture.
           GNOME needs the AppIndicator extension to show it.
Magnifier: under labwc (Raspberry Pi OS), the menu's Magnifier switches on
           labwc's own magnifier around the pointer, 240 px at 2x (the sizes
           of the Windows settings). It is square: labwc draws no round one.
           Shotoshot adds a marked block to ~/.config/labwc/rc.xml and
           themerc-override while it is on and takes it out when you switch
           it off, quit or uninstall. Ctrl+Alt+M switches labwc's magnifier
           too, without Shotoshot knowing: use one or the other.

Requires a desktop with the XDG screenshot portal (GNOME, KDE, wlroots
compositors such as labwc or Sway, most X11 desktops).
Not yet on Linux: presentation mode, the Board (desktop notes), crop-size
fields, Google Lens, live capture.

License and source code: see ABOUT.txt.
