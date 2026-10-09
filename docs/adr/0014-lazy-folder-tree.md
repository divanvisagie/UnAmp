# 0014. Show folders as a lazily loaded tree rooted at the current location

Date: 2026-10-09

## Status

Proposed

## Context

The Media Library sidebar listed the current folder's subfolders as a flat list of buttons:
you could only go down one level at a time, and back up with the parent button or path bar. The
feedback was that "our file tree isn't really a tree".

A real tree means listing many folders, which over a network share can be slow or hang
([ADR-0004](0004-file-io-off-ui-thread.md)). Reading the whole hierarchy up front isn't an option
for a NAS-sized collection.

## Decision

The sidebar shows a collapsible tree (`src/tree.rs`) under **FOLDERS**:

- It's rooted at a location: the one you clicked in the sidebar (Home, Music, Computer, a drive,
  a share), or, when you navigate by path bar, the most specific sidebar location containing the
  folder.
- A folder's subfolders are listed on a worker thread the first time it's expanded, then cached.
  Entering a folder re-lists it, so new folders appear. Folders known to have no subfolders get
  no expander; ones not yet read show one until they're opened.
- The current folder is highlighted, and the folders leading to it open automatically and scroll
  into view, including after path-bar navigation or opening a search result's folder.
- Hidden folders are skipped, as elsewhere. Symlinked folders are followed for listing.

## Consequences

- Browsing a deep collection is direct, and a slow share delays one folder's expansion instead
  of freezing the window.
- Expansion state is remembered by egui per folder for the session, not across restarts.
- The cache isn't invalidated by changes on disk except when you enter a folder; a folder added
  elsewhere appears after re-entering its parent.
- Reading the current folder's tracks is still on the UI thread (ADR-0004's open issue); only the
  tree's listings moved off it.
