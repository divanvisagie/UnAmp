//! The three classic windows. Each draws into a fixed-size area and
//! reports what the user did as `Action`s; the app owns all state.

use std::time::Duration;

use egui::{Color32, Pos2, Rect, Sense, Vec2};

use super::{Bmp, ClassicSkin, EQ_SIZE, MAIN_SIZE, PLAYLIST_SIZE, SCALE};
use crate::command::{Command, PlaylistAction, Window, with_queue_marker};
use crate::display::{self, PlayerStatus};
use crate::reorder::{DragList, Reorder};
use crate::config::Repeat;
use crate::eq::{self, EqSnapshot};
use crate::library::Track;
use crate::player::PlayState;
use crate::visualizer;

/// Everything the classic windows show, borrowed from the app per frame.
pub struct View<'a> {
    /// Playback state, volume, shuffle/repeat, time display mode.
    pub status: PlayerStatus,
    /// Ticker text, e.g. "1. Artist - Title (3:20)".
    pub title: &'a str,
    pub kbps: Option<u32>,
    pub khz: Option<u32>,
    pub channels: Option<u8>,
    pub eq_open: bool,
    pub pl_open: bool,
    pub eq: EqSnapshot,
    pub bars: &'a [f32; visualizer::BANDS],
    pub peaks: &'a [f32; visualizer::BANDS],
    pub tracks: &'a [Track],
    /// The playlist entry that's playing (`None` while the queue plays).
    pub current: Option<usize>,
    /// Each playlist entry's up-next queue position, for `[n]` markers.
    pub queued: &'a [Option<usize>],
    /// Total playlist length, for the running-time display.
    pub total: Duration,
    pub time: f64,
}


/// Fills a rectangle given in skin pixels.
fn px(painter: &egui::Painter, origin: Pos2, rect: [f32; 4], color: Color32) {
    let r = Rect::from_min_size(
        origin + Vec2::new(rect[0], rect[1]) * SCALE,
        Vec2::new(rect[2], rect[3]) * SCALE,
    );
    painter.rect_filled(r, 0.0, color);
}

/// A sprite button: draws `normal` or `pressed` (bitmap x, y) of `size`.
#[allow(clippy::too_many_arguments)]
fn button(
    ui: &egui::Ui,
    skin: &ClassicSkin,
    painter: &egui::Painter,
    origin: Pos2,
    id: egui::Id,
    bmp: Bmp,
    at: [f32; 2],
    size: [u32; 2],
    normal: [u32; 2],
    pressed: [u32; 2],
) -> egui::Response {
    let resp = ClassicSkin::hit(ui, origin, id, [at[0], at[1], size[0] as f32, size[1] as f32], Sense::click());
    let src = if resp.is_pointer_button_down_on() { pressed } else { normal };
    skin.sprite(painter, origin, bmp, [src[0], src[1], size[0], size[1]], at);
    resp
}

/// Pointer position in skin pixels relative to the window origin.
fn pointer_skin(ui: &egui::Ui, origin: Pos2) -> Option<Vec2> {
    ui.input(|i| i.pointer.interact_pos()).map(|p| (p - origin) / SCALE)
}

