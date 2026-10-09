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
- Media Library: folder browser whose sidebar lists mounted drives, ZFS pools and network shares (NFS/SMB/sshfs/GVfs)
- Plays MP3, FLAC, Ogg Vorbis, WAV, AAC/ALAC (`.m4a`), AIFF, CAF and MKA via symphonia
- Reads tags (title, artist, album, track number, duration, bitrate) for the folder you're viewing
- Album art from embedded covers, or `cover`/`folder`/`front`/`album` images beside the tracks
- Playlist: double-click a track in the library to play its folder from there, or enqueue
  tracks/folders (right-click)
- Shuffle, repeat (off / all / one), seek, volume
- Spectrum analyzer with falling peak caps; elapsed/remaining time (click the time to toggle)
- Network-safe: files are opened, probed and tag-read on worker threads, never on the UI thread
  ([ADR-0004](docs/adr/0004-file-io-off-ui-thread.md))

Not yet: Opus, recursive folder play, saved playlists (`.m3u`), custom EQ presets, MPRIS/media keys.

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

## Skins

UnAmp starts with egui's stock look. Choose another from **Skins** in the menu bar.

A skin is one TOML file. Copy [`skins/steam-classic.toml`](skins/steam-classic.toml), which
documents every key, into `~/.config/unamp/skins/`, change its `name` and colours, then use
**Skins → Reload skins**. Any colour you leave out keeps egui's default. Typos and bad colours are
reported in the Skins menu.

### Classic Winamp skins

Got an old `.wsz`? Drop it into `~/.config/unamp/skins/` and choose **Skins → Reload skins**.
UnAmp converts it to a `.toml` of the same name and lists it in the menu. Pick it, and the Player,
Equalizer and Playlist are drawn from the skin's own bitmaps, at Winamp's layout and double size:
title bars, sprite buttons, LED digits, pixel-font ticker, sliders and all
([ADR-0010](docs/adr/0010-classic-wsz-renderer.md)). The Media Library and menus take on the skin's
colours.

The `.toml` is yours to tweak and won't be regenerated unless you delete it. Remove its
`classic = …` line to keep the skin's colours on UnAmp's regular windows instead.

Find thousands of skins at the [Winamp Skin Museum](https://skins.webamp.org).

```toml
name = "My Skin"
base = "dark"          # egui theme to start from: "dark" or "light"
corner_radius = 0
shadows = false

[colors]
background = "#4C5844"
accent = "#958831"

[player]
time = "#C4B550"
```

## Install

### From Source

```bash
sudo apt install -y libasound2-dev pkg-config
cargo run --release
```

### Linux (.deb)

```bash
sudo apt install -y dpkg-dev
make install
```

## Configuration

Settings are saved on exit to `~/.config/unamp/config.toml`: last folder, volume, shuffle,
repeat, time display mode, equalizer settings, skin and which windows are open. Window positions and
sizes are kept by eframe in `~/.local/share/unamp/`.

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
