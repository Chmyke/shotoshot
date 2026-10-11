# Shotoshot

Native screen capture and annotation for Windows 11. Press a key, drag a region, annotate on the
frozen screen with objects that stay editable, then copy or save. The same editor writes notes
on your desktop, under the icons: the [Board](#board-notes-under-the-icons). The whole interface
is in English; the on-screen keyboard always follows the active keyboard layout.

- **Light.** A small resident process starts one editor process per capture. That process ends
  with the session, which returns its memory to Windows, graphics caches included.
- **Private.** No telemetry and no network request of its own. Google Lens receives a capture only
  when you click its button.
- **Open.** GPL-3.0, written in Rust on Direct2D, with no web engine.

![Shotoshot at work: an area is selected and widened, an arrow points at a chart, a note is typed, enlarged and rotated, and a square is drawn then moved.](https://shotoshot.com/assets/capture.gif)

## Install

Download the installer from [shotoshot.com](https://shotoshot.com). It is not code-signed yet, so
Windows SmartScreen may ask for confirmation; the SHA-256 of each download is published on the
site.

A Linux preview (x86_64 and ARM64, glibc 2.31 or later, Wayland and X11) is published as a
`tar.gz` with its own `README.txt`. Presentation mode, the Board, crop-size fields, Google Lens
and live capture are not on Linux yet.

## First run and tray menu

On first launch the proposed shortcut is **Print Screen**: **Save** confirms it. Otherwise click
the displayed key and press the global shortcut you want (for example **Pause**, a single letter
or Ctrl+Shift+F8), then click **Save**. Opened from the menu, the window listens right away; the
shortcut is shown as keys, and a click on it starts listening again. While listening, Esc and
Enter are assignable keys too; **Cancel** cancels with the mouse. After entry, Tab moves between
the field and the buttons and Enter activates the selected control.

Combinations use Ctrl, Shift, Alt or Win with a key. Windows remains in charge of reserved or
already taken keys: a conflict is shown in the panel without losing the previous shortcut.
Shotoshot never modifies another app's settings. If Windows reserves Print Screen for the
Snipping Tool, Shotoshot turns off your account's "Use the Print screen key to open screen
capture" option (`HKCU\Control Panel\Keyboard`), only when you save Print Screen. It restores the
option when no shortcut uses the key any more, and on uninstall.

The icon lives in the notification area. Left click and right click open the same menu, in titled
groups: capture and **Board** first, then **Presentation**, **Shortcuts** (the six shortcuts, one
click to change one) and **Options**, and finally the exit. Every row has its icon; what can be
turned on carries a switch. **Start with Windows** is on by default and can be unchecked; the
choice is remembered.

## Instant screenshot

**Scroll Lock** saves the screen of the active application straight to the Desktop, with no
selection, no editor and no focus change. Files are named `Shot 001.png`, `Shot 002.png` and so
on, never replacing an existing capture. The **Instant screenshot** menu row changes this
shortcut; **Screenshot sound** turns the short confirmation click on or off. Both settings are
remembered.

The built-in sound is an original mechanical click. A personal WAV placed at
`%LOCALAPPDATA%/Shotoshot/screenshot-custom.wav` replaces it locally; if that file is missing or
cannot be played, the built-in sound is used. The personal file is never part of the installer.

A light blue 2 px outline pulses for 0.4 s on the four edges of the captured monitor, without
dimming its center or intercepting input; it is excluded from captures. The outline and the sound
fire only once the PNG has been written; nothing is displayed on success. On failure, only an
error message appears, top center (24 px at 96 DPI), at 35 % opacity for 0.75 s including the
fade; its box fits the text (12 px horizontal and 8 px vertical padding at 96 DPI) and is excluded
from captures.

Capture and encoding run off the input thread, with no permanent capture at rest. A press during
a capture already in progress does not start a second one. The shortcut is inactive while editing
or configuring. In exclusive full screen a game may hide the message; the sound remains useful.
Behaviour in demanding games and under GPU load still has to be verified in game; there is no
promise of zero stutter.

## Capture and draw

Drag the capture region. Handles resize it while keeping the annotations. Esc cancels the gesture
in progress, then closes the capture when no gesture is active. When a capture starts, "Select a
region" follows the pointer and reminds you that Esc cancels; this hint disappears on the first
press and is not part of the exported image.

Drawing tools run along the right edge; settings and actions sit under the bottom right corner.
The bars place themselves automatically, move when they touch a screen edge and cannot be dragged.
Icons show their name and shortcut on hover. **Tab** hides or shows the tools outside text entry,
so you can draw anywhere, even on a full-screen capture.

The eight white handles resize the sides and corners of the region, with the matching cursor.
Geometric shapes always have crisp outlines; the pencil is lightly smoothed. Annotations stay
visible outside the frame while editing; only the selected region is copied or saved. With the
selection tool (**V**), objects outside the frame can still be selected, moved and resized: the
frame only delimits the exported image.

The wheel shows a circle at the stroke size and its width in pixels. One notch moves to the next
step, increasingly spaced: 1 px up to 8, 2 px up to 16, 4 px up to 40. During a pencil or
highlighter stroke it changes only the thickness of the rest of the gesture; the parts already
drawn keep theirs. Ctrl+Z undoes the whole stroke.

| Key | Tool or action |
|---|---|
| V | Selection: click, Shift+click to add or remove, rubber band |
| R | Rectangle from corner to corner; Alt from its center; Shift for a square |
| C | Circle from its center; Alt for an ellipse |
| L | Straight line; Shift keeps the angle in 45° steps; Ctrl or Alt snaps its ends to closed shapes |
| A | Arrow; Shift constrains to 45°; holding Ctrl or Alt snaps its ends to closed shapes, which then carry it along |
| P / H | Pencil / highlighter; Shift draws a straight line in 45° steps |
| T | Text; click an existing text with this tool to edit it |
| E | Eraser: click an object, or sweep across several, to remove them whole; Ctrl+Z brings them back |
| I | Eyedropper: pick a color and copy its hex code |
| G | Bucket: each click recolors the stroke or outline under it, fills the rectangle or circle, or else the whole capture; Alt+click clears a fill |
| F | Whole screen / back to the previous selection |
| S / 1–6 | Color wheel / six quick colors |
| Wheel | Stroke width; text size while typing it, with it selected or with the Text tool |
| Arrows / Shift+arrows | Move the selected objects by 1 / 10 pixels |
| Page Up / Page Down | Bring the selection forward / send it back, past the next object it overlaps; with Shift, to the front / to the back |
| Del | Delete the selection |
| Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z | Undo / redo |
| Ctrl+C / Ctrl+V | Copy the selected annotations / duplicate them in the session; with no selection, Ctrl+C copies the image |
| Ctrl+Shift+C / Ctrl+S | Copy the image explicitly / save as PNG or JPEG |
| Ctrl+Shift+S | Save as: always asks where, also for a reopened editable image |
| Ctrl+G / F2 | Google Lens / live or frozen |
| Esc | Cancel the gesture or the text entry in progress, then close the capture |

### Colors and eyedropper

![The eyedropper wanders around the edge of a moon: the loupe magnifies the pixels under the pointer and its color code changes.](https://shotoshot.com/assets/eyedropper.gif)

The swatches offer six quick colors. The swatch ringed with colors opens the wheel: the ring sets
the hue, the square sets saturation and brightness. Enter or a click outside the panel applies the
color; Esc cancels. The wheel and the eyedropper are two tools: the wheel never reads the screen.

The **eyedropper (I)** samples anywhere on the frozen screen, including outside the chosen region.
It shows a pixel magnifier with the central pixel framed and its hex color before the click. The
click samples that pixel of the captured image and copies its code (`#RRGGBB`) to the clipboard;
Esc cancels. The last color sampled or confirmed in the wheel is remembered and becomes the
wheel's starting color, including after a restart. The six swatches do not replace this custom
color.

### Bucket and slate

The **bucket (G)**, beside the eyedropper, fills with the current color at every click and stays
in hand, zone after zone, until you pick a tool or press `G` or Esc. It colors what you click:
on a line, an arrow, a pen stroke, a text or the outline of a shape, it recolors that drawing;
inside a rectangle or a circle it fills that shape, under its outline; anywhere else in the frame
it fills the whole capture, under every drawing. To use Shotoshot as a slate, capture anything,
press `6`, `G`, click, then `P` and `5`: a dark background to draw on in white.

The bucket waits while you choose its color (swatches, wheel, eyedropper); any other action or Esc
puts it down. Clicking a fill again with another color recolors it, with the same color removes
it, and Alt+click removes it whatever the color. Each click is one undo step.

A fill belongs to its shape: it moves, turns, is copied and deleted with it, and a filled shape is
grabbed by its interior. The background belongs to the capture: it cannot be selected and follows
the frame when you resize it. Fills are opaque, so a filled rectangle also hides what lies under
it in the saved image. Shapes drawn with the pencil are not fillable, and the eyedropper still
reads the captured screen under a background.

### Exact size and whole screen

Click the dimensions above the top right corner of the frame to type a width and a height. The
fields accept `800+20`, `1920/2`, parentheses and the operations `+ - * /`. Tab moves from one
field to the other, Enter applies and Esc cancels. The top left corner stays fixed; an invalid
expression, a non-integer dimension or an overflow keeps the frame and shows an explanation.
Annotations are not resized with the frame.

The four-arrow button (**F**), right of the dimensions, extends the frame to the whole screen: the
one that contains most of the selection, without spilling onto neighbouring screens. A second
click restores the previous selection; after the frame has been adjusted, it extends again.

### Text

While typing text, the usual keys remain those of Windows. **Ctrl+Enter** confirms and **Esc**
cancels the entry; a click on the capture also confirms the text. Text appears directly on the
image with no opaque box: a click inside places the caret, a drag selects. The caret is currently
steady, without blinking.

Swatches, wheel and eyedropper also change the text color while typing. Confirming records text,
size and color together as a single undoable action. The wheel adjusts text in steps between 8 and
300 pixels: 2 px up to 24, 4 px up to 48, 8 px up to 96, then always six notches per doubling. A
wheel gesture applied to several selected texts is also undone as a single action.

### Editing objects

![A circle is shrunk and moved while the arrow bound to it follows, then a note written earlier is enlarged and tilted.](https://shotoshot.com/assets/editing.gif)

Handles resize the selected objects. The ends of an arrow can be moved to change what they attach
to. Moving a shape carries its attached arrows along; moving an arrow alone detaches it; deleting
a shape leaves its arrows in place. The round handle above the selection rotates it, texts
included, and Shift constrains the rotation to 45°. Rotated shapes resize proportionally to keep
their geometry.

New objects land on top of the pile. **Page Up** and **Page Down** move the selection through it,
one visible step at a time: past the next object it overlaps, so each press changes what you see.
With Shift they send it straight to the front or to the back. A multiple selection moves as one
block, keeping its own order, and each move is one undo step. The bucket's background always
stays underneath.

Copying and pasting annotations keeps their styles and the attachments between the copied objects.
It stays within the session: a new copy in another application, or with the eyedropper, replaces
that clipboard content. In text fields, Ctrl+C and Ctrl+V remain the Windows commands.

### Live capture

The **Live capture** button (a screen with a play symbol, bottom bar) switches between:

- **Off**: the image is frozen when the capture opens.
- **On, blue**: the desktop and videos keep moving; **Copy** or **Save** captures the instant of
  the click, with your annotations. The file dialog comes afterwards, without changing the image
  already taken.

Turning live capture off while editing freezes the image at that instant. To choose the initial
behaviour, check **Live capture by default** in the notification-area menu.

### Google Lens

The multicolored **G** button, **Search with Google Lens**, sends only the selected region with
its annotations to Google, then opens the results in the default browser. The upload happens on
the click, with no intermediate host and no change to the clipboard. The capture stays open if the
upload fails; the PNG is limited to 20 MB.

### Output

The saved file and the clipboard output are flattened and contain neither bars nor handles. The
initial folder is the Windows Desktop; the last folder used is remembered, falling back to the
Desktop if it disappears. A successful output closes the session; a failure keeps it open.

The type list of the Save dialog chooses the format, **PNG** or **JPEG**, and the proposed name
follows it; a name typed with `.png`, `.jpg` or `.jpeg` is written in that format. Both are
always written at their best, so there is no quality setting: PNG is lossless, JPEG uses the
highest quality with color at full resolution. On an interface capture a JPEG is usually larger
than the PNG; it is there for photos, video frames and sites that refuse PNG. The last format
used is remembered, `Shot NNN` numbers are shared by both formats, and the instant screenshot
stays a PNG. Linux has no Save dialog: **Ctrl+S** writes a PNG into the Pictures folder.

### Editable images (`.shoto`)

In the Save dialog, the last file type saves an **editable image**. Double-click it later and
the capture comes back as it was: same screen, same selection, every annotation still an object
you can move, recolor or delete. **Ctrl+S** then rewrites the file; **Copy**, **Esc** and
**Ctrl+Shift+S** (save as an image or another file) leave it untouched.

A `.shoto` file is a real PNG: renamed to `.png`, any viewer shows the annotated capture. Inside
it, Shotoshot keeps the objects and the whole screen the selection was on, without annotations.
**That screen stays in the file**, including what lies around the selection and under an opaque
annotation: share the PNG, keep the `.shoto` for yourself. Your other screens are never saved,
and the copy, PNG, JPEG and Google Lens outputs never carry this data. On Windows, the desktop
and Explorer show the annotated capture as the file's thumbnail, with the Shotoshot icon in
its corner.

A file from a newer Shotoshot, a damaged file or an image larger than the current screens is
refused with a message; there is no zoom yet. Live capture is not offered on a reopened image.

On Linux, where there is no Save dialog, **Ctrl+Shift+S** saves in the other format: an editable
image from a capture, a PNG from a reopened image, both into the Pictures folder; **Ctrl+S**
still rewrites a reopened image. A message confirms each editable save and recalls that the
screen is in the file. `install.sh` declares the file type, so a double-click opens it in
Shotoshot; `shotoshot <file>` does the same from a terminal. An image as large as a monitor
opens full screen on it, a smaller one in a window of its size.

## Board: notes under the icons

![The Board on a desktop: a zone is filled behind icons that stay in front, a to-do is typed, a note and an arrow point at a folder, then the notes stay on the desktop.](https://shotoshot.com/assets/board.gif)

**Board** keeps notes, drawn or typed, on your desktop, below the icons. Press **Alt+B** (the
**Board** row of the Shortcuts group changes it; it needs Ctrl or Alt) or choose **Edit board**
in the tray menu. The editor opens on a picture of your desktop, background and icons but no
window, with its usual tools; live capture is greyed. The toolbar sits on the screen under the
pointer and follows it from screen to screen, and your icons are drawn in front of the notes, as
the desktop will show them. **Esc** ends the session and applies it: the notes are baked into the
wallpaper of each screen. With a text box open, the first Esc commits the text and the next one
ends the session. Alt+F4 and a change of display end it the same way, and **Ctrl+C** or **Ctrl+S**
copy or save the picture and also end the session.

- **Zoom** a screen with **Ctrl+wheel** (up to 8×) around the pointer; move around it by
  dragging with the middle button or with **Space** held; **Ctrl+0** puts that screen back at
  1:1. Each screen zooms on its own, the toolbar keeps its size, and the view is for editing
  only: the picture, the notes and the wallpaper ignore it.
- Signing out of Windows, or shutting it down, while a session is open saves the notes; the
  wallpaper shows them at the next start.
- **Show board** in the tray menu is a switch, checked while the notes are shown: turning it off
  gives your own background back at once, turning it on bakes the notes again in the background.
  The notes stay on the desktop when Shotoshot is not running.
- **Clear board** removes every note. In the tray menu it acts at once, without asking, on every
  screen, unplugged ones included; it is refused while a session is open. In a session, the broom
  button before **Close** removes the notes shown and a painted background in one step:
  **Ctrl+Z** brings them back.
- Uninstalling Shotoshot gives your own background back if the Board is shown. The notes stay in
  their folder and show again once Shotoshot is installed again.
- Notes belong to the screen they are on. An unplugged screen keeps its notes and a new screen
  starts empty.
- If you change your wallpaper (picture, color or position) while the Board is shown, Shotoshot
  takes the new one as your background and bakes the notes over it a moment later.
- It uses only the documented Windows wallpaper API; nothing stays resident for it and nothing
  leaves your machine. The notes are text and one baked picture per screen in
  `%LOCALAPPDATA%/Shotoshot/board`.
- Limits: a slideshow or Spotlight background is replaced by a still picture, and hiding gives back
  the picture that was shown when the Board was first used; the Tile and Span positions are drawn
  like Fill. Windows
  only.

## Presenting live, without capturing

![Presentation mode over a slide: the shortcuts light up on the mini keyboard, the click shows under the pointer halo, and a rectangle, an arrow and a circle are drawn then fade away.](https://shotoshot.com/assets/presentation.gif)

Left or right click the Shotoshot icon: every control is in the menu. Turn on **Pointer halo**,
**Temporary shapes**, **Keyboard** and **Mouse** separately. Halo, keyboard and mouse are off at
launch; the shape shortcuts are available.

| Command | Action |
|---|---|
| Alt+A | Draw an arrow; Shift for 45° angles |
| Alt+R | Draw a rectangle; Shift for a square |
| Alt+C | Draw a circle from its center; Alt while drawing for an ellipse |
| Esc or right click while drawing | Cancel the gesture |
| Ctrl+Alt+Esc | Hide everything and turn the three features off |

Each shape keeps its letter from the capture editor (A, R, C), preceded by Alt. The drawing
shortcuts work while **Temporary shapes** is on. The **Arrow**, **Rectangle** and **Ellipse** menu
commands change each of the three combinations directly (Ctrl or Alt required). If another
application reserves a shape shortcut, Shotoshot takes priority while shapes are on. After each
release you get clicks and scrolling back in your applications at once; press the shortcut again
for a new shape. Each shape disappears after **3 seconds**, with a final fade; **Shape duration**
sets it to 1, 3 or 5 seconds and shows the current value.

The stroke is **5 px by default**, adjustable with the wheel while drawing (1–40 px, same steps
as the capture editor). The last thickness is remembered, like the one used in the capture editor.
The arrow head has no menu setting. Arrow length follows the drag; rectangles have slightly
rounded corners.

The halo changes during a click, with a short pulse: red while pressed, back to its resting color
at once. The mini keyboard shows letters, digits and special keys while they are pressed, then
fades them out over 600 ms. Space, Shift, Enter, Tab and the arrows use symbols; Ctrl, Alt and Num
use short labels. Key color returns to rest at once; mouse buttons keep a 180 ms afterglow. You
can drag the keyboard and lock it with the padlock key in its top right corner. It is unlocked by
default; the free position and the lock are remembered. The background and the mouse are more
transparent than the keys. No keystroke is written to a file. Keyboard and mouse keep working in
your applications.

**Keyboard** and **Mouse** are turned on separately: keyboard alone, mouse alone or both in the
same block, which shrinks to what is displayed. **Keyboard layout** picks the shape of the
keyboard:

| Shape | Content |
|---|---|
| Full | The whole keyboard, numeric keypad included |
| Compact | Without the numeric keypad |
| Gaming | Left-hand block: Esc, F1 to F4, the key left of 1, 1 to 5, then each row up to T, G and B, Ctrl, Win, Alt and a shortened space bar |

The shapes follow the physical position of the keys: on AZERTY the Gaming block shows ZQSD with no
setting. A key pressed outside the chosen shape is not displayed. **Keyboard and mouse size**
scales the block to 100, 150 or 200 %. Shape and size are remembered; a keyboard that is off is
not listened to, a mouse that is off is not drawn. The padlock takes the highest free key slot at
the right of the keyboard, or the top of the mouse when the mouse is alone.

Duration, position, shape, size and shortcuts are remembered; activations are not. Presentation
hides during a capture, then gets its previous activations back when the capture is closed or
cancelled. A change in the display configuration keeps each feature's choice and brings the
keyboard back onto a work area. Finish a capture in progress before turning these tools back on.
The temporary layer is not a saved image.

### In OBS

A **Display Capture** already shows the halo, the shapes and the keyboard. A **Game Capture**, or
the **Window Capture** of a single application, does not see them: add a **Window Capture** source
above it, window `[shotoshot.exe]: Shotoshot overlay`, capture method **Windows 10 (1903 and
up)**. It reproduces the halo, the shapes and the keyboard with their transparency, aligned on the
whole set of screens: with several screens, crop the source to the game's screen. The padlock
stays visible to you only. This off-screen window exists only while a presentation feature is on;
OBS finds it again on reactivation.

## Labels, keyboard layouts and confirmations

Menus and tooltips use consistent English labels. Value submenus have a fitted width; shortcuts
read "Shift" and "Print Screen". The mini keyboard translates keys according to the layout of the
active window at each press, including after a language switch: accented characters, Shift and
AltGr. Dead keys show their accent without altering what you type. Verified on French, US, UK and
German layouts. The drawing uses the ANSI shape for US layouts and ISO otherwise; this
conventional choice does not detect the physical hardware (JIS and IMEs are not validated).

Confirmations for copy, save, Google search and shortcut saving appear in Shotoshot's own dark
banner, bottom center of the pointer's screen. It stays about two seconds, then fades out, with no
sound, no focus change and no click interception. A new confirmation replaces the previous one.
Windows notifications are never used; the banner is removed before a new capture.

Tooltips appear after 250 ms of hover, stay anchored under the button with an offset of half a
width to the right, then fade out over 120 ms (they flip above near the bottom edge). The
dimension badge keeps its width while you type.

## Build and verify

Prerequisites: Rust with the x64 MSVC target and the `x86_64-unknown-linux-gnu` target (used for a
cross lint), Visual Studio C++ and the Windows SDK. In PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File check.ps1 -SkipLinux
```

The script sets up the MSVC environment if needed, checks the format and the dependency licenses,
runs Clippy with no warnings for Windows and Linux, the tests, the release build, the demo render
and a bounded diagnostic. `-Fast` skips the release build and its probes. Without `-SkipLinux`,
the run ends with the Linux tests on the maintainer's test device over SSH. The binary is
`target/release/shotoshot.exe`.

The checks use Direct2D/WIC and hidden test windows; they also capture the desktop in memory to
verify dimensions and the release of resources, without writing those captures. They do not touch
your clipboard or the start-with-Windows setting. A separate probe makes a real copy and reads its
pixels back in a non-interactive window station with its own clipboard, destroyed at the end of
the test. The presentation probe verifies, in a fresh process, the Raw Input registrations, the
conflicts and the release of shortcuts, with no simulated keystroke and no visible window.

Check outputs: `target/shotoshot-demo.png`, `target/shotoshot-interface.png`,
`target/shotoshot-presentation.png`, `target/shotoshot-menu.png`,
`target/presentation-diagnostics.txt`, `target/board-diagnostics.txt` and
`target/diagnostics.json`. The last one measures the
idle state and five captures with creation and destruction of the editor; it does not measure the
delay until the first pixel shown on screen.

## Data and limits

- No implicit network and no telemetry. Google Lens receives the capture only after a click on its
  button. Build dependencies are downloaded by Cargo.
- Preferences: `%LOCALAPPDATA%/Shotoshot/settings.txt`, versioned format. An unreadable file is
  reported and preserved; renaming it starts again with fresh settings.
- SDR capture through GDI; HDR fidelity is not guaranteed. Protected content may stay black.
- The capture interface and the starting stroke and text sizes follow the display scale of the
  screen holding the selection (Per-Monitor-V2). This is checked by tests and a 200 % reference
  render, not yet on a real high-density screen; IMEs still need validation on real setups.
- Objects stay editable during the session, and later in an editable `.shoto` image saved on
  purpose. No automatic history, detached editor, blur or share by link yet. Pinning captures
  was ruled out.

## License and contributing

Shotoshot is free software under the [GNU General Public License, version 3](LICENSE). The
licenses of its dependencies are listed in [THIRD_PARTY.md](THIRD_PARTY.md).

Bug reports and ideas are welcome as issues; code contributions are not accepted for now. See
[CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).
