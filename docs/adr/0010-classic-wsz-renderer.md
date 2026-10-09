# 0010. Render classic `.wsz` skins pixel-for-pixel in fixed-size windows

Date: 2026-10-09

## Status

Accepted

## Context

[ADR-0009](0009-convert-winamp-wsz-colours.md) brought classic skins in as colours only. On the
first real-world skin tried (a SkinAmp-made Shakira skin) that missed almost everything: the look
lives in the bitmaps, meaning photo backgrounds, custom title bars, sprite buttons, LED digits, a
pixel font and slider art. Three options were put to the user:

- **Classic renderer**: draw the Player, Equalizer and Playlist from the skin's sprites at
  Winamp's fixed sizes, like Webamp does.
- **Hybrid**: paint the skin's artwork behind the existing egui widgets. Less work, but the art
  has its own buttons painted in, so egui controls would sit on top of drawings of other
  controls.
- **Colours only**: keep ADR-0009 as is.

The user chose the classic renderer.

## Decision

When the active skin's TOML has `classic = "<file>.wsz"`, UnAmp draws the Player (275×116),
Equalizer (275×116) and Playlist (275×145) from that archive's bitmaps (`src/classic/`), at a
fixed 2× scale with nearest-neighbour sampling:

- Sprite coordinates follow the Winamp 2 skin layout as documented by the skinning community and
  Webamp. Sprites a skin's bitmaps are too small for are skipped. Only `main.bmp` is required.
- Text in the ticker, kbps/kHz and playlist time displays uses the skin's `text.bmp` font. Time
  uses `numbers.bmp`/`nums_ex.bmp`. Playlist rows use `pledit.txt` colours with egui's font.
- The renderers only read state and return actions; the app applies them, so classic and egui
  windows share one playback model.
- The windows are frameless, undecorated egui windows: they still float, stack and drag
  ([ADR-0006](0006-floating-egui-windows.md)) but can't be resized. The Media Library and menus
  stay egui, coloured by the TOML.
- The converter ([ADR-0009](0009-convert-winamp-wsz-colours.md)) now writes the `classic` line;
  removing it gives the regular egui windows in the skin's colours.
- Classic windows aren't constrained to the desktop: egui's constraint pushed the frameless
  playlist up over the equalizer even when the stack fitted. Windows → Reset layout recovers
  any that end up off-screen.

## Consequences

- Old skins look like themselves: the Shakira skin and Winamp's base skin both match Winamp
  closely in screenshots.
- A second UI surface to maintain: features added to the egui player (e.g. a new button) don't
  appear in classic mode unless the skin layout has a place for them.
- Not implemented: windowshade (collapsed) modes, the clutterbar, double-size toggle, the
  playlist's ADD/REM/SEL/MISC/LIST popup menus, resizing the playlist, `region.txt` window
  shapes, balance control, and cursors. Skins relying on transparency via `region.txt` will
  show square corners.
- Fixed 2× scale: at 1× the windows are tiny on modern screens, and at other scales
  nearest-neighbour sampling looks uneven. A scale setting is a natural follow-up.
