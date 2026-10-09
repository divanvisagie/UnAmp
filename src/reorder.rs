//! Drag-to-reorder for lists of rows, shared by the egui playlist and queue
//! and the classic skin playlist.
//!
//! Each frame: call `row` for every visible row (with its screen rect),
//! scroll by `edge_scroll` when it says to, then call `finish`, which draws
//! the drop line and returns the move once the pointer is released.

use egui::{Color32, Rect, Stroke};

use crate::playlist;

/// Which list a drag belongs to. The egui and classic playlists are the
/// same list, and only one of them is on screen at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragList {
    Playlist,
    Queue,
}

#[derive(Default)]
pub struct Reorder {
    /// The list and row being dragged.
    pub(crate) dragging: Option<(DragList, usize)>,
    /// The gap under the pointer this frame (0..=len) and its y position.
    gap: Option<(usize, f32)>,
}

impl Reorder {
    pub fn is_dragging(&self, list: DragList, index: usize) -> bool {
        self.dragging == Some((list, index))
    }

    fn active(&self, list: DragList) -> bool {
        self.dragging.is_some_and(|(l, _)| l == list)
    }

    /// Notes row `index` at `rect`. `drag_started` says a drag began on
    /// this row this frame.
    pub fn row(&mut self, ui: &egui::Ui, list: DragList, index: usize, rect: Rect, drag_started: bool) {
        if drag_started {
            self.dragging = Some((list, index));
        }
        if !self.active(list) {
            return;
        }
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            if rect.y_range().contains(p.y) {
                let below = p.y > rect.center().y;
                self.gap = Some(if below { (index + 1, rect.bottom()) } else { (index, rect.top()) });
            }
        }
    }

    /// `row` for a widget row's response.
    pub fn row_response(&mut self, ui: &egui::Ui, list: DragList, index: usize, resp: &egui::Response) {
        self.row(ui, list, index, resp.rect, resp.drag_started());
    }

    /// While dragging near the top or bottom `edge` of `area`: +1 to scroll
    /// towards the start, -1 towards the end. The caller scrolls its list.
    pub fn edge_scroll(&self, ui: &egui::Ui, list: DragList, area: Rect, edge: f32) -> Option<f32> {
        if !self.active(list) {
            return None;
        }
        let p = ui.input(|i| i.pointer.interact_pos())?;
        let direction = if p.y < area.top() + edge {
            1.0
        } else if p.y > area.bottom() - edge {
            -1.0
        } else {
            return None;
        };
        ui.ctx().request_repaint();
        Some(direction)
    }

    /// Draws the drop line in `color` within `area` and, once the pointer
    /// is released, returns `(from, to)` for a real move.
    pub fn finish(&mut self, ui: &egui::Ui, list: DragList, area: Rect, color: Color32) -> Option<(usize, usize)> {
        let (l, from) = self.dragging?;
        if l != list {
            return None;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let gap = self.gap.take();
        if let Some((_, y)) = gap {
            ui.painter()
                .with_clip_rect(area.expand(2.0))
                .hline(area.x_range(), y, Stroke::new(2.0, color));
        }
        if ui.input(|i| !i.pointer.any_down()) {
            self.dragging = None;
            let (gap, _) = gap?;
            let to = playlist::drop_index(from, gap);
            return (to != from).then_some((from, to));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library;

    /// Runs one headless egui frame of a 5-row draggable list, returning
    /// each row's rect and the move `finish` reported, if any.
    fn drag_frame(
        ctx: &egui::Context,
        reorder: &mut Reorder,
        events: Vec<egui::Event>,
        time: f64,
    ) -> (Vec<Rect>, Option<(usize, usize)>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let (mut rects, mut moved) = (Vec::new(), None);
        let _ = ctx.run_ui(input, |ui| {
            let output = egui::ScrollArea::vertical().show_rows(ui, library::ROW_HEIGHT, 5, |ui, range| {
                for i in range {
                    let resp = library::track_row(ui, i, "track", None, false, false, egui::Sense::click_and_drag());
                    reorder.row_response(ui, DragList::Playlist, i, &resp);
                    rects.push(resp.rect);
                }
            });
            moved = reorder.finish(ui, DragList::Playlist, output.inner_rect, Color32::WHITE);
        });
        (rects, moved)
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn run(steps: Vec<Vec<egui::Event>>) -> (Reorder, Option<(usize, usize)>) {
        let ctx = egui::Context::default();
        let mut reorder = Reorder::default();
        // egui hit-tests against the previous frame's widgets, so lay the
        // rows out once before pressing on them.
        drag_frame(&ctx, &mut reorder, vec![], 0.0);
        let mut moved = None;
        for (i, events) in steps.into_iter().enumerate() {
            let (_, m) = drag_frame(&ctx, &mut reorder, events, 0.1 * (i + 1) as f64);
            moved = moved.or(m);
        }
        (reorder, moved)
    }

    fn layout() -> Vec<Rect> {
        drag_frame(&egui::Context::default(), &mut Reorder::default(), vec![], 0.0).0
    }

    #[test]
    fn dragging_a_row_and_releasing_reports_the_move() {
        let rects = layout();
        let grab = rects[0].center();
        // Lower half of row 2: the gap after it, so row 0 lands at index 2.
        let drop = egui::pos2(grab.x, rects[2].center().y + 4.0);
        let (reorder, moved) = run(vec![
            vec![egui::Event::PointerMoved(grab), button(grab, true)],
            vec![egui::Event::PointerMoved(grab + egui::vec2(0.0, 10.0))],
            vec![egui::Event::PointerMoved(rects[1].center())],
            vec![egui::Event::PointerMoved(drop)],
            vec![egui::Event::PointerMoved(drop)],
            vec![button(drop, false)],
        ]);
        assert_eq!(moved, Some((0, 2)));
        assert!(reorder.dragging.is_none(), "drag ends on release");
    }

    #[test]
    fn a_plain_click_is_not_a_move() {
        let at = layout()[1].center();
        let (_, moved) = run(vec![vec![egui::Event::PointerMoved(at), button(at, true)], vec![button(at, false)]]);
        assert_eq!(moved, None);
    }
}
