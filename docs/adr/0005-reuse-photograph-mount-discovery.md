# 0005. Copy Photograph's mount discovery rather than sharing a crate

Date: 2026-10-09

## Status

Proposed

## Context

Photograph's `locations.rs` already discovers storage and network mounts for its sidebar —
`/proc/self/mounts` parsing, filtering of system trees, collapsing of nested ZFS datasets, and GVfs
`smb://`/`sftp://` shares — without touching the mounts themselves, so a hung share can't block
the UI ([Photograph ADR-0017](https://github.com/divanvisagie/Photograph/blob/master/docs/adr/0017-discover-mounts-for-sidebar.md)).
UnAmp needs exactly the same thing.

Options: copy the module, pull it out into a shared crate that both apps depend on, or write it
again from scratch. A shared crate means publishing and versioning it (or a path/git dependency
across two repos) for about 300 lines that rarely change.

## Decision

We will copy `locations.rs` from Photograph into UnAmp with its tests, changing only
comments and test fixtures that mentioned photos.

## Consequences

- No cross-repo dependency to manage; each app can change its copy freely.
- Fixes must be ported between the two by hand, and the copies can drift apart.
- If a third app needs it, or the copies start to diverge in ways that matter, that's the point to
  extract a crate and supersede this ADR.
