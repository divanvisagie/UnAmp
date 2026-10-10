# 0029. List album subfolders as sections of the folder view, one level deep

Date: 2026-10-10

## Status

Accepted. Backfilled from commit f21472a. Extends [ADR-0003](0003-browse-folders-no-library-database.md)
("tags are read only for the folder being viewed") to one level of subfolders.

## Context

Following [ADR-0003](0003-browse-folders-no-library-database.md), the track list shows only the
current folder's own files. Commit 01a7930 listed subfolders above them, so an artist folder with
only albums inside wasn't a dead end, but you still had to open each album to see its tracks, and
"Play folder" played only the folder's own tracks.

The request was that browsing to a folder containing several albums should show every song,
grouped by album. Two readings were possible: album subfolders (the usual `Artist/Album/NN
Title.flac` layout), or one flat folder of mixed files grouped by their album tag. The second was
ruled out for now: tags arrive gradually ([ADR-0004](0004-file-io-off-ui-thread.md)), so sections
would reshuffle as they load.

## Decision

- When a folder is opened, the tag worker also lists each subfolder's audio files, one level
  down, before reading any tags. Each subfolder with audio becomes a section: a header with the
  folder name, track count and total length, then its tracks, numbered from 1.
- The folder view is, top to bottom: subfolders with no audio directly inside (still folder rows),
  the folder's own tracks, then each album section.
- Headers use the folder name, not the album tag, so they don't change while tags load.
  Double-clicking a header plays just that album; its context menu adds it to the queue or
  playlist, or opens the folder.
- "Play folder" and double-clicking a track play the whole list, so playback runs on through the
  following albums.
- Only folders with at most 64 subfolders get sections, so opening a collection's root on a share
  doesn't list every artist. Above that, the old folder-row view is kept.
- A scan stops as soon as another folder is opened.

## Consequences

- An artist folder shows everything at once, and albums can be played or queued without opening
  them.
- Opening a folder now costs a directory listing per subfolder (up to 64) on top of its own, which
  is noticeable on a slow share. It runs on the worker, so the UI isn't blocked, but sections
  appear after the listings finish.
- Deeper nesting (`Artist/Album/CD1`) shows only the nearest level: opening the artist shows the
  album as a folder row, and opening the album shows the discs as sections.
- The 64-folder cap is a guess; a large artist folder silently falls back to folder rows.
- A flat folder of mixed albums is still one unsectioned list.
