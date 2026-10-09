//! Skins: TOML files that recolour egui's widgets and UnAmp's own painted
//! parts (spectrum, time display, seek bar). "Default" is stock egui and
//! follows the system theme; others are built in from `skins/` or loaded
//! from `~/.config/unamp/skins/*.toml` — where classic Winamp `.wsz`
//! skins are also converted to TOML (see `wsz.rs`).

use std::path::{Path, PathBuf};

use egui::{Color32, CornerRadius, Shadow, Stroke, Theme, ThemePreference};
use serde::{Deserialize, Deserializer};

pub const DEFAULT_SKIN: &str = "Default";

/// Skins compiled into the binary, as (file name, contents).
const BUILT_IN: &[(&str, &str)] = &[(
    "steam-classic.toml",
    include_str!("../skins/steam-classic.toml"),
)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    Dark,
    Light,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    /// `None` only for the Default skin, which follows the system theme.
    #[serde(default)]
    pub base: Option<Base>,
    #[serde(default)]
    pub corner_radius: Option<u8>,
    #[serde(default)]
    pub shadows: Option<bool>,
    /// A classic Winamp `.wsz`, relative to this file, whose bitmaps draw
    /// the Player, Equalizer and Playlist windows (see ADR-0010).
    #[serde(default)]
    pub classic: Option<String>,
    #[serde(default)]
    pub colors: Colors,
    #[serde(default)]
    pub player: PlayerColors,
    /// Where it was loaded from; `None` for built-ins.
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub background: Option<Hex>,
    pub surface: Option<Hex>,
    pub stripe: Option<Hex>,
    pub border: Option<Hex>,
    pub text: Option<Hex>,
    pub text_strong: Option<Hex>,
    pub text_weak: Option<Hex>,
    pub accent: Option<Hex>,
    pub accent_text: Option<Hex>,
    pub button: Option<Hex>,
    pub button_hover: Option<Hex>,
    pub button_active: Option<Hex>,
    pub link: Option<Hex>,
    pub error: Option<Hex>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerColors {
    pub display: Option<Hex>,
    pub time: Option<Hex>,
    pub title: Option<Hex>,
    pub spectrum_low: Option<Hex>,
    pub spectrum_mid: Option<Hex>,
    pub spectrum_high: Option<Hex>,
    pub spectrum_peak: Option<Hex>,
    /// Full bar gradient, bottom to top; overrides the low/mid/high stops.
    pub spectrum: Option<Vec<Hex>>,
}

/// A colour written as `#RRGGBB` or `#RRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hex(pub Color32);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex(&s)
            .map(Hex)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid colour {s:?}, expected #RRGGBB or #RRGGBBAA")))
    }
}

