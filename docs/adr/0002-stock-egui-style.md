# 0002. Use egui's stock style instead of a custom theme

Date: 2026-10-09

## Status

Superseded by [ADR-0008](0008-toml-skins.md)

## Context

Photograph replaces egui's look with a Yaru-style theme: custom `Visuals` for light and dark,
GTK-like spacing and rounding, and bundled Ubuntu fonts. That's a fair amount of code and assets to
carry, and it has to be maintained across egui upgrades.

For UnAmp the request was to "use the base egui style". The Winamp feel comes from what the
player shows — the scrolling title, the big time display, the spectrum analyzer — not from
reskinning every widget.

## Decision

We will use egui's default `Style`, `Visuals` and fonts unchanged: no `configure_visuals`, no
bundled fonts. Custom painting is limited to things egui has no widget for (spectrum analyzer,
seek bar, title ticker, track rows), and these take their colours from the active `Visuals` —
except the analyzer, which keeps the classic green→yellow→red on black.

## Consequences

- No theme code or font assets to maintain; egui upgrades bring their own style improvements for
  free, and light/dark follows the system through eframe's default.
- UnAmp looks like an egui app, not a GNOME app, and doesn't match Photograph visually.
- Glyphs depend on egui's bundled emoji/icon font; anything it lacks shows as a box.
- If a skin system (classic Winamp `.wsz` skins, say) is ever wanted, that's a new decision that
  supersedes this one, not a tweak to it.