pub fn show_player(ui: &mut egui::Ui, skin: &mut ClassicSkin, v: &View, out: &mut Vec<Command>) {
    let (rect, _) = ui.allocate_exact_size(MAIN_SIZE * SCALE, Sense::hover());
    let o = rect.min;
    let p = ui.painter_at(rect);
    let id = ui.id().with("classic_main");

    skin.sprite(&p, o, Bmp::Main, [0, 0, 275, 116], [0.0, 0.0]);
    skin.sprite(&p, o, Bmp::Titlebar, [27, 0, 275, 14], [0.0, 0.0]);

    // Title-bar buttons.
    if button(ui, skin, &p, o, id.with("min"), Bmp::Titlebar, [244.0, 3.0], [9, 9], [9, 0], [9, 9]).clicked() {
        out.push(Command::Minimize);
    }
    if button(ui, skin, &p, o, id.with("close"), Bmp::Titlebar, [264.0, 3.0], [9, 9], [18, 0], [18, 9]).clicked() {
        out.push(Command::CloseWindow(Window::Player));
    }

    // Play / pause / stop indicator and the "working" sliver beside it.
    let status_x = match v.status.state {
        PlayState::Playing | PlayState::Loading => 0,
        PlayState::Paused => 9,
        PlayState::Stopped => 18,
    };
    skin.sprite(&p, o, Bmp::Playpaus, [status_x, 0, 9, 9], [26.0, 28.0]);
    let working = if v.status.state == PlayState::Playing { 39 } else { 36 };
    skin.sprite(&p, o, Bmp::Playpaus, [working, 0, 3, 9], [24.0, 28.0]);

    draw_time(ui, skin, &p, o, id, v, out);
    draw_visualizer(skin, &p, o, v);

    // Song ticker in the bitmap font, stepping 5px at a time like the original.
    let ticker = v.title.to_string();
    let width = ticker.chars().count() as f32 * 5.0;
    if width <= 154.0 {
        skin.text(&p, o, &ticker, [111.0, 27.0], 154.0, 0.0);
    } else {
        let looped = format!("{ticker}  ***  ");
        let span = looped.chars().count() as f32 * 5.0;
        let offset = ((v.time / 0.15) as i64 as f32 * 5.0) % span;
        skin.text(&p, o, &format!("{looped}{looped}"), [111.0, 27.0], 154.0, offset);
    }
    if let Some(kbps) = v.kbps {
        skin.text(&p, o, &format!("{:>3}", kbps.min(999)), [111.0, 43.0], 15.0, 0.0);
    }
    if let Some(khz) = v.khz {
        skin.text(&p, o, &format!("{:>2}", khz.min(99)), [156.0, 43.0], 10.0, 0.0);
    }

    // Mono / stereo lights.
    let stereo = v.channels.is_some_and(|c| c >= 2) && v.status.state != PlayState::Stopped;
    let mono = v.channels == Some(1) && v.status.state != PlayState::Stopped;
    skin.sprite(&p, o, Bmp::Monoster, [29, if mono { 0 } else { 12 }, 27, 12], [212.0, 41.0]);
    skin.sprite(&p, o, Bmp::Monoster, [0, if stereo { 0 } else { 12 }, 29, 12], [239.0, 41.0]);

    // Volume slider: 28 background frames, thumb only if the bitmap has one.
    let vol_resp = ClassicSkin::hit(ui, o, id.with("volume"), [107.0, 57.0, 68.0, 13.0], Sense::click_and_drag());
    let mut volume = v.status.volume;
    if vol_resp.is_pointer_button_down_on() {
        if let Some(pt) = pointer_skin(ui, o) {
            volume = ((pt.x - 107.0 - 7.0) / 54.0).clamp(0.0, 1.0);
            out.push(Command::SetVolume(volume));
        }
    }
    let frame = (volume * 27.0).round() as u32;
    skin.sprite(&p, o, Bmp::Volume, [0, frame * 15, 68, 13], [107.0, 57.0]);
    if skin.sheet_size(Bmp::Volume).is_some_and(|s| s[1] >= 433) {
        let thumb_x = if vol_resp.is_pointer_button_down_on() { 0 } else { 15 };
        skin.sprite(&p, o, Bmp::Volume, [thumb_x, 422, 14, 11], [107.0 + volume * 54.0, 58.0]);
    }

    // Balance (display only), centred. Skins without balance.bmp reuse volume.bmp.
    let balance = if skin.has(Bmp::Balance) { Bmp::Balance } else { Bmp::Volume };
    skin.sprite(&p, o, balance, [9, 0, 38, 13], [177.0, 57.0]);
    if skin.sheet_size(balance).is_some_and(|s| s[1] >= 433) {
        skin.sprite(&p, o, balance, [15, 422, 14, 11], [177.0 + 12.0, 58.0]);
    }

    // EQ / PL window toggles.
    let toggle = |open: bool, x: u32, pressed: bool| [if pressed { x + 46 } else { x }, if open { 73 } else { 61 }];
    let eq_resp = ClassicSkin::hit(ui, o, id.with("eq"), [219.0, 58.0, 23.0, 12.0], Sense::click());
    skin.sprite(&p, o, Bmp::Shufrep, {
        let [x, y] = toggle(v.eq_open, 0, eq_resp.is_pointer_button_down_on());
        [x, y, 23, 12]
    }, [219.0, 58.0]);
    if eq_resp.clicked() {
        out.push(Command::ToggleWindow(Window::Equalizer));
    }
    let pl_resp = ClassicSkin::hit(ui, o, id.with("pl"), [242.0, 58.0, 23.0, 12.0], Sense::click());
    skin.sprite(&p, o, Bmp::Shufrep, {
        let [x, y] = toggle(v.pl_open, 23, pl_resp.is_pointer_button_down_on());
        [x, y, 23, 12]
    }, [242.0, 58.0]);
    if pl_resp.clicked() {
        out.push(Command::ToggleWindow(Window::Playlist));
    }

    // Position bar.
    skin.sprite(&p, o, Bmp::Posbar, [0, 0, 248, 10], [16.0, 72.0]);
    let seekable = v.status.duration.is_some() && matches!(v.status.state, PlayState::Playing | PlayState::Paused);
    let pos_resp = ClassicSkin::hit(ui, o, id.with("posbar"), [16.0, 72.0, 248.0, 10.0], Sense::click_and_drag());
    if seekable {
        let pointer = pointer_skin(ui, o).map(|pt| (pt.x - 16.0 - 14.5) / 219.0);
        if let (Some(f), Some(total)) = (display::seek_drag(&pos_resp, pointer, &mut skin.seek_drag), v.status.duration) {
            out.push(Command::SeekTo(total.mul_f32(f)));
        }
        let fraction = display::seek_fraction(skin.seek_drag, v.status.position, v.status.duration);
        let thumb = if pos_resp.is_pointer_button_down_on() { 278 } else { 248 };
        skin.sprite(&p, o, Bmp::Posbar, [thumb, 0, 29, 10], [16.0 + fraction * 219.0, 72.0]);
    } else {
        skin.seek_drag = None;
    }

    // Transport.
    let transport: [(&str, f32, u32, u32, Command); 5] = [
        ("prev", 16.0, 0, 23, Command::Previous),
        ("play", 39.0, 23, 23, Command::Play),
        ("pause", 62.0, 46, 23, Command::TogglePause),
        ("stop", 85.0, 69, 23, Command::Stop),
        ("next", 108.0, 92, 22, Command::Next),
    ];
    for (name, x, sx, w, action) in transport {
        if button(ui, skin, &p, o, id.with(name), Bmp::Cbuttons, [x, 88.0], [w, 18], [sx, 0], [sx, 18]).clicked() {
            out.push(action);
        }
    }
    if button(ui, skin, &p, o, id.with("eject"), Bmp::Cbuttons, [136.0, 89.0], [22, 16], [114, 0], [114, 16]).clicked() {
        // Winamp's eject opens files; ours opens the library.
        out.push(Command::OpenWindow(Window::Library));
    }

    // Shuffle and repeat.
    let state_y = |on: bool, pressed: bool| match (on, pressed) {
        (false, false) => 0,
        (false, true) => 15,
        (true, false) => 30,
        (true, true) => 45,
    };
    let sh = ClassicSkin::hit(ui, o, id.with("shuffle"), [164.0, 89.0, 47.0, 15.0], Sense::click());
    skin.sprite(&p, o, Bmp::Shufrep, [28, state_y(v.status.shuffle, sh.is_pointer_button_down_on()), 47, 15], [164.0, 89.0]);
    if sh.clicked() {
        out.push(Command::ToggleShuffle);
    }
    let rp = ClassicSkin::hit(ui, o, id.with("repeat"), [210.0, 89.0, 28.0, 15.0], Sense::click());
    skin.sprite(&p, o, Bmp::Shufrep, [0, state_y(v.status.repeat != Repeat::Off, rp.is_pointer_button_down_on()), 28, 15], [210.0, 89.0]);
    let rp = rp.on_hover_text(match v.status.repeat {
        Repeat::Off => "Repeat: off",
        Repeat::All => "Repeat: all",
        Repeat::One => "Repeat: one",
    });
    if rp.clicked() {
        out.push(Command::CycleRepeat);
    }
}

