# 0012. Add an "up next" queue that plays before the playlist continues

Date: 2026-10-09

## Status

Proposed

## Context

The playlist was the only notion of "what plays next": an ordered list moved through with
shuffle and repeat. The first functionality request after skins was queue logic: **play next**
and **add to queue**. The old "Enqueue" menu item only appended to the playlist, which is what
most players call "add to playlist".

Two established models:

- **Winamp's queue** marks playlist entries with a play order (`[1]`, `[2]`). Queued items must
  already be in the playlist, so queuing a track from the library means adding it to the playlist
  first.
- **The streaming-app model** (Spotify, Apple Music, YouTube Music) keeps a separate "up next"
  queue in front of whatever you're playing through. It can hold any track, and the playlist picks
  up where it left off once the queue is empty.

## Decision

We will keep the playlist as it is and add a separate up-next queue (`Playlist::queue`):

- **Play next** puts a track at the front of the queue; **Add to queue** at the back. Both are in
  the Media Library's and the Playlist's context menus, plus "Add folder → to queue". The old
  "Enqueue" becomes **Add to playlist** / "Add folder → to playlist".
- When a track ends, or Next is pressed, the queue plays first. While it plays, the playlist's
  position stays on the track that was playing, so the playlist resumes right after it.
  Shuffle and repeat apply to the playlist only; Repeat One repeats whatever is playing.
- **Previous** retraces everything actually played, queued tracks included, without putting
  them back in the queue.
- Starting a new playlist ("Play folder") or clearing it keeps the queue.
- Queued playlist entries show Winamp-style `[n]` markers in both the egui and classic
  playlists. The egui Playlist window lists the queue above the playlist (play now, move to
  front, remove, clear).

## Consequences

- Queue anything from anywhere, in the order most people now expect, while still showing
  Winamp's `[n]` markers for familiarity.
- Two lists to understand instead of one. "Add to queue" and "Add to playlist" read similarly,
  so the menus always offer them side by side with distinct labels.
- The classic Playlist window has no place for the queue itself. It shows markers on playlist
  entries, but tracks queued from the library that aren't in the playlist only appear once they
  play.
- Neither the queue nor the playlist is saved across restarts yet; saving both (e.g. as `.m3u`)
  is the natural next step.
