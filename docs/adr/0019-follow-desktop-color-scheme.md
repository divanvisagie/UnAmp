# 0019. Follow the desktop's light/dark setting through the XDG portal

Date: 2026-10-10

## Status

Accepted

## Context

The Default skin is meant to follow the system theme ([ADR-0008](0008-toml-skins.md)): it sets
egui's theme preference to "system". But egui learns the system theme only from winit, and winit
0.30 never reports one on Linux. Its `system_theme()` returns `None`, it never sends a
`ThemeChanged` event, and on Wayland `Window::theme()` only echoes what the app set itself. So
egui always used its fallback theme, which is dark. On a GNOME desktop in light mode, UnAmp came
up dark. The user asked for the Default skin to react to system theme changes, meaning the
desktop's light/dark setting.

On Linux the desktop publishes that setting through the XDG desktop portal, as `color-scheme` in
the `org.freedesktop.appearance` namespace of `org.freedesktop.portal.Settings`. Its values are
0 for no preference, 1 for dark and 2 for light, and the portal emits `SettingChanged` when it
changes. GNOME's "Dark style" switch sets it, and so do KDE and others through their portal
backends. UnAmp already depends on zbus for MPRIS ([ADR-0016](0016-mpris-media-controls.md)).

## Decision

- **A watcher thread** (`src/appearance.rs`) connects to the session bus, subscribes to
  `SettingChanged`, then reads `color-scheme` with `ReadOne`. Older portals only have the
  deprecated `Read`, so it falls back to that. It then follows the signal for as long as the app
  runs.
- **The setting maps onto egui's fallback theme**: 1 means dark, and 0 or 2 mean light. Light is
  GNOME's default and egui's light visuals are its stock look. The watcher sets
  `Options::fallback_theme` and requests a repaint.
  - egui uses the fallback only when the platform reports no system theme. Where winit does
    report one (macOS, Windows), that still wins, so nothing changes there.
  - Skins with a fixed `base` set an explicit theme and ignore the fallback.
- **The palette follows the theme.** The Default skin's custom-painted colours (spectrum,
  waveform and so on) come from the active visuals. The app remembers which theme its palette was
  built for and re-applies the skin when `ctx.theme()` differs.
- **The window frame follows too.** On GNOME's Wayland session the title bar is drawn by winit
  (sctk-adwaita), which reads the portal once when the window opens. Whenever the app's theme
  changes, it sends `ViewportCommand::SetTheme`, which also keeps the frame matching a skin's
  fixed theme, such as Steam Classic's dark frame.
- **It stays optional at runtime, like MPRIS.** With no session bus or portal, the thread logs
  one line and ends, and the Default skin stays dark as before.

## Consequences

- The Default skin and the window frame now match the desktop at startup and switch live when
  the user flips dark style, with no restart.
- The behaviour is confined to one small module plus a theme check per frame. It can go once
  winit or eframe report the Linux system theme themselves.
- It follows only light/dark. GTK themes, accent colours and high contrast are not read.
- A desktop without a portal backend that implements `org.freedesktop.appearance` (some minimal
  window managers) gets the old behaviour, which is always dark.
- Mapping value 0 (no preference) to light changes the look for users whose desktops report
  nothing. Before, they always got dark.
