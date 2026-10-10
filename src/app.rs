use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::appearance;
use crate::config::{AppConfig, Repeat};
use crate::display;
use crate::eq::{self, EqParams, EqSnapshot};
use crate::frame;
use crate::library::{self, Library, Track, TrackAction};
use crate::metadata::{self, TrackInfo};
use crate::player::{Engine, PlayState};
use crate::mpris::{self, Mpris};
use crate::command::{Command, PlaylistAction, Window, with_queue_marker};
use crate::playlist::Playlist;
use crate::reorder::{DragList, Reorder};
use crate::session;
use crate::classic::{self, ClassicSkin};
use crate::skin::{self, Palette, Skin};
use crate::visualizer::Visualizer;
use crate::waveform::{self, WaveColors, Waveforms};

const SIDEBAR_WIDTH: f32 = 200.0;
const ART_SIZE: f32 = 132.0;
/// Width of the player and equalizer windows' contents, so the classic
/// stack (player, equalizer, playlist) lines up.
const STACK_WIDTH: f32 = 540.0;
/// How far the arrow keys seek.
const SEEK_STEP_SECS: f64 = 5.0;
/// Id of the Media Library's search box, so Ctrl+F can focus it.
const SEARCH_BOX: &str = "library_search";

/// Fresh tags and cover for the now-playing track, read off the UI thread.
struct NowPlayingResult {
    generation: u64,
    info: TrackInfo,
    art: Option<image::RgbaImage>,
    /// `file://` URL of the art in UnAmp's cache, for media controls.
    art_url: Option<String>,
}

pub struct UnAmpApp {
    config: AppConfig,
    library: Library,
    playlist: Playlist,
    /// Playlist revision last written to disk.
    saved_revision: u64,
    /// Why the last save of the playlist/queue failed, shown in the status bar.
    session_error: Option<String>,
    engine: Engine,
    visualizer: Visualizer,
    waveforms: Waveforms,
    eq: Arc<EqParams>,
    skins: Vec<Skin>,
    /// Problems loading skin files, shown in the Skins menu.
    skin_errors: Vec<String>,
    /// Result of the last "Copy built-in skins to folder", shown in the menu.
    skin_notice: Option<String>,
    palette: Palette,
    /// The theme `palette` and the window frame were last set up for; the
    /// Default skin follows the system's. `None` until the first frame.
    frame_theme: Option<egui::Theme>,
    /// Bitmaps for the Player/EQ/Playlist when the skin has a `classic` file.
    classic: Option<ClassicSkin>,
    now_info: Option<TrackInfo>,
    art: Option<egui::TextureHandle>,
    art_url: Option<String>,
    /// Bumped per started track; MPRIS identifies tracks by it.
    track_serial: u64,
    mpris: Mpris,
    art_generation: u64,
    art_tx: mpsc::Sender<NowPlayingResult>,
    art_rx: mpsc::Receiver<NowPlayingResult>,
    /// Drag-to-reorder state shared by the playlist and up-next lists.
    reorder: Reorder,
    /// Seek-bar position while the handle is being dragged.
    seek_drag: Option<f32>,
    window_title: String,
}

