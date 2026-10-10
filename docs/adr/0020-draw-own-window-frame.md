# 0020. Draw UnAmp's own window frame, following the skin

Date: 2026-10-10

## Status

Accepted

## Context

After [ADR-0019](0019-follow-desktop-color-scheme.md) the app followed the desktop's light/dark
setting, but the title bar didn't. On GNOME's Wayland session apps draw their own frames, and
winit's built-in one (sctk-adwaita) reads the setting once when the window opens. Telling it the
theme on every change fixed light/dark, but its colours are hard-coded: a skin can't recolour
it, so Steam Classic or a Winamp skin sat under a stock Adwaita bar. Winamp itself never used the
system frame. The user chose to draw UnAmp's own frame so it follows the theme. They asked for
the default frame's corners to match what GNOME uses on Ubuntu.

## Decision

- **The main window opens undecorated** and transparent. `src/frame.rs` draws the frame with the
  active egui visuals, so it follows the skin and, on the Default skin, the desktop's light/dark
  setting.
- **The title bar is a header bar**, as in GNOME apps. The existing menus are on the left, the
  window title is centred and shortened if needed, and minimise, maximise/restore and close are
  on the right. Close turns red on hover.
  - Pressing and dragging empty space moves the window. Double-click toggles maximise, and
    right-click opens a window menu.
  - Moving and resizing go to the compositor (`StartDrag`, `BeginResize`), so snapping and
    tiling work as with a native frame.
- **Resize handles** are invisible 5 px edges, with 12 px at the corners, plus a one-pixel
  border in the skin's window stroke colour. They're placed above the floating windows, so a
  window dragged to the edge can't block resizing. They're off while maximised.
- **Rounded corners:** libadwaita's `--window-radius` is 15 px, read from the Yaru styles
  shipped in Ubuntu's libadwaita, and libadwaita rounds all four corners.
  - The Default skin uses 15 px. A skin's `corner_radius`, if set, is used instead, so Steam
    Classic and converted `.wsz` skins stay square.
  - Corners are square while maximised or full screen.
  - The clear colour is transparent and the title and status bars carry the rounding.
- **Escape hatch:** Windows → System title bar (`system_title_bar` in the config) switches back to
  desktop decorations at runtime. The ADR-0019 frame theming still applies then.

## Consequences

- The frame matches every skin and switches live with the desktop's light/dark setting.
- UnAmp now owns behaviour the system frame gave for free. What we draw works. What's missing:
  - no drop shadow, because the window has no margin to draw one in;
  - no rounded corners in half-tiled states, since egui isn't told about tiling, only maximise;
  - no native window menu entries such as "Always on top".
- Without a compositor (bare X11), the transparent corners show black. Picking the system title
  bar avoids that.
- On desktops with server-side decorations (KDE, X11), UnAmp no longer gets the native frame by
  default, so it looks the same everywhere.
