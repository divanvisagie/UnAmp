//! Classic Winamp 2 skin rendering: the Player, Equalizer and Playlist
//! drawn pixel-for-pixel from a `.wsz`'s bitmaps at Winamp's fixed sizes,
//! scaled up (see ADR-0010). Sprite coordinates follow the Winamp 2 skin
//! layout as documented by the skinning community and Webamp.

mod windows;

use std::collections::HashMap;
use std::path::Path;

use egui::{Color32, Pos2, Rect, TextureHandle, Vec2};

pub use windows::{Action, View, show_eq, show_player, show_playlist};

/// Screen points per skin pixel ("double size" mode).
pub const SCALE: f32 = 2.0;

pub const MAIN_SIZE: Vec2 = Vec2::new(275.0, 116.0);
pub const EQ_SIZE: Vec2 = Vec2::new(275.0, 116.0);
/// Playlist at Winamp's minimum width, one 29px step taller than its
/// minimum, so the three-window stack fits a 900px-tall screen at 2x.
pub const PLAYLIST_SIZE: Vec2 = Vec2::new(275.0, 145.0);

/// The skin bitmaps the renderer draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bmp {
    Main,
    Titlebar,
    Cbuttons,
    Numbers,
    NumsEx,
    Text,
    Posbar,
    Volume,
    Balance,
    Shufrep,
    Playpaus,
    Monoster,
    Eqmain,
    Pledit,
}

impl Bmp {
    const ALL: [Bmp; 14] = [
        Bmp::Main,
        Bmp::Titlebar,
        Bmp::Cbuttons,
        Bmp::Numbers,
        Bmp::NumsEx,
        Bmp::Text,
        Bmp::Posbar,
        Bmp::Volume,
        Bmp::Balance,
        Bmp::Shufrep,
        Bmp::Playpaus,
        Bmp::Monoster,
        Bmp::Eqmain,
        Bmp::Pledit,
    ];

    fn file(self) -> &'static str {
        match self {
            Bmp::Main => "main.bmp",
            Bmp::Titlebar => "titlebar.bmp",
            Bmp::Cbuttons => "cbuttons.bmp",
            Bmp::Numbers => "numbers.bmp",
            Bmp::NumsEx => "nums_ex.bmp",
            Bmp::Text => "text.bmp",
            Bmp::Posbar => "posbar.bmp",
            Bmp::Volume => "volume.bmp",
            Bmp::Balance => "balance.bmp",
            Bmp::Shufrep => "shufrep.bmp",
            Bmp::Playpaus => "playpaus.bmp",
            Bmp::Monoster => "monoster.bmp",
            Bmp::Eqmain => "eqmain.bmp",
            Bmp::Pledit => "pledit.bmp",
        }
    }
}

const TEXT_FILES: &[&str] = &["pledit.txt", "viscolor.txt"];

struct Sheet {
    texture: TextureHandle,
    size: [u32; 2],
}

/// Playlist colours from `pledit.txt`.
#[derive(Debug, Clone, Copy)]
pub struct PlColors {
    pub normal: Color32,
    pub current: Color32,
    pub normal_bg: Color32,
    pub selected_bg: Color32,
}

/// A loaded classic skin plus the little UI state its windows keep.
pub struct ClassicSkin {
    sheets: HashMap<Bmp, Sheet>,
    /// The EQ graph's per-row line colours, sampled from `eqmain.bmp`.
    eq_graph_colors: Vec<Color32>,
    pub vis: [Color32; 24],
    pub pl: PlColors,
    // UI state
    seek_drag: Option<f32>,
    playlist_scroll: f32,
    playlist_selected: Option<usize>,
}

