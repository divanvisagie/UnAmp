# 0004. Keep track I/O off the UI thread, with rodio/symphonia for playback and lofty for tags

Date: 2026-10-09

## Status

Proposed

## Context

Playing from network mounts means any file access can take hundreds of milliseconds, or hang
if the share drops. Three things touch files: opening and probing a track to play it, reading tags
for the folder list, and finding album art (embedded, or a `cover.jpg`-style file beside it).

For audio the options were rodio (on cpal/ALSA, decoding with symphonia — pure Rust), GStreamer
bindings, or libmpv. rodio keeps the build pure-Rust apart from ALSA, its `Player` already gives
pause/seek/volume/position, and a custom `Source` wrapper can tap samples for the visualizer.
Symphonia's limits come with it: no Opus, and WMA/APE aren't supported. GStreamer/mpv would play
nearly anything, but they're large system dependencies and harder to tap for visualization.

For tags, symphonia exposes metadata but unevenly across formats; lofty reads ID3, Vorbis
comments, MP4 atoms and APE uniformly, including embedded pictures and stream properties.

## Decision

We will use rodio 0.22 (with `symphonia-all`) for decoding and output, and lofty for tags and
embedded art. The UI thread never opens a music file:

- Playback opens and probes the decoder on a worker thread, then swaps it into the `Player`. A
  generation counter discards loads that were overtaken by a newer click.
- Each folder scan starts one worker that reads tags sequentially, top to bottom — gentle on a
  NAS — with results tagged by scan generation so a stale folder's results are dropped.
- The now-playing track's tags and album art are read on their own worker.
- Folder listing itself stays synchronous (one `read_dir`, using `d_type` instead of a `stat` per
  entry), as in Photograph.

## Consequences

- A slow share delays the title and art, not the window; a hung share leaves a worker blocked
  rather than the app frozen.
- Formats are those symphonia decodes: MP3, FLAC, Vorbis, WAV, AAC/ALAC in MP4, AIFF, CAF and
  MKA. Opus is not supported and isn't listed; adding it means a different decoder.
- Directory listing is still on the UI thread, so a hung mount can still freeze the app while you
  navigate into it. Moving that to a worker is the next step if it causes trouble.
- The visualizer taps decoded samples before they reach the device buffer, so it runs a few tens
  of milliseconds ahead of what you hear.