fn parse_hex(s: &str) -> Option<Color32> {
    let hex = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// Resolved colours for the parts UnAmp paints itself.
#[derive(Debug, Clone)]
pub struct Palette {
    pub display: Color32,
    pub time: Color32,
    pub title: Color32,
    pub spectrum_low: Color32,
    pub spectrum_mid: Color32,
    pub spectrum_high: Color32,
    pub spectrum_peak: Color32,
    /// Full gradient (bottom to top), when the skin gives one.
    pub spectrum_stops: Vec<Color32>,
    /// Corner rounding for painted boxes, matching the skin's widgets.
    pub radius: f32,
}

impl Palette {
    /// Bar colour at height `t` (0 = bottom, 1 = top): low → mid → high.
    pub fn spectrum(&self, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        if self.spectrum_stops.len() >= 2 {
            let pos = t * (self.spectrum_stops.len() - 1) as f32;
            let i = (pos.floor() as usize).min(self.spectrum_stops.len() - 2);
            return lerp(self.spectrum_stops[i], self.spectrum_stops[i + 1], pos - i as f32);
        }
        if t < 0.6 {
            lerp(self.spectrum_low, self.spectrum_mid, t / 0.6)
        } else {
            lerp(self.spectrum_mid, self.spectrum_high, (t - 0.6) / 0.4)
        }
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

impl Skin {
    pub fn default_skin() -> Self {
        Self {
            name: DEFAULT_SKIN.to_string(),
            author: None,
            base: None,
            corner_radius: None,
            shadows: None,
            classic: None,
            colors: Colors::default(),
            player: PlayerColors::default(),
            path: None,
        }
    }

    pub fn parse(toml_text: &str) -> Result<Self, String> {
        toml::from_str(toml_text).map_err(|e| e.to_string())
    }

    /// Applies the skin to egui and returns the colours for custom painting.
    pub fn apply(&self, ctx: &egui::Context) -> Palette {
        let Some(base) = self.base else {
            ctx.set_theme(ThemePreference::System);
            for theme in [Theme::Dark, Theme::Light] {
                ctx.set_style_of(theme, egui::Style {
                    visuals: theme.default_visuals(),
                    ..Default::default()
                });
            }
            return self.palette(&ctx.global_style().visuals);
        };

        let theme = match base {
            Base::Dark => Theme::Dark,
            Base::Light => Theme::Light,
        };
        let visuals = self.visuals(theme.default_visuals());
        // A skin fixes the theme; both slots get it so a system theme switch
        // can't swap it out.
        ctx.set_theme(theme);
        for slot in [Theme::Dark, Theme::Light] {
            ctx.set_style_of(slot, egui::Style {
                visuals: visuals.clone(),
                ..Default::default()
            });
        }
        self.palette(&visuals)
    }

    fn visuals(&self, mut v: egui::Visuals) -> egui::Visuals {
        let c = &self.colors;
        let get = |h: Option<Hex>| h.map(|h| h.0);

        if let Some(bg) = get(c.background) {
            v.panel_fill = bg;
            v.window_fill = bg;
            v.widgets.noninteractive.bg_fill = bg;
            v.widgets.noninteractive.weak_bg_fill = bg;
        }
        if let Some(surface) = get(c.surface) {
            v.extreme_bg_color = surface;
            v.text_edit_bg_color = Some(surface);
            v.code_bg_color = surface;
        }
        if let Some(stripe) = get(c.stripe) {
            v.faint_bg_color = stripe;
        }
        if let Some(border) = get(c.border) {
            v.window_stroke.color = border;
            v.widgets.noninteractive.bg_stroke.color = border;
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
            v.widgets.hovered.bg_stroke.color = border;
            v.widgets.active.bg_stroke.color = border;
            v.widgets.open.bg_stroke.color = border;
        }
        if let Some(text) = get(c.text) {
            v.widgets.noninteractive.fg_stroke.color = text;
            v.widgets.inactive.fg_stroke.color = text;
            v.widgets.open.fg_stroke.color = text;
        }
        if let Some(strong) = get(c.text_strong) {
            v.widgets.hovered.fg_stroke.color = strong;
            v.widgets.active.fg_stroke.color = strong;
        }
        if let Some(weak) = get(c.text_weak) {
            v.weak_text_color = Some(weak);
        }
        if let Some(accent) = get(c.accent) {
            v.selection.bg_fill = accent;
        }
        if let Some(accent_text) = get(c.accent_text) {
            v.selection.stroke.color = accent_text;
        }
        // egui paints buttons with `weak_bg_fill` and slider/checkbox tracks
        // with `bg_fill`, so idle tracks get the inset colour rather than
        // vanishing into a button-coloured background.
        for (fill, widget) in [
            (get(c.button), &mut v.widgets.inactive),
            (get(c.button_hover), &mut v.widgets.hovered),
            (get(c.button_active), &mut v.widgets.active),
            (get(c.button), &mut v.widgets.open),
        ] {
            if let Some(fill) = fill {
                widget.bg_fill = fill;
                widget.weak_bg_fill = fill;
            }
        }
        if let Some(surface) = get(c.surface) {
            v.widgets.inactive.bg_fill = surface;
        }
        if let Some(link) = get(c.link) {
            v.hyperlink_color = link;
        }
        if let Some(error) = get(c.error) {
            v.error_fg_color = error;
        }

        if let Some(r) = self.corner_radius {
            let radius = CornerRadius::same(r);
            v.window_corner_radius = radius;
            v.menu_corner_radius = radius;
            for w in [
                &mut v.widgets.noninteractive,
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
                &mut v.widgets.open,
            ] {
                w.corner_radius = radius;
            }
        }
        if self.shadows == Some(false) {
            v.window_shadow = Shadow::NONE;
            v.popup_shadow = Shadow::NONE;
        }
        v
    }

    fn palette(&self, v: &egui::Visuals) -> Palette {
        let p = &self.player;
        let or = |h: Option<Hex>, fallback: Color32| h.map(|h| h.0).unwrap_or(fallback);
        Palette {
            display: or(p.display, Color32::BLACK),
            time: or(p.time, v.strong_text_color()),
            title: or(p.title, v.strong_text_color()),
            spectrum_low: or(p.spectrum_low, Color32::from_rgb(30, 210, 30)),
            spectrum_mid: or(p.spectrum_mid, Color32::from_rgb(230, 210, 30)),
            spectrum_high: or(p.spectrum_high, Color32::from_rgb(230, 0, 30)),
            spectrum_peak: or(p.spectrum_peak, Color32::from_gray(200)),
            spectrum_stops: p.spectrum.iter().flatten().map(|h| h.0).collect(),
            radius: self.corner_radius.map(f32::from).unwrap_or(2.0),
        }
    }
}

/// Folder for user skins.
pub fn user_skins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("unamp").join("skins"))
}

/// What `export_built_ins` did, by file name.
#[derive(Debug, Default, PartialEq)]
pub struct Export {
    pub copied: Vec<String>,
    /// Already present; left untouched so edits survive.
    pub skipped: Vec<String>,
}

/// Writes the built-in skins' TOML into `dir` so they can be edited there.
/// Never overwrites: an existing file of the same name is kept as is.
pub fn export_built_ins(dir: &Path) -> Result<Export, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut export = Export::default();
    for (file, text) in BUILT_IN {
        let target = dir.join(file);
        if target.exists() {
            export.skipped.push(file.to_string());
            continue;
        }
        std::fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))?;
        export.copied.push(file.to_string());
    }
    Ok(export)
}

