# Annotation font

`Roboto-Medium.ttf` is the typeface of text annotations. It is embedded in the binary on every
platform (`shotoshot_ui::annotation_font`), so an annotation looks the same on Windows, Linux and
macOS, whatever fonts the system has. The interface keeps the system font.

Chosen on 2026-10-10, on comparison boards, over Segoe UI Semibold: the same stroke weight, and
the only option that does not depend on the fonts installed.

## Origin and license

Roboto, copyright 2011 The Roboto Project Authors, under the SIL Open Font License 1.1
(`Roboto-OFL.txt`, shipped in `THIRD-PARTY-NOTICES.txt` with every package). The font declares no
Reserved Font Name.

The file is a static instance of the variable font published by Google Fonts, cut at weight 500
and normal width, with nothing else changed:

- source: `ofl/roboto/Roboto[wdth,wght].ttf` of `github.com/google/fonts`, commit `6183fc0d2636`
  (2026-01-27), SHA-256 `d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134`;
- tool: fontTools 4.63.0,
  `instancer.instantiateVariableFont(font, {"wght": 500, "wdth": 100}, updateFontNames=True)`.

One static weight keeps the binary small (154 KB against 477 KB for the variable file) and lets
DirectWrite reach it with the plain text format call. Latin, Greek and Cyrillic are covered; other
scripts and emoji fall back to system fonts, as before.

`annotation_font::LINE_HEIGHT` is the line height DirectWrite gives this font (1.2 times the
size); the portable renderer uses the same value so that both lay multi-line text out alike.
