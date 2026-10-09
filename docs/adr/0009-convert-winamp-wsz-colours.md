# 0009. Convert classic Winamp `.wsz` skins to TOML skins, colours only

Date: 2026-10-09

## Status

Superseded by [ADR-0010](0010-classic-wsz-renderer.md), which renders the bitmaps too.
The colour conversion described here remains, and now also writes the `classic` key.

## Context

People have classic Winamp 2 skins they've kept for decades (the first asked for: a friend's
Squall / Final Fantasy VIII skin) and want them in UnAmp. A `.wsz` is a zip of bitmaps
(`main.bmp`, `cbuttons.bmp`, `eqmain.bmp`, …) for fixed-size, pixel-drawn windows, plus two text
files: `pledit.txt` (playlist colours) and `viscolor.txt` (24 visualizer colours).

[ADR-0008](0008-toml-skins.md) chose TOML colour skins and ruled out bitmap skins, because egui
windows are resizable and drawn with egui widgets. Rendering `.wsz` bitmaps faithfully would mean
a second, sprite-based UI.

## Decision

We will convert a `.wsz` into an UnAmp TOML skin, carrying over colours only (`src/wsz.rs`):

- `pledit.txt`'s Normal, Current, NormalBG and SelectedBG become text, now-playing, list
  background and selection colours.
- `viscolor.txt` supplies the analyzer background, the bar gradient (entries 17 → 8 → 2,
  bottom to top) and the peak dots.
- The time colour is the brightest pixel of `numbers.bmp` (or `nums_ex.bmp`), and window
  backgrounds use `main.bmp`'s average colour. Whether the skin uses egui's dark or light base
  follows from how bright that background is.
- Missing files leave their keys out, so egui's defaults fill in.

Conversion happens on Skins → Reload (and at startup): every `.wsz` in
`~/.config/unamp/skins/` without a same-named `.toml` gets one, named after the file. Existing
TOML is never overwritten, so it can be hand-tuned; delete it to convert again. Archives are read
with a per-file size cap, and only the five files above are extracted.

## Consequences

- A favourite old skin is one file drop away, and the result is an ordinary, editable TOML skin.
- It's a colour impression, not the skin: artwork, button shapes, fonts and window chrome don't
  carry over. Skins whose look lives in their bitmaps rather than their colours won't feel
  like the original until tuned by hand.
- The `main.bmp` average can be muddy for busy artwork. Sampling specific regions (e.g. the
  title bar) is a possible refinement.
- Adds the `zip` crate (decompression only).
- Rendering real bitmap skins remains a separate, much larger decision.
