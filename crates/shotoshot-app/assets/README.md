# Shotoshot icon

A lens with framing corners, chosen on 2026-09-23. It is an original geometric drawing; the SVG
keeps the vector source.

`generate-icon.ps1` reproduces this geometry with GDI+, supersamples each size, then writes the
transparent PNGs and the ICO holding the 16, 20, 24, 32, 48 and 256 pixel images. It needs no
third-party graphics tool. The PNGs are for visual checking; only the ICO is embedded in the
executable, as resource 1.

From PowerShell: `./generate-icon.ps1`. Keep the coordinates of the SVG and of the generator
identical if the geometry changes.

## Capture sound

`screenshot.wav` is an original mechanical sound synthesized on 2026-09-24: a dry clack, a brief
friction and a mechanical return. It contains no third-party recording or sample and derives from
no existing file.

Offline regeneration: `python generate-screenshot-sound.py` (standard library only, fixed seed).
PCM16 mono 44,100 Hz, about 202 ms, peak −6.38 dBFS with no clipping. Played asynchronously from
memory after a successful save; it can be turned off in the menu.

A personal sound can replace this file at playback through
`%LOCALAPPDATA%/Shotoshot/screenshot-custom.wav`. That local replacement does not change the
distributed assets.
