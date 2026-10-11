//! Skins: the suite skin format, read by Patina's `patina::skin` (see
//! ADR-0030 and Patina's docs/skinning.md). This module adds UnAmp's own
//! part, the `[app.unamp]` section: the display and spectrum colours and the
//! classic Winamp bitmaps.
//!
//! Skins come from Patina's built-ins (Default, Dark Orange, Steam Classic
//! and the others), the shared `~/.config/patina/skins/`, and UnAmp's own
//! `~/.config/unamp/skins/`. Classic Winamp `.wsz` skins in either folder
//! are converted to TOML (see `wsz.rs`).

use std::path::PathBuf;

use egui::Color32;
use patina::frame::FrameStyle;
use patina::skin::Hex;
pub use patina::skin::{DEFAULT_SKIN, FORMAT, Skin};
use serde::Deserialize;

/// This app's section under `[app]`.
const APP: &str = "unamp";

/// UnAmp's own parts: the display, spectrum and classic bitmaps.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UnampSkin {
    /// A classic Winamp `.wsz`, relative to the skin file, whose bitmaps
    /// draw the Player, Equalizer and Playlist windows (see ADR-0010).
    pub classic: Option<String>,
    /// Background of the spectrum analyzer and seek bar.
    pub display: Option<Hex>,
    pub time: Option<Hex>,
    pub title: Option<Hex>,
    /// Bar colours from bottom to top, two or more.
    pub spectrum: Option<Vec<Hex>>,
    pub spectrum_peak: Option<Hex>,
}

/// The skin's `[app.unamp]` section, with anything worth telling its author.
/// A section that doesn't parse is reported and left out.
pub fn unamp_section(skin: &Skin) -> (UnampSkin, Vec<String>) {
    let (mut section, mut warnings) = match skin.app_section::<UnampSkin>(APP) {
        Ok(found) => found,
        Err(e) => (UnampSkin::default(), vec![e]),
    };
    if section.spectrum.as_ref().is_some_and(|s| s.len() < 2) {
        warnings.push("`app.unamp.spectrum` needs at least two colours; using the default".into());
        section.spectrum = None;
    }
    (section, warnings)
}

/// The classic `.wsz` file to draw windows from, if any.
pub fn classic(skin: &Skin) -> Option<String> {
    unamp_section(skin).0.classic
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
    /// The window frame (Patina's).
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

/// The colours UnAmp paints with under `skin`, in the visuals `v`.
pub fn palette(skin: &Skin, v: &egui::Visuals) -> Palette {
    let p = unamp_section(skin).0;
    let or = |h: Option<Hex>, fallback: Color32| h.map(|h| h.0).unwrap_or(fallback);
    let spectrum_stops = match &p.spectrum {
        Some(stops) => stops.iter().map(|h| h.0).collect(),
        None => vec![Color32::from_rgb(30, 210, 30), Color32::from_rgb(230, 210, 30), Color32::from_rgb(230, 0, 30)],
    };
    Palette {
        display: or(p.display, Color32::BLACK),
        time: or(p.time, v.strong_text_color()),
        title: or(p.title, v.strong_text_color()),
        spectrum_stops,
        spectrum_peak: or(p.spectrum_peak, Color32::from_gray(200)),
        radius: skin.corner_radius.map(f32::from).unwrap_or(2.0),
        frame: skin.frame_style(v),
    }
}

/// Applies the skin to egui and returns the colours for custom painting.
pub fn apply(skin: &Skin, ctx: &egui::Context) -> Palette {
    skin.apply(ctx);
    palette(skin, &ctx.global_style().visuals)
}

/// UnAmp's own skins folder, where `.wsz` files are converted. Skins for
/// every Patina app go in `patina::skin::shared_dir()`.
pub fn user_skins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("unamp").join("skins"))
}

/// Skin names earlier versions saved, and what they're called now.
pub fn current_name(name: &str) -> &str {
    match name {
        // UnAmp's Photograph skin became Patina's Dark Orange.
        "Photograph" => "Dark Orange",
        other => other,
    }
}

/// Default, Patina's built-ins, then the shared and UnAmp's own skins,
/// sorted by name. A later skin with an earlier one's name replaces it.
/// Returns the skins, and load errors and warnings to show.
pub fn load_all() -> (Vec<Skin>, Vec<String>) {
    let mut errors = Vec::new();
    let dirs: Vec<PathBuf> = patina::skin::shared_dir().into_iter().chain(user_skins_dir()).collect();
    for dir in &dirs {
        // Classic Winamp skins dropped in either folder become TOML first.
        errors.extend(crate::wsz::convert_new_in(dir));
    }
    let (skins, load_errors) = patina::skin::load_all(patina::skin::BUILT_IN, &dirs);
    errors.extend(load_errors);
    for skin in &skins {
        errors.extend(unamp_section(skin).1.into_iter().map(|w| format!("{}: {w}", skin.name)));
    }
    (skins, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patinas_built_ins_keep_unamps_sections() {
        let (skins, errors) = patina::skin::load_all(patina::skin::BUILT_IN, &[]);
        assert!(errors.is_empty(), "{errors:?}");
        for name in ["Dark Orange", "Steam Classic"] {
            let skin = skins.iter().find(|s| s.name == name).unwrap();
            let (section, warnings) = unamp_section(skin);
            assert!(warnings.is_empty(), "{name}: {warnings:?}");
            assert!(section.display.is_some(), "{name} colours the display");
        }
    }

    #[test]
    fn the_old_photograph_skin_is_dark_orange() {
        assert_eq!(current_name("Photograph"), "Dark Orange");
        assert_eq!(current_name("Steam Classic"), "Steam Classic");
    }

    #[test]
    fn spectrum_gradient_runs_through_its_stops() {
        let skin = Skin::parse("format = 1\nname = \"S\"\n[app.unamp]\nspectrum = [\"#0000FF\", \"#00FF00\", \"#FF0000\"]").unwrap();
        let p = palette(&skin, &egui::Visuals::dark());
        assert_eq!(p.spectrum(0.0), Color32::BLUE);
        assert_eq!(p.spectrum(0.5), Color32::GREEN);
        assert_eq!(p.spectrum(1.0), Color32::RED);
    }

    #[test]
    fn a_one_colour_spectrum_falls_back_to_the_default() {
        let skin = Skin::parse("format = 1\nname = \"S\"\n[app.unamp]\nspectrum = [\"#0000FF\"]").unwrap();
        assert_eq!(palette(&skin, &egui::Visuals::dark()).spectrum_stops.len(), 3);
        assert_eq!(unamp_section(&skin).1.len(), 1);
    }

    #[test]
    fn unknown_unamp_keys_are_warnings() {
        let skin = Skin::parse("format = 1\nname = \"S\"\n[app.unamp]\nsparkle = true").unwrap();
        let warnings = unamp_section(&skin).1;
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("sparkle"), "{warnings:?}");
    }
}
