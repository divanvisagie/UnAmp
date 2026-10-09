# UnAmp

<img src="packaging/linux/unamp.svg" alt="UnAmp logo" width="140" />

It really whips the llama's ass — on Unix. (UnAmp, as in Unix.)

Native Rust/egui music player. Point it at a folder, a mounted drive or a network share and play
what's there: album art, a scrolling title, a big time display, a proper bar spectrum analyzer and
a 10-band equalizer. Like the original, it comes in separate windows — Player, Equalizer,
Playlist and Media Library — that you can open, close and arrange.

## Status

New (v0.1). Linux only (see [ADR-0001](docs/adr/0001-linux-only-egui-native-app.md)).

## What It Does Today

- Floating Player / Equalizer / Playlist / Media Library windows; toggle them from the Windows
  menu or the player's EQ / PL / ML buttons. Layout is remembered; Windows → Reset layout restores it
  ([ADR-0006](docs/adr/0006-floating-egui-windows.md))
- Skins: stock egui by default, or pick **Steam Classic** (the old olive-green Steam client) from
  the Skins menu; write your own in TOML ([ADR-0008](docs/adr/0008-toml-skins.md)), or drop in a
  classic Winamp `.wsz` and get its windows drawn from its own bitmaps
  ([ADR-0010](docs/adr/0010-classic-wsz-renderer.md))
- 10-band graphic equalizer (60 Hz – 16 kHz) with preamp, presets and a live response curve;
  double-click a slider to zero it ([ADR-0007](docs/adr/0007-biquad-equalizer-in-source-chain.md))
- Search (**Ctrl+F**): finds tracks and folders anywhere under the current location by matching
  every word against their paths, streaming results as it goes, no scan or database needed
  ([ADR-0015](docs/adr/0015-search-by-walking.md))
- Media Library: a collapsible folder tree that loads as you expand it, so slow shares never
  freeze the window ([ADR-0014](docs/adr/0014-lazy-folder-tree.md)); the sidebar lists mounted drives, ZFS pools and network shares (NFS/SMB/sshfs/GVfs)
- Plays MP3, FLAC, Ogg Vorbis, WAV, AAC/ALAC (`.m4a`), AIFF, CAF and MKA via symphonia
- Reads tags (title, artist, album, track number, duration, bitrate) for the folder you're viewing
- Album art from embedded covers, or `cover`/`folder`/`front`/`album` images beside the tracks
- Playlist: double-click a track in the library to play its folder from there, or right-click to
  add tracks or whole folders to the playlist
- Up-next queue: right-click any track for **Play next** or **Add to queue**. Queued tracks play
  before the playlist carries on from where it was, show as `[1]`, `[2]` in the playlist (classic
  skins too), and Previous walks back through everything you heard
  ([ADR-0012](docs/adr/0012-up-next-queue.md))
- Media controls (MPRIS): shows up in GNOME's notification-panel mini player with art and
  controls, and works with media keys, headset buttons and `playerctl`. Optional: without a
  D-Bus session it's simply off ([ADR-0016](docs/adr/0016-mpris-media-controls.md))
- The playlist and queue are saved as you go and restored on startup, as standard M3U files other
  players can open ([ADR-0013](docs/adr/0013-save-session-as-m3u.md))
- Shuffle, repeat (off / all / one), seek, volume
- Spectrum analyzer with falling peak caps; elapsed/remaining time (click the time to toggle)
- Network-safe: files are opened, probed and tag-read on worker threads, never on the UI thread
  ([ADR-0004](docs/adr/0004-file-io-off-ui-thread.md))

Not yet: Opus, recursive folder play, named playlists (load/save list), resume-at-position, custom EQ presets.

## Keyboard

The classic bottom row, with nothing focused:

| Key | Action |
|-----|--------|
| `Z` | Previous (restarts the track if more than 3 s in) |
| `X` | Play |
| `C` / `Space` | Pause / resume |
| `V` | Stop |
| `B` | Next |
| `←` / `→` | Seek 5 s |
| `Ctrl+F` | Search the Media Library |

## Skins

UnAmp starts with egui's stock look; pick another from the **Skins** menu.

- **TOML skins** recolour everything. Steam Classic is built in; write your own in
  `~/.config/unamp/skins/`, or use **Skins → Copy built-in skins to folder** for an annotated one to edit.
- **Classic Winamp skins**: drop a `.wsz` in the same folder and reload. The Player, Equalizer
  and Playlist are drawn from the skin's own bitmaps, the way Winamp drew them. Find thousands at
  the [Winamp Skin Museum](https://skins.webamp.org).

The full guide, covering every TOML key, what a `.wsz` turns into, and what classic mode does and
doesn't draw, is in [docs/skinning.md](docs/skinning.md).

## Install

### From Source

```bash
sudo apt install -y libasound2-dev pkg-config
cargo run --release
```

Running from source? `make install-desktop` registers a launcher and icon for your checkout in
`~/.local/share` (undo with `make uninstall-desktop`), so GNOME's dock and media controls show
UnAmp's name and icon.

### Linux (.deb)

```bash
sudo apt install -y dpkg-dev
make install
```

## Configuration

Settings are saved on exit to `~/.config/unamp/config.toml`: last folder, volume, shuffle,
repeat, time display mode, equalizer settings, skin and which windows are open. Window positions and
sizes are kept by eframe in `~/.local/share/unamp/`, next to `playlist.m3u8` and `queue.m3u8`.
Those are ordinary extended M3U playlists; UnAmp marks where you were with `#UNAMP-CURRENT` and
`#UNAMP-PLAYING` comment lines, which other players ignore.

## Development

```bash
cargo test
make dev     # live reload, requires cargo-watch
make help    # all targets
```

Significant decisions are recorded as ADRs in [`docs/adr/`](docs/adr/README.md).

UnAmp was built with Claude Code from ten short prompts, listed word for word in
[`docs/prompts.txt`](docs/prompts.txt).

## License

GPL-2.0-only. Not affiliated with Winamp or Llama Group.
