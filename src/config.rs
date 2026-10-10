use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::eq;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
/// Persisted UI/application settings for UnAmp.
pub struct AppConfig {
    pub window_width: Option<f32>,
    pub window_height: Option<f32>,
    pub browse_path: Option<PathBuf>,
    pub volume: f32,
    /// Silenced without losing `volume`; unmuting restores it.
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// Show the time display as time remaining instead of elapsed.
    pub show_remaining: bool,
    pub eq_enabled: bool,
    pub eq_preamp: f32,
    pub eq_bands: [f32; eq::BANDS],
    /// Skin name, as in the skin file's `name`; "Default" is stock egui.
    pub skin: String,
    /// Which windows are open; their positions and sizes are kept by egui.
    pub show_player: bool,
    pub show_equalizer: bool,
    pub show_playlist: bool,
    pub show_library: bool,
    /// The Waveform window; off unless turned on from the Windows menu.
    pub show_waveform: bool,
    /// Let the desktop draw the window frame instead of UnAmp's own title
    /// bar, which follows the skin (see ADR-0020).
    pub system_title_bar: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_width: None,
            window_height: None,
            browse_path: None,
            volume: 0.8,
            muted: false,
            shuffle: false,
            repeat: Repeat::Off,
            show_remaining: false,
            eq_enabled: false,
            eq_preamp: 0.0,
            eq_bands: [0.0; eq::BANDS],
            skin: crate::skin::DEFAULT_SKIN.to_string(),
            show_player: true,
            show_equalizer: true,
            show_playlist: true,
            show_library: true,
            show_waveform: false,
            system_title_bar: false,
        }
    }
}

impl AppConfig {
    /// Returns the user config file path, if a config directory is available.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("unamp").join("config.toml"))
    }

    /// Loads config from disk, falling back to defaults on any error.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&contents).unwrap_or_default()
    }

    /// Writes config to disk, ignoring filesystem/serialization errors.
    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&path, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_cycles_off_all_one() {
        assert_eq!(Repeat::Off.next(), Repeat::All);
        assert_eq!(Repeat::All.next(), Repeat::One);
        assert_eq!(Repeat::One.next(), Repeat::Off);
    }

    #[test]
    fn partial_config_fills_defaults() {
        let cfg: AppConfig = toml::from_str("shuffle = true").unwrap();
        assert!(cfg.shuffle);
        assert_eq!(cfg.volume, 0.8);
        assert_eq!(cfg.repeat, Repeat::Off);
        assert!(!cfg.show_waveform, "the waveform window starts off");
    }
}