impl UnAmpApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: AppConfig) -> Self {
        let library = Library::new(config.browse_path.clone(), &cc.egui_ctx);
        let playlist = session::dir()
            .map(|dir| Playlist::restore(session::load(&dir)))
            .unwrap_or_default();
        let saved_revision = playlist.revision();
        let eq = EqParams::new(config.eq_enabled, config.eq_preamp, config.eq_bands);
        let engine = Engine::new(effective_volume(&config), Arc::clone(&eq));
        let visualizer = Visualizer::new(engine.tap());
        let (art_tx, art_rx) = mpsc::channel();
        let (skins, mut skin_errors) = skin::load_all();
        let current = skins
            .iter()
            .find(|s| s.name == config.skin)
            .cloned()
            .unwrap_or_else(Skin::default_skin);
        let palette = current.apply(&cc.egui_ctx);
        appearance::watch(&cc.egui_ctx);
        let classic = load_classic(&current, &cc.egui_ctx, &mut skin_errors);
        Self {
            config,
            library,
            playlist,
            saved_revision,
            session_error: None,
            engine,
            visualizer,
            waveforms: Waveforms::default(),
            eq,
            skins,
            skin_errors,
            skin_notice: None,
            palette,
            frame_theme: None,
            classic,
            now_info: None,
            art: None,
            art_url: None,
            track_serial: 0,
            mpris: Mpris::start(&cc.egui_ctx),
            art_generation: 0,
            art_tx,
            art_rx,
            reorder: Reorder::default(),
            seek_drag: None,
            window_title: String::new(),
        }
    }

    fn start_track(&mut self, track: Track, ctx: &egui::Context) {
        self.engine.play(track.path.clone(), track.duration(), ctx);
        self.now_info = track.info.clone();
        self.track_serial += 1;
        self.art = None;
        self.art_url = None;

        // The playlist's copy of the tags may predate the folder's tag
        // reader finishing, so always read them fresh alongside the cover.
        self.art_generation += 1;
        let generation = self.art_generation;
        let tx = self.art_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let info = metadata::read_track_info(&track.path);
            let bytes = metadata::album_art_bytes(&track.path);
            let art = bytes.as_deref().and_then(metadata::decode_art);
            let art_url = bytes
                .as_deref()
                .filter(|_| art.is_some())
                .and_then(metadata::cache_art)
                .map(|p| mpris::file_url(&p));
            let _ = tx.send(NowPlayingResult { generation, info, art, art_url });
            ctx.request_repaint();
        });
    }

    fn poll_now_playing(&mut self, ctx: &egui::Context) {
        while let Ok(NowPlayingResult { generation, info, art, art_url }) = self.art_rx.try_recv() {
            if generation != self.art_generation {
                continue;
            }
            self.now_info = Some(info);
            self.art_url = art_url;
            self.art = art.map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                ctx.load_texture("album-art", color, egui::TextureOptions::LINEAR)
            });
        }
    }

    /// Plays what the library is showing (the folder, or search results)
    /// as the new playlist, starting at `index`.
    fn play_from_folder(&mut self, index: usize, ctx: &egui::Context) {
        self.playlist.replace(self.library.visible_tracks().to_vec(), index);
        if let Some(track) = self.playlist.now_playing().cloned() {
            self.start_track(track, ctx);
        }
    }

    fn next(&mut self, auto: bool, ctx: &egui::Context) {
        let next = self
            .playlist
            .advance(self.config.shuffle, self.config.repeat, auto)
            .cloned();
        match next {
            Some(track) => self.start_track(track, ctx),
            None => self.engine.stop(),
        }
    }

    fn previous(&mut self, ctx: &egui::Context) {
        // Like most players: restart the track unless we're near its start.
        if self.engine.position() > Duration::from_secs(3) {
            self.engine.seek(Duration::ZERO);
            return;
        }
        if let Some(track) = self.playlist.back().cloned() {
            self.start_track(track, ctx);
        }
    }

    /// Play button: resume, restart the current track, or start the
    /// playlist (or, with no playlist yet, the open folder).
    fn play(&mut self, ctx: &egui::Context) {
        match self.engine.state() {
            PlayState::Paused => self.engine.toggle_pause(),
            PlayState::Playing => self.engine.seek(Duration::ZERO),
            PlayState::Loading => {}
            PlayState::Stopped => {
                if let Some(track) = self.playlist.now_playing().cloned() {
                    self.start_track(track, ctx);
                } else if !self.playlist.tracks().is_empty() || !self.playlist.queue().is_empty() {
                    self.next(false, ctx);
                } else if !self.library.tracks.is_empty() {
                    self.play_from_folder(0, ctx);
                }
            }
        }
    }

    /// Carries out a command. Every front end goes through here: the
    /// regular windows, classic skins, keyboard shortcuts and MPRIS.
    fn apply(&mut self, command: Command, ctx: &egui::Context) {
        match command {
            Command::Previous => self.previous(ctx),
            Command::Play => self.play(ctx),
            Command::TogglePause => self.engine.toggle_pause(),
            Command::Stop => self.engine.stop(),
            Command::Next => self.next(false, ctx),
            Command::SeekTo(pos) => self.engine.seek(pos),
            Command::SeekBy(secs) => {
                let target = self.engine.position().as_secs_f64() + secs;
                self.engine.seek(Duration::from_secs_f64(target.max(0.0)));
            }
            Command::SetVolume(volume) => {
                self.config.volume = volume.clamp(0.0, 1.0);
                self.config.muted = false;
                self.apply_volume();
            }
            Command::ToggleMute => {
                self.config.muted = !self.config.muted;
                self.apply_volume();
            }
            Command::ToggleShuffle => self.config.shuffle = !self.config.shuffle,
            Command::SetShuffle(on) => self.config.shuffle = on,
            Command::CycleRepeat => self.config.repeat = self.config.repeat.next(),
            Command::SetRepeat(repeat) => self.config.repeat = repeat,
            Command::SetEq(snap) => {
                self.eq.set(snap);
                self.config.eq_enabled = snap.enabled;
                self.config.eq_preamp = snap.preamp;
                self.config.eq_bands = snap.gains;
            }
            Command::ToggleRemaining => self.config.show_remaining = !self.config.show_remaining,
            Command::OpenWindow(w) => *self.window_open(w) = true,
            Command::CloseWindow(w) => *self.window_open(w) = false,
            Command::ToggleWindow(w) => {
                let open = self.window_open(w);
                *open = !*open;
            }
            Command::Playlist(action) => self.apply_playlist_action(action, ctx),
            Command::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            Command::Raise => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn window_open(&mut self, window: Window) -> &mut bool {
        match window {
            Window::Player => &mut self.config.show_player,
            Window::Equalizer => &mut self.config.show_equalizer,
            Window::Playlist => &mut self.config.show_playlist,
            Window::Library => &mut self.config.show_library,
            Window::Waveform => &mut self.config.show_waveform,
        }
    }

    /// Sends the volume to the engine, as silence while muted.
    fn apply_volume(&self) {
        self.engine.set_volume(effective_volume(&self.config));
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.config.show_library = true;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_BOX)));
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let pressed = |key| ctx.input(|i| i.key_pressed(key));
        // Winamp's bottom row: Z prev, X play, C pause, V stop, B next.
        let keys = [
            (egui::Key::Z, Command::Previous),
            (egui::Key::X, Command::Play),
            (egui::Key::C, Command::TogglePause),
            (egui::Key::Space, Command::TogglePause),
            (egui::Key::V, Command::Stop),
            (egui::Key::B, Command::Next),
            (egui::Key::M, Command::ToggleMute),
            (egui::Key::ArrowRight, Command::SeekBy(SEEK_STEP_SECS)),
            (egui::Key::ArrowLeft, Command::SeekBy(-SEEK_STEP_SECS)),
        ];
        for (key, command) in keys {
            if pressed(key) {
                self.apply(command, ctx);
            }
        }
    }

    fn now_playing_title(&self) -> Option<String> {
        let path = self.engine.current()?;
        let title = match &self.now_info {
            Some(info) => info.display_title(path),
            None => metadata::file_stem(path),
        };
        // Queued tracks aren't at a playlist position, so they get no number.
        let number = self.playlist.playing_index().map(|i| format!("{}. ", i + 1)).unwrap_or_default();
        let length = self
            .engine
            .duration()
            .map(|d| format!(" ({})", metadata::format_duration(d)))
            .unwrap_or_default();
        Some(format!("{number}{title}{length}"))
    }

    fn update_window_title(&mut self, ctx: &egui::Context) {
        let title = match (self.engine.state(), self.engine.current()) {
            (PlayState::Stopped, _) | (_, None) => "UnAmp".to_string(),
            (_, Some(path)) => {
                let name = self
                    .now_info
                    .as_ref()
                    .map(|i| i.display_title(path))
                    .unwrap_or_else(|| metadata::file_stem(path));
                format!("{name} - UnAmp")
            }
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    fn show_player(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.set_width(STACK_WIDTH);
        ui.horizontal(|ui| {
            self.show_art(ui);
            ui.vertical(|ui| {
                let title = self
                    .now_playing_title()
                    .unwrap_or_else(|| "UnAmp".to_string());
                marquee(ui, &title, self.engine.state() == PlayState::Playing, self.palette.title);
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    self.show_time(ui);
                    ui.add_space(6.0);
                    self.visualizer
                        .show(ui, egui::vec2(176.0, 48.0), &self.palette)
                        .on_hover_text("Spectrum analyzer");
                    ui.add_space(6.0);
                    self.show_stream_info(ui);
                    // The main window's classic EQ / PL toggles, plus the library.
                    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        for (window, label, hint) in [
                            (Window::Equalizer, "EQ", "Equalizer"),
                            (Window::Playlist, "PL", "Playlist"),
                            (Window::Library, "ML", "Media Library"),
                        ] {
                            let mut open = *self.window_open(window);
                            if ui.toggle_value(&mut open, label).on_hover_text(hint).clicked() {
                                self.apply(Command::ToggleWindow(window), &ctx);
                            }
                        }
                    });
                });
                ui.add_space(4.0);
                self.show_seek_bar(ui);
                ui.add_space(4.0);
                self.show_transport(ui, &ctx);
            });
        });
    }

    fn show_art(&self, ui: &mut egui::Ui) {
        let size = egui::vec2(ART_SIZE, ART_SIZE);
        match &self.art {
            Some(tex) => {
                let img_size = tex.size_vec2();
                let scale = (ART_SIZE / img_size.x).min(ART_SIZE / img_size.y);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let draw = egui::Rect::from_center_size(rect.center(), img_size * scale);
                ui.painter().image(
                    tex.id(),
                    draw,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let visuals = ui.visuals();
                ui.painter().rect_filled(rect, visuals.widgets.inactive.corner_radius, visuals.extreme_bg_color);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "\u{1F3B5}",
                    egui::FontId::proportional(48.0),
                    visuals.weak_text_color(),
                );
            }
        }
    }

    fn show_time(&mut self, ui: &mut egui::Ui) {
        let shown = display::time_display(
            self.engine.state(),
            self.engine.position(),
            self.engine.duration(),
            self.config.show_remaining,
            ui.input(|i| i.time),
        );
        let (text, color) = match shown {
            display::TimeDisplay::Blank => ("  :  ".to_string(), self.palette.time),
            // Same width as the digits, drawn invisible, so nothing shifts.
            display::TimeDisplay::BlinkOff => ("  :  ".to_string(), egui::Color32::TRANSPARENT),
            display::TimeDisplay::Shown { negative, time } => {
                let sign = if negative { "-" } else { " " };
                (format!("{sign}{}", metadata::format_duration(time)), self.palette.time)
            }
        };
        let resp = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(format!("{text:>7}"))
                        .font(egui::FontId::monospace(30.0))
                        .color(color),
                )
                .sense(egui::Sense::click()),
            )
            .on_hover_text("Click to toggle elapsed / remaining");
        if resp.clicked() {
            let ctx = ui.ctx().clone();
            self.apply(Command::ToggleRemaining, &ctx);
        }
    }

    fn show_stream_info(&self, ui: &mut egui::Ui) {
        let Some(info) = self.now_info.as_ref().filter(|_| self.engine.current().is_some())
        else {
            return;
        };
        ui.vertical(|ui| {
            let small = |ui: &mut egui::Ui, s: String| {
                ui.label(egui::RichText::new(s).monospace().small().weak());
            };
            if let Some(kbps) = info.bitrate_kbps {
                small(ui, format!("{kbps} kbps"));
            }
            if let Some(rate) = info.sample_rate {
                small(ui, format!("{} kHz", rate / 1000));
            }
            match info.channels {
                Some(1) => small(ui, "mono".into()),
                Some(2) => small(ui, "stereo".into()),
                Some(n) => small(ui, format!("{n} ch")),
                None => {}
            }
        });
    }

    fn show_seek_bar(&mut self, ui: &mut egui::Ui) {
        let height = 14.0;
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        let duration = self.engine.duration().filter(|d| !d.is_zero());
        let fraction_at = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);

        if let Some(total) = duration {
            let pointer = resp.interact_pointer_pos().map(|p| fraction_at(p.x));
            if let Some(f) = display::seek_drag(&resp, pointer, &mut self.seek_drag) {
                let ctx = ui.ctx().clone();
                self.apply(Command::SeekTo(total.mul_f32(f)), &ctx);
            }
        }
        let fraction = display::seek_fraction(self.seek_drag, self.engine.position(), duration);

        let visuals = ui.visuals();
        let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
        ui.painter().rect_filled(track, visuals.widgets.inactive.corner_radius, visuals.extreme_bg_color);
        if duration.is_some() {
            let filled = egui::Rect::from_min_max(
                track.min,
                egui::pos2(track.left() + track.width() * fraction, track.max.y),
            );
            ui.painter().rect_filled(filled, visuals.widgets.inactive.corner_radius, visuals.selection.bg_fill);
            // Keep the handle inside the bar at either end.
            let handle_w = 24.0;
            let handle_x = rect.left() + handle_w / 2.0 + (rect.width() - handle_w) * fraction;
            let handle = egui::Rect::from_center_size(
                egui::pos2(handle_x, rect.center().y),
                egui::vec2(handle_w, height - 2.0),
            );
            let style = if resp.hovered() || resp.dragged() {
                &visuals.widgets.hovered
            } else {
                &visuals.widgets.inactive
            };
            ui.painter().rect_filled(handle, style.corner_radius, style.bg_fill);
            ui.painter()
                .rect_stroke(handle, style.corner_radius, style.bg_stroke, egui::StrokeKind::Inside);

            if let (Some(total), Some(hover)) = (duration, resp.hover_pos()) {
                resp.on_hover_text_at_pointer(metadata::format_duration(
                    total.mul_f32(fraction_at(hover.x)),
                ));
            }
        }
    }

    fn show_transport(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let big = |s: &str| egui::RichText::new(s).size(16.0);
            let buttons = [
                ("\u{23EE}", "Previous (Z)", Command::Previous),
                ("\u{25B6}", "Play (X)", Command::Play),
                ("\u{23F8}", "Pause (C)", Command::TogglePause),
                ("\u{23F9}", "Stop (V)", Command::Stop),
                ("\u{23ED}", "Next (B)", Command::Next),
            ];
            for (icon, hint, command) in buttons {
                if ui.button(big(icon)).on_hover_text(hint).clicked() {
                    self.apply(command, ctx);
                }
            }
            ui.separator();
            let mut shuffle = self.config.shuffle;
            if ui
                .toggle_value(&mut shuffle, big("\u{1F500}"))
                .on_hover_text(if self.config.shuffle { "Shuffle: on" } else { "Shuffle: off" })
                .clicked()
            {
                self.apply(Command::ToggleShuffle, ctx);
            }
            let (icon, label) = match self.config.repeat {
                Repeat::Off => ("\u{1F501}", "Repeat: off"),
                Repeat::All => ("\u{1F501}", "Repeat: all"),
                Repeat::One => ("\u{1F502}", "Repeat: one"),
            };
            if ui
                .selectable_label(self.config.repeat != Repeat::Off, big(icon))
                .on_hover_text(format!("{label} (click to cycle)"))
                .clicked()
            {
                self.apply(Command::CycleRepeat, ctx);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().slider_width = 90.0;
                // Muted shows as 0; the level underneath is kept for unmuting.
                let mut shown = effective_volume(&self.config);
                let resp = ui.add(
                    egui::Slider::new(&mut shown, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if resp.changed() {
                    self.apply(Command::SetVolume(shown), ctx);
                }
                resp.on_hover_text(if self.config.muted {
                    format!("Muted (unmutes to {:.0}%)", self.config.volume * 100.0)
                } else {
                    format!("Volume {:.0}%", self.config.volume * 100.0)
                });
                let speaker = ui
                    .add(egui::Button::new(speaker_icon(&self.config)).frame(false))
                    .on_hover_text(if self.config.muted { "Unmute (M)" } else { "Mute (M)" });
                if speaker.clicked() {
                    self.apply(Command::ToggleMute, ctx);
                }
            });
        });
    }

    fn show_library(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::left("library_sidebar")
            .resizable(true)
            .default_size(SIDEBAR_WIDTH)
            .min_size(140.0)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                right: 8,
                ..Default::default()
            }))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.library.show_sidebar(ui));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 8,
                ..Default::default()
            }))
            .show(ui, |ui| self.show_folder_view(ui, ctx));
    }

    fn show_search_bar(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let search = &mut self.library.search;
        let active = search.is_active();
        // Right to left, so the clear button takes its width first and the
        // box fills exactly what's left (a guessed width made the resizable
        // window grow a few pixels every frame).
        // (Inside a horizontal row so the layout only takes one line.)
        ui.horizontal(|ui| ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if active && ui.button("\u{2716}").on_hover_text("Clear search (Esc)").clicked() {
                search.clear();
            }
            let hint = format!("\u{1F50D} Search {}  (Ctrl+F)", self.library.tree.root_label());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut search.query)
                    .id(egui::Id::new(SEARCH_BOX))
                    .hint_text(hint)
                    .desired_width(ui.available_width()),
            );
            if resp.changed() {
                search.edited(now);
            }
            if resp.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    search.submit();
                } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    search.clear();
                }
            }
        }));
    }

    fn show_folder_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.show_search_bar(ui);
        ui.add_space(4.0);
        let searching = self.library.search.is_active();

        ui.horizontal(|ui| {
            if searching {
                ui.label(egui::RichText::new(format!("In {}", self.library.tree.root_label())).strong().size(16.0));
            } else {
                let name = self
                    .library
                    .current_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| self.library.current_dir.display().to_string());
                ui.label(egui::RichText::new(name).strong().size(16.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let has_tracks = !self.library.visible_tracks().is_empty();
                let what = if searching { "results" } else { "folder" };
                ui.add_enabled_ui(has_tracks, |ui| {
                    ui.menu_button(format!("\u{2795} Add {what} \u{23F7}"), |ui| {
                        if ui.button("to queue").clicked() {
                            self.playlist.add_to_queue(self.library.visible_tracks().iter().cloned());
                        }
                        if ui.button("to playlist").clicked() {
                            self.playlist.append(self.library.visible_tracks().iter().cloned());
                        }
                    });
                });
                if ui
                    .add_enabled(has_tracks, egui::Button::new(format!("\u{25B6} Play {what}")))
                    .clicked()
                {
                    self.play_from_folder(0, ctx);
                }
            });
        });

        if searching {
            let search = &self.library.search;
            let mut status = format!("{} tracks", search.tracks.len());
            if !search.folders.is_empty() {
                status.push_str(&format!(", {} folders", search.folders.len()));
            }
            if search.running {
                status.push_str(&format!(" · searching ({} folders read)", search.scanned));
            } else if search.truncated {
                status.push_str(" · stopped at the limit, refine the search");
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(status).weak());
                if search.running {
                    ui.spinner();
                }
            });
        }

        // Folders whose path matches: open one to browse it.
        if searching && !self.library.search.folders.is_empty() {
            let root = self.library.tree.root().to_path_buf();
            let mut open = None;
            egui::ScrollArea::vertical()
                .id_salt("search_folders")
                .max_height(64.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for folder in &self.library.search.folders {
                            let rel = folder.strip_prefix(&root).unwrap_or(folder);
                            if ui
                                .button(format!("\u{1F4C1} {}", rel.display()))
                                .on_hover_text("Open this folder")
                                .clicked()
                            {
                                open = Some(folder.clone());
                            }
                        }
                    });
                });
            if let Some(folder) = open {
                self.library.search.clear();
                self.library.navigate(folder);
            }
        }
        ui.separator();

        let playing = self.engine.current().map(|p| p.to_path_buf());
        let action = self.library.show_tracks(ui, playing.as_deref());
        let track = |i: usize| self.library.visible_tracks().get(i).cloned();
        match action {
            Some(TrackAction::Play(index)) => self.play_from_folder(index, ctx),
            Some(TrackAction::PlayNext(index)) => {
                if let Some(t) = track(index) {
                    self.playlist.play_next(t);
                }
            }
            Some(TrackAction::AddToQueue(index)) => {
                if let Some(t) = track(index) {
                    self.playlist.add_to_queue([t]);
                }
            }
            Some(TrackAction::AddToPlaylist(index)) => {
                if let Some(t) = track(index) {
                    self.playlist.append([t]);
                }
            }
            None => {}
        }
    }

    fn show_playlist_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.playlist.queue().is_empty() {
            self.show_up_next(ui, ctx);
            ui.add_space(6.0);
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Playlist").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!self.playlist.tracks().is_empty(), egui::Button::new("Clear"))
                    .clicked()
                {
                    self.playlist.clear();
                }
                let total: Duration = self.playlist.tracks().iter().filter_map(|t| t.duration()).sum();
                ui.label(
                    egui::RichText::new(format!(
                        "{} tracks, {}",
                        self.playlist.tracks().len(),
                        metadata::format_duration(total)
                    ))
                    .weak(),
                );
            });
        });
        ui.separator();

        if self.playlist.tracks().is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("Playlist is empty — double-click a track in the Media Library");
            });
            return;
        }

        let mut action: Option<PlaylistAction> = None;
        let playing_index = self.playlist.playing_index();
        let playing = self.engine.state() != PlayState::Stopped;
        let output = egui::ScrollArea::vertical()
            .id_salt("playlist_rows")
            .auto_shrink([false, false])
            .show_rows(ui, library::ROW_HEIGHT, self.playlist.tracks().len(), |ui, range| {
                for index in range {
                    let track = &self.playlist.tracks()[index];
                    let title = with_queue_marker(self.playlist.queue_position(&track.path), track.title());
                    let resp = library::track_row(
                        ui,
                        index,
                        &title,
                        track.duration(),
                        playing && playing_index == Some(index),
                        self.reorder.is_dragging(DragList::Playlist, index),
                        egui::Sense::click_and_drag(),
                    );
                    self.reorder.row_response(ui, DragList::Playlist, index, &resp);
                    if resp.double_clicked() {
                        action = Some(PlaylistAction::Play(index));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("\u{25B6} Play").clicked() {
                            action = Some(PlaylistAction::Play(index));
                        }
                        if ui.button(library::PLAY_NEXT).clicked() {
                            action = Some(PlaylistAction::PlayNext(index));
                        }
                        if ui.button(library::ADD_TO_QUEUE).clicked() {
                            action = Some(PlaylistAction::AddToQueue(index));
                        }
                        if ui.button("\u{2796} Remove").clicked() {
                            action = Some(PlaylistAction::Remove(index));
                        }
                    });
                }
                if let Some(dir) = self.reorder.edge_scroll(ui, DragList::Playlist, ui.clip_rect(), library::ROW_HEIGHT) {
                    ui.scroll_with_delta(egui::vec2(0.0, dir * 6.0));
                }
            });
        let line = ui.visuals().selection.bg_fill;
        if let Some((from, to)) = self.reorder.finish(ui, DragList::Playlist, output.inner_rect, line) {
            action = Some(PlaylistAction::Move { from, to });
        }
        if let Some(action) = action {
            self.apply_playlist_action(action, ctx);
        }
    }

    /// The up-next queue, above the playlist while it has anything in it.
    fn show_up_next(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Up next ({})", self.playlist.queue().len())).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear queue").clicked() {
                    self.playlist.clear_queue();
                }
            });
        });
        ui.separator();

        enum QueueAction {
            PlayNow(usize),
            MoveToFront(usize),
            Remove(usize),
        }
        let mut action = None;
        let rows = self.playlist.queue().len();
        let output = egui::ScrollArea::vertical()
            .id_salt("up_next_rows")
            .max_height(library::ROW_HEIGHT * 5.0)
            .auto_shrink([false, true])
            .show_rows(ui, library::ROW_HEIGHT, rows, |ui, range| {
                for index in range {
                    let track = &self.playlist.queue()[index];
                    let dragging = self.reorder.is_dragging(DragList::Queue, index);
                    let resp = library::track_row(
                        ui,
                        index,
                        &track.title(),
                        track.duration(),
                        false,
                        dragging,
                        egui::Sense::click_and_drag(),
                    );
                    self.reorder.row_response(ui, DragList::Queue, index, &resp);
                    if resp.double_clicked() {
                        action = Some(QueueAction::PlayNow(index));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("\u{25B6} Play now").clicked() {
                            action = Some(QueueAction::PlayNow(index));
                        }
                        if index > 0 && ui.button(library::PLAY_NEXT).clicked() {
                            action = Some(QueueAction::MoveToFront(index));
                        }
                        if ui.button("\u{2796} Remove from queue").clicked() {
                            action = Some(QueueAction::Remove(index));
                        }
                    });
                }
                if let Some(dir) = self.reorder.edge_scroll(ui, DragList::Queue, ui.clip_rect(), library::ROW_HEIGHT) {
                    ui.scroll_with_delta(egui::vec2(0.0, dir * 6.0));
                }
            });
        let line = ui.visuals().selection.bg_fill;
        if let Some((from, to)) = self.reorder.finish(ui, DragList::Queue, output.inner_rect, line) {
            self.playlist.move_in_queue(from, to);
        }

        match action {
            Some(QueueAction::PlayNow(index)) => {
                if let Some(track) = self.playlist.play_from_queue(index).cloned() {
                    self.start_track(track, ctx);
                }
            }
            Some(QueueAction::MoveToFront(index)) => self.playlist.move_to_front(index),
            Some(QueueAction::Remove(index)) => self.playlist.remove_from_queue(index),
            None => {}
        }
    }

    /// Playlist-entry actions shared by the egui and classic playlists.
    fn apply_playlist_action(&mut self, action: PlaylistAction, ctx: &egui::Context) {
        match action {
            PlaylistAction::Play(index) => {
                if let Some(track) = self.playlist.jump(index).cloned() {
                    self.start_track(track, ctx);
                }
            }
            PlaylistAction::PlayNext(index) => {
                if let Some(track) = self.playlist.tracks().get(index).cloned() {
                    self.playlist.play_next(track);
                }
            }
            PlaylistAction::AddToQueue(index) => {
                if let Some(track) = self.playlist.tracks().get(index).cloned() {
                    self.playlist.add_to_queue([track]);
                }
            }
            PlaylistAction::Move { from, to } => self.playlist.move_track(from, to),
            PlaylistAction::Remove(index) => {
                let was_playing = self.playlist.playing_index() == Some(index);
                self.playlist.remove(index);
                if was_playing {
                    self.engine.stop();
                }
            }
        }
    }
}

