# 0015. Search the current location by walking its folders, without an index

Date: 2026-10-09

## Status

Proposed

## Context

[ADR-0003](0003-browse-folders-no-library-database.md) chose folder browsing over a library
database and accepted "no library-wide search" as a consequence. The next request was search.

Options considered:

- **Index the library** (scan into SQLite or similar, search tags). Fast, rich queries, but it's
  the database ADR-0003 avoided: a full scan up front, which is slow over SMB/NFS, plus keeping it
  in sync.
- **Search tags without an index.** Reading tags means opening every file, which over a NAS is
  far too slow for interactive search.
- **Walk the folders and match paths.** Directory listings are cheap compared to opening files,
  and music collections are usually organised `Artist/Album/NN Title.ext`, so the path already
  carries artist, album and title.

## Decision

We will search by walking folders (`src/search.rs`):

- The search box at the top of the Media Library (**Ctrl+F**) searches under the tree's root
  ([ADR-0014](0014-lazy-folder-tree.md)), i.e. the current location.
- Every whitespace-separated word must appear, case-insensitively, somewhere in the file's path
  relative to that root, so `daft homework` finds `Daft Punk/Homework/03 Revolution 909.flac`.
- The walk runs on a worker thread, breadth-first so shallow matches come first, and streams
  results into the list as it goes. Typing pauses for 300 ms (or Enter) before searching, and a new
  query or a new root cancels the old walk. It stops at 2,000 tracks or 200 folders and says so.
- Folders whose path completes the match are shown as chips above the results; clicking one opens
  it and ends the search. Results can be played, queued or added to the playlist like any list,
  and "Play results" makes them the playlist.
- Hidden folders and symlinked folders aren't walked (no loops), and from `/`, `/proc`, `/sys`,
  `/dev` and `/run` are skipped.

## Consequences

- Search works anywhere, local or remote, with no setup or scan, and nothing to keep in sync.
- It finds paths, not tags: a file named `track01.mp3` in an untidy folder won't match its artist.
  Results show file names and folders, not tag titles or durations.
- Every search re-walks the location. Fine for a local disk and acceptable on a NAS with results
  streaming in, but slow from `/` or on huge shares. Caching listings shared with the tree, or an
  optional index, would be the next steps.
- ADR-0003's "no library-wide search" no longer holds; the rest of it (no database, folders as the
  organisation) still does.
