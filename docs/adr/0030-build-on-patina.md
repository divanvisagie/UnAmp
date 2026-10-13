# 0030. Build on Patina: its frame, appearance watcher, skins and folder tree

Date: 2026-10-11

## Status

The skins folder is superseded by [ADR-0031](0031-own-skins-folder-again.md). The rest stands.

Accepted: the user asked for UnAmp to be ported to Patina. The File/View/Windows/Skins menu
layout follows Patina's ADR-0006, which the user set.

Supersedes:

- the parts of [ADR-0008](0008-toml-skins.md), [ADR-0011](0011-built-in-skins-compiled-in-copy-out.md)
  and [ADR-0024](0024-suite-theme-format.md) that kept the skin loader and the built-in skin files
  in this repo;
- the parts of [ADR-0014](0014-lazy-folder-tree.md), [ADR-0019](0019-follow-desktop-color-scheme.md)
  and [ADR-0020](0020-draw-own-window-frame.md) that kept the folder tree, the portal watcher and
  the frame here.

What those ADRs decided still stands.

## Context

UnAmp's frame, appearance watcher, folder tree and skin loader were moved into
[Patina](https://github.com/Sector-F-Labs/patina) (`patina-ui`), the suite's shared crate, when it
was made. Photograph, Notebook and Calendar now use Patina's, and Patina has gained fixes UnAmp's
copies don't have:

- title-bar buttons that no longer shift on hover;
- a working second window drag;
- title-bar controls that read on a coloured bar.

Patina also has more built-in skins. UnAmp still carried its own copies.

## Decision

- **Frame:** `patina::frame` replaces `src/frame.rs`. That means `viewport`, `header_frame`,
  `TitleBar` with `.menu_bar(true)`, `edges`, `clear_color` and `release_grab`.
- **Appearance:** `patina::appearance` replaces `src/appearance.rs`.
- **Folder tree:** `patina::tree::FolderTree` replaces `src/tree.rs`. It's the same code, moved.
- **Skins:**
  - **Loading:** `patina::skin` reads and applies the format. `src/skin.rs` keeps only UnAmp's
    part: the `[app.unamp]` section (`Skin::app_section`), the `Palette` UnAmp paints with, and
    the `.wsz` conversion.
  - **Built-ins:** Patina's (Default, Calendar, Dark Orange, Notebook, Steam Classic) replace
    UnAmp's `skins/`.
  - **The old Photograph skin** is Patina's Dark Orange, with the same `[app.unamp]` section. A
    saved `skin = "Photograph"` is read as Dark Orange.
  - **Folders:** skins load from the shared `~/.config/patina/skins/` and from UnAmp's own
    `~/.config/unamp/skins/`. `.wsz` files in either are converted.
  - **Skins menu:** Copy built-in skins and Open skins folder now use the shared folder.
  - UnAmp still starts in Default.
- **Menus:** the header becomes File (Quit, Ctrl+Q), View (System Title Bar, moved from Windows),
  Windows, and Skins. The right side stays empty: UnAmp is for listening, so it has no output
  actions (Patina's ADR-0006).

## Consequences

- Frame, tree and skin fixes come from Patina releases, and UnAmp must stay on Patina's egui
  version.
- A skin written for any Patina app works in UnAmp, and UnAmp's `.wsz`-converted skins in the
  shared folder recolour the other apps too. They ignore `[app.unamp]`.
- Patina's known issues apply here too: windows can stop redrawing on GNOME Shell until
  minimised. See Patina's `docs/known-issues.md`.
- `cargo clippy -D warnings` already failed on this code with the current clippy (17 errors, now
  15, none in the ported parts). The Makefile has no `check` target that would enforce it.
