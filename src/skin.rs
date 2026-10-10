//! Themes (shown as "skins" in the menu): TOML files in the suite theme
//! format (see ADR-0024 and docs/skinning.md). The shared sections
//! (`[colors]`, `[controls]`, `[window]`) mean the same in any app that
//! reads the format; each app keeps its own parts under `[app.<name>]`,
//! here `[app.unamp]`, and ignores the others'.
//!
//! "Default" is stock egui and follows the system theme; others are built in
//! from `skins/` or loaded from `~/.config/unamp/skins/*.toml`, where
//! classic Winamp `.wsz` skins are also converted to TOML (see `wsz.rs`).

use std::path::{Path, PathBuf};

use egui::{Color32, CornerRadius, Shadow, Stroke, Theme, ThemePreference};
use serde::{Deserialize, Deserializer};

use crate::frame::{FrameStyle, WindowControlColors};

pub const DEFAULT_SKIN: &str = "Default";
/// The theme format this build reads and writes.
pub const FORMAT: u32 = 1;
/// This app's section under `[app]`.
const APP: &str = "unamp";

/// Skins compiled into the binary, as (file name, contents).
const BUILT_IN: &[(&str, &str)] = &[
    ("steam-classic.toml", include_str!("../skins/steam-classic.toml")),
    ("photograph.toml", include_str!("../skins/photograph.toml")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    Dark,
    Light,
}

/// A theme file. Unknown keys are reported but never fatal, so a theme
/// written for a newer format or another app still loads.
#[derive(Debug, Clone, Deserialize)]
pub struct Skin {
    /// The theme format version; missing means the pre-suite skin format.
    pub format: Option<u32>,
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    /// `None` only for the Default skin, which follows the system theme.
    #[serde(default)]
    pub base: Option<Base>,
    /// Rounding for panels, menus and controls.
    #[serde(default)]
    pub corner_radius: Option<u8>,
    #[serde(default)]
    pub shadows: Option<bool>,
    #[serde(default)]
    pub colors: Colors,
    #[serde(default)]
    pub controls: Controls,
    #[serde(default)]
    pub window: WindowTheme,
    #[serde(default)]
    pub app: Apps,
    /// Where it was loaded from; `None` for built-ins.
    #[serde(skip)]
    pub path: Option<PathBuf>,
    /// Ignored keys and other things worth telling the author.
    #[serde(skip)]
    pub warnings: Vec<String>,
}

/// The base palette every app uses.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Colors {
    /// Panels and the windows inside the app.
    pub background: Option<Hex>,
    /// Inset areas: text fields, lists, slider tracks.
    pub surface: Option<Hex>,
    /// Alternate rows in lists.
    pub surface_alt: Option<Hex>,
    pub border: Option<Hex>,
    pub text: Option<Hex>,
    /// Headings, and text on hovered or pressed controls.
    pub text_strong: Option<Hex>,
    pub text_weak: Option<Hex>,
    /// Selection and anything that's on or current.
    pub accent: Option<Hex>,
    /// Text on top of `accent`.
    pub accent_text: Option<Hex>,
    pub link: Option<Hex>,
    pub warning: Option<Hex>,
    pub error: Option<Hex>,
}

/// Buttons, sliders, checkboxes, drop-downs and other widgets.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Controls {
    pub background: Option<Hex>,
    pub hover: Option<Hex>,
    pub pressed: Option<Hex>,
    pub text: Option<Hex>,
    pub text_hover: Option<Hex>,
    pub border: Option<Hex>,
}

/// The app's own window frame: its title bar, edge and corners.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WindowTheme {
    pub title_bar: Option<Hex>,
    pub title_text: Option<Hex>,
    pub border: Option<Hex>,
    /// Corner rounding of the whole window; GNOME's when unset.
    pub corner_radius: Option<u8>,
    /// Minimise, maximise and close: themed apart from `[controls]`.
    #[serde(default)]
    pub controls: WindowControls,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WindowControls {
    pub icon: Option<Hex>,
    pub icon_hover: Option<Hex>,
    pub hover: Option<Hex>,
    pub pressed: Option<Hex>,
    pub close_hover: Option<Hex>,
    pub close_icon_hover: Option<Hex>,
}

