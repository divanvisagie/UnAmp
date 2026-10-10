# 0022. Snap floating windows to a grid, or tile them

Date: 2026-10-10

## Status

Accepted (how tiles are drawn superseded by [ADR-0023](0023-tile-with-pinned-windows.md))

## Context

The windows float freely ([ADR-0006](0006-floating-egui-windows.md)), so lining them up by hand
is fiddly, and they leave unused space when the main window is large. The user asked for a
Windows-menu toggle for tiling, where windows snap to a grid when resized. Asked which they
meant, snapping floating windows to a grid or a tiling layout that fills the app, they chose
both.

## Decision

- **Windows → Snap to grid** (`snap_to_grid` in the config) works on floating windows when a move
  or resize ends (`src/layout.rs`):
  - A window's top-left corner goes to the nearest point on a 10 pt grid, measured from the
    desktop's default window origin.
  - An edge of another window within 8 pt wins over the grid, on either side. This lets the
    fixed-size classic windows (232 pt tall, off the grid) stack flush, as in Winamp.
  - Resizable windows also round their size *up* to the grid. Rounding up never asks a window
    to be smaller than its contents, so egui can't grow it back and start a loop.
  - Faint grid dots show on the desktop while a window is being dragged.
- **How the snap is applied:** egui has no public setter for window positions and sizes. The snap
  is decided the frame after the mouse button is released, then applied with
  `Window::current_pos` and `Window::fixed_size`, which takes the outer size. It's repeated for up
  to three frames until the window is there.
  - In egui 0.35 a title-bar-draggable window restores its old position after `current_pos`, so
    dragging is switched off for those frames. The mouse is up then anyway.
  - Snapping only after release leaves egui's own dragging untouched.
- **Windows → Tile windows** (`tile_windows`) replaces the floating windows with fixed panels:
  - The Player, Equalizer and Playlist stack down the left, with the Playlist filling the rest
    of the column.
  - The Media Library fills the remaining space, with the Waveform in a resizable panel beneath.
    A window that's turned off gives its space to the others.
  - Tiles are framed boxes with the window's title and a close button. Classic skins draw their
    bitmaps flush down the column, as Winamp's docked stack does.
  - Snap to grid is disabled while tiling, since there's nothing to move.
- Floating positions are kept while tiled, so turning tiling off restores the previous layout.

## Consequences

- Windows line up without pixel-hunting, and a big main window can be filled edge to edge.
- Snapping happens on release rather than live, so a window jumps slightly when you let go.
- Snapping depends on egui behaviour that has no public API (the position restore in
  `Window::show_dyn`). A headless test drives the whole cycle through egui so an upgrade that
  breaks it fails the build.
- Any mouse release, even clicking a button, re-checks the snap. This keeps everything on the
  grid while snapping is on, but a window placed while snapping was off moves the first time
  anything is clicked after turning it on.
- The tiled layout is fixed. Only the Waveform's height and the library's own sidebar split can
  be resized, and windows can't be rearranged into other tile positions.