impl UnAmpApp {
    fn show_classic_windows(
        &mut self,
        ctx: &egui::Context,
        skin: &mut ClassicSkin,
        origin: egui::Pos2,
    ) {
        let title = self.now_playing_title().unwrap_or_else(|| "UnAmp".to_string());
        let info = self.now_info.as_ref().filter(|_| self.engine.current().is_some());
        let (bars, peaks) = self.visualizer.levels();
        let queued: Vec<Option<usize>> = self
            .playlist
            .tracks()
            .iter()
            .map(|t| self.playlist.queue_position(&t.path))
            .collect();
        let view = classic::View {
            status: self.status(),
            title: &title,
            kbps: info.and_then(|i| i.bitrate_kbps),
            khz: info.and_then(|i| i.sample_rate).map(|r| r / 1000),
            channels: info.and_then(|i| i.channels),
            eq_open: self.config.show_equalizer,
            pl_open: self.config.show_playlist,
            eq: self.eq.snapshot(),
            bars,
            peaks,
            tracks: self.playlist.tracks(),
            current: self.playlist.playing_index(),
            queued: &queued,
            total: self.playlist.tracks().iter().filter_map(|t| t.duration()).sum(),
            time: ctx.input(|i| i.time),
        };

        // Fixed-size, frameless windows stacked like Winamp's; drag them
        // by any part that isn't a control.
        let mut actions = Vec::new();
        // Unconstrained: egui's desktop constraint pushed the frameless
        // playlist up over the equalizer even when the stack fit (measured
        // 2026-10-09). Windows → Reset layout recovers a window lost off-screen.
        let window = |id: &str, y: f32, size: egui::Vec2| {
            egui::Window::new(id)
                .id(egui::Id::new(id))
                .title_bar(false)
                .frame(egui::Frame::NONE)
                .resizable(false)
                .fixed_size(size * classic::SCALE)
                .default_pos(origin + egui::vec2(0.0, y))
                .constrain(false)
        };
        let row = classic::MAIN_SIZE.y * classic::SCALE;
        if self.config.show_player {
            window("classic_player", 0.0, classic::MAIN_SIZE).show(ctx, |ui| classic::show_player(ui, skin, &view, &mut actions));
        }
        if self.config.show_equalizer {
            window("classic_equalizer", row, classic::EQ_SIZE).show(ctx, |ui| classic::show_eq(ui, skin, &view, &mut actions));
        }
        if self.config.show_playlist {
            window("classic_playlist", row * 2.0, classic::PLAYLIST_SIZE).show(ctx, |ui| classic::show_playlist(ui, skin, &view, &mut self.reorder, &mut actions));
        }

        for command in actions {
            self.apply(command, ctx);
        }
    }

