# Shotoshot

Native screen capture and annotation for Windows 11 and Linux. Press a key, drag a region,
annotate on the frozen screen with objects that stay editable, then copy or save. On Windows, the
same editor writes notes on your desktop, under the icons: the
[Board](docs/MANUAL.md#board-notes-under-the-icons). The interface is available in 15 languages.

- **Light.** A small resident process starts one editor process per capture. That process ends
  with the session, which returns its memory to Windows, graphics caches included.
- **Private.** No telemetry and no network request of its own. Google Lens receives a capture only
  when you click its button, sent by your browser.
- **Open.** GPL-3.0, written in Rust on Direct2D, with no web engine.

![Shotoshot at work: an area is selected and widened, an arrow points at a chart legend, a note is typed, enlarged and rotated, a square is drawn then moved, then the arrow is re-aimed by its tip alone, recoloured with the eyedropper and thickened with the wheel.](https://shotoshot.com/assets/capture.gif)

## Install

Download the installer from [shotoshot.com](https://shotoshot.com). It is not code-signed yet, so
Windows SmartScreen may ask for confirmation; the SHA-256 of each download is published on the
site.

On Linux (x86_64 and ARM64, glibc 2.31 or later, Wayland and X11), Shotoshot comes as a
`tar.gz` with its own `README.txt`. The Board, presentation mode (except the Magnifier, on
labwc), crop-size fields, Google Lens and live capture are not on Linux yet. It runs on a
Raspberry Pi; reports from a regular x86_64 PC are welcome.

## Quick start

1. On first launch, **Save** keeps **Print Screen** as the capture shortcut, or click the key and
   press another one.
2. Press the shortcut, drag a region and annotate.
3. **Ctrl+C** copies the image, **Ctrl+S** saves it as PNG or JPEG. **Esc** closes the capture.

The icon in the notification area opens the menu: capture, instant screenshot, Board,
Presentation, Settings and Language.

## Features

- **Annotations that stay objects**: rectangles, circles, lines and arrows that hook onto shapes,
  pencil, highlighter, text, text boxes, blur, eraser, eyedropper and a fill bucket. Select, move,
  resize, rotate, recolor and reorder them until you export.
- **Precise framing**: resize the region with its handles, type an exact size, or fill the whole
  screen with **F**. **Live capture** keeps videos moving until you copy or save.
- **Instant screenshot**: **Scroll Lock** saves the active application's screen to the Desktop
  as `001.png`, `002.png`…, with no editor.
- **Editable images (`.shoto`)**: a real PNG that reopens with every annotation still editable.
  It keeps the whole screen the selection was on, so share the PNG and keep the `.shoto` for
  yourself.
- **Google Lens**: search the selected region in your browser.
- **Board** (Windows): notes drawn or typed on the desktop, under the icons, baked into the
  wallpaper.
- **Presentation**: pointer halo, trail, clicks, spotlight and magnifier, temporary shapes that
  fade, and an on-screen keyboard and mouse showing what you press. Works in OBS.

Everything is described in the [manual](docs/MANUAL.md).

## Main keys

| Key | Tool or action |
|---|---|
| V | Selection |
| R / C | Rectangle / circle; Shift for a square, Alt for an ellipse |
| L / A | Line / arrow; Shift for 45° steps, Ctrl or Alt snaps the ends to shapes |
| P / H | Pencil / highlighter |
| X | Blur (the hidden pixels are destroyed in saved files) |
| B / T | Text box / text |
| E / I / G | Eraser / eyedropper / bucket |
| F | Whole screen / back to the previous selection |
| S / 1–6 | Color wheel / six quick colors |
| Wheel | Stroke width, text size, or size of the selected shape |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+C / Ctrl+S | Copy / save |
| Tab | Hide or show the toolbars |
| Esc | Cancel the gesture in progress, then close the capture |

The [full table](docs/MANUAL.md#capture-and-draw) lists every modifier and shortcut.

## Build and verify

Prerequisites: Rust with the x64 MSVC target and the `x86_64-unknown-linux-gnu` target (used for a
cross lint), Visual Studio C++ and the Windows SDK. In PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File check.ps1 -SkipLinux
```

The script checks the format and the dependency licenses, runs Clippy for Windows and Linux, the
tests, the release build and the diagnostics; `-Fast` skips the release build. The binary is
`target/release/shotoshot.exe`. What the probes do and produce is in the
[manual](docs/MANUAL.md#build-and-verify).

## Data and limits

- No telemetry and no implicit network request. Preferences live in
  `%LOCALAPPDATA%/Shotoshot/settings.txt`.
- SDR capture through GDI; HDR fidelity is not guaranteed and protected content may stay black.
- More in the [manual](docs/MANUAL.md#data-and-limits).

## License and contributing

Shotoshot is free software under the [GNU General Public License, version 3](LICENSE). The
licenses of its dependencies are listed in [THIRD_PARTY.md](THIRD_PARTY.md).

Bug reports and ideas are welcome as issues; code contributions are not accepted for now. See
[CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

Translations live in `crates/shotoshot-text/locales`, one text file per language; issues with
translation corrections are welcome.
