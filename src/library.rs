//! Folder browsing: the sidebar (locations, path bar, subfolders) and the
//! current folder's track list, whose tags are read on a worker thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::locations::{self, Location};
use crate::metadata::{self, TrackInfo};

pub const ROW_HEIGHT: f32 = 22.0;

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    pub name: String,
    /// `None` until the tag reader gets to it.
    pub info: Option<TrackInfo>,
}

impl Track {
    pub fn new(path: PathBuf) -> Self {
        let name = metadata::file_stem(&path);
        Self {
            path,
            name,
            info: None,
        }
    }

    pub fn title(&self) -> String {
        match &self.info {
            Some(info) => info.display_title(&self.path),
            None => self.name.clone(),
        }
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        self.info.as_ref().and_then(|i| i.duration)
    }
}

/// What the user did in a track list.
pub enum TrackAction {
    /// Play this folder from the given track.
    Play(usize),
    /// Put the track at the front of the up-next queue.
    PlayNext(usize),
    /// Put the track at the back of the up-next queue.
    AddToQueue(usize),
    /// Append the track to the playlist.
    AddToPlaylist(usize),
}

/// Labels shared by every track context menu, so they read the same everywhere.
pub const PLAY_NEXT: &str = "\u{23ED} Play next";
pub const ADD_TO_QUEUE: &str = "\u{2795} Add to queue";

struct InfoResult {
    generation: u64,
    index: usize,
    info: TrackInfo,
}

pub struct Library {
    pub current_dir: PathBuf,
    subdirs: Vec<(PathBuf, String)>,
    pub tracks: Vec<Track>,
    pending_nav: Option<PathBuf>,
    path_edit: String,
    path_error: Option<String>,
    scan_error: Option<String>,
    storage_locations: Vec<Location>,
    network_locations: Vec<Location>,
    /// Bumped on every scan so a slow tag reader for the previous folder is ignored.
    generation: u64,
    tx: mpsc::Sender<InfoResult>,
    rx: mpsc::Receiver<InfoResult>,
    selected: Option<usize>,
}

impl Library {
    /// Creates a library rooted at `initial_dir`, or ~/Music, or home.
    pub fn new(initial_dir: Option<PathBuf>, ctx: &egui::Context) -> Self {
        let dir = initial_dir.filter(|p| p.is_dir()).unwrap_or_else(|| {
            dirs::audio_dir()
                .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
        });
        let (tx, rx) = mpsc::channel();
        let mut lib = Self {
            path_edit: dir.display().to_string(),
            current_dir: dir,
            subdirs: Vec::new(),
            tracks: Vec::new(),
            pending_nav: None,
            path_error: None,
            scan_error: None,
            storage_locations: Vec::new(),
            network_locations: Vec::new(),
            generation: 0,
            tx,
            rx,
            selected: None,
        };
        lib.scan_locations();
        lib.scan(ctx);
        lib
    }

    fn scan_locations(&mut self) {
        let home = dirs::home_dir();
        let (storage, mut network) = locations::mounted_locations(home.as_deref());
        network.extend(locations::gvfs_locations());
        self.storage_locations = storage;
        self.network_locations = network;
    }

    fn scan(&mut self, ctx: &egui::Context) {
        self.subdirs.clear();
        self.tracks.clear();
        self.scan_error = None;
        self.selected = None;
        self.generation += 1;

        let rd = match std::fs::read_dir(&self.current_dir) {
            Ok(rd) => rd,
            Err(e) => {
                self.scan_error = Some(if e.kind() == std::io::ErrorKind::PermissionDenied {
                    "Cannot read this folder: permission denied".to_string()
                } else {
                    format!("Cannot read this folder: {e}")
                });
                return;
            }
        };

        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            // d_type from readdir avoids a stat per entry on network mounts;
            // only symlinks need following.
            let is_dir = match entry.file_type() {
                Ok(ft) if ft.is_symlink() => path.is_dir(),
                Ok(ft) => ft.is_dir(),
                Err(_) => false,
            };
            if is_dir {
                self.subdirs.push((path, name));
            } else if metadata::is_audio(&path) {
                self.tracks.push(Track::new(path));
            }
        }
        self.subdirs
            .sort_by_key(|(_, name)| name.to_lowercase());
        self.tracks.sort_by_key(|t| t.name.to_lowercase());

