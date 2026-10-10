# Skinning UnAmp

UnAmp has three kinds of skin, all chosen from the **Skins** menu:

| Kind | What it is | What changes |
|---|---|---|
| **Default** | Built in, no file | Nothing: egui's stock look, following your system's light/dark setting |
| **TOML skin** | A `.toml` theme file | Colours, corner rounding and shadows of every window, and of the app's own frame |
| **Classic skin** | A Winamp 2 `.wsz` plus the `.toml` UnAmp makes from it | Player, Equalizer and Playlist drawn from the skin's own bitmaps; everything else in its colours |

Your choice is saved as `skin = "<name>"` in `~/.config/unamp/config.toml`. If that skin
disappears, UnAmp falls back to Default.

## Where skins live

- **Built-in skins** are TOML files in the repo's [`skins/`](../skins) folder, compiled into the
  binary. Today those are [`steam-classic.toml`](../skins/steam-classic.toml), which recreates the
  olive-green Steam client and doubles as the commented reference for the format, and
  [`photograph.toml`](../skins/photograph.toml), the dark greys and Ubuntu orange of Photograph.
- **Your skins** go in `~/.config/unamp/skins/`. UnAmp reads every `*.toml` there, and converts
  every `*.wsz` there (see [Classic skins](#classic-winamp-skins)).

A skin in your folder with the same `name` as a built-in **replaces** it. That's how you
customize Steam Classic: **Skins → Copy built-in skins to folder** writes `steam-classic.toml` into
your folder, and from then on your copy is the one in use. The copy never overwrites an existing
file, so running it again is safe. Delete your copy to go back to the built-in.

### The Skins menu

| Item | Does |
|---|---|
| *(skin names)* | Switch skin. Hover a user skin to see its file path. |
| **Reload skins** | Rescan both folders, convert any new `.wsz`, and re-apply the current skin. Use it after editing. |
| **Copy built-in skins to folder** | Put editable copies of the built-ins in your folder (never overwrites). |
| **Open skins folder** | Open `~/.config/unamp/skins/` in your file manager, creating it if needed. |

Problems loading a skin are listed in red at the bottom of the Skins menu. A bad colour, an
old-format file or a broken `.wsz` means the skin is skipped. An unknown key is listed but the
skin still loads. Nothing fails silently.

## Writing a TOML skin

Skins are written in the **suite theme format**, version 1 ([ADR-0024](adr/0024-suite-theme-format.md)).
It's designed so one file can theme several apps:

- The shared sections are `[colors]`, `[controls]` and `[window]`. They mean the same in every
  app that reads the format.
- Each app's own parts go under `[app.<name>]`, here `[app.unamp]`.
- An app skips other apps' sections, and a file written for a newer version of the format still
  loads.

Start from a copy of `steam-classic.toml` (**Copy built-in skins to folder**), change `name`, and
edit. Every key except `format` and `name` is optional, and **any colour you leave out keeps
egui's default** for the `base` theme. So a skin can be as small as this:

```toml
format = 1
name = "Just Purple"
base = "dark"

[colors]
accent = "#8A2BE2"
```

Colours are `"#RRGGBB"` or `"#RRGGBBAA"`. A key UnAmp doesn't know is ignored but listed in the
Skins menu, so a misspelt `bakground` is still caught. Keys under another app's `[app.*]`
section aren't listed. A bad value, such as `text = "green"`, is an error and the skin is
skipped.

### Top-level keys

| Key | Type | Meaning |
|---|---|---|
| `format` | integer, **required** | Theme format version: `1`. A file without it is the old skin format (see [Upgrading](#upgrading-from-the-old-skin-format)). |
| `name` | string, **required** | Name in the Skins menu. Matching a built-in's name replaces it. |
| `author` | string | Shown next to user skins in the menu. |
| `base` | `"dark"` or `"light"` | The egui theme the skin starts from. A skin fixes the theme; only Default follows the system. |
| `corner_radius` | integer (px) | Rounding of panels, menus, controls and UnAmp's painted boxes. `0` is square. Also the main window's corners, unless `[window] corner_radius` is set. |
| `shadows` | bool | `false` removes window and popup shadows. |

### `[colors]`: the base palette

| Key | Used for |
|---|---|
| `background` | Panels, and the windows inside the app |
| `surface` | Inset areas: text fields, lists, slider tracks, seek bar track, EQ curve background, album-art placeholder |
| `surface_alt` | Alternate rows in lists; EQ curve grid lines |
| `border` | Outlines of windows and panels (and of controls, unless `[controls] border` is set) |
| `text` | Normal text |
| `text_strong` | Headings (and text on hovered controls, unless `[controls] text_hover` is set) |
| `text_weak` | Secondary labels (section headings, counts, status) |
| `accent` | Selection, slider fill, toggles that are on, the seek bar's played part, the EQ curve |
| `accent_text` | Text drawn on top of `accent` (e.g. a selected track) |
| `link` | Links, and the current item in lists (the now-playing track) |
| `warning` | Warnings |
| `error` | Error messages |

### `[controls]`: buttons, sliders, checkboxes, drop-downs

| Key | Used for |
|---|---|
| `background` | A control at rest |
| `hover` | Under the mouse |
| `pressed` | While pressed |
| `text` | Text and icons on controls (defaults to `colors.text`) |
| `text_hover` | Text on hovered or pressed controls (defaults to `colors.text_strong`) |
| `border` | Control outlines (defaults to `colors.border`) |

egui draws buttons with one fill and slider/checkbox tracks with another. Tracks at rest use
`colors.surface`, so they stay visible when `controls.background` matches `colors.background`.

### `[window]`: the app's own frame

UnAmp draws its own title bar and window edge ([ADR-0020](adr/0020-draw-own-window-frame.md)).

| Key | Used for |
|---|---|
| `title_bar` | The title bar's background (defaults to `colors.background`) |
| `title_text` | The window title (defaults to `colors.text`) |
| `border` | The one-pixel window edge (defaults to `colors.border`) |
| `corner_radius` | The window's corners, in px. Defaults to the top-level `corner_radius`, else GNOME's 15. Square while maximised. |

### `[window.controls]`: minimise, maximise, close

These are themed separately from `[controls]`, so the window buttons can look like a title bar's
and not like the app's buttons. Each falls back to the matching control colour.

| Key | Used for |
|---|---|
| `icon` | The symbols at rest |
| `icon_hover` | The symbols under the mouse |
| `hover` | Background under the mouse |
| `pressed` | Background while pressed |
| `close_hover` | Close's background under the mouse (default GNOME red) |
| `close_icon_hover` | Close's symbol under the mouse (default white) |

### `[app.unamp]`: UnAmp's own parts

| Key | Used for |
|---|---|
| `classic` | A `.wsz` file, relative to this TOML, to draw the Player/Equalizer/Playlist from. See [Classic skins](#classic-winamp-skins). |
| `display` | Spectrum analyzer background |
| `time` | The big time display |
| `title` | The scrolling song title |
| `spectrum` | Analyzer bar colours, bottom to top: a list of two or more |
| `spectrum_peak` | The falling peak caps |

When these are left out, UnAmp uses a black analyzer, `text_strong` for the time and title, and
the classic green → yellow → red gradient.

### Upgrading from the old skin format

Skins written before format 1 have no `format` key and won't load. The Skins menu says so.
Skins that UnAmp converted from a `.wsz` are redone automatically on the next reload, and the old
file is kept as `<name>.toml.v0`. To upgrade a hand-written skin, add `format = 1` and move
these keys:

| Old | New |
|---|---|
| `classic` (top level) | `[app.unamp] classic` |
| `[colors] stripe` | `[colors] surface_alt` |
| `[colors] button`, `button_hover`, `button_active` | `[controls] background`, `hover`, `pressed` |
| `[player]` | `[app.unamp]` |
| `spectrum_low`, `spectrum_mid`, `spectrum_high` | `spectrum = [low, mid, high]` |

## Classic Winamp skins

Thousands of Winamp 2 skins survive at the [Winamp Skin Museum](https://skins.webamp.org). A
`.wsz` is a renamed `.zip` of bitmaps and two text files. Skins downloaded as `.zip` work too:
just rename them to `.wsz`, as long as the skin files are inside it (directly or in one folder)
rather than another `.wsz`.

### Installing one

1. Put the `.wsz` in `~/.config/unamp/skins/` (**Skins → Open skins folder**).
2. **Skins → Reload skins**. UnAmp writes `<same name>.toml` next to it.
3. Pick it from the Skins menu. The menu name is the file name; edit `name` in the TOML or rename
   the `.wsz` (before converting) for something tidier.

The TOML is generated **once**. UnAmp never overwrites it, so tune it freely. To convert again
(say, after a newer UnAmp improves conversion), delete the `.toml` and reload.

### What the generated TOML contains

Colours are pulled from the skin so the Media Library and menus match it
([ADR-0009](adr/0009-convert-winamp-wsz-colours.md)):

| From the skin | Becomes |
|---|---|
| `pledit.txt` Normal, Current, NormalBG, SelectedBG | `text`, `text_strong`/`link`/`title`, `surface`/`controls.pressed`, `accent` |
| `viscolor.txt` (24 colours) | `display`, the full `spectrum`, `spectrum_peak` |
| `numbers.bmp` / `nums_ex.bmp` (brightest pixel) | `time` |
| `main.bmp` (average colour) | `background`, and whether `base` is dark or light |

Plus the line, under `[app.unamp]`, that turns on the bitmap windows:

```toml
[app.unamp]
classic = "Shakira_04.wsz"
```

Remove it and the skin becomes an ordinary TOML skin: UnAmp's regular windows in the skin's
colours.

### What classic mode draws

With `classic` set, the Player, Equalizer and Playlist are drawn pixel-for-pixel from the skin's
bitmaps at Winamp's fixed sizes, at double size ([ADR-0010](adr/0010-classic-wsz-renderer.md)):

| Window | From the skin |
|---|---|
| **Player** (275×116) | `main.bmp` background, `titlebar.bmp`, `cbuttons.bmp` transport, `numbers.bmp`/`nums_ex.bmp` time, `text.bmp` ticker and kbps/kHz, `viscolor.txt` analyzer, `posbar.bmp`, `volume.bmp`, `balance.bmp`, `monoster.bmp`, `playpaus.bmp`, `shufrep.bmp` (shuffle, repeat, EQ, PL) |
| **Equalizer** (275×116) | `eqmain.bmp`: background, title bar, ON/AUTO/PRESETS buttons, slider art and thumbs, graph background and line colours |
| **Playlist** (275×145) | `pledit.bmp` frame and scroll thumb; `pledit.txt` colours for the list; `text.bmp` for the running time |

The windows float and drag like the others (drag any part that isn't a control) but can't be
resized. The Media Library stays a regular window in the skin's colours. In classic mode the
`[app.unamp]` colours aren't used by these three windows; the skin's own bitmaps and `viscolor.txt`
are.

Only `main.bmp` is required. If a bitmap is missing or too small for a sprite, that part is just
not drawn. Many skins ship trimmed bitmaps; some deliberately leave controls invisible (e.g. a
volume slider painted to match the background, with no thumb).

### Not supported (yet)

Windowshade (collapsed) modes, the clutterbar, the double-size toggle and other scales, resizing
the playlist, the playlist's ADD/REM/SEL/MISC/LIST menus, the balance control, `region.txt`
window shapes (so non-rectangular skins show square corners), custom cursors (`*.cur`), and
`.wsz` files packaged as `.zip` without renaming.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| A new `.wsz` doesn't appear | Not reloaded yet (**Skins → Reload skins**), or it's still named `.zip`. |
| A converted skin looks like plain colours | Its TOML has no `[app.unamp] classic` line. Delete the `.toml` and reload. |
| "written in the old skin format" | The file predates format 1. See [Upgrading](#upgrading-from-the-old-skin-format). |
| Edits don't show | Reload skins. Make sure you edited the file in `~/.config/unamp/skins/`, not the repo copy. |
| My skin isn't in the menu | Check the red errors at the bottom of the Skins menu. |
| A classic window is off-screen | **Windows → Reset layout**. |

## Licensing

Skins are their authors' work. UnAmp reads them like any other file you open, and the repo ships
none apart from its own Steam Classic recreation. Don't add third-party skins to the repo unless
their licence allows redistribution.
