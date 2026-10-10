//! Arranging the floating windows (see ADR-0022): snapping them to a grid,
//! and to each other's edges, when a move or resize ends.
//!
//! egui keeps window positions and sizes in its own memory with no public
//! setter, so a snap is decided after the windows are shown and applied
//! over the next frames through `Window::current_pos` and
//! `Window::fixed_size` (which takes the outer size) until the window is
//! there. Snapping waits for the mouse button to be released, which leaves
//! egui's dragging untouched and means a snap can't fight the layout frame
//! after frame.

use std::collections::HashMap;

use egui::{Id, Pos2, Rect, Vec2};

/// Grid spacing, in points.
pub const GRID: f32 = 10.0;
/// How close another window's edge must be to win over the grid.
const EDGE_PULL: f32 = 8.0;

/// Frames a snap is applied for before giving up on it, should egui keep
/// the window somewhere else (say, too small for its contents).
const ATTEMPTS: u8 = 3;

/// A move or resize to apply to a window over the next frames.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Snap {
    pos: Pos2,
    /// New outer size, for resizable windows.
    size: Option<Vec2>,
    attempts: u8,
}

/// A window shown this frame, for deciding snaps.
pub struct Shown {
    pub id: Id,
    /// Outer rectangle, frame and title bar included.
    pub rect: Rect,
    /// Whether the user can resize the window, so its size snaps too.
    pub resizable: bool,
}

#[derive(Default)]
pub struct Snapper {
    pending: HashMap<Id, Snap>,
    /// Snap everything after the next frame: set on release, or when
    /// snapping is turned on.
    due: bool,
    shown: Vec<Shown>,
}

impl Snapper {
    /// Snaps every window once the windows have been shown again.
    pub fn snap_all(&mut self) {
        self.due = true;
    }

    /// Applies a snap decided last frame to the window about to be shown.
    pub fn prepare<'a>(&mut self, id: Id, window: egui::Window<'a>) -> egui::Window<'a> {
        match self.pending.get(&id) {
            Some(&Snap { pos, size, .. }) => {
                // Dragging off for the frame: a title-bar-draggable window
                // otherwise restores its old position after `current_pos`
                // (egui 0.35, `Window::show_dyn`). The mouse is up anyway.
                let window = window.current_pos(pos).drag_area(egui::WindowDrag::Off);
                match size {
                    Some(size) => window.fixed_size(size),
                    None => window,
                }
            }
            None => window,
        }
    }

    /// Records a window shown this frame, and retires its snap once it
    /// has landed (or has had enough tries).
    pub fn shown(&mut self, shown: Shown) {
        if let Some(snap) = self.pending.get_mut(&shown.id) {
            let target = Rect::from_min_size(snap.pos, snap.size.unwrap_or(shown.rect.size()));
            snap.attempts += 1;
            if close(target, shown.rect) || snap.attempts >= ATTEMPTS {
                self.pending.remove(&shown.id);
            }
        }
        self.shown.push(shown);
    }

    /// After all windows are shown: when a drag has just ended, works out
    /// where each window should snap to, with the grid starting at `origin`.
    /// Does nothing but forget this frame's windows unless `enabled`.
    pub fn finish(&mut self, ctx: &egui::Context, origin: Pos2, enabled: bool) {
        let shown = std::mem::take(&mut self.shown);
        if !enabled {
            self.due = false;
            self.pending.clear();
            return;
        }
        let (released, down) = ctx.input(|i| (i.pointer.any_released(), i.pointer.any_down()));
        if released {
            // Decide next frame, once the drag's last move has landed.
            self.due = true;
            ctx.request_repaint();
            return;
        }
        if !self.due || down {
            return;
        }
        self.due = false;
        let rects: Vec<Rect> = shown.iter().map(|s| s.rect).collect();
        for (i, window) in shown.iter().enumerate() {
            let others: Vec<Rect> = rects.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, r)| *r).collect();
            let target = snap_rect(window.rect, window.resizable, origin, &others);
            if close(target, window.rect) {
                continue;
            }
            let size = window.resizable.then(|| target.size());
            self.pending.insert(window.id, Snap { pos: target.min, size, attempts: 0 });
        }
        if !self.pending.is_empty() {
            ctx.request_repaint();
        }
    }
}

fn close(a: Rect, b: Rect) -> bool {
    a.min.distance(b.min) < 0.5 && (a.size() - b.size()).length() < 0.5
}

