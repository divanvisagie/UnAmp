# 0021. Generate the docs screenshot from a fake library, with a screenshot mode in the app

Date: 2026-10-10

## Status

Proposed

## Context

The first commit left out a screenshot because the real one showed private network share names
and album art from the user's library. The user asked for a `make` target that produces a
screenshot for `docs/` from a temporary directory of fake songs. It has to be repeatable and must
never show anything from the machine it runs on.

## Decision

- **`examples/fake_library.rs`** builds a throwaway home in a temp directory:
  - four albums of synthesised songs, as 22 kHz mono WAVs with ID3 tags and generated
    `cover.png` art (bass, chords, kick, hi-hats and a lead), so the spectrum and waveform have
    something to show;
  - `user-dirs.dirs`, `config.toml` with the library open on an album, EQ on and the Waveform
    window shown, and a saved playlist with `#EXTINF` lines.
- **Screenshot mode in the app** (`src/screenshot.rs`): with `UNAMP_SCREENSHOT=<png>` set, UnAmp
  plays the restored track with the engine silent (the slider keeps its level), waits four
  seconds, captures its own window with `ViewportCommand::Screenshot`, saves it and quits. In
  this mode it also:
  - skips mount discovery, so drives and network shares stay out of the sidebar;
  - doesn't register on MPRIS, so it doesn't flash up in the desktop's media controls;
  - doesn't save its config on exit.
- **`make screenshot`** builds both, points `HOME` and the XDG config, data and cache folders at
  the temp directory, runs the app and moves the image to `docs/screenshot.png`. The temp
  directory is removed afterwards.

## Consequences

- The screenshot can be regenerated after any UI change, from any machine, without exposing
  anything personal.
- The image captures the app's own frame, including its transparent rounded corners
  ([ADR-0020](0020-draw-own-window-frame.md)).
- The theme follows the desktop's light/dark setting at the time
  ([ADR-0019](0019-follow-desktop-color-scheme.md)). Run it in the mode the docs should show.
- It needs a running graphical session and audio device, so it isn't a CI job.
- The path bar shows the temp directory's path.
- Screenshot mode is a small, env-gated branch in the app that ordinary runs never enter.