    fn show_skins_menu(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        for skin in &self.skins {
            let label = match &skin.author {
                Some(author) if skin.path.is_some() => format!("{} — {author}", skin.name),
                _ => skin.name.clone(),
            };
            let resp = ui.radio(self.config.skin == skin.name, label);
            let resp = match &skin.path {
                Some(path) => resp.on_hover_text(path.display().to_string()),
                None => resp,
            };
            if resp.clicked() {
                chosen = Some(skin.clone());
            }
        }
        if let Some(skin) = chosen {
            self.palette = skin.apply(ui.ctx());
            self.classic = load_classic(&skin, ui.ctx(), &mut self.skin_errors);
            self.config.skin = skin.name;
        }

        ui.separator();
        if ui.button("Reload skins").clicked() {
            self.reload_skins(ui.ctx());
        }
        if let Some(dir) = skin::user_skins_dir() {
            if ui
                .button("Copy built-in skins to folder")
                .on_hover_text("Puts editable copies in the skins folder; never overwrites existing files")
                .clicked()
            {
                self.skin_notice = Some(match skin::export_built_ins(&dir) {
                    Ok(export) => {
                        // The copies share the built-ins' names, so after a
                        // reload they're the versions in use.
                        self.reload_skins(ui.ctx());
                        let mut parts = Vec::new();
                        if !export.copied.is_empty() {
                            parts.push(format!("Copied {}", export.copied.join(", ")));
                        }
                        if !export.skipped.is_empty() {
                            parts.push(format!("kept your existing {}", export.skipped.join(", ")));
                        }
                        parts.join("; ")
                    }
                    Err(e) => format!("Couldn't copy skins: {e}"),
                });
            }
            if ui
                .button("Open skins folder")
                .on_hover_text(dir.display().to_string())
                .clicked()
            {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
            }
        }
        if let Some(notice) = &self.skin_notice {
            ui.label(egui::RichText::new(notice).weak());
        }
        if !self.skin_errors.is_empty() {
            ui.separator();
            for err in &self.skin_errors {
                ui.colored_label(ui.visuals().error_fg_color, err);
            }
        }
    }