fn draw_time(
    ui: &egui::Ui,
    skin: &ClassicSkin,
    p: &egui::Painter,
    o: Pos2,
    id: egui::Id,
    v: &View,
    out: &mut Vec<Command>,
) {
    if ClassicSkin::hit(ui, o, id.with("time"), [36.0, 26.0, 63.0, 13.0], Sense::click()).clicked() {
        out.push(Command::ToggleRemaining);
    }
    // Winamp shows nothing when stopped and blinks the digits while paused.
    let display::TimeDisplay::Shown { negative, time } =
        display::time_display(v.status.state, v.status.position, v.status.duration, v.status.show_remaining, v.time)
    else {
        return;
    };
    let secs = time.as_secs();
    let (mins, secs) = ((secs / 60) % 100, secs % 60);
    let digits = [mins / 10, mins % 10, secs / 10, secs % 10];
    let (bmp, extended) = if skin.has(Bmp::NumsEx) { (Bmp::NumsEx, true) } else { (Bmp::Numbers, false) };
    for (d, x) in digits.iter().zip([48.0, 60.0, 78.0, 90.0]) {
        skin.sprite(p, o, bmp, [*d as u32 * 9, 0, 9, 13], [x, 26.0]);
    }
    if negative {
        if extended {
            skin.sprite(p, o, bmp, [99, 0, 9, 13], [36.0, 26.0]);
        } else {
            skin.sprite(p, o, bmp, [20, 6, 5, 1], [38.0, 32.0]);
        }
    }
}

