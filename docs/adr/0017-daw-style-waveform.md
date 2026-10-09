# 0017. Draw a DAW-style whole-track waveform by decoding the track a second time

Date: 2026-10-09

## Status

Proposed

## Context

The request was a waveform window, off by default. A live oscilloscope of the playing audio was
built first (it reads the existing sample tap and costs nothing extra), but the intent was "what
you see in a DAW": the whole track laid out left to right, levels per channel, a playhead, and
seeking by clicking the waveform.

That needs the entire track's samples, not just what's playing now, so the track has to be decoded
from start to finish independently of playback. That's an extra full read of the file, which for
music on a NAS is extra network traffic.

## Decision

We will draw the whole-track waveform (`src/waveform.rs`) and drop the oscilloscope:

- When the window is open and a track plays, a worker thread decodes it with rodio and stores one
  min/max/RMS summary per 512 frames (about 12 ms) for each of up to two channels, roughly
  600 KB for a five-minute track. Summaries are published in batches, so the waveform fills in left
  to right while decoding runs faster than real time.
- Starting another track cancels an unfinished decode. The last 8 finished waveforms are kept in
  memory, so going back to a recent track doesn't decode again.
- Each pixel column merges the blocks under it. Channels get their own lanes, each mirrored around
  its centre line: a dimmer peak envelope with a brighter RMS body. Left of the playhead is drawn
  in the played colour, right of it in the unplayed colour. The time scale comes from the track
  length, so the playhead is right even before decoding finishes.
- Click, or drag and release, to seek; hovering shows the time.
- Colours come from the skin: a classic skin's playlist colours (current, normal, background),
  else the accent on the inset background.
- Nothing is decoded while the window is closed (it's off by default).

## Consequences

- A familiar, information-dense view of the track: loud and quiet sections, transients, stereo
  balance, and seeking by sight.
- Each new track is read twice while the window is open, once to play and once to draw. Decoding
  is quick, but on a slow share the waveform can lag and adds network load.
- The cache is memory only, so waveforms are recomputed after a restart. A disk cache (keyed by
  path, size and modification time) would avoid that if it matters.
- No zoom or scrolling yet; the whole track always fits the window width.
