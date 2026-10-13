# 0031. Skins only from UnAmp's own folder, `~/.config/unamp/skins`

Date: 2026-10-13

## Status

Accepted. The user asked for it: "each app should look at their own, not the patina folder, thats
the whole point". Supersedes the folder part of [ADR-0030](0030-build-on-patina.md), and follows
Patina's ADR-0011.

## Context

ADR-0030 made UnAmp load skins from Patina's shared `~/.config/patina/skins/` as well as its own
folder, and pointed Copy built-in skins and Open skins folder at the shared one. The user's skins
had all stayed in `~/.config/unamp/skins/`, so the menu opened an empty folder.

## Decision

- UnAmp loads skins, and converts `.wsz` files, only from `~/.config/unamp/skins/`
  (`patina::skin::app_dir("unamp")`).
- Copy built-in skins and Open skins folder use that folder again.

## Consequences

- Open skins folder shows the skins UnAmp lists.
- A skin from another Patina app has to be copied in to use it here.
