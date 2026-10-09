# 0016. Expose playback over MPRIS with zbus, optional at runtime

Date: 2026-10-09

## Status

Accepted

## Context

GNOME's notification/calendar panel shows a mini player for whatever music app is playing. It
isn't drawn by the app: GNOME Shell finds players on the session D-Bus through MPRIS (the Media
Player Remote Interfacing Specification), the freedesktop standard every Linux player implements.
The same interface is how media keys, Bluetooth headset buttons, KDE Connect/GSConnect and
`playerctl` control a player. The user asked for the GNOME mini player, and confirmed the plan
after asking whether UnAmp would still work on systems without D-Bus.

Options considered for the implementation:

- **`zbus` directly.** Pure Rust (no `libdbus`), already in the dependency tree through eframe's
  accessibility support, and lets us implement the full spec (shuffle, loop, volume).
- **`souvlaki`.** A cross-platform media-controls wrapper. Less code, but only the basics, and its
  cross-platform reach doesn't matter for a Linux-only app ([ADR-0001](0001-linux-only-egui-native-app.md)).

## Decision

We will implement MPRIS with `zbus` in `src/mpris.rs`:

- UnAmp owns `org.mpris.MediaPlayer2.unamp` (a second instance takes
  `….unamp.instance<pid>`, as the spec suggests) and serves `org.mpris.MediaPlayer2` (Identity
  "UnAmp", DesktopEntry "unamp", Raise, Quit) and `org.mpris.MediaPlayer2.Player`: status, metadata
  (title, artist, album, length, track id, art URL), position, volume, shuffle, loop status
  (repeat off/one/all ↔ None/Track/Playlist), the Can* flags, and Play/Pause/PlayPause/Stop/Next/
  Previous/Seek/SetPosition.
- **D-Bus is optional at runtime.** All of it runs on its own thread. The app sends state
  snapshots there and reads commands from a channel, so the UI never waits on the bus. With no
  session bus (or a broken one) the thread logs one line and ends, and UnAmp runs as before.
  `zbus` doesn't link `libdbus`, so the binary has no D-Bus dependency at all.
- Only changed properties are announced (`PropertiesChanged`). Position is extrapolated from a
  clock, so clients reading it between frames get the right value. Any position jump the clock
  didn't predict is announced as `Seeked`, whatever caused it.
- Commands are applied by the app exactly as its own buttons would be, so the queue, shuffle and
  repeat behave the same from the panel.
- Album art goes to `~/.cache/unamp/art/`, named by a hash of its content and capped at 256 files.
  Embedded art has no file of its own, and a desktop shell may not be able to read covers on a
  network share.
- Playback housekeeping (engine polling, advancing tracks, MPRIS, session saving) moved from
  `ui()` into eframe's `logic()`, which also runs while the window is hidden. Otherwise a minimised
  UnAmp controlled from the panel could stop advancing at the end of a track.
- `make install-desktop` registers a launcher and icon for a source checkout in
  `~/.local/share`, so GNOME shows UnAmp's name and icon in the panel without the `.deb`.

## Consequences

- GNOME's panel, media keys, headset buttons and any MPRIS client control UnAmp, verified on a
  private session bus (`dbus-run-session` + `busctl`): status, metadata and art URL, position,
  pause/play, seek, next, volume, shuffle and loop.
- Raise may not focus the window on Wayland, where the compositor restricts focus stealing.
- The art cache is a new place UnAmp writes to; it's safe to delete.
- No TrackList or Playlists interfaces: clients can't browse UnAmp's queue or playlist over D-Bus.