/// Winamp's base-skin visualizer colours, used when `viscolor.txt` is missing.
const DEFAULT_VIS: [[u8; 3]; 24] = [
    [0, 0, 0], [24, 33, 41], [239, 49, 16], [206, 41, 16], [214, 90, 0], [214, 102, 0],
    [214, 115, 0], [198, 123, 8], [222, 165, 24], [214, 181, 33], [189, 222, 41], [148, 222, 33],
    [41, 206, 16], [50, 190, 16], [57, 181, 16], [49, 156, 8], [41, 148, 0], [24, 132, 8],
    [255, 255, 255], [214, 214, 222], [181, 189, 189], [160, 170, 175], [148, 156, 165],
    [150, 150, 150],
];

impl ClassicSkin {
    /// Loads a `.wsz`. Only `main.bmp` is required; any other missing
    /// bitmap just means that part isn't drawn.
    pub fn load(path: &Path, ctx: &egui::Context) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut wanted: Vec<&str> = Bmp::ALL.iter().map(|b| b.file()).collect();
        wanted.extend_from_slice(TEXT_FILES);
        let files = crate::wsz::read_files(file, &wanted)?;

        let mut sheets = HashMap::new();
        let mut eq_graph_colors = Vec::new();
        for bmp in Bmp::ALL {
            let Some(bytes) = files.get(bmp.file()) else {
                continue;
            };
            let Ok(img) = image::load_from_memory(bytes) else {
                continue;
            };
            let img = img.into_rgba8();
            if bmp == Bmp::Eqmain {
                eq_graph_colors = (0..19)
                    .filter_map(|y| img.get_pixel_checked(115, 294 + y))
                    .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
                    .collect();
            }
            let size = [img.width(), img.height()];
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [size[0] as usize, size[1] as usize],
                img.as_raw(),
            );
            let texture = ctx.load_texture(
                format!("classic-{}", bmp.file()),
                color,
                egui::TextureOptions::NEAREST,
            );
            sheets.insert(bmp, Sheet { texture, size });
        }
        if !sheets.contains_key(&Bmp::Main) {
            return Err("not a classic skin: it has no main.bmp".into());
        }

        let text = |name: &str| files.get(name).map(|b| String::from_utf8_lossy(b).into_owned());
        let vis = text("viscolor.txt")
            .and_then(|t| crate::wsz::parse_viscolor(&t))
            .map(|v| std::array::from_fn(|i| v[i]))
            .unwrap_or(DEFAULT_VIS.map(|[r, g, b]| Color32::from_rgb(r, g, b)));
        let pledit = text("pledit.txt").unwrap_or_default();
        let get = |key: &str, fallback: Color32| {
            crate::wsz::ini_value(&pledit, key)
                .and_then(|v| crate::wsz::parse_colour(&v))
                .unwrap_or(fallback)
        };
        let pl = PlColors {
            normal: get("normal", Color32::from_rgb(0, 255, 0)),
            current: get("current", Color32::WHITE),
            normal_bg: get("normalbg", Color32::BLACK),
            selected_bg: get("selectedbg", Color32::from_rgb(0, 0, 0xC6)),
        };

        Ok(Self {
            sheets,
            eq_graph_colors,
            vis,
            pl,
            seek_drag: None,
            playlist_scroll: 0.0,
            playlist_selected: None,
        })
    }

    pub fn has(&self, bmp: Bmp) -> bool {
        self.sheets.contains_key(&bmp)
    }

    fn sheet_size(&self, bmp: Bmp) -> Option<[u32; 2]> {
        self.sheets.get(&bmp).map(|s| s.size)
    }

    /// Draws `src` (x, y, w, h in the bitmap) at skin position `at` inside
    /// a window whose top-left is `origin`. Silently skips sprites the
    /// bitmap is too small for — many skins ship trimmed bitmaps.
    pub fn sprite(&self, painter: &egui::Painter, origin: Pos2, bmp: Bmp, src: [u32; 4], at: [f32; 2]) {
        let Some(sheet) = self.sheets.get(&bmp) else {
            return;
        };
        let [x, y, w, h] = src;
        if x + w > sheet.size[0] || y + h > sheet.size[1] || w == 0 || h == 0 {
            return;
        }
        let (sw, sh) = (sheet.size[0] as f32, sheet.size[1] as f32);
        let uv = Rect::from_min_max(
            Pos2::new(x as f32 / sw, y as f32 / sh),
            Pos2::new((x + w) as f32 / sw, (y + h) as f32 / sh),
        );
        let dest = Rect::from_min_size(
            origin + Vec2::new(at[0], at[1]) * SCALE,
            Vec2::new(w as f32, h as f32) * SCALE,
        );
        painter.image(sheet.texture.id(), dest, uv, Color32::WHITE);
    }

    /// Draws `text` in the skin's 5×6 bitmap font (`text.bmp`), clipped to
    /// `width` skin pixels, starting `offset` pixels into the string.
    pub fn text(&self, painter: &egui::Painter, origin: Pos2, text: &str, at: [f32; 2], width: f32, offset: f32) {
        let clip = Rect::from_min_size(origin + Vec2::new(at[0], at[1]) * SCALE, Vec2::new(width, 6.0) * SCALE);
        let painter = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        for (i, c) in text.chars().enumerate() {
            let x = at[0] + i as f32 * 5.0 - offset;
            if x + 5.0 < at[0] || x > at[0] + width {
                continue;
            }
            let (col, row) = glyph(c);
            self.sprite(&painter, origin, Bmp::Text, [col * 5, row * 6, 5, 6], [x, at[1]]);
        }
    }

    /// Clickable region at skin coordinates.
    pub fn hit(ui: &egui::Ui, origin: Pos2, id: egui::Id, rect: [f32; 4], sense: egui::Sense) -> egui::Response {
        let r = Rect::from_min_size(
            origin + Vec2::new(rect[0], rect[1]) * SCALE,
            Vec2::new(rect[2], rect[3]) * SCALE,
        );
        ui.interact(r, id, sense)
    }

    pub fn eq_graph_color(&self, row: usize) -> Color32 {
        self.eq_graph_colors
            .get(row)
            .copied()
            .unwrap_or(self.vis[2 + row.min(15)])
    }
}

