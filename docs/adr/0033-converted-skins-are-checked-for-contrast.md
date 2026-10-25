# 0033. Converted `.wsz` skins are checked for contrast, and old conversions redone once

Date: 2026-10-25

## Status

Accepted. The user found the TOML generated from Winamp skins "kind of sucks": it didn't consider
contrast, so text was sometimes unreadable. The approach below is mine.

## Context

[ADR-0009](0009-convert-winamp-wsz-colours.md) maps a `.wsz`'s colours onto the TOML skin's keys
as they are: `pledit.txt`'s Normal becomes `text`, `main.bmp`'s average colour the window
`background`, and so on. Those colours were chosen in pairs for Winamp's bitmaps: Normal on
NormalBG in the playlist. UnAmp draws `text` on the window background, the playlist background
and controls too, `text_strong` on hovered controls, and `accent_text` on the selection, and none
of these pairs were checked. On the user's skins:

- Shakira_04: orange text (`#EB8B1C`) on the brown window (`#885B45`), about 2.3:1, and the
  selection's text the same orange on a lighter orange (`#FBAD29`).
- Windows_XP: green text (`#00FF00`) on a mid blue (`#3F83E9`), about 2.7:1, and on the lighter
  window blue (`#6D8EDD`), about 2.3:1, under a light base, so egui's own text was dark on it.

Considered: leaving the colours alone and documenting hand-tuning, which is where we were; and
making all text black or white, which reads but loses what the skin looks like.

## Decision

The converter checks every text colour against each colour it's drawn on, using WCAG's contrast
ratio, and requires 4.5:1 (3:1 for `text_weak`):

- **The playlist's pair first.** If Normal doesn't read on NormalBG, NormalBG moves toward black
  or white (away from the text), up to halfway, and only then does the text move. A skin is
  recognised by its text colour more than its list's background.
- **The window background** (`main.bmp`'s average, which ADR-0009 already called muddy) moves
  toward black or white as far as the text needs, and `base` follows from the result.
- **Every other text colour** (`text_strong` on the panels and hovered controls, `text_weak`,
  `time` on the display) moves toward black or white by the smallest step that reads.
  `accent_text` is Normal or Current if either reads on the selection, else Normal moved.
- Colours that already read are written unchanged.
- **Old conversions are redone once.** The TOML has always been written once and then left
  alone. A file whose header is the converter's but which lacks the new contrast note is redone,
  with the old file kept as `.toml.v1`, as ADR-0024's format change kept `.toml.v0`.

## Consequences

- Converted skins read everywhere UnAmp draws text, while keeping their hues: Shakira keeps its
  orange on a darker brown, Windows XP its green on a darker blue.
- Some skins look darker or lighter than the original around the text; the classic bitmap windows
  are unchanged, since they draw from the `.wsz` itself.
- Redoing old conversions replaces hand edits made to them. They're kept in `.toml.v1`, but they
  have to be copied back by hand.
- Only text is checked. Borders, the spectrum and the waveform keep the skin's colours.