/// Where `rect` should go: its top-left corner on the grid, or against a
/// nearby edge of one of `others`, and with `resizable` its size rounded up
/// so its far edges land on the grid too. Rounding sizes up never asks a
/// window to be smaller than its contents, so a snap can't be undone by
/// egui growing the window back.
pub fn snap_rect(rect: Rect, resizable: bool, origin: Pos2, others: &[Rect]) -> Rect {
    let x_edges: Vec<f32> = others.iter().flat_map(|r| [r.left(), r.right()]).collect();
    let y_edges: Vec<f32> = others.iter().flat_map(|r| [r.top(), r.bottom()]).collect();
    let min = Pos2::new(
        snap_start(rect.left(), rect.width(), origin.x, &x_edges),
        snap_start(rect.top(), rect.height(), origin.y, &y_edges),
    );
    let size = if resizable {
        let up = |len: f32, start: f32, origin: f32| {
            let end = origin + ((start + len - origin) / GRID).ceil() * GRID;
            end - start
        };
        Vec2::new(up(rect.width(), min.x, origin.x), up(rect.height(), min.y, origin.y))
    } else {
        rect.size()
    };
    Rect::from_min_size(min, size)
}

/// The start of a span of length `len` beginning at `start`: moved so that
/// either end touches a nearby edge in `edges`, else onto the grid.
fn snap_start(start: f32, len: f32, origin: f32, edges: &[f32]) -> f32 {
    let nearest = edges
        .iter()
        .flat_map(|&e| [e - start, e - (start + len)])
        .filter(|d| d.abs() <= EDGE_PULL)
        .min_by(|a, b| a.abs().total_cmp(&b.abs()));
    match nearest {
        Some(shift) => start + shift,
        None => origin + ((start - origin) / GRID).round() * GRID,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn moves_onto_the_grid_from_the_origin() {
        let r = snap_rect(rect(23.0, 47.0, 100.0, 50.0), false, Pos2::new(10.0, 40.0), &[]);
        assert_eq!(r, rect(20.0, 50.0, 100.0, 50.0));
    }

    #[test]
    fn resizable_windows_grow_to_the_next_grid_line() {
        let r = snap_rect(rect(20.0, 20.0, 101.0, 50.0), true, Pos2::ZERO, &[]);
        assert_eq!(r, rect(20.0, 20.0, 110.0, 50.0));
    }

    #[test]
    fn a_nearby_window_edge_beats_the_grid() {
        // A fixed-size window (232 tall, off the grid) stacked under another:
        // its top snaps to the other's bottom, not to the nearest grid line.
        let above = rect(10.0, 10.0, 550.0, 232.0);
        let r = snap_rect(rect(14.0, 245.0, 550.0, 232.0), false, Pos2::ZERO, &[above]);
        assert_eq!(r.min, Pos2::new(10.0, 242.0));
    }

    #[test]
    fn far_edges_snap_too() {
        // The right edge lines up with a window to the right.
        let neighbour = rect(600.0, 10.0, 100.0, 100.0);
        let r = snap_rect(rect(33.0, 10.0, 563.0, 100.0), false, Pos2::ZERO, &[neighbour]);
        assert_eq!(r.right(), 600.0);
    }

    /// One headless frame with a resizable window; returns its outer rect.
    fn window_frame(ctx: &egui::Context, snapper: &mut Snapper, events: Vec<egui::Event>, time: f64) -> Rect {
        let input = egui::RawInput {
            screen_rect: Some(rect(0.0, 0.0, 800.0, 600.0)),
            time: Some(time),
            events,
            ..Default::default()
        };
        let id = Id::new("test_window");
        let mut out = Rect::NOTHING;
        let _ = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            let window = egui::Window::new("Test")
                .id(id)
                .default_pos([33.0, 47.0])
                .default_size([203.0, 104.0])
                .resizable(true);
            // Filling its space, like the playlist, library and waveform.
            let shown = snapper.prepare(id, window).show(&ctx, |ui| {
                ui.allocate_space(ui.available_size());
            });
            out = shown.unwrap().response.rect;
            snapper.shown(Shown { id, rect: out, resizable: true });
            snapper.finish(&ctx, Pos2::ZERO, true);
        });
        out
    }

    #[test]
    fn a_window_lands_on_the_grid_after_snapping() {
        let ctx = egui::Context::default();
        let mut snapper = Snapper::default();
        let first = window_frame(&ctx, &mut snapper, vec![], 0.0);
        assert!(first.min.x % GRID != 0.0, "starts off the grid: {first:?}");
        snapper.snap_all();
        let mut r = first;
        for frame in 1..5 {
            r = window_frame(&ctx, &mut snapper, vec![], frame as f64 * 0.1);
        }
        for v in [r.left(), r.top(), r.right(), r.bottom()] {
            assert_eq!(v % GRID, 0.0, "edge {v} of {r:?} is off the grid");
        }
        // And it stays there.
        assert_eq!(window_frame(&ctx, &mut snapper, vec![], 1.0), r);
    }

    #[test]
    fn distant_edges_are_ignored() {
        let far = rect(500.0, 500.0, 10.0, 10.0);
        assert_eq!(snap_start(103.0, 50.0, 0.0, &[far.left(), far.right()]), 100.0);
    }
}
