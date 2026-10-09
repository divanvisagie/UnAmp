# 0008. Support skins as TOML files, keeping stock egui as the default

Date: 2026-10-09

## Status

Accepted. Ruling out bitmap skins is superseded by [ADR-0010](0010-classic-wsz-renderer.md)
for classic `.wsz` skins; TOML colour skins are unchanged.

## Context

[ADR-0002](0002-stock-egui-style.md) kept egui's stock style and ruled out theming, noting that a
skin system would be a new decision that supersedes it. Skins are part of what Winamp was, and
the first one asked for recreates the old olive-green Steam client (VGUI, roughly 2003–2008).

Requirements as stated: skins are TOML files; UnAmp starts with the stock egui look, and you pick
a skin (Steam Classic first) from a menu.

Options considered for what a skin can change:

- **Colours and a few style knobs** mapped onto egui `Visuals` (rounding, shadows), plus colours
  for the parts UnAmp paints itself (spectrum, time display, title). Small and hard to get wrong.
- **Bitmap skins** like Winamp 2's `.wsz` (sprite sheets for every button). They would need
  fixed-size, sprite-drawn widgets instead of egui's, which conflicts with
  resizable egui windows ([ADR-0006](0006-floating-egui-windows.md)). Not now.
- **Fonts**: VGUI used Tahoma. Bundling or locating system fonts adds licensing and lookup
  questions; left out for now.

## Decision

We will load skins from TOML files (`src/skin.rs`):

- A skin has a `name`, an optional `author`, a `base` (`"dark"` or `"light"`, the egui theme it
  starts from), optional `corner_radius` and `shadows`, a `[colors]` table for egui widgets, and a
  `[player]` table for UnAmp's own painted parts. Every colour is optional (`#RRGGBB` or
  `#RRGGBBAA`); anything unset keeps egui's default for the base theme.
- Unknown keys and bad colours are errors, shown in the Skins menu, so a typo doesn't fail
  silently.
- **Default** is built in, isn't a file, and is stock egui following the system light/dark
  setting, as before.
- Built-in skins are TOML files in `skins/`, compiled into the binary with `include_str!`.
  The first is `skins/steam-classic.toml`, which is also the documented example of the format.
- User skins go in `~/.config/unamp/skins/*.toml`. A user skin with a built-in's name
  replaces it. The Skins menu lists them all and has Reload and Open skins folder.
- The chosen skin's `name` is saved in `config.toml`; a missing skin falls back to Default.

## Consequences

- Making a skin is copying one commented TOML file and editing colours; no rebuild needed.
- A skin fixes the theme (dark or light), so the system light/dark switch only affects
  Default.
- The skin can't change layout, widget shapes beyond rounding, or fonts. Anything closer to
  Winamp's bitmap skins would be a new decision.
- The colour keys are now a public format; renaming or removing one breaks people's skin
  files, so changes should add keys rather than rename them.
