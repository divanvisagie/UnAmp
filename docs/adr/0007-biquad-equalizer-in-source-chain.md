# 0007. Implement the equalizer as peaking biquads in a rodio `Source`, ahead of the visualizer tap

Date: 2026-10-09

## Status

Proposed

## Context

The Equalizer window needs real DSP. rodio has low/high-pass filters but no peaking or graphic EQ,
and the alternatives (a DSP crate, or routing through PipeWire's filter chain) either bring in
more than ten filters need, or move the EQ out of the app altogether.

Winamp's EQ has ten bands at 60, 170, 310, 600 Hz and 1, 3, 6, 12, 14, 16 kHz, plus a preamp and
an on/off switch. The audio thread can't block on the UI, so settings must reach it without a
lock.

## Decision

We will implement the EQ in `src/eq.rs` as a `Source` wrapper that runs one RBJ-cookbook
peaking biquad per band (Q 1.2, ±12 dB) per channel, in transposed direct form II, with a
preamp gain:

- Settings live in atomics (`EqParams`) with a version counter. The audio thread checks the
  version at frame boundaries and recalculates the filter coefficients only when it changes.
- Flat bands, and bands too close to Nyquist (≥ 0.45 × the sample rate), are skipped.
- The chain is decoder → equalizer → visualizer tap → player, so the spectrum shows the EQ.
- When the EQ is off, samples pass through untouched, preamp included.
- The UI's response curve is computed from the same filter coefficients, so it shows exactly what
  the filters do.

## Consequences

- No extra dependencies; changes take effect within a few milliseconds, with no lock on the
  audio thread.
- Large boosts can push samples past full scale; there's no limiter, so loud presets can clip
  unless the preamp is turned down. A soft limiter after the EQ is the obvious follow-up.
- The EQ is per track. Each new decoder gets fresh filters from the shared settings, so changing
  track resets filter state, which is inaudible at a track boundary.
- Presets are hardcoded approximations of Winamp's built-ins; saving your own presets isn't
  supported yet.