fn draw_visualizer(skin: &ClassicSkin, p: &egui::Painter, o: Pos2, v: &View) {
    px(p, o, [24.0, 43.0, 76.0, 16.0], skin.vis[0]);
    // The faint dot grid behind the bars.
    for y in (1..16).step_by(2) {
        for x in (1..76).step_by(2) {
            px(p, o, [24.0 + x as f32, 43.0 + y as f32, 1.0, 1.0], skin.vis[1]);
        }
    }
    for (i, (&bar, &peak)) in v.bars.iter().zip(v.peaks).enumerate() {
        let x = 24.0 + i as f32 * 4.0;
        let height = (bar * 16.0).round().clamp(0.0, 16.0) as usize;
        // viscolor 2 is the top row, 17 the bottom.
        for row in (16 - height)..16 {
            px(p, o, [x, 43.0 + row as f32, 3.0, 1.0], skin.vis[2 + row]);
        }
        if peak > 0.01 {
            let row = (16.0 - (peak * 16.0).round()).clamp(0.0, 15.0);
            px(p, o, [x, 43.0 + row, 3.0, 1.0], skin.vis[23]);
        }
    }
}

pub fn show_eq(ui: &mut egui::Ui, skin: &mut ClassicSkin, v: &View, out: &mut Vec<Command>) {
    let (rect, _) = ui.allocate_exact_size(EQ_SIZE * SCALE, Sense::hover());
    let o = rect.min;
    let p = ui.painter_at(rect);
    let id = ui.id().with("classic_eq");
    let mut snap = v.eq;

    skin.sprite(&p, o, Bmp::Eqmain, [0, 0, 275, 116], [0.0, 0.0]);
    skin.sprite(&p, o, Bmp::Eqmain, [0, 134, 275, 14], [0.0, 0.0]);
    if button(ui, skin, &p, o, id.with("close"), Bmp::Eqmain, [264.0, 3.0], [9, 9], [0, 116], [0, 125]).clicked() {
        out.push(Command::CloseWindow(Window::Equalizer));
    }

    // ON toggle; AUTO is drawn but does nothing, as in most players.
    let on = ClassicSkin::hit(ui, o, id.with("on"), [14.0, 18.0, 26.0, 12.0], Sense::click());
    let on_x = match (snap.enabled, on.is_pointer_button_down_on()) {
        (false, false) => 10,
        (true, false) => 69,
        (false, true) => 128,
        (true, true) => 187,
    };
    skin.sprite(&p, o, Bmp::Eqmain, [on_x, 119, 26, 12], [14.0, 18.0]);
    if on.clicked() {
        snap.enabled = !snap.enabled;
    }
    skin.sprite(&p, o, Bmp::Eqmain, [36, 119, 32, 12], [40.0, 18.0]);

    let presets = button(ui, skin, &p, o, id.with("presets"), Bmp::Eqmain, [217.0, 18.0], [44, 12], [224, 164], [224, 176]);
    egui::Popup::menu(&presets).show(|ui| {
        for (name, gains) in eq::PRESETS {
            if ui.button(*name).clicked() {
                snap.gains = *gains;
                snap.enabled = true;
            }
        }
        ui.separator();
        if ui.button("Reset").clicked() {
            snap.gains = [0.0; eq::BANDS];
            snap.preamp = 0.0;
        }
    });

    // Response graph in the skin's own line colours, preamp as a flat line.
    skin.sprite(&p, o, Bmp::Eqmain, [0, 294, 113, 19], [86.0, 17.0]);
    // Graph rows: 0 is +MAX_DB at the top, 18 is −MAX_DB at the bottom.
    let row_for = |db: f32| ((1.0 - eq::level(db)) * 18.0).round() as usize;
    skin.sprite(&p, o, Bmp::Eqmain, [0, 314, 113, 1], [86.0, 17.0 + row_for(snap.preamp) as f32]);
    let bands_only = EqSnapshot { preamp: 0.0, ..snap };
    let mut prev: Option<usize> = None;
    for x in 0..113 {
        let f = eq::graph_freq(x as f32 / 112.0);
        let row = row_for(eq::response_db(&bands_only, f, 44_100.0));
        let (a, b) = match prev {
            Some(pr) => (pr.min(row), pr.max(row)),
            None => (row, row),
        };
        for r in a..=b {
            px(&p, o, [86.0 + x as f32, 17.0 + r as f32, 1.0, 1.0], skin.eq_graph_color(r));
        }
        prev = Some(row);
    }

    // Sliders: preamp, then the ten bands.
    let slider = |key: &str, x: f32, value: &mut f32| {
        let resp = ClassicSkin::hit(ui, o, id.with(key), [x, 38.0, 14.0, 63.0], Sense::click_and_drag());
        if resp.double_clicked() {
            *value = 0.0;
        } else if resp.is_pointer_button_down_on() {
            if let Some(pt) = pointer_skin(ui, o) {
                let t = (pt.y - 38.0 - 5.5) / 52.0;
                *value = eq::db_at_level(1.0 - t);
            }
        }
        let level = eq::level(*value);
        let frame = (level * 27.0).round() as u32;
        skin.sprite(&p, o, Bmp::Eqmain, [13 + (frame % 14) * 15, 164 + (frame / 14) * 65, 14, 63], [x, 38.0]);
        let thumb_y = if resp.is_pointer_button_down_on() { 176 } else { 164 };
        skin.sprite(&p, o, Bmp::Eqmain, [0, thumb_y, 11, 11], [x + 1.0, 38.0 + (1.0 - level) * 52.0]);
        resp.on_hover_text(format!("{key}: {:+.1} dB (double-click to reset)", *value));
    };
    slider("Preamp", 21.0, &mut snap.preamp);
    for (i, label) in eq::LABELS.iter().enumerate() {
        slider(label, 78.0 + i as f32 * 18.0, &mut snap.gains[i]);
    }

    if snap != v.eq {
        out.push(Command::SetEq(snap));
    }
}

