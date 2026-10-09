# 0011. Keep built-in skins compiled in, with a non-overwriting "copy to folder" for editing

Date: 2026-10-09

## Status

Accepted

## Context

Built-in skins (today only Steam Classic) are TOML files in the repo's `skins/` folder, compiled
into the binary ([ADR-0008](0008-toml-skins.md)). Users looking in `~/.config/unamp/skins/` don't
find them there, which came up as "how come the Steam skin isn't a TOML in the skins folder?"

Options considered:

- **Write built-ins into the user folder on first run.** Easy to find, but the copies go stale
  when a newer UnAmp improves a built-in, and UnAmp can't safely refresh them because the user may
  have edited them. It also turns every install's folder into a mix of shipped and personal files.
- **Leave them compiled in and document `cp` from the repo.** Correct, but needs a checkout of the
  source and knowing where to look.
- **Compiled in, plus a menu item that copies them out on request.** The built-ins stay the
  always-working fallback, and an editable copy is one click away.

## Decision

We will keep built-in skins compiled in, and add **Skins → Copy built-in skins to folder**
(`skin::export_built_ins`), which writes each built-in's TOML into the user skins folder (creating
it if needed) and then reloads skins:

- It never overwrites: a file with the same name is left as is and reported as kept.
- A user skin with a built-in's `name` already replaces the built-in (ADR-0008), so after copying,
  the copy is the version in use. Deleting it falls back to the built-in.
- The result ("Copied …", "kept your existing …") is shown in the Skins menu.

## Consequences

- An editable, fully commented skin is one click away, with no source checkout needed.
- Once copied, a built-in no longer gets updates from new UnAmp versions until the user deletes
  their copy. That's the price of not overwriting edits, and it's the user's explicit action.
- Two files can now share a skin name (built-in and copy), so the Skins menu shows the user copy's
  path on hover to make clear which is active.
