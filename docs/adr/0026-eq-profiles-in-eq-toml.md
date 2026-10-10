# 0026. Keep user EQ profiles as named tables in one `eq.toml`, separate from `config.toml`

Date: 2026-10-10

## Status

Proposed

## Context

The equalizer ([ADR-0007](0007-biquad-equalizer-in-source-chain.md)) had eight built-in presets
compiled in (`eq::PRESETS`) and saved only the current setting (`eq_enabled`, `eq_preamp`,
`eq_bands` in `config.toml`). The request was to let people make their own named profiles.

Decisions involved:

- **One file per profile or one file for all.** One file per profile in `~/.config/unamp/eq/`
  would match how user skins live in `skins/`, and makes a profile easy to share by copying a
  file. A single TOML file with one named table per profile is less to manage, still easy to edit
  by hand, and TOML rejects duplicate table names, so the name can be the profile's identity. The
  single file was chosen.
- **Inside `config.toml` or beside it.** `config.toml` is written from memory on exit, so a
  profile edited there by hand while UnAmp runs would be lost on quit. A separate file is only
  written when a profile is saved, renamed or deleted.
- **Winamp preset files.** Importing Winamp's `.eqf` / `winamp.q1` presets was considered and
  ruled out: UnAmp aims to be a modern equivalent of Winamp, not a clone, and classic `.wsz`
  skins ([ADR-0010](0010-classic-wsz-renderer.md)) are where Winamp compatibility stops.
- **Knowing which profile is loaded.** The first prototype stored no name and matched the current
  values against the profiles each frame. Nothing could go stale that way, but two profiles with
  the same values showed whichever sorted first, and moving one slider dropped to "Custom", which
  lost the profile you were tweaking and so the "update it" flow.
- **Preamp.** The built-ins only set the bands. A curve with big boosts needs its own preamp to
  avoid clipping, so user profiles save it too.

## Decision

We will keep user EQ profiles in `~/.config/unamp/eq.toml` (`src/eq_profiles.rs`), one table per
profile, keyed by name:

```toml
["Late Night"]
preamp = -2.0
bands = [3.0, 2.0, 0.0, -1.0, -2.0, -1.0, 0.0, 1.0, 1.0, 0.0]
```

- `preamp` is optional (0 dB). Values outside ±`MAX_DB` are clamped on load.
- The file is read once at startup. A missing file means no profiles; an unreadable or invalid one
  is reported in the status bar and treated as empty. It is rewritten whole after each save,
  rename or delete, and a failed write is reported the same way.
- `config.toml` keeps the live EQ state. Profiles are snapshots loaded into it, and
  `eq_profile` records the name of the built-in or user profile last loaded or saved
  (`SetEqProfile`). The EQ button shows that name, with " *" once the sliders have moved from it
  (bands and preamp for a user profile, bands only for a built-in), or "Custom" when nothing is
  loaded or the name no longer exists. Reset loads Flat with the preamp at 0.
- Renaming the loaded profile renames `eq_profile` with it; deleting it clears `eq_profile`.
- The built-in presets stay compiled in and read-only. Both the regular and the classic
  Equalizer window show one shared presets menu (`eq_profiles::menu`): built-ins, then the user's
  profiles in name order with rename (✏ or double-click) and delete (🗑), then a "Save as…"
  field, prefilled with the loaded user profile's name so updating one is a slider move and Enter.
- Saving, renaming and deleting are `Command`s (`SaveEqProfile`, `RenameEqProfile`,
  `DeleteEqProfile`), applied in one place like every other action
  ([ADR-0018](0018-one-command-path.md)). Saving over an existing name replaces it; renaming onto
  a taken name is refused.

## Consequences

- People can keep and switch between their own curves from either EQ window, and edit or back
  them up as plain TOML.
- Comments and ordering in a hand-edited `eq.toml` are lost the first time UnAmp writes it
  (`toml` without `preserve_order`; `toml_edit` would keep them). Profiles always list
  alphabetically.
- Edits made to `eq.toml` while UnAmp runs aren't picked up until restart, and are overwritten if
  a profile is saved from the app in the meantime.
- The file is written in place, not via a temporary file and rename, so a crash mid-write could
  truncate it. Fine for a file this small for now; [ADR-0013](0013-save-session-as-m3u.md)'s
  write-then-rename is the fix if it bites.
- Sharing one profile means copying a table out of the file rather than sending a file.
- Delete has no confirmation or undo.
- `eq_profile` is a second piece of state to keep in step with `eq.toml`. Renames and deletes made
  in the app keep it in step. A profile renamed or removed by hand just shows as "Custom", and one
  whose values were edited by hand shows " *" until it is loaded again.
- Setting the sliders back to a profile's values by hand doesn't select it unless it is the one
  loaded; an existing config without `eq_profile` shows "Custom" until a profile is picked.
- Picking a built-in preset still leaves the preamp where it was, while a user profile sets it.
- Choosing a profile automatically per audio output (headphones vs. speakers) was discussed as a
  follow-up; the file could grow an `[auto]` table mapping device to profile, which would need
  table names to stay clear of that key.