/// App-specific sections; other apps' sections are skipped.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Apps {
    #[serde(default)]
    pub unamp: UnampTheme,
}

/// UnAmp's own parts: the display, spectrum and classic bitmaps.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UnampTheme {
    /// A classic Winamp `.wsz`, relative to this file, whose bitmaps draw
    /// the Player, Equalizer and Playlist windows (see ADR-0010).
    pub classic: Option<String>,
    /// Background of the spectrum analyzer and seek bar.
    pub display: Option<Hex>,
    pub time: Option<Hex>,
    pub title: Option<Hex>,
    /// Bar colours from bottom to top, two or more.
    pub spectrum: Option<Vec<Hex>>,
    pub spectrum_peak: Option<Hex>,
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
    /// Spectrum bar colours from bottom to top, at least two.
    pub spectrum_stops: Vec<Color32>,
    pub spectrum_peak: Color32,
    /// Corner rounding for painted boxes, matching the skin's widgets.
    pub radius: f32,
    /// The app's own window frame.
    pub frame: FrameStyle,
}

impl Palette {
    /// Bar colour at height `t` (0 = bottom, 1 = top), along the stops.
    pub fn spectrum(&self, t: f32) -> Color32 {
        let stops = &self.spectrum_stops;
        let pos = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
        let i = (pos.floor() as usize).min(stops.len() - 2);
        lerp(stops[i], stops[i + 1], pos - i as f32)
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

impl Skin {
    pub fn default_skin() -> Self {
        Self {
            format: Some(FORMAT),
            name: DEFAULT_SKIN.to_string(),
            author: None,
            base: None,
            corner_radius: None,
            shadows: None,
            colors: Colors::default(),
            controls: Controls::default(),
            window: WindowTheme::default(),
            app: Apps::default(),
            path: None,
            warnings: Vec::new(),
        }
    }

    /// Parses a theme. Keys this build doesn't know are listed in
    /// `warnings` (except other apps' sections) rather than rejected.
    pub fn parse(toml_text: &str) -> Result<Self, String> {
        let table: toml::Table = toml::from_str(toml_text).map_err(|e| e.to_string())?;
        if !table.contains_key("format") {
            return Err(format!(
                "written in the old skin format; add `format = {FORMAT}` and update it as described in docs/skinning.md"
            ));
        }
        let mut ignored = Vec::new();
        let mut skin: Skin = serde_ignored::deserialize(toml::Value::Table(table), |path| {
            let path = path.to_string();
            let other_app = path.strip_prefix("app.").is_some_and(|rest| rest.split('.').next() != Some(APP));
            if !other_app {
                ignored.push(path);
            }
        })
        .map_err(|e| e.to_string())?;
        skin.warnings.extend(ignored.into_iter().map(|key| format!("unknown key `{key}` ignored")));
        match skin.format {
            Some(f) if f > FORMAT => skin.warnings.push(format!(
                "written for theme format {f}; this build reads format {FORMAT}, so newer parts are left out"
            )),
            _ => {}
        }
        if skin.app.unamp.spectrum.as_ref().is_some_and(|s| s.len() < 2) {
            skin.warnings.push("`app.unamp.spectrum` needs at least two colours; using the default".into());
            skin.app.unamp.spectrum = None;
        }
        Ok(skin)
    }

    /// The classic `.wsz` file to draw windows from, if any.
    pub fn classic(&self) -> Option<&str> {
        self.app.unamp.classic.as_deref()
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
        let k = &self.controls;
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
            // Slider and checkbox tracks: inset, so they stand out from
            // button-coloured backgrounds.
            v.widgets.inactive.bg_fill = surface;
        }
        if let Some(alt) = get(c.surface_alt) {
            v.faint_bg_color = alt;
        }
        if let Some(border) = get(c.border) {
            v.window_stroke.color = border;
            v.widgets.noninteractive.bg_stroke.color = border;
        }
        if let Some(border) = get(k.border).or(get(c.border)) {
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
            v.widgets.hovered.bg_stroke.color = border;
            v.widgets.active.bg_stroke.color = border;
            v.widgets.open.bg_stroke.color = border;
        }
        if let Some(text) = get(c.text) {
            v.widgets.noninteractive.fg_stroke.color = text;
        }
        if let Some(text) = get(k.text).or(get(c.text)) {
            v.widgets.inactive.fg_stroke.color = text;
            v.widgets.open.fg_stroke.color = text;
        }
        if let Some(strong) = get(k.text_hover).or(get(c.text_strong)) {
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
        // with `bg_fill` (set from `surface` above for idle controls).
        for (fill, widget, track) in [
            (get(k.background), &mut v.widgets.inactive, false),
            (get(k.hover), &mut v.widgets.hovered, true),
            (get(k.pressed), &mut v.widgets.active, true),
            (get(k.background), &mut v.widgets.open, true),
        ] {
            if let Some(fill) = fill {
                widget.weak_bg_fill = fill;
                if track || get(c.surface).is_none() {
                    widget.bg_fill = fill;
                }
            }
        }
        if let Some(link) = get(c.link) {
            v.hyperlink_color = link;
        }
        if let Some(warning) = get(c.warning) {
            v.warn_fg_color = warning;
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
        let p = &self.app.unamp;
        let or = |h: Option<Hex>, fallback: Color32| h.map(|h| h.0).unwrap_or(fallback);
        let spectrum_stops = match &p.spectrum {
            Some(stops) => stops.iter().map(|h| h.0).collect(),
            None => vec![
                Color32::from_rgb(30, 210, 30),
                Color32::from_rgb(230, 210, 30),
                Color32::from_rgb(230, 0, 30),
            ],
        };
        Palette {
            display: or(p.display, Color32::BLACK),
            time: or(p.time, v.strong_text_color()),
            title: or(p.title, v.strong_text_color()),
            spectrum_stops,
            spectrum_peak: or(p.spectrum_peak, Color32::from_gray(200)),
            radius: self.corner_radius.map(f32::from).unwrap_or(2.0),
            frame: self.frame_style(v),
        }
    }

    /// The window frame's colours: the `[window]` section, falling back to
    /// the panel colours and, for the window controls, the `[controls]`
    /// colours.
    fn frame_style(&self, v: &egui::Visuals) -> FrameStyle {
        let w = &self.window;
        let wc = &w.controls;
        let or = |h: Option<Hex>, fallback: Color32| h.map(|h| h.0).unwrap_or(fallback);
        FrameStyle {
            title_bar: or(w.title_bar, v.panel_fill),
            title_text: or(w.title_text, v.text_color()),
            border: or(w.border, v.window_stroke.color),
            radius: w.corner_radius.or(self.corner_radius).unwrap_or(GNOME_WINDOW_RADIUS),
            controls: WindowControlColors {
                icon: or(wc.icon, v.widgets.inactive.fg_stroke.color),
                icon_hover: or(wc.icon_hover, v.widgets.hovered.fg_stroke.color),
                hover: or(wc.hover, v.widgets.hovered.weak_bg_fill),
                pressed: or(wc.pressed, v.widgets.active.weak_bg_fill),
                close_hover: or(wc.close_hover, Color32::from_rgb(0xc0, 0x1c, 0x28)),
                close_icon_hover: or(wc.close_icon_hover, Color32::WHITE),
                radius: v.widgets.hovered.corner_radius,
            },
        }
    }
}

/// libadwaita's `--window-radius`, as shipped in Ubuntu's Yaru styles too.
const GNOME_WINDOW_RADIUS: u8 = 15;

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
/// a built-in's name replaces it. Returns the skins, and any load errors
/// and warnings.
pub fn load_all() -> (Vec<Skin>, Vec<String>) {
    let mut skins = vec![Skin::default_skin()];
    let mut errors = Vec::new();
    for (file, text) in BUILT_IN {
        match Skin::parse(text) {
            Ok(skin) => {
                errors.extend(skin.warnings.iter().map(|w| format!("built-in {file}: {w}")));
                skins.push(skin);
            }
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
                errors.extend(skin.warnings.iter().map(|w| format!("{name}: {w}")));
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
        assert_eq!(first.copied, vec!["steam-classic.toml", "photograph.toml"]);
        let path = dir.join("steam-classic.toml");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), BUILT_IN[0].1);

        std::fs::write(&path, "name = \"Steam Classic\"\n").unwrap();
        let second = export_built_ins(&dir).unwrap();
        assert!(second.copied.is_empty());
        assert_eq!(second.skipped, vec!["steam-classic.toml", "photograph.toml"]);
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
    fn built_in_skins_parse_cleanly() {
        for (file, text) in BUILT_IN {
            let skin = Skin::parse(text).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert!(skin.warnings.is_empty(), "{file}: {:?}", skin.warnings);
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
        assert_eq!(skin.palette(&v).frame.radius, 0);
    }

    #[test]
    fn unset_colours_keep_egui_defaults() {
        let skin = Skin::parse("format = 1\nname = \"Tiny\"\nbase = \"light\"\n[colors]\naccent = \"#FF0000\"").unwrap();
        let v = skin.visuals(egui::Visuals::light());
        assert_eq!(v.selection.bg_fill, Color32::RED);
        assert_eq!(v.panel_fill, egui::Visuals::light().panel_fill);
    }

    #[test]
    fn window_controls_are_themed_apart_from_controls() {
        let skin = Skin::parse(
            "format = 1\nname = \"W\"\nbase = \"dark\"\n\
             [controls]\nhover = \"#111111\"\n\
             [window.controls]\nhover = \"#222222\"\nclose_hover = \"#333333\"",
        )
        .unwrap();
        let v = skin.visuals(egui::Visuals::dark());
        assert_eq!(v.widgets.hovered.weak_bg_fill, Color32::from_gray(0x11));
        let frame = skin.palette(&v).frame;
        assert_eq!(frame.controls.hover, Color32::from_gray(0x22));
        assert_eq!(frame.controls.close_hover, Color32::from_gray(0x33));
    }

    #[test]
    fn window_controls_fall_back_to_control_colours() {
        let skin = Skin::parse("format = 1\nname = \"W\"\nbase = \"dark\"\n[controls]\nhover = \"#111111\"").unwrap();
        let v = skin.visuals(egui::Visuals::dark());
        assert_eq!(skin.palette(&v).frame.controls.hover, Color32::from_gray(0x11));
    }

    #[test]
    fn unknown_keys_are_warnings_and_other_apps_are_skipped() {
        let skin = Skin::parse(
            "format = 1\nname = \"X\"\n[colors]\nbakground = \"#000000\"\n\
             [app.photograph]\nloupe = \"#FFFFFF\"\n[app.unamp]\nspectrom = []",
        )
        .unwrap();
        assert_eq!(
            skin.warnings,
            ["unknown key `app.unamp.spectrom` ignored", "unknown key `colors.bakground` ignored"]
        );
    }

    #[test]
    fn bad_values_are_still_errors() {
        let err = Skin::parse("format = 1\nname = \"X\"\n[colors]\ntext = \"green\"").unwrap_err();
        assert!(err.contains("invalid colour"), "{err}");
    }

    #[test]
    fn newer_formats_load_with_a_warning() {
        let skin = Skin::parse("format = 7\nname = \"Future\"\n[hologram]\nglow = 3").unwrap();
        assert!(skin.warnings.iter().any(|w| w.contains("format 7")), "{:?}", skin.warnings);
    }

    #[test]
    fn the_old_format_is_rejected_with_directions() {
        let err = Skin::parse("name = \"Old\"\n[player]\ntime = \"#FFFFFF\"").unwrap_err();
        assert!(err.contains("old skin format") && err.contains("format = 1"), "{err}");
    }

    #[test]
    fn spectrum_gradient_runs_through_its_stops() {
        let skin = Skin::parse("format = 1\nname = \"S\"\n[app.unamp]\nspectrum = [\"#0000FF\", \"#00FF00\", \"#FF0000\"]").unwrap();
        let p = skin.palette(&egui::Visuals::dark());
        assert_eq!(p.spectrum(0.0), Color32::BLUE);
        assert_eq!(p.spectrum(0.5), Color32::GREEN);
        assert_eq!(p.spectrum(1.0), Color32::RED);
    }

    #[test]
    fn a_one_colour_spectrum_falls_back_to_the_default() {
        let skin = Skin::parse("format = 1\nname = \"S\"\n[app.unamp]\nspectrum = [\"#0000FF\"]").unwrap();
        assert_eq!(skin.palette(&egui::Visuals::dark()).spectrum_stops.len(), 3);
        assert_eq!(skin.warnings.len(), 1);
    }
}