pub fn show_playlist(ui: &mut egui::Ui, skin: &mut ClassicSkin, v: &View, reorder: &mut Reorder, out: &mut Vec<Command>) {
    let size = PLAYLIST_SIZE;
    let (w, h) = (size.x, size.y);
    let (rect, _) = ui.allocate_exact_size(size * SCALE, Sense::hover());
    let o = rect.min;
    let p = ui.painter_at(rect);
    let id = ui.id().with("classic_playlist");

    // Frame: tiled edges, then corners and the title on top.
    let mut x = 25.0;
    while x < w - 25.0 {
        skin.sprite(&p, o, Bmp::Pledit, [127, 0, 25, 20], [x, 0.0]);
        x += 25.0;
    }
    let mut y = 20.0;
    while y < h - 38.0 {
        skin.sprite(&p, o, Bmp::Pledit, [0, 42, 12, 29], [0.0, y]);
        skin.sprite(&p, o, Bmp::Pledit, [31, 42, 20, 29], [w - 20.0, y]);
        y += 29.0;
    }
    let mut x = 125.0;
    while x < w - 150.0 {
        skin.sprite(&p, o, Bmp::Pledit, [179, 0, 25, 38], [x, h - 38.0]);
        x += 25.0;
    }
    skin.sprite(&p, o, Bmp::Pledit, [0, 0, 25, 20], [0.0, 0.0]);
    skin.sprite(&p, o, Bmp::Pledit, [26, 0, 100, 20], [((w - 100.0) / 2.0).round(), 0.0]);
    skin.sprite(&p, o, Bmp::Pledit, [153, 0, 25, 20], [w - 25.0, 0.0]);
    skin.sprite(&p, o, Bmp::Pledit, [0, 72, 125, 38], [0.0, h - 38.0]);
    skin.sprite(&p, o, Bmp::Pledit, [126, 72, 150, 38], [w - 150.0, h - 38.0]);

    // The close button is painted into the corner; only its pressed state is a sprite.
    let close = ClassicSkin::hit(ui, o, id.with("close"), [w - 11.0, 3.0, 9.0, 9.0], Sense::click());
    if close.is_pointer_button_down_on() {
        skin.sprite(&p, o, Bmp::Pledit, [52, 42, 9, 9], [w - 11.0, 3.0]);
    }
    if close.clicked() {
        out.push(Command::CloseWindow(Window::Playlist));
    }

    // Track list, in pledit.txt's colours at Winamp's 13px row height.
    let list = [12.0, 20.0, w - 20.0 - 12.0, h - 38.0 - 20.0];
    px(&p, o, list, skin.pl.normal_bg);
    let list_resp = ClassicSkin::hit(ui, o, id.with("list"), list, Sense::click_and_drag());
    let row_h = 13.0;
    let visible = (list[3] / row_h).floor() as usize;
    let max_scroll = v.tracks.len().saturating_sub(visible) as f32;
    if list_resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        skin.playlist_scroll = (skin.playlist_scroll - dy / (row_h * SCALE)).clamp(0.0, max_scroll);
    }
    skin.playlist_scroll = skin.playlist_scroll.clamp(0.0, max_scroll);
    let first = skin.playlist_scroll.floor() as usize;

    if list_resp.clicked() || list_resp.double_clicked() || list_resp.secondary_clicked() {
        if let Some(pt) = pointer_skin(ui, o) {
            let index = first + ((pt.y - list[1]) / row_h).floor() as usize;
            if index < v.tracks.len() {
                skin.playlist_selected = Some(index);
                if list_resp.double_clicked() {
                    out.push(Command::Playlist(PlaylistAction::Play(index)));
                }
            }
        }
    }
    // Right-click acts on the row under the pointer (selected just above).
    list_resp.context_menu(|ui| {
        let Some(index) = skin.playlist_selected.filter(|&i| i < v.tracks.len()) else {
            ui.close();
            return;
        };
        for (label, action) in [
            ("\u{25B6} Play", PlaylistAction::Play(index)),
            (crate::library::PLAY_NEXT, PlaylistAction::PlayNext(index)),
            (crate::library::ADD_TO_QUEUE, PlaylistAction::AddToQueue(index)),
            ("\u{2796} Remove", PlaylistAction::Remove(index)),
        ] {
            if ui.button(label).clicked() {
                out.push(Command::Playlist(action));
            }
        }
    });

    // Scrollbar thumb in the right edge's track; drag it or use the wheel.
    let track = [w - 15.0, 20.0, 8.0, list[3]];
    let scroll_resp = ClassicSkin::hit(ui, o, id.with("scroll"), track, Sense::click_and_drag());
    if scroll_resp.is_pointer_button_down_on() && max_scroll > 0.0 {
        if let Some(pt) = pointer_skin(ui, o) {
            let t = ((pt.y - 20.0 - 9.0) / (list[3] - 18.0)).clamp(0.0, 1.0);
            skin.playlist_scroll = (t * max_scroll).round();
        }
    }
    let thumb_t = if max_scroll > 0.0 { skin.playlist_scroll / max_scroll } else { 0.0 };
    let thumb_x = if scroll_resp.is_pointer_button_down_on() { 61 } else { 52 };
    skin.sprite(&p, o, Bmp::Pledit, [thumb_x, 53, 8, 18], [w - 15.0, (20.0 + thumb_t * (list[3] - 18.0)).round()]);

    // Running time ("selected/total") and the mini position counter, in the
    // black boxes of the bottom-right corner piece.
    let fmt = |d: Duration| crate::metadata::format_duration(d);
    let selected = skin
        .playlist_selected
        .and_then(|i| v.tracks.get(i))
        .and_then(|t| t.duration())
        .unwrap_or_default();
    skin.text(&p, o, &format!("{}/{}", fmt(selected), fmt(v.total)), [w - 150.0 + 7.0, h - 38.0 + 10.0], 80.0, 0.0);
    let mini = match v.status.state {
        PlayState::Playing | PlayState::Paused => fmt(v.status.position),
        _ => "  :  ".to_string(),
    };
    skin.text(&p, o, &format!("{mini:>5}"), [w - 150.0 + 66.0, h - 38.0 + 23.0], 25.0, 0.0);

    let list_rect = Rect::from_min_size(o + Vec2::new(list[0], list[1]) * SCALE, Vec2::new(list[2], list[3]) * SCALE);
    let text_painter = p.with_clip_rect(list_rect);
    let font = egui::FontId::proportional(9.0 * SCALE);
    let press = ui.input(|i| i.pointer.press_origin());
    for (row, index) in (first..v.tracks.len()).take(visible + 1).enumerate() {
        let track = &v.tracks[index];
        let top = list[1] + row as f32 * row_h;
        // Drag to reorder, through the same helper as the regular playlist.
        let row_rect = Rect::from_min_size(o + Vec2::new(list[0], top) * SCALE, Vec2::new(list[2], row_h) * SCALE);
        let started = list_resp.drag_started() && press.is_some_and(|p| row_rect.contains(p));
        reorder.row(ui, DragList::Playlist, index, row_rect, started);
        let highlighted = skin.playlist_selected == Some(index) || reorder.is_dragging(DragList::Playlist, index);
        if highlighted {
            px(&text_painter, o, [list[0], top, list[2], row_h], skin.pl.selected_bg);
        }
        let color = if v.current == Some(index) { skin.pl.current } else { skin.pl.normal };
        let y = o.y + (top + row_h / 2.0) * SCALE;
        text_painter.text(
            Pos2::new(o.x + (list[0] + 2.0) * SCALE, y),
            egui::Align2::LEFT_CENTER,
            format!("{}. {}", index + 1, with_queue_marker(v.queued.get(index).copied().flatten(), track.title())),
            font.clone(),
            color,
        );
        if let Some(d) = track.duration() {
            // Paint a backing box so long titles don't run under the time.
            let time = crate::metadata::format_duration(d);
            let galley = text_painter.layout_no_wrap(time, font.clone(), color);
            let right = o.x + (list[0] + list[2] - 2.0) * SCALE;
            let bg = if highlighted { skin.pl.selected_bg } else { skin.pl.normal_bg };
            text_painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(right - galley.size().x - 6.0, y - row_h * SCALE / 2.0),
                    Pos2::new(list_rect.right(), y + row_h * SCALE / 2.0),
                ),
                0.0,
                bg,
            );
            text_painter.galley(Pos2::new(right - galley.size().x, y - galley.size().y / 2.0), galley, color);
        }
    }
    if let Some(dir) = reorder.edge_scroll(ui, DragList::Playlist, list_rect, row_h * SCALE) {
        skin.playlist_scroll = (skin.playlist_scroll - dir * 0.25).clamp(0.0, max_scroll);
    }
    if let Some((from, to)) = reorder.finish(ui, DragList::Playlist, list_rect, skin.pl.current) {
        out.push(Command::Playlist(PlaylistAction::Move { from, to }));
    }
}
