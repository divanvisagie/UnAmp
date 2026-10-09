# 0003. Browse folders directly instead of scanning into a library database

Date: 2026-10-09

## Status

Proposed

## Context

Music players split into two camps: ones that scan a collection into a database and browse by
artist/album/genre (Rhythmbox, Lollypop, foobar2000's media library), and ones that browse the file
system and play what's in a folder (classic Winamp, Audacious). UnAmp is asked to "point to a
folder or a network mount and play music".

A database needs a full scan up front — slow over SMB/NFS, and a NAS-backed collection can
take minutes — plus change detection, a schema, and a place to store it. Folders are already how
most collections on a NAS are organised (`Artist/Album/NN Title.flac`).

## Decision

We will browse the file system directly: the sidebar lists locations and mounts, the central
panel lists the current folder's playable files, and tags are read only for the folder being
viewed. The playlist is an in-memory queue built from folders (play / enqueue). Nothing is
indexed or cached on disk.

## Consequences

- Opening any folder, local or remote, is immediate; tags fill in as they're read.
- No "all albums by this artist" or library-wide search; organisation is whatever the folders say.
- The playlist is lost on exit (saving it as an `.m3u` would be a natural follow-up).
- "Play this folder recursively" isn't supported yet; it would need a background walk, with the
  same off-UI-thread care as [ADR-0004](0004-file-io-off-ui-thread.md).
- If a library view is wanted later, it should be a separate, opt-in index that supersedes this
  ADR, not a cache bolted onto folder browsing.