    /// Writes the playlist and queue to disk if they changed since the last
    /// save. A failure is reported and retried on the next change, not
    /// every frame.
    fn save_session(&mut self) {
        if self.playlist.revision() == self.saved_revision {
            return;
        }
        self.saved_revision = self.playlist.revision();
        let Some(dir) = session::dir() else {
            return;
        };
        self.session_error = session::save(&dir, &self.playlist.session())
            .err()
            .map(|e| format!("Couldn't save playlist: {e}"));
    }

    /// What media controls should show right now, and the position in µs.
    /// The player state all front ends show, gathered in one place.
    fn status(&self) -> display::PlayerStatus {
        display::PlayerStatus {
            state: self.engine.state(),
            position: self.engine.position(),
            duration: self.engine.duration(),
            volume: effective_volume(&self.config),
            shuffle: self.config.shuffle,
            repeat: self.config.repeat,
            show_remaining: self.config.show_remaining,
        }
    }

    fn mpris_state(&self) -> (mpris::State, i64) {
        let status = self.status();
        let state = status.state;
        let current = self.engine.current().filter(|_| state != PlayState::Stopped);
        let info = self.now_info.as_ref();
        let has_lists = !self.playlist.tracks().is_empty() || !self.playlist.queue().is_empty();
        let playback = match state {
            PlayState::Playing => mpris::Status::Playing,
            PlayState::Paused => mpris::Status::Paused,
            // Loading is the gap between tracks; reporting it as playing
            // stops the panel flickering to stopped on every track change.
            PlayState::Loading => mpris::Status::Playing,
            PlayState::Stopped => mpris::Status::Stopped,
        };
        let mpris_state = mpris::State {
            status: playback,
            track_id: current.map(|_| format!("/org/unamp/track/{}", self.track_serial)),
            title: current.map(|p| info.and_then(|i| i.title.clone()).unwrap_or_else(|| metadata::file_stem(p))),
            artist: info.and_then(|i| i.artist.clone()).filter(|_| current.is_some()),
            album: info.and_then(|i| i.album.clone()).filter(|_| current.is_some()),
            length_us: status.duration.filter(|_| current.is_some()).map(|d| d.as_micros() as i64),
            art_url: self.art_url.clone().filter(|_| current.is_some()),
            volume: status.volume as f64,
            shuffle: status.shuffle,
            repeat: status.repeat,
            can_go_next: has_lists,
            can_go_previous: current.is_some(),
            can_play: current.is_some() || has_lists || !self.library.visible_tracks().is_empty(),
            can_seek: current.is_some() && status.duration.is_some(),
        };
        (mpris_state, status.position.as_micros() as i64)
    }