        // One sequential reader per folder: gentle on a NAS, and results
        // stream in top to bottom.
        let paths: Vec<PathBuf> = self.tracks.iter().map(|t| t.path.clone()).collect();
        let generation = self.generation;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (index, path) in paths.iter().enumerate() {
                let info = metadata::read_track_info(path);
                if tx.send(InfoResult { generation, index, info }).is_err() {
                    return;
                }
                if index % 8 == 7 {
                    ctx.request_repaint();
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn navigate(&mut self, dir: PathBuf) {
        self.pending_nav = Some(dir);
    }

    /// Applies pending navigation and incoming tags. Call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(dir) = self.pending_nav.take() {
            self.current_dir = dir;
            self.path_edit = self.current_dir.display().to_string();
            self.path_error = None;
            self.scan_locations();
            self.scan(ctx);
        }
        while let Ok(InfoResult { generation, index, info }) = self.rx.try_recv() {
            if generation == self.generation {
                if let Some(track) = self.tracks.get_mut(index) {
                    track.info = Some(info);
                }
            }
        }
    }

    /// Renders the left-hand sidebar: locations, path bar, subfolders.
    pub fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        let mut nav_to: Option<PathBuf> = None;

        ui.label(egui::RichText::new("LOCATIONS").weak().small());
        let mut places: Vec<(PathBuf, &str, &str)> = Vec::new();
        if let Some(home) = dirs::home_dir() {
            places.push((home, "\u{1F3E0}", "Home"));
        }
        if let Some(music) = dirs::audio_dir().filter(|p| p.is_dir()) {
            places.push((music, "\u{1F3B5}", "Music"));
        }
        places.push((PathBuf::from("/"), "\u{1F4BB}", "Computer"));
        for (path, icon, label) in places {
            let is_current = path == self.current_dir;
            if ui
                .selectable_label(is_current, format!("{icon} {label}"))
                .on_hover_text(path.display().to_string())
                .clicked()
            {
                nav_to = Some(path);
            }
        }
        ui.add_space(8.0);

        for (heading, icon, list) in [
            ("STORAGE", "\u{1F4BE}", &self.storage_locations),
            ("NETWORK", "\u{1F310}", &self.network_locations),
        ] {
            if list.is_empty() {
                continue;
            }
            ui.label(egui::RichText::new(heading).weak().small());
            for loc in list {
                let is_current = loc.path == self.current_dir;
                if ui
                    .selectable_label(is_current, format!("{icon} {}", loc.label))
                    .on_hover_text(&loc.detail)
                    .clicked()
                {
                    nav_to = Some(loc.path.clone());
                }
            }
            ui.add_space(8.0);
        }

        ui.separator();
        ui.add_space(4.0);

