# 0006. Use floating egui windows for the player, equalizer, playlist and media library

Date: 2026-10-09

## Status

Accepted

## Context

The first layout copied Photograph: fixed panels, with the player across the top, locations
on the left, and a central area with Folder/Playlist tabs. That works for a photo browser, but
Winamp is defined by its separate windows (main player, equalizer, playlist editor, media
library) that you open, close and arrange yourself. Tabs also meant you couldn't see the
playlist and a folder at the same time.

Two ways to get separate windows in egui:

- **`egui::Window`**: floating windows inside the one native window. Positions and sizes are
  kept in egui's memory, and that persists with eframe's `persistence` feature.
- **Native viewports** (`show_viewport_deferred`): real OS windows. Wayland doesn't let a client
  place its own windows, so the classic stacked layout, and snapping windows together, can't be
  done there. Each viewport also renders separately.

## Decision

We will show the Player, Equalizer, Playlist and Media Library as `egui::Window`s floating over
an empty desktop in the main native window. A Windows menu and the player's EQ/PL/ML buttons open
and close them. Which windows are open is saved in UnAmp's config; positions and sizes are saved
by eframe's `persistence` feature. The default layout stacks player, equalizer and playlist on the
left (as Winamp did) and puts the library on the right. Windows → Reset layout restores it.

## Consequences

- Any arrangement is possible, and the playlist and library can be open side by side.
- Everything stays inside one OS window: you can't drag the equalizer to another monitor, or
  keep only the player visible as a small desktop window.
- The player and equalizer have fixed widths so the stack lines up; their heights come from their
  content, so the default positions assume those heights stay roughly the same.
- eframe now stores egui state (window rects) in its data directory, as well as UnAmp's own
  `config.toml`.
- If real OS windows become important (multi-monitor, a mini player), native viewports would
  supersede this, at the cost of manual placement on Wayland.