    /// Carries out a request from GNOME's media controls, media keys, etc.
    fn apply_mpris(&mut self, request: mpris::Command, ctx: &egui::Context) {
        let track_id = format!("/org/unamp/track/{}", self.track_serial);
        let now = mpris::Playback {
            state: self.engine.state(),
            position: self.engine.position(),
            duration: self.engine.duration(),
            track_id: Some(&track_id),
        };
        if let Some(command) = mpris::to_player_command(request, &now) {
            self.apply(command, ctx);
        }
    }

    /// The current track's waveform, decoded on demand while this window is open.
    fn show_waveform(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(path) = self.engine.current().filter(|_| self.engine.state() != PlayState::Stopped) else {
            ui.centered_and_justified(|ui| ui.weak("Play something to see its waveform"));
            return;
        };
        let wf = self.waveforms.get(path, ctx);
        let wf = wf.lock().unwrap_or_else(|e| e.into_inner());
        let total = self.engine.duration().map(|d| d.as_secs_f64()).unwrap_or(0.0);

        ui.horizontal(|ui| {
            let title = self.now_playing_title().unwrap_or_default();
            ui.label(egui::RichText::new(title).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(err) = &wf.error {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Can't draw: {err}"));
                } else if !wf.done {
                    let pct = if total > 0.0 { (wf.decoded_secs() / total * 100.0).min(99.0) } else { 0.0 };
                    ui.weak(format!("Reading track… {pct:.0}%"));
                    ui.spinner();
                }
            });
        });

        let colors = self.wave_colors(ui.visuals());
        let position = self.engine.position().as_secs_f64();
        if let Some(target) = waveform::show(ui, &wf, total, position, &colors) {
            drop(wf);
            self.apply(Command::SeekTo(Duration::from_secs_f64(target)), ctx);
        }
    }

    /// Waveform colours: a classic skin's playlist colours (current track,
    /// normal text, list background), else the skin's accent on its inset
    /// background.
    fn wave_colors(&self, visuals: &egui::Visuals) -> WaveColors {
        match &self.classic {
            Some(skin) => WaveColors {
                background: skin.pl.normal_bg,
                center: skin.pl.normal.gamma_multiply(0.25),
                played: skin.pl.current,
                unplayed: skin.pl.normal,
                playhead: skin.pl.current,
            },
            None => WaveColors {
                background: visuals.extreme_bg_color,
                center: visuals.weak_text_color().gamma_multiply(0.4),
                played: visuals.selection.bg_fill,
                unplayed: visuals.weak_text_color(),
                playhead: visuals.strong_text_color(),
            },
        }
    }

    /// Rescans built-in and user skins and re-applies the current one.
    fn reload_skins(&mut self, ctx: &egui::Context) {
        let (skins, errors) = skin::load_all();
        self.skins = skins;
        self.skin_errors = errors;
        let current = self
            .skins
            .iter()
            .find(|s| s.name == self.config.skin)
            .cloned()
            .unwrap_or_else(Skin::default_skin);
        self.config.skin = current.name.clone();
        self.palette = current.apply(ctx);
        self.classic = load_classic(&current, ctx, &mut self.skin_errors);
    }

    /// Keeps the palette and the window frame in step with the active theme,
    /// which changes with the desktop's light/dark setting on the Default
    /// skin (see ADR-0019) or with a skin switch. On Wayland the frame is
    /// drawn by winit, which only reads the desktop setting when the window
    /// opens, so it's told the theme explicitly.
    fn follow_theme(&mut self, ctx: &egui::Context) {
        let theme = ctx.theme();
        if self.frame_theme == Some(theme) {
            return;
        }
        if self.frame_theme.is_some() {
            let current = self.skins.iter().find(|s| s.name == self.config.skin).cloned();
            self.palette = current.unwrap_or_else(Skin::default_skin).apply(ctx);
        }
        self.frame_theme = Some(theme);
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(match theme {
            egui::Theme::Dark => egui::SystemTheme::Dark,
            egui::Theme::Light => egui::SystemTheme::Light,
        }));
    }

    fn show_equalizer(&mut self, ui: &mut egui::Ui) {
        ui.set_width(STACK_WIDTH);
        let before = self.eq.snapshot();
        let mut snap = before;

        ui.horizontal(|ui| {
            ui.toggle_value(&mut snap.enabled, "ON")
                .on_hover_text("Enable the equalizer");
            let preset = eq::PRESETS
                .iter()
                .find(|(_, gains)| *gains == snap.gains)
                .map(|(name, _)| *name)
                .unwrap_or("Custom");
            egui::ComboBox::from_id_salt("eq_preset")
                .selected_text(preset)
                .show_ui(ui, |ui| {
                    for (name, gains) in eq::PRESETS {
                        if ui.selectable_label(snap.gains == *gains, *name).clicked() {
                            snap.gains = *gains;
                            snap.enabled = true;
                        }
                    }
                });
            if ui.button("Reset").clicked() {
                snap.gains = [0.0; eq::BANDS];
                snap.preamp = 0.0;
            }
        });
        ui.add_space(4.0);
        let sample_rate = self.engine.tap().lock().map(|t| t.sample_rate).unwrap_or(0);
        eq_curve(ui, &snap, if sample_rate > 0 { sample_rate as f32 } else { 44_100.0 });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            eq_slider(ui, &mut snap.preamp, "PRE");
            ui.separator();
            for (gain, label) in snap.gains.iter_mut().zip(eq::LABELS) {
                eq_slider(ui, gain, label);
            }
        });

        if snap != before {
            let ctx = ui.ctx().clone();
            self.apply(Command::SetEq(snap), &ctx);
        }
    }
}

