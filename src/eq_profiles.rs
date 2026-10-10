//! Named EQ profiles the user saves, kept as TOML tables in their own file
//! (`~/.config/unamp/eq.toml`) so hand edits survive `config.toml` being
//! rewritten on exit:
//!
//! ```toml
//! ["Late Night"]
//! preamp = -2.0
//! bands = [3.0, 2.0, 0.0, -1.0, -2.0, -1.0, 0.0, 1.0, 1.0, 0.0]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::eq::{self, EqSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub preamp: f32,
    pub bands: [f32; eq::BANDS],
}

impl Profile {
    fn from_snapshot(snap: &EqSnapshot) -> Self {
        Self { preamp: snap.preamp, bands: snap.gains }
    }

    fn matches(&self, snap: &EqSnapshot) -> bool {
        self.preamp == snap.preamp && self.bands == snap.gains
    }

    /// Hand-edited values may be out of range; keep them on the sliders.
    fn clamped(self) -> Self {
        let clamp = |g: f32| g.clamp(-eq::MAX_DB, eq::MAX_DB);
        Self { preamp: clamp(self.preamp), bands: self.bands.map(clamp) }
    }
}

/// The user's profiles by name, sorted.
#[derive(Debug, Default)]
pub struct Profiles {
    pub by_name: BTreeMap<String, Profile>,
}

