# 0013. Save the playlist and queue as extended M3U files, after every change

Date: 2026-10-09

## Status

Proposed

## Context

The playlist and the up-next queue ([ADR-0012](0012-up-next-queue.md)) were lost on every
restart. The request was to save them.

Decisions involved:

- **Format.** A custom TOML or JSON file could hold everything, including where playback was. But
  M3U is what Winamp and nearly every other player read and write, so a standard file makes the
  playlist useful outside UnAmp too. Extended M3U's `#EXTINF:<seconds>,<title>` also carries
  title and length, so a restored list displays properly without re-reading tags, which over a
  network mount at startup would be slow or could hang ([ADR-0004](0004-file-io-off-ui-thread.md)).
- **Where you were.** M3U has no "current entry" field. Comment lines are ignored by other
  players, so UnAmp marks the current playlist entry with `#UNAMP-CURRENT` and a queued track that
  was playing with `#UNAMP-PLAYING`.
- **Location.** This is session state, not settings, so it goes in the data directory
  (`~/.local/share/unamp/`, where eframe keeps window state) rather than `config.toml`.
- **When.** Saving only on exit loses everything on a crash or kill. Saving on every change is
  cheap for playlist-sized files.

## Decision

We will keep `playlist.m3u8` and `queue.m3u8` in `~/.local/share/unamp/` (`src/session.rs`):

- `Playlist`'s fields are private and every change bumps a revision counter. After each frame,
  if the revision moved, both files are rewritten: to a temporary file, then renamed, so a crash
  can't leave a truncated playlist. They are written once more on exit.
- On startup both files are loaded. `#EXTINF` titles and durations become provisional track info,
  and paths aren't checked. A missing file stays in the list and is skipped when it comes up
  (playback reports the error and moves on). Relative paths, `CRLF` line endings and other
  players' comments are accepted.
- Shuffle history isn't saved, and playback restarts at the beginning of the current track.
- A failed save shows in the status bar and is retried on the next change.

## Consequences

- The playlist and queue survive restarts, crashes included, and the files open in other players.
- Paths are written as UTF-8; a path that isn't valid UTF-8 is written lossily and won't resolve
  on reload, and a path containing a newline is left out. Both are rare on Linux music libraries.
- Previous can't go back past a restart, and there's no resume-at-position yet.
- Saving on every change rewrites the files once per track change or edit. Fine at playlist sizes;
  a library-sized playlist would want debouncing.
- Opening, saving and switching between multiple named playlists (Winamp's "Load list" and "Save
  list") is a separate feature that would build on the same M3U code.