/// One vertical EQ slider with its label underneath. Double-click resets it.
fn eq_slider(ui: &mut egui::Ui, value: &mut f32, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(38.0, 150.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.spacing_mut().slider_width = 120.0;
            let resp = ui.add(
                egui::Slider::new(value, -eq::MAX_DB..=eq::MAX_DB)
                    .vertical()
                    .show_value(false)
                    .trailing_fill(true),
            );
            if resp.double_clicked() {
                *value = 0.0;
            }
            resp.on_hover_text(format!("{label}: {:+.1} dB (double-click to reset)", value));
            ui.label(egui::RichText::new(label).small().monospace());
        },
    );
}

/// The equalizer's actual frequency response, 20 Hz – 20 kHz on a log axis.
fn eq_curve(ui: &mut egui::Ui, snap: &EqSnapshot, sample_rate: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::hover());
    let visuals = ui.visuals();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, visuals.widgets.inactive.corner_radius, visuals.extreme_bg_color);
    // Overlapping boosted bands sum past a single band's range.
    let range_db = eq::MAX_DB * 2.0;
    let y_for = |db: f32| rect.center().y - (db / range_db).clamp(-1.0, 1.0) * rect.height() / 2.0;
    painter.hline(rect.x_range(), y_for(0.0), visuals.widgets.noninteractive.bg_stroke);

    let (lo, hi) = (20f32.ln(), 20_000f32.ln());
    let x_for = |f: f32| rect.left() + (f.ln() - lo) / (hi - lo) * rect.width();
    let at = |i: usize| eq::graph_freq(i as f32 / 96.0);
    for f in eq::FREQS {
        painter.vline(x_for(f), rect.y_range(), egui::Stroke::new(1.0, visuals.faint_bg_color));
    }
    let points: Vec<egui::Pos2> = (0..=96)
        .map(|i| {
            let f = at(i);
            egui::pos2(x_for(f), y_for(eq::response_db(snap, f, sample_rate)))
        })
        .collect();
    let color = if snap.enabled {
        visuals.selection.bg_fill
    } else {
        visuals.weak_text_color()
    };
    painter.add(egui::Shape::line(points, egui::Stroke::new(2.0, color)));
}

/// A single line of text that scrolls when it doesn't fit, like the
/// classic song-title ticker.
fn marquee(ui: &mut egui::Ui, text: &str, scrolling: bool, color: egui::Color32) {
    let font = egui::FontId::proportional(16.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let height = galley.size().y;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    if galley.size().x <= rect.width() {
        painter.galley(rect.min, galley, color);
        return;
    }
    let gap = 60.0;
    let span = galley.size().x + gap;
    let offset = if scrolling {
        ((ui.input(|i| i.time) * 40.0) as f32) % span
    } else {
        0.0
    };
    painter.galley(rect.min - egui::vec2(offset, 0.0), galley.clone(), color);
    painter.galley(rect.min + egui::vec2(span - offset, 0.0), galley, color);
}

/// Loads the skin's classic `.wsz` (if it names one), noting failures.
fn load_classic(skin: &Skin, ctx: &egui::Context, errors: &mut Vec<String>) -> Option<ClassicSkin> {
    let file = skin.classic.as_ref()?;
    let path = skin.path.as_ref()?.parent()?.join(file);
    match ClassicSkin::load(&path, ctx) {
        Ok(classic) => Some(classic),
        Err(e) => {
            errors.push(format!("{}: {e}", skin.name));
            None
        }
    }
}

/// The volume the engine should play at: nothing while muted.
fn effective_volume(config: &AppConfig) -> f32 {
    if config.muted { 0.0 } else { config.volume }
}

/// Speaker icon for the mute button: crossed out when muted (or at zero),
/// else one, two or three waves by level.
fn speaker_icon(config: &AppConfig) -> &'static str {
    match effective_volume(config) {
        v if v <= 0.0 => "\u{1F507}",
        v if v < 0.34 => "\u{1F508}",
        v if v < 0.67 => "\u{1F509}",
        _ => "\u{1F50A}",
    }
}

fn version_label() -> String {
    format!("UnAmp v{}", env!("CARGO_PKG_VERSION"))
}

