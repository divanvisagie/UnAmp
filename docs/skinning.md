# Skinning UnAmp

UnAmp has three kinds of skin, all chosen from the **Skins** menu:

| Kind | What it is | What changes |
|---|---|---|
| **Default** | Built in, no file | Nothing: egui's stock look, following your system's light/dark setting |
| **TOML skin** | A `.toml` file of colours | Colours, corner rounding and shadows of every window |
| **Classic skin** | A Winamp 2 `.wsz` plus the `.toml` UnAmp makes from it | Player, Equalizer and Playlist drawn from the skin's own bitmaps; everything else in its colours |

Your choice is saved as `skin = "<name>"` in `~/.config/unamp/config.toml`. If that skin
disappears, UnAmp falls back to Default.

## Where skins live

- **Built-in skins** are TOML files in the repo's [`skins/`](../skins) folder, compiled into the
  binary. Today that's [`steam-classic.toml`](../skins/steam-classic.toml), which recreates the
  olive-green Steam client and doubles as the commented reference for the format.
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

Problems loading a skin (a typo in a key, a bad colour, a broken `.wsz`) are listed in red at the
bottom of the Skins menu. The skin is skipped, and nothing fails silently.

## Writing a TOML skin

Start from a copy of `steam-classic.toml` (**Copy built-in skins to folder**), change `name`, and
edit. Every key except `name` is optional, and **any colour you leave out keeps egui's default**
for the `base` theme, so a skin can be as small as:

```toml
name = "Just Purple"
base = "dark"

[colors]
accent = "#8A2BE2"
```

Colours are `"#RRGGBB"` or `"#RRGGBBAA"`. Unknown keys are errors, not ignored, so a misspelt
`bakground` is reported in the Skins menu instead of silently doing nothing.

### Top-level keys

| Key | Type | Meaning |
|---|---|---|
| `name` | string, **required** | Name in the Skins menu. Matching a built-in's name replaces it. |
| `author` | string | Shown next to user skins in the menu. |
| `base` | `"dark"` or `"light"` | The egui theme the skin starts from. A skin fixes the theme; only Default follows the system. |
| `corner_radius` | integer (px) | Rounding of windows, menus, buttons and UnAmp's painted boxes. `0` is square. |
| `shadows` | bool | `false` removes window and popup shadows. |
| `classic` | string | A `.wsz` file, relative to this TOML, to draw the Player/Equalizer/Playlist from. See [Classic skins](#classic-winamp-skins). |

### `[colors]`: the egui widgets

| Key | Used for |
|---|---|
| `background` | Window and panel backgrounds |
| `surface` | Inset areas: text fields, lists, slider tracks, seek bar track, EQ curve background, album-art placeholder |
| `stripe` | Alternate rows in track lists; EQ curve grid lines |
| `border` | Window and widget outlines |
| `text` | Normal text |
| `text_strong` | Headings, and text on hovered or pressed widgets |
| `text_weak` | Secondary labels (section headings, counts, status) |
| `accent` | Selection, slider fill, toggles that are on, the seek bar's played part, the EQ curve |
| `accent_text` | Text drawn on top of `accent` (e.g. a selected track) |
| `button` | Button background |
| `button_hover` | Button background under the mouse |
| `button_active` | Button background while pressed |
| `link` | Links, and the now-playing track in lists |
| `error` | Error messages |

egui draws buttons with one fill and slider/checkbox tracks with another. UnAmp gives tracks the
`surface` colour so they stay visible when `button` matches `background`.

### `[player]`: UnAmp's own painted parts

| Key | Used for |
|---|---|
| `display` | Spectrum analyzer background |
| `time` | The big time display |
| `title` | The scrolling song title |
| `spectrum_low`, `spectrum_mid`, `spectrum_high` | Analyzer gradient: bottom, 60% up, top |
| `spectrum` | A list of colours, bottom to top, for the whole gradient; overrides the three stops above |
| `spectrum_peak` | The falling peak caps |

When these are left out: black analyzer, `text_strong` for time and title, and the classic
green → yellow → red gradient.

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
| `pledit.txt` Normal, Current, NormalBG, SelectedBG | `text`, `text_strong`/`link`/`title`, `surface`, `accent` |
| `viscolor.txt` (24 colours) | `display`, the full `spectrum`, `spectrum_peak` |
| `numbers.bmp` / `nums_ex.bmp` (brightest pixel) | `time` |
| `main.bmp` (average colour) | `background`, and whether `base` is dark or light |

Plus the line that turns on the bitmap windows:

```toml
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
`[player]` colours aren't used by these three windows; the skin's own bitmaps and `viscolor.txt`
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
| A converted skin looks like plain colours | Its TOML has no `classic` line, e.g. it was converted by an older UnAmp. Delete the `.toml` and reload. |
| Edits don't show | Reload skins. Make sure you edited the file in `~/.config/unamp/skins/`, not the repo copy. |
| My skin isn't in the menu | Check the red errors at the bottom of the Skins menu. |
| A classic window is off-screen | **Windows → Reset layout**. |

## Licensing

Skins are their authors' work. UnAmp reads them like any other file you open, and the repo ships
none apart from its own Steam Classic recreation. Don't add third-party skins to the repo unless
their licence allows redistribution.
