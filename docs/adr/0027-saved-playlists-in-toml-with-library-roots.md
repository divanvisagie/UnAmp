# 0027. Save playlists as TOML in a marked library root, finding moved tracks by path then tags

Date: 2026-10-10

## Status

Proposed

## Context

UnAmp keeps one session playlist and queue ([ADR-0013](0013-save-session-as-m3u.md)) but can't
save named playlists. The request is to let people save playlists, which raises what happens when
files in the library are moved or renamed after a playlist was saved.

The motivating setup is a Samba share (`Music` on urd) read by several machines, all running
UnAmp. The same library has a different absolute path on each: `/home/divan/Music` on urd,
`/run/user/1000/gvfs/smb-share:server=urd.local,share=music` through GVfs (which lowercases share
names), `/Volumes/Music` on a Mac. Playlists should live on the share so every machine sees them,
and should work from every machine.

Decisions involved:

- **Format.** ADR-0013 chose extended M3U for the session because other players read it. M3U holds
  only a path and an `#EXTINF` title and length per entry, so a moved file is simply lost. In
  practice nobody we know moves UnAmp's playlists into other players, so interoperability isn't
  worth that. TOML (as for skins and [EQ profiles](0026-eq-profiles-in-eq-toml.md)) can keep
  enough per track to find it again. An M3U exporter can be added if someone needs one.
- **What identifies a track.** Paths break on remounts, moves and renames. Tags survive moves and
  renames. MusicBrainz recording IDs survive retagging too, but only in libraries tagged with
  Picard or beets. A sample of 120 files from the urd share had artist and title on all 120, album
  on 116, a MusicBrainz track/recording ID on 13, and an AcoustID on none.
- **Acoustic fingerprints** (Chromaprint/AcoustID) would identify a file even after renaming,
  retagging and re-encoding, but matching means decoding about two minutes of audio per
  candidate file over the network. That only works with a stored fingerprint index, which is the
  library database [ADR-0003](0003-browse-folders-no-library-database.md) avoids. Ruled out.
- **Where playlists live.** XDG has no playlist directory. `$XDG_MUSIC_DIR/Playlists` is where M3U
  files go for other software. `$XDG_DATA_HOME/unamp/playlists` suits files only UnAmp reads, but
  is per machine, not shared. Neither gives a playlist on the share that every machine can open.
- **Finding the library root.** Matching a folder named "Music" was considered and ruled out: the
  case differs between machines (GVfs's `music`), it would also match the local `~/Music`, and a
  mount can be named anything. A marker in the root works wherever and however it's mounted.
- **Where the session belongs.** ADR-0013 put the session in `$XDG_DATA_HOME`. The XDG base
  directory spec has since added `$XDG_STATE_HOME` for state that should survive restarts but
  isn't important or portable enough to be data, such as the current state of the application.
  Keeping the session there means clearing it can't touch saved playlists.
- **Missing tracks.** Showing a missing track greyed out, marked "Not found", with a Remove
  button, was the starting point. Searching before giving up is better when it can be done without
  freezing the UI or scanning at startup.

## Decision

We will save playlists as TOML, inside the library they belong to when there is one:

- **Library roots.** "Use as library" on a folder creates `.unamp/` in it. Any folder containing
  `.unamp/` is a library root on every machine that mounts it. UnAmp checks the roots of known
  locations for it on a worker thread, never at the point of drawing.
- **Where playlists live.** `<library root>/.unamp/playlists/<name>.toml`, with track paths relative
  to the root, so each machine adds its own mount point. Playlists for tracks outside any library go
  in `$XDG_DATA_HOME/unamp/playlists/` (`~/.local/share/unamp/playlists/`) with absolute paths.
- **File format.** One `[[track]]` table per entry, in order. Only `path` is required:

  ```toml
  name = "Sunday Morning"
  created = 2026-10-10T09:30:00Z

  [[track]]
  path = "Nils Frahm/Spaces/03 Says.flac"
  title = "Says"
  artist = "Nils Frahm"
  album = "Spaces"
  duration = 498.2
  mbid = "…"  # MusicBrainz recording ID, when the file has one
  ```

  Files are written to a temporary file and renamed into place.
- **Finding a track.** Paths aren't checked at load. A track is looked for when its playlist is
  opened or it comes up to play, on a worker thread:
  1. its saved path;
  2. the same path ending (`Album/03 Track.flac`) under the library root, which catches remounts
     and moved folders without reading tags;
  3. one walk of the library root per playlist (reusing [ADR-0015](0015-search-by-walking.md)'s
     walker), matching `mbid` when both sides have one, otherwise artist and title with the
     duration within two seconds, preferring the same album;
  4. otherwise the track is shown greyed out, marked "Not found", with a Remove button.

  Several equally good matches count as not found, so UnAmp never silently guesses. A track
  found somewhere new has its `path` updated and the playlist saved, so each move is searched
  for once.
- **The session.** The current playlist and queue use the same format and lookup, as
  `$XDG_STATE_HOME/unamp/session.toml` (`~/.local/state/unamp/`), falling back to the data
  directory where there is no state directory (`dirs::state_dir()` is `None` on macOS). On first
  run, an existing `playlist.m3u8` and `queue.m3u8` are imported. If accepted, this supersedes
  ADR-0013's format and location; its save-after-every-change rule stays.

## Consequences

- Playlists on a shared library work from every machine that mounts it, wherever it's mounted,
  and survive remounts, moved folders, and renamed files with good tags.
- Saved playlists and the session no longer open in other players. Getting that back means writing
  an M3U exporter, which could write root-relative `.m3u8` files that Navidrome on urd would pick
  up.
- UnAmp now writes a hidden `.unamp/` folder into the user's library. Other tools ignore it, but it
  is UnAmp's first write outside its own directories.
- Two machines editing the same playlist at once: the last save wins. The rename keeps the file
  whole, but one machine's edit can be lost.
- Searching for a track walks the whole library over the network: slow on a big share, though at
  most once per playlist open and only when step 2 failed.
- Matching on artist, title and duration can pick the wrong file if the same recording exists twice
  (a remaster, a compilation); preferring the same album and refusing ties limits that. A file that
  was moved and retagged can't be found.
- AcoustID tags are ignored. Fingerprinting is the thing to revisit if UnAmp ever gets a library
  index.
- The session changes format and location, needing a one-off import and a code path for it until
  it can be dropped.