/// Default, the built-ins, then user skins sorted by name. A user skin with
/// a built-in's name replaces it. Returns the skins and any load errors.
pub fn load_all() -> (Vec<Skin>, Vec<String>) {
    let mut skins = vec![Skin::default_skin()];
    let mut errors = Vec::new();
    for (file, text) in BUILT_IN {
        match Skin::parse(text) {
            Ok(skin) => skins.push(skin),
            Err(e) => errors.push(format!("built-in {file}: {e}")),
        }
    }
    if let Some(dir) = user_skins_dir() {
        // Classic Winamp skins dropped in the folder become TOML first.
        errors.extend(crate::wsz::convert_new_in(&dir));
        let (user, user_errors) = load_dir(&dir);
        errors.extend(user_errors);
        for skin in user {
            skins.retain(|s| s.name != skin.name || s.name == DEFAULT_SKIN);
            if skin.name != DEFAULT_SKIN {
                skins.push(skin);
            }
        }
    }
    skins[1..].sort_by_key(|s| s.name.to_lowercase());
    (skins, errors)
}

fn load_dir(dir: &Path) -> (Vec<Skin>, Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut skins = Vec::new();
    let mut errors = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Skin::parse(&t)) {
            Ok(mut skin) => {
                skin.path = Some(path);
                skins.push(skin);
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    (skins, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_copies_built_ins_once_and_keeps_edits() {
        let dir = std::env::temp_dir().join(format!("unamp-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = export_built_ins(&dir).unwrap();
        assert_eq!(first.copied, vec!["steam-classic.toml"]);
        let path = dir.join("steam-classic.toml");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), BUILT_IN[0].1);

        std::fs::write(&path, "name = \"Steam Classic\"\n").unwrap();
        let second = export_built_ins(&dir).unwrap();
        assert!(second.copied.is_empty());
        assert_eq!(second.skipped, vec!["steam-classic.toml"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "name = \"Steam Classic\"\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_hex_colours() {
        assert_eq!(parse_hex("#4C5844"), Some(Color32::from_rgb(0x4C, 0x58, 0x44)));
        assert_eq!(
            parse_hex("#00000080"),
            Some(Color32::from_rgba_unmultiplied(0, 0, 0, 0x80))
        );
        assert_eq!(parse_hex("4C5844"), None);
        assert_eq!(parse_hex("#4C58"), None);
        assert_eq!(parse_hex("#GG5844"), None);
    }

    #[test]
    fn built_in_skins_parse() {
        for (file, text) in BUILT_IN {
            Skin::parse(text).unwrap_or_else(|e| panic!("{file}: {e}"));
        }
    }

    #[test]
    fn steam_skin_is_square_and_shadowless() {
        let skin = Skin::parse(BUILT_IN[0].1).unwrap();
        let v = skin.visuals(egui::Visuals::dark());
        assert_eq!(v.window_corner_radius, CornerRadius::ZERO);
        assert_eq!(v.window_shadow, Shadow::NONE);
        assert_eq!(v.panel_fill, Color32::from_rgb(0x4C, 0x58, 0x44));
        assert_eq!(v.selection.bg_fill, Color32::from_rgb(0x95, 0x88, 0x31));
        // Slider tracks must stand out from the window background.
        assert_ne!(v.widgets.inactive.bg_fill, v.panel_fill);
        assert_eq!(v.widgets.inactive.weak_bg_fill, Color32::from_rgb(0x4C, 0x58, 0x44));
    }

    #[test]
    fn unset_colours_keep_egui_defaults() {
        let skin = Skin::parse("name = \"Tiny\"\nbase = \"light\"\n[colors]\naccent = \"#FF0000\"").unwrap();
        let v = skin.visuals(egui::Visuals::light());
        assert_eq!(v.selection.bg_fill, Color32::RED);
        assert_eq!(v.panel_fill, egui::Visuals::light().panel_fill);
    }

    #[test]
    fn typos_are_errors_not_silently_ignored() {
        let err = Skin::parse("name = \"X\"\n[colors]\nbakground = \"#000000\"").unwrap_err();
        assert!(err.contains("bakground"), "{err}");
        let err = Skin::parse("name = \"X\"\n[colors]\ntext = \"green\"").unwrap_err();
        assert!(err.contains("invalid colour"), "{err}");
    }

    #[test]
    fn full_spectrum_list_overrides_stops() {
        let skin = Skin::parse("name = \"S\"\n[player]\nspectrum = [\"#0000FF\", \"#00FF00\", \"#FF0000\"]").unwrap();
        let p = skin.palette(&egui::Visuals::dark());
        assert_eq!(p.spectrum(0.0), Color32::BLUE);
        assert_eq!(p.spectrum(0.5), Color32::GREEN);
        assert_eq!(p.spectrum(1.0), Color32::RED);
    }

    #[test]
    fn spectrum_gradient_hits_its_stops() {
        let skin = Skin::parse(BUILT_IN[0].1).unwrap();
        let p = skin.palette(&egui::Visuals::dark());
        assert_eq!(p.spectrum(0.0), p.spectrum_low);
        assert_eq!(p.spectrum(0.6), p.spectrum_mid);
        assert_eq!(p.spectrum(1.0), p.spectrum_high);
    }
}
