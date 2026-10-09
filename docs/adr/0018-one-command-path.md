# 0018. Route every front end through one command type and shared display rules

Date: 2026-10-10

## Status

Accepted

## Context

UnAmp has three front ends driving one player: the regular egui windows, the classic-skin windows
([ADR-0010](0010-classic-wsz-renderer.md)), and remote control over MPRIS
([ADR-0016](0016-mpris-media-controls.md)), plus keyboard shortcuts. Each turned input into player
changes its own way: the egui windows called the engine inline, the classic windows emitted their
own `Action` enum that `app.rs` mapped, and MPRIS had a third mapping. Display rules (the time
display and its paused blink, seek-bar dragging, EQ value mapping, the volume shown while muted)
were also written separately for each window type.

That caused real drift. Adding mute meant making "changing the volume unmutes" happen in three
places, and new features such as drag-to-reorder landed in the egui playlist but not the classic
one. Asked what the classic mode was costing, the answer was about 150 lines of repeated logic and
growing feature gaps. The user asked to bring the two closer together without losing
functionality.

## Decision

- **One `Command` type** (`src/command.rs`) for everything a user or remote control can ask:
  transport, seeking, volume and mute, shuffle and repeat, EQ, window visibility, playlist actions.
  **One `apply()`** in the app carries them out. Egui buttons, keyboard shortcuts, the Windows
  menu, classic sprites and the waveform all emit commands; the classic `Action` enum is gone.
- **MPRIS translation** is a pure function (`mpris::to_player_command`) that applies the
  specification's state-dependent rules (Play does nothing while playing, Pause only pauses,
  seeking past the end skips, stale SetPosition is ignored) and returns a `Command`. It's unit
  tested without audio or D-Bus.
- **One `PlayerStatus` snapshot** (state, position, length, volume as heard, shuffle, repeat,
  time mode) is built once and used by the classic windows and MPRIS.
- **Shared display rules** in `src/display.rs` (time display, seek dragging and handle position)
  and `src/eq.rs` (dB ↔ slider level, graph frequency scale), used by both renderers.
- **Drag-to-reorder** is a shared helper (`src/reorder.rs`) working from row rectangles, so the
  classic playlist reorders through the same code as the egui playlist and queue.

The two renderers stay separate: they draw genuinely different things.

## Consequences

- Behaviour lives in one place: a rule like "changing the volume unmutes" is written once, in
  `apply()`, and every front end follows it. The classic playlist gained drag-to-reorder.
- Behaviour is testable without UI: commands, the MPRIS translation and the display rules have
  unit tests. Before the refactor, classic mode's behaviour was only covered by screenshots.
- Total code size is roughly unchanged. Repetition was removed, but the shared pieces are now
  separate, documented modules, and classic mode gained a feature.
- A new feature still needs drawing in both renderers if it has a visual part, but its behaviour
  is added once, as a command.