impl Profiles {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("unamp").join("eq.toml"))
    }

    /// Reads the profiles; a missing file is no profiles, a broken one is an error.
    pub fn load() -> Result<Self, String> {
        let Some(path) = Self::path() else {
            return Ok(Self::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => Self::parse(&contents).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    fn parse(contents: &str) -> Result<Self, toml::de::Error> {
        let by_name: BTreeMap<String, Profile> = toml::from_str(contents)?;
        Ok(Self { by_name: by_name.into_iter().map(|(n, p)| (n, p.clamped())).collect() })
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let s = toml::to_string_pretty(&self.by_name).map_err(|e| e.to_string())?;
        std::fs::write(&path, s).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn insert(&mut self, name: String, snap: &EqSnapshot) {
        self.by_name.insert(name, Profile::from_snapshot(snap));
    }

    pub fn remove(&mut self, name: &str) {
        self.by_name.remove(name);
    }

    /// Renames `from` to `to`. Returns false, changing nothing, if `to` is
    /// already taken or `from` doesn't exist.
    pub fn rename(&mut self, from: &str, to: String) -> bool {
        if self.by_name.contains_key(&to) {
            return false;
        }
        match self.by_name.remove(from) {
            Some(profile) => {
                self.by_name.insert(to, profile);
                true
            }
            None => false,
        }
    }

    /// Whether `snap` has moved away from the named profile: a user profile
    /// (bands and preamp) or else a built-in preset (bands only). `None` if
    /// there's no such profile.
    fn modified(&self, name: &str, snap: &EqSnapshot) -> Option<bool> {
        if let Some(profile) = self.by_name.get(name) {
            return Some(!profile.matches(snap));
        }
        eq::PRESETS.iter().find(|(n, _)| *n == name).map(|(_, gains)| *gains != snap.gains)
    }

    /// What to show as the current setting: the loaded profile's name, with
    /// " *" once the sliders have moved from it, or "Custom".
    pub fn label(&self, loaded: Option<&str>, snap: &EqSnapshot) -> String {
        match loaded.and_then(|name| Some((name, self.modified(name, snap)?))) {
            Some((name, false)) => name.to_string(),
            Some((name, true)) => format!("{name} *"),
            None => "Custom".to_string(),
        }
    }
}

/// The presets menu shared by the regular and classic Equalizer windows:
/// built-ins, the user's profiles (rename, delete), and "Save as".
/// `loaded` is the profile last loaded or saved. Loading edits `snap` and
/// pushes `SetEqProfile`; saving, renaming and deleting are pushed onto `out`.
pub fn menu(
    ui: &mut egui::Ui,
    profiles: &Profiles,
    loaded: Option<&str>,
    snap: &mut EqSnapshot,
    out: &mut Vec<Command>,
) {
    let draft_id = ui.id().with("eq_profile_name");
    let rename_id = ui.id().with("eq_profile_rename");
    let seen_id = ui.id().with("eq_profile_menu_seen");

    // Freshly opened: start "Save as" from the loaded profile's name, so
    // tweaking a slider and pressing Enter updates it, and drop any rename.
    let pass = ui.ctx().cumulative_pass_nr();
    let last_seen: Option<u64> = ui.data_mut(|d| d.get_temp(seen_id));
    if last_seen.is_none_or(|n| n + 1 < pass) {
        let current = loaded.filter(|n| profiles.by_name.contains_key(*n)).unwrap_or_default();
        ui.data_mut(|d| {
            d.insert_temp(draft_id, current.to_string());
            d.remove::<Option<(String, String)>>(rename_id);
        });
    }
    ui.data_mut(|d| d.insert_temp(seen_id, pass));

    for (name, gains) in eq::PRESETS {
        if ui.selectable_label(loaded == Some(*name), *name).clicked() {
            snap.gains = *gains;
            snap.enabled = true;
            out.push(Command::SetEqProfile(Some(name.to_string())));
            ui.close();
        }
    }

    if !profiles.by_name.is_empty() {
        ui.separator();
        // (profile being renamed, its new name so far)
        let mut renaming: Option<(String, String)> = ui.data_mut(|d| d.get_temp(rename_id)).flatten();
        for (name, profile) in &profiles.by_name {
            ui.horizontal(|ui| {
                if ui.small_button("🗑").on_hover_text(format!("Delete \"{name}\"")).clicked() {
                    out.push(Command::DeleteEqProfile(name.clone()));
                }
                match &mut renaming {
                    Some((from, to)) if from == name => {
                        let to_trimmed = to.trim().to_string();
                        let valid = !to_trimmed.is_empty()
                            && (to_trimmed == *name || !profiles.by_name.contains_key(&to_trimmed));
                        let mut edit = egui::TextEdit::singleline(to).desired_width(110.0);
                        if !valid {
                            edit = edit.text_color(ui.visuals().error_fg_color);
                        }
                        let resp = ui.add(edit);
                        resp.request_focus();
                        if !valid {
                            resp.on_hover_text("That name is taken");
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            renaming = None;
                        } else if valid && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if to_trimmed != *name {
                                out.push(Command::RenameEqProfile { from: name.clone(), to: to_trimmed });
                            }
                            renaming = None;
                        }
                    }
                    _ => {
                        if ui.small_button("✏").on_hover_text(format!("Rename \"{name}\"")).clicked() {
                            renaming = Some((name.clone(), name.clone()));
                        }
                        let label = ui.selectable_label(loaded == Some(name.as_str()), name);
                        if label.double_clicked() {
                            renaming = Some((name.clone(), name.clone()));
                        } else if label.clicked() {
                            snap.preamp = profile.preamp;
                            snap.gains = profile.bands;
                            snap.enabled = true;
                            out.push(Command::SetEqProfile(Some(name.clone())));
                            ui.close();
                        }
                    }
                }
            });
        }
        ui.data_mut(|d| d.insert_temp(rename_id, renaming));
    }

    ui.separator();
    let mut draft: String = ui.data_mut(|d| d.get_temp(draft_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        let edit = ui.add(egui::TextEdit::singleline(&mut draft).hint_text("Save as…").desired_width(110.0));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let name = draft.trim();
        let exists = profiles.by_name.contains_key(name);
        let save = ui.add_enabled(!name.is_empty(), egui::Button::new(if exists { "Overwrite" } else { "Save" }));
        if !name.is_empty() && (save.clicked() || enter) {
            out.push(Command::SaveEqProfile(name.to_string()));
            ui.close();
        }
    });
    ui.data_mut(|d| d.insert_temp(draft_id, draft));
}

/// Reset: flat bands and preamp, with "Flat" as the loaded preset.
pub fn reset(snap: &mut EqSnapshot, out: &mut Vec<Command>) {
    snap.gains = [0.0; eq::BANDS];
    snap.preamp = 0.0;
    out.push(Command::SetEqProfile(Some("Flat".to_string())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_tables_and_clamps() {
        let p = Profiles::parse(
            "[\"Late Night\"]\npreamp = -2.0\nbands = [3, 2, 0, -1, -2, -1, 0, 1, 1, 40]\n\
             [Podcast]\nbands = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]\n",
        )
        .unwrap();
        let late = p.by_name["Late Night"];
        assert_eq!(late.preamp, -2.0);
        assert_eq!(late.bands[9], eq::MAX_DB);
        assert_eq!(p.by_name["Podcast"].preamp, 0.0, "preamp is optional");
    }

    #[test]
    fn round_trips() {
        let mut p = Profiles::default();
        let snap = EqSnapshot { enabled: true, preamp: -1.5, gains: [1.0; eq::BANDS] };
        p.insert("Two Words".into(), &snap);
        let back = Profiles::parse(&toml::to_string_pretty(&p.by_name).unwrap()).unwrap();
        assert_eq!(back.by_name["Two Words"], Profile::from_snapshot(&snap));
    }

    #[test]
    fn rename_keeps_values_and_wont_clobber() {
        let mut p = Profiles::default();
        let a = EqSnapshot { enabled: true, preamp: -1.0, gains: [1.0; eq::BANDS] };
        let b = EqSnapshot { enabled: true, preamp: 0.0, gains: [2.0; eq::BANDS] };
        p.insert("A".into(), &a);
        p.insert("B".into(), &b);
        assert!(!p.rename("A", "B".into()), "taken name leaves it alone");
        assert_eq!(p.by_name["B"], Profile::from_snapshot(&b));
        assert!(p.rename("A", "Renamed".into()));
        assert_eq!(p.by_name["Renamed"], Profile::from_snapshot(&a));
        assert!(!p.by_name.contains_key("A"));
        assert!(!p.rename("A", "Again".into()), "gone");
    }

    #[test]
    fn label_follows_the_loaded_name() {
        let mut p = Profiles::default();
        let flat = EqSnapshot { enabled: true, preamp: 0.0, gains: [0.0; eq::BANDS] };
        assert_eq!(p.label(None, &flat), "Custom", "nothing loaded, even if it matches");
        assert_eq!(p.label(Some("Flat"), &flat), "Flat");
        let quieter = EqSnapshot { preamp: -3.0, ..flat };
        assert_eq!(p.label(Some("Flat"), &quieter), "Flat", "built-ins ignore the preamp");

        p.insert("Mine".into(), &flat);
        p.insert("Twin".into(), &flat);
        assert_eq!(p.label(Some("Twin"), &flat), "Twin", "same values, still the one loaded");
        assert_eq!(p.label(Some("Mine"), &quieter), "Mine *", "user profiles include the preamp");
        assert_eq!(p.label(Some("Gone"), &flat), "Custom");
    }
}
