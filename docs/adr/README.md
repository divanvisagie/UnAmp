# Architecture Decision Records

This directory holds ADRs for UnAmp — short records of significant architecture decisions,
written at the time they're made, following Michael Nygard's format as popularized by Martin
Fowler: <https://martinfowler.com/bliki/ArchitectureDecisionRecord.html>.

## When to write one

Write an ADR when a decision is:

- Hard, or expensive, to reverse once other code depends on it.
- Not obvious from reading the code — a "why", not a "what".
- The kind of thing that'll get re-litigated later without a record ("didn't we already decide
  this?").

Small, reversible choices don't need one. Rule of thumb: if you'd want to explain the choice to a
new contributor in a paragraph beyond what the code itself shows, it's ADR-sized.

An ADR is a point-in-time record of *a decision as it was made*, including the options considered
and rejected. It does not get rewritten later — a decision that reverses or replaces an earlier one
gets its own new ADR that supersedes the old one, which stays in place with its status updated.

## Convention

- One file per decision: `NNNN-short-kebab-title.md`, numbered sequentially, zero-padded to 4
  digits. Numbers are never reused, even for a decision that gets reversed.
- Start from [`0000-template.md`](0000-template.md) — copy it, don't write from scratch.
- `Status` is one of: `Proposed`, `Accepted`, `Rejected`, `Deprecated`, or `Superseded by
  ADR-NNNN` (linked to the superseding record).
- Every new ADR gets a row in the index below.
- Keep records short — a paragraph or two per section is normal. An ADR documents a decision and
  its reasoning, not a design document.

If you're working with Claude Code in this repo, `.claude/skills/adr/SKILL.md` automates the
mechanics of adding one.

## Index

| # | Title | Status |
|---|-------|--------|
| [0001](0001-linux-only-egui-native-app.md) | Build UnAmp as a Linux-only native egui/eframe app, following Photograph | Accepted |
| [0002](0002-stock-egui-style.md) | Use egui's stock style instead of a custom theme | Superseded by [ADR-0008](0008-toml-skins.md) |
| [0003](0003-browse-folders-no-library-database.md) | Browse folders directly instead of scanning into a library database | Proposed |
| [0004](0004-file-io-off-ui-thread.md) | Keep track I/O off the UI thread, with rodio/symphonia for playback and lofty for tags | Proposed |
| [0005](0005-reuse-photograph-mount-discovery.md) | Copy Photograph's mount discovery rather than sharing a crate | Proposed |
| [0006](0006-floating-egui-windows.md) | Use floating egui windows for the player, equalizer, playlist and media library | Accepted |
| [0007](0007-biquad-equalizer-in-source-chain.md) | Implement the equalizer as peaking biquads in a rodio `Source`, ahead of the visualizer tap | Proposed |
| [0008](0008-toml-skins.md) | Support skins as TOML files, keeping stock egui as the default | Accepted (bitmap exclusion superseded by [ADR-0010](0010-classic-wsz-renderer.md)) |
| [0009](0009-convert-winamp-wsz-colours.md) | Convert classic Winamp `.wsz` skins to TOML skins, colours only | Superseded by [ADR-0010](0010-classic-wsz-renderer.md) |
| [0010](0010-classic-wsz-renderer.md) | Render classic `.wsz` skins pixel-for-pixel in fixed-size windows | Accepted |

## Decision Relationship

```mermaid
flowchart TD
    A[0001: Linux-only egui app, like Photograph] --> B[0002: Stock egui style]
    A --> E[0005: Copy Photograph's mount discovery]
    C[0003: Browse folders, no library DB] --> D[0004: Track I/O off the UI thread]
    E --> C
    B --> F[0006: Floating egui windows]
    D --> G[0007: Biquad EQ in the Source chain]
    F --> G
    B --> H[0008: TOML skins]
    H --> I[0009: Convert .wsz colours to TOML]
    I --> J[0010: Classic .wsz renderer]
    F --> J
```

## Revisit Triggers

- A hung network mount freezes the UI while navigating into it — move directory listing to a
  worker ([ADR-0004](0004-file-io-off-ui-thread.md)).
- Opus (or other symphonia-unsupported formats) becomes a real need — revisit the decoder.
- Demand for artist/album browsing or library-wide search — see [ADR-0003](0003-browse-folders-no-library-database.md).
- Multi-monitor use or a mini player needs real OS windows — see [ADR-0006](0006-floating-egui-windows.md).
- Clipping from EQ boosts is audible in practice — add a limiter ([ADR-0007](0007-biquad-equalizer-in-source-chain.md)).
- A third app wants mount discovery, or the two copies diverge — extract a crate ([ADR-0005](0005-reuse-photograph-mount-discovery.md)).
