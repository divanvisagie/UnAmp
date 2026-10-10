# 0023. Tile with the real windows, pinned in place, instead of panels

Date: 2026-10-10

## Status

Accepted

## Context

[ADR-0022](0022-snap-to-grid-and-tiling.md) built tiled mode from egui panels, each wrapped in a
hand-drawn `tile()` box imitating a window. The tiles didn't match the floating windows:
- the title was left-aligned and bold;
- there was no collapse arrow;
- the close button and the title bar differed.

The user asked why the windows looked different in tiled mode. Two drawing paths for the same
window is the drift [ADR-0018](0018-one-command-path.md) set out to remove. The user chose to
make the tiles real windows.

## Decision

- **Tiles are the same `egui::Window`s as in floating mode**, pinned with `Window::fixed_pos` or
  `Window::fixed_rect` and built by the same code (`show_windows` in `src/app.rs`). Only the
  placement differs: default position and size when floating, a computed spot when tiled.
  - Pinned windows aren't movable or resizable.
  - Pinned windows have no shadow, so neighbours don't darken each other.
  - The hand-drawn `tile()` box and the panel layout are gone.
- **The layout is computed, not handed to panels:**
  - The Player and Equalizer take their natural heights at the top of the left column, and the
    Playlist gets the rest. Closing one moves the ones below up.
  - The Media Library and Waveform share the right side.
  - The classic stack is pinned flush down the column in the same way.
- **Library/Waveform split:** a thin drag handle in the gap between them sets the Waveform's
  height (`tile_waveform_height` in the config), replacing the resizable panel.
- **Tiled is the default** (`tile_windows = true`), at the user's request. Floating windows are
  one menu click away. Existing configs keep whatever they last saved.
- **Tiled windows get ids of their own** (`<id>.tiled`). egui stores a window's position and size
  by id, so pinning the floating ids would overwrite the floating layout. With separate ids it's
  still there when tiling is turned off, as ADR-0022 promised.

## Consequences

- Tiles look exactly like the floating windows in every skin, and any change to how windows are
  drawn reaches both modes.
- One code path builds the windows for both modes, so a new window only needs a floating spot
  and a tiled spot.
- Windows no longer have collapse arrows, in either mode, at the user's request. A window is
  opened or closed whole, so a tile never leaves a collapsed stub.
- The split handle is our own small widget instead of egui's panel resizing. More splits, such
  as the left column's width, can be added the same way.
- Window state such as the library sidebar's width is shared between modes. Only position and
  size are kept separately.