impl eframe::App for UnAmpApp {
    /// Playback housekeeping. eframe calls this before every frame and also
    /// while the window is hidden (as long as a repaint is requested), so
    /// tracks keep advancing and media controls keep working when UnAmp is
    /// minimised or on another workspace.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.engine.poll();
        self.poll_now_playing(ctx);
        if self.engine.take_finished() {
            self.next(true, ctx);
        }
        for command in self.mpris.commands() {
            self.apply_mpris(command, ctx);
        }
        let (state, position) = self.mpris_state();
        self.mpris.update(state, position);
        self.save_session();
        if self.engine.state() == PlayState::Playing {
            // Keeps logic() running while hidden, to catch the end of a track.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;

        // The content rect, not viewport().inner_rect: on Wayland a window
        // can't know its position, so inner_rect is always None there.
        let size = ctx.content_rect().size();
        self.config.window_width = Some(size.x);
        self.config.window_height = Some(size.y);

        self.follow_theme(ctx);
        self.library.poll(ctx);
        self.handle_keys(ctx);
        self.update_window_title(ctx);

        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let playing = self.engine.state() == PlayState::Playing;
        self.visualizer.update(dt, playing);
        if playing || self.visualizer.is_animating() {
            ctx.request_repaint();
        } else if self.engine.state() == PlayState::Paused {
            // Keep the paused time display blinking.
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        let custom_frame = !self.config.system_title_bar;
        let radius = if custom_frame { frame::corner_radius(ctx, self.palette.window_radius) } else { 0 };
        let title = self.window_title.clone();
        egui::Panel::top("menu")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .corner_radius(egui::CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 }),
            )
            .show(ui, |ui| {
                frame::title_bar(ui, &title, custom_frame, |ui| {
                    ui.menu_button("Windows", |ui| {
                        for (window, label) in [
                            (Window::Player, "Player"),
                            (Window::Equalizer, "Equalizer"),
                            (Window::Playlist, "Playlist"),
                            (Window::Library, "Media Library"),
                            (Window::Waveform, "Waveform"),
                        ] {
                            let mut open = *self.window_open(window);
                            if ui.checkbox(&mut open, label).clicked() {
                                self.apply(Command::ToggleWindow(window), ctx);
                            }
                        }
                        ui.separator();
                        if ui.button("Reset layout").clicked() {
                            ctx.memory_mut(|m| m.reset_areas());
                            self.config.show_player = true;
                            self.config.show_equalizer = true;
                            self.config.show_playlist = true;
                            self.config.show_library = true;
                        }
                        ui.separator();
                        if ui
                            .checkbox(&mut self.config.system_title_bar, "System title bar")
                            .on_hover_text("Let the desktop draw the window frame instead of the skin")
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                                self.config.system_title_bar,
                            ));
                        }
                    });
                    ui.menu_button("Skins", |ui| self.show_skins_menu(ui));
                });
            });

        egui::Panel::bottom("status")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: radius, se: radius }),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Window size in physical pixels, styled like the version label.
                    let size = ctx.content_rect().size() * ctx.pixels_per_point();
                    ui.label(
                        egui::RichText::new(format!("{} \u{00D7} {}", size.x.round(), size.y.round()))
                            .weak()
                            .small(),
                    )
                    .on_hover_text("Window size in pixels");
                    let error = self.engine.error.as_ref().or(self.session_error.as_ref());
                    if error.is_some() || self.engine.state() == PlayState::Loading {
                        ui.separator();
                    }
                    if let Some(err) = self.engine.error.as_ref().or(self.session_error.as_ref()) {
                        ui.colored_label(ui.visuals().error_fg_color, err);
                    } else if self.engine.state() == PlayState::Loading {
                        ui.spinner();
                        ui.label("Opening…");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(version_label()).weak().small());
                    });
                });
            });

        // The desktop the windows float on.
        let desktop = egui::CentralPanel::default()
            .show(ui, |ui| {
                // A whisper of the text colour over the background, so the
                // watermark stays faint whatever the skin's contrast.
                let visuals = ui.visuals();
                let watermark = egui::Color32::from(egui::lerp(
                    egui::Rgba::from(visuals.panel_fill)..=egui::Rgba::from(visuals.text_color()),
                    0.06,
                ));
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("UnAmp").size(64.0).color(watermark));
                });
            })
            .response
            .rect;
        let left = desktop.left() + 10.0;
        let top = desktop.top() + 10.0;

        // Default layout: the classic stack on the left, library on the right.
        // egui remembers where the user moves and resizes them.
        if let Some(mut skin) = self.classic.take() {
            self.show_classic_windows(ctx, &mut skin, egui::pos2(left, top));
            self.classic = Some(skin);
        } else {
            let mut open = self.config.show_player;
            egui::Window::new("UnAmp")
                .id(egui::Id::new("player_window"))
                .open(&mut open)
                .resizable(false)
                .collapsible(true)
                .default_pos([left, top])
                .constrain_to(desktop)
                .show(ctx, |ui| self.show_player(ui));
            self.config.show_player = open;

            let mut open = self.config.show_equalizer;
            egui::Window::new("Equalizer")
                .id(egui::Id::new("equalizer_window"))
                .open(&mut open)
                .resizable(false)
                .collapsible(true)
                .default_pos([left, top + 200.0])
                .constrain_to(desktop)
                .show(ctx, |ui| self.show_equalizer(ui));
            self.config.show_equalizer = open;

            let mut open = self.config.show_playlist;
            egui::Window::new("Playlist")
                .id(egui::Id::new("playlist_window"))
                .open(&mut open)
                .resizable(true)
                .default_pos([left, top + 500.0])
                // Window frame included, so it lines up with the fixed-width stack above.
                .default_size([STACK_WIDTH + 14.0, 220.0])
                .min_size([320.0, 120.0])
                .constrain_to(desktop)
                .show(ctx, |ui| self.show_playlist_view(ui, ctx));
            self.config.show_playlist = open;
        }

        // Off by default; Reset layout leaves it as it is.
        let mut open = self.config.show_waveform;
        egui::Window::new("Waveform")
            .id(egui::Id::new("waveform_window"))
            .open(&mut open)
            .resizable(true)
            .default_pos([left + STACK_WIDTH + 40.0, top + 590.0])
            .default_size([620.0, 170.0])
            .min_size([240.0, 110.0])
            .constrain_to(desktop)
            .show(ctx, |ui| self.show_waveform(ui, ctx));
        self.config.show_waveform = open;

        let mut open = self.config.show_library;
        egui::Window::new("Media Library")
            .id(egui::Id::new("library_window"))
            .open(&mut open)
            .resizable(true)
            .default_pos([left + STACK_WIDTH + 40.0, top])
            .default_size([620.0, 560.0])
            .min_size([420.0, 240.0])
            .constrain_to(desktop)
            .show(ctx, |ui| self.show_library(ui, ctx));
        self.config.show_library = open;

        if custom_frame {
            frame::edges(ctx, radius);
        }
    }

    /// Transparent behind UnAmp's own frame, so its rounded corners show the
    /// desktop; the panels paint everything else.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.config.system_title_bar {
            visuals.panel_fill.to_normalized_gamma_f32()
        } else {
            [0.0; 4]
        }
    }

    fn on_exit(&mut self) {
        self.save_session();
        self.config.browse_path = Some(self.library.current_dir.clone());
        self.config.save();
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn config(volume: f32, muted: bool) -> AppConfig {
        AppConfig { volume, muted, ..Default::default() }
    }

    #[test]
    fn muting_silences_without_losing_the_level() {
        let muted = config(0.7, true);
        assert_eq!(effective_volume(&muted), 0.0);
        assert_eq!(muted.volume, 0.7);
        assert_eq!(effective_volume(&config(0.7, false)), 0.7);
    }

    #[test]
    fn speaker_icon_follows_mute_and_level() {
        assert_eq!(speaker_icon(&config(0.9, true)), "\u{1F507}");
        assert_eq!(speaker_icon(&config(0.0, false)), "\u{1F507}");
        assert_eq!(speaker_icon(&config(0.2, false)), "\u{1F508}");
        assert_eq!(speaker_icon(&config(0.5, false)), "\u{1F509}");
        assert_eq!(speaker_icon(&config(0.9, false)), "\u{1F50A}");
    }
}