/// Position of a character in `text.bmp` as (column, row) of 5×6 cells.
/// Unknown characters render as a space, as in Winamp.
fn glyph(c: char) -> (u32, u32) {
    let c = c.to_ascii_lowercase();
    if c.is_ascii_lowercase() {
        return (c as u32 - 'a' as u32, 0);
    }
    if c.is_ascii_digit() {
        return (c as u32 - '0' as u32, 1);
    }
    match c {
        '"' => (26, 0),
        '@' => (27, 0),
        '…' => (10, 1),
        '.' => (11, 1),
        ':' => (12, 1),
        '(' | '{' => (13, 1),
        ')' | '}' => (14, 1),
        '-' | '~' | '–' | '—' => (15, 1),
        '\'' | '`' | '’' => (16, 1),
        '!' => (17, 1),
        '_' => (18, 1),
        '+' => (19, 1),
        '\\' => (20, 1),
        '/' => (21, 1),
        '[' | '<' => (22, 1),
        ']' | '>' => (23, 1),
        '^' => (24, 1),
        '&' => (25, 1),
        '%' => (26, 1),
        ',' => (27, 1),
        '=' => (28, 1),
        '$' => (29, 1),
        '#' => (30, 1),
        'å' | 'Å' => (0, 2),
        'ö' | 'Ö' => (1, 2),
        'ä' | 'Ä' => (2, 2),
        '?' => (3, 2),
        '*' => (4, 2),
        _ => (30, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_map_letters_digits_and_fallback() {
        assert_eq!(glyph('A'), (0, 0));
        assert_eq!(glyph('z'), (25, 0));
        assert_eq!(glyph('7'), (7, 1));
        assert_eq!(glyph(':'), (12, 1));
        assert_eq!(glyph('?'), (3, 2));
        assert_eq!(glyph('ж'), (30, 0));
    }
}