        // Editable path bar + up button
        ui.horizontal(|ui| {
            let has_parent = self.current_dir.parent().is_some();
            if ui
                .add_enabled(has_parent, egui::Button::new("\u{2B06}"))
                .on_hover_text("Parent folder")
                .clicked()
            {
                if let Some(p) = self.current_dir.parent() {
                    nav_to = Some(p.to_path_buf());
                }
            }

            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.path_edit)
                    .desired_width(ui.available_width())
                    .font(egui::TextStyle::Monospace),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let candidate =
                    locations::expand_typed_path(&self.path_edit, dirs::home_dir().as_deref());
                if candidate.is_dir() {
                    nav_to = Some(candidate);
                } else if candidate.is_file() {
                    if let Some(parent) = candidate.parent() {
                        nav_to = Some(parent.to_path_buf());
                    }
                } else {
                    self.path_error = Some(format!("No such folder: {}", candidate.display()));
                }
            }
            if resp.changed() {
                self.path_error = None;
            }
        });
        if let Some(err) = &self.path_error {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }

        ui.add_space(4.0);
        ui.separator();

        if !self.subdirs.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("FOLDERS").weak().small());
            for (path, name) in &self.subdirs {
                if ui.button(format!("\u{1F4C1} {name}")).clicked() {
                    nav_to = Some(path.clone());
                }
            }
        }

        if let Some(nav) = nav_to {
            self.navigate(nav);
        }
    }

    /// Renders the current folder's tracks. Double-click plays the folder
    /// from that track; the context menu can enqueue instead.
    pub fn show_tracks(&mut self, ui: &mut egui::Ui, playing: Option<&Path>) -> Option<TrackAction> {
        if let Some(err) = &self.scan_error {
            ui.centered_and_justified(|ui| ui.label(err.as_str()));
            return None;
        }
        if self.tracks.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(if self.subdirs.is_empty() {
                    "No music in this folder"
                } else {
                    "No music here — pick a folder on the left"
                })
            });
            return None;
        }

        let mut action = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, self.tracks.len(), |ui, range| {
                for index in range {
                    let track = &self.tracks[index];
                    let resp = track_row(
                        ui,
                        index,
                        &track.title(),
                        track.duration(),
                        playing == Some(track.path.as_path()),
                        self.selected == Some(index),
                    );
                    if resp.double_clicked() {
                        action = Some(TrackAction::Play(index));
                    } else if resp.clicked() {
                        self.selected = Some(index);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("\u{25B6} Play").clicked() {
                            action = Some(TrackAction::Play(index));
                        }
                        if ui.button(PLAY_NEXT).clicked() {
                            action = Some(TrackAction::PlayNext(index));
                        }
                        if ui.button(ADD_TO_QUEUE).clicked() {
                            action = Some(TrackAction::AddToQueue(index));
                        }
                        if ui.button("\u{1F3B6} Add to playlist").clicked() {
                            action = Some(TrackAction::AddToPlaylist(index));
                        }
                    });
                }
            });
        action
    }
}

/// One list row: number, title and duration, highlighted when it's the
/// playing track. Shared by the folder view and the playlist.
pub fn track_row(
    ui: &mut egui::Ui,
    index: usize,
    title: &str,
    duration: Option<std::time::Duration>,
    is_playing: bool,
    is_selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click());
    let visuals = ui.visuals();
    let painter = ui.painter_at(rect);

    if is_selected {
        painter.rect_filled(rect, visuals.widgets.inactive.corner_radius, visuals.selection.bg_fill);
    } else if resp.hovered() {
        painter.rect_filled(rect, visuals.widgets.inactive.corner_radius, visuals.widgets.hovered.weak_bg_fill);
    } else if index % 2 == 1 {
        painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
    }

    let text_color = if is_selected {
        visuals.selection.stroke.color
    } else if is_playing {
        visuals.hyperlink_color
    } else {
        visuals.text_color()
    };
    let body = egui::TextStyle::Body.resolve(ui.style());
    let mono = egui::TextStyle::Monospace.resolve(ui.style());
    let y = rect.center().y;

    let marker = if is_playing { "\u{25B6}" } else { "" };
    painter.text(
        egui::pos2(rect.left() + 6.0, y),
        egui::Align2::LEFT_CENTER,
        marker,
        body.clone(),
        text_color,
    );
    painter.text(
        egui::pos2(rect.left() + 46.0, y),
        egui::Align2::RIGHT_CENTER,
        format!("{}.", index + 1),
        mono.clone(),
        text_color,
    );
    let duration_text = duration.map(metadata::format_duration).unwrap_or_default();
    painter.text(
        egui::pos2(rect.right() - 8.0, y),
        egui::Align2::RIGHT_CENTER,
        duration_text,
        mono,
        text_color,
    );
    // Title, clipped before the duration column.
    let title_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 54.0, rect.top()),
        egui::pos2(rect.right() - 70.0, rect.bottom()),
    );
    ui.painter_at(title_rect).text(
        egui::pos2(title_rect.left(), y),
        egui::Align2::LEFT_CENTER,
        title,
        body,
        text_color,
    );
    resp
}
