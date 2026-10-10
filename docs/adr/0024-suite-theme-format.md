# 0024. A versioned theme format with shared sections and per-app sections

Date: 2026-10-10

## Status

Proposed. The user asked for a breaking change: window controls themed apart from buttons, names
as generic as possible, and room for several apps in a suite to share themes. The section
layout and key names below are my design.

## Context

The skin format from [ADR-0008](0008-toml-skins.md) grew around UnAmp:

- `[colors]` mixed the base palette with button colours (`button`, `button_hover`,
  `button_active`).
- `[player]` and the top-level `classic` key were UnAmp-only.
- The title bar's window controls ([ADR-0020](0020-draw-own-window-frame.md)) took the button
  colours, so minimise, maximise and close looked like the app's buttons.
- Unknown keys were errors, so a theme with any key an app didn't know would be rejected.
- Nothing said which version of the format a file was written in.

Photograph is getting the same window frame, and more apps may follow. A theme should be one
file that every app can read, each taking the parts it understands.

## Decision

- **A `format` key versions the file**, starting at `1`. A file without it is the old format and
  is rejected with directions to the upgrade table in `docs/skinning.md`.
- **Shared sections, meaning the same in every app:**
  - top level: `name`, `author`, `base`, `corner_radius`, `shadows`;
  - `[colors]`, the base palette: `background`, `surface`, `surface_alt` (was `stripe`),
    `border`, `text`, `text_strong`, `text_weak`, `accent`, `accent_text`, `link`, `warning`,
    `error`;
  - `[controls]`, the widgets: `background`, `hover`, `pressed`, `text`, `text_hover`, `border`;
  - `[window]`, the app's own frame: `title_bar`, `title_text`, `border`, `corner_radius`;
  - `[window.controls]`, minimise/maximise/close: `icon`, `icon_hover`, `hover`, `pressed`,
    `close_hover`, `close_icon_hover`. These are themed apart from `[controls]` but fall back to
    them, and to GNOME's red close button.
- **Generic names:** keys describe a role (`surface`, `pressed`, `title_bar`), not a widget or an
  app feature. The same `hover`/`pressed` names are used in both control sections.
- **App sections:** each app's own parts go under `[app.<name>]`. UnAmp's is `[app.unamp]`, with
  `classic`, `display`, `time`, `title`, `spectrum` and `spectrum_peak`. The three spectrum stops
  became one `spectrum` list.
- **Forward-compatible reading:**
  - Unknown keys are ignored and listed in the Skins menu (via `serde_ignored`), so typos are
    still caught but never fatal.
  - Other apps' `[app.*]` sections are skipped silently.
  - A file with a higher `format` loads with a warning that newer parts are left out.
  - Bad values, such as a malformed colour, are still errors.
- **Migration:** `.wsz` conversions in the old format (recognised by the converter's header line)
  are redone automatically, with the old file kept as `.toml.v0`. Hand-written skins are upgraded
  by hand, using the old-to-new table in `docs/skinning.md`.

## Consequences

- Old skin files stop loading until upgraded. The Skins menu says why, and converted skins
  upgrade themselves.
- Window controls can be styled like a title bar's (quiet until hovered) without changing every
  button.
- A theme file can be dropped into another app of the suite. The shared look carries over and the
  app-specific parts wait for the app that knows them. Photograph can adopt the format by reading
  the shared sections and adding `[app.photograph]`.
- Typos are warnings rather than errors, so a misspelt key shows up in the menu while the rest of
  the skin applies.
- `serde_ignored` is a new dependency.
- Where themes live is still per app (`~/.config/unamp/skins/`). A shared folder for the suite is
  left for when there's a second app reading the format.
