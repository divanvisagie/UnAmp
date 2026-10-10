//! UnAmp's own window frame (see ADR-0020): the main window opens without
//! system decorations, and this draws a title bar holding the menus, the
//! window title and minimise/maximise/close buttons, plus a one-pixel border
//! and invisible resize handles along the edges. Its colours come from the
//! theme's `[window]` section (see ADR-0024), which defaults to the active
//! egui visuals, so the frame follows the skin and the desktop's light/dark
//! setting like the rest of the window. The window controls have colours of
//! their own, apart from the app's buttons.
//!
//! Moving and resizing are handed to the compositor (`StartDrag`,
//! `BeginResize`), so snapping and tiling behave as with a native frame.

use egui::{Color32, CursorIcon, Id, Order, ResizeDirection, Sense, Stroke, ViewportCommand, vec2};

/// How far in from the window edge a press starts a resize.
const EDGE: f32 = 5.0;
/// Corners get a larger grab area than straight edges.
const CORNER: f32 = 12.0;
const BUTTON: egui::Vec2 = vec2(30.0, 22.0);
/// Height of the title bar's row, as in Photograph, whose GTK-style
/// buttons set it there.
const ROW_HEIGHT: f32 = 25.0;

/// The frame's colours, resolved from the theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStyle {
    pub title_bar: Color32,
    pub title_text: Color32,
    pub border: Color32,
    /// Corner rounding of the window when it isn't maximised.
    pub radius: u8,
    pub controls: WindowControlColors,
}

/// Minimise, maximise and close.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowControlColors {
    pub icon: Color32,
    pub icon_hover: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub close_hover: Color32,
    pub close_icon_hover: Color32,
    pub radius: egui::CornerRadius,
}

#[derive(Clone, Copy)]
enum Button {
    Minimize,
    Maximize,
    Restore,
    Close,
}

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        let v = i.viewport();
        v.maximized.unwrap_or(false) || v.fullscreen.unwrap_or(false)
    })
}

/// Corner rounding for the window right now: `radius`, or square while
/// maximised or full screen, as GNOME does.
pub fn corner_radius(ctx: &egui::Context, radius: u8) -> u8 {
    if maximized(ctx) { 0 } else { radius }
}

/// The title bar: `menus` on the left, `title` centred, and with `custom`
/// set, window buttons on the right and drag-to-move on the empty space.
/// With `custom` off the desktop draws the frame and this is a plain menu bar.
pub fn title_bar(ui: &mut egui::Ui, style: &FrameStyle, title: &str, custom: bool, menus: impl FnOnce(&mut egui::Ui)) {
    let ctx = ui.ctx().clone();
    let bar = ui.max_rect();
    if custom {
        // Registered before the menus and buttons, so they win where they
        // overlap and only empty space moves the window.
        let resp = ui.interact(bar, Id::new("title_bar_drag"), Sense::click_and_drag());
        if resp.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized(&ctx)));
        }
        resp.context_menu(|ui| window_menu(ui, &ctx));
    }

    // Laid out side by side in areas of their own, so nothing can push the
    // row wider than the bar: the window buttons at the right first, then the
    // menus in the width that's left. (A menu bar takes its whole width, so
    // putting the buttons after it in one row made the panel overflow the
    // window by the spacing between them, and its rounded corner with it.)
    let row = egui::Rect::from_min_size(bar.min, egui::vec2(bar.width(), ROW_HEIGHT));
    let mut right_width = 0.0;
    if custom {
        let mut buttons = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        buttons.spacing_mut().item_spacing.x = 4.0;
        if button(&mut buttons, &style.controls, Button::Close).on_hover_text("Close").clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        let max = maximized(&ctx);
        let (kind, tip) = if max { (Button::Restore, "Restore") } else { (Button::Maximize, "Maximise") };
        if button(&mut buttons, &style.controls, kind).on_hover_text(tip).clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
        }
        if button(&mut buttons, &style.controls, Button::Minimize).on_hover_text("Minimise").clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }
        right_width = bar.right() - buttons.min_rect().left();
    }
    let menus_rect = row.with_max_x(row.right() - right_width - ui.spacing().item_spacing.x);
    let mut menus_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(menus_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // Where the menus end: the menu bar itself spans its whole area.
    let menus_end = egui::MenuBar::new()
        .ui(&mut menus_ui, |ui| {
            menus(ui);
            ui.cursor().left()
        })
        .inner;
    let left_width = menus_end - bar.left();
    ui.advance_cursor_after_rect(row);

    // Centred on the window, not on the gap between menus and buttons, and
    // shortened with an ellipsis rather than running into either.
    let room = bar.width() - 2.0 * left_width.max(right_width) - 16.0;
    if custom && room > 40.0 {
        let galley = egui::WidgetText::from(egui::RichText::new(title).strong()).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            room,
            egui::TextStyle::Body,
        );
        let pos = bar.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, style.title_text);
    }
}

fn window_menu(ui: &mut egui::Ui, ctx: &egui::Context) {
    if ui.button("Minimise").clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    let max = maximized(ctx);
    if ui.button(if max { "Restore" } else { "Maximise" }).clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
    }
    ui.separator();
    if ui.button("Close").clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

fn button(ui: &mut egui::Ui, colors: &WindowControlColors, kind: Button) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(BUTTON, Sense::click());
    let fg = if resp.hovered() || resp.has_focus() { colors.icon_hover } else { colors.icon };
    let (fg, bg) = match kind {
        Button::Close if resp.hovered() => (colors.close_icon_hover, Some(colors.close_hover)),
        _ if resp.is_pointer_button_down_on() => (fg, Some(colors.pressed)),
        _ if resp.hovered() => (fg, Some(colors.hover)),
        _ => (fg, None),
    };
    let painter = ui.painter();
    if let Some(bg) = bg {
        painter.rect_filled(rect, colors.radius, bg);
    }
    let stroke = Stroke::new(1.2, fg);
    let c = rect.center();
    let r = 4.5;
    match kind {
        Button::Minimize => {
            painter.line_segment([c + vec2(-r, r), c + vec2(r, r)], stroke);
        }
        Button::Maximize => {
            painter.rect_stroke(egui::Rect::from_center_size(c, vec2(2.0 * r, 2.0 * r)), 0.0, stroke, egui::StrokeKind::Middle);
        }
        Button::Restore => {
            let back = egui::Rect::from_center_size(c + vec2(1.5, -1.5), vec2(2.0 * r - 1.5, 2.0 * r - 1.5));
            let front = egui::Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(2.0 * r - 1.5, 2.0 * r - 1.5));
            painter.line_segment([back.left_top(), back.right_top()], stroke);
            painter.line_segment([back.right_top(), back.right_bottom()], stroke);
            painter.line_segment([back.left_top(), egui::pos2(back.left(), front.top())], stroke);
            painter.line_segment([back.right_bottom(), egui::pos2(front.right(), back.bottom())], stroke);
            painter.rect_stroke(front, 0.0, stroke, egui::StrokeKind::Middle);
        }
        Button::Close => {
            painter.line_segment([c + vec2(-r, -r), c + vec2(r, r)], stroke);
            painter.line_segment([c + vec2(-r, r), c + vec2(r, -r)], stroke);
        }
    }
    resp
}

/// Which edge or corner of `window` `pos` is on, if any.
fn edge_at(window: egui::Rect, pos: egui::Pos2) -> Option<ResizeDirection> {
    if !window.contains(pos) {
        return None;
    }
    let near = |d: f32, zone: f32| d < zone;
    let (l, r) = (pos.x - window.left(), window.right() - pos.x);
    let (t, b) = (pos.y - window.top(), window.bottom() - pos.y);
    let corner = |a: f32, b: f32| near(a, CORNER) && near(b, CORNER) && (near(a, EDGE) || near(b, EDGE));
    Some(match () {
        _ if corner(t, l) => ResizeDirection::NorthWest,
        _ if corner(t, r) => ResizeDirection::NorthEast,
        _ if corner(b, l) => ResizeDirection::SouthWest,
        _ if corner(b, r) => ResizeDirection::SouthEast,
        _ if near(t, EDGE) => ResizeDirection::North,
        _ if near(b, EDGE) => ResizeDirection::South,
        _ if near(l, EDGE) => ResizeDirection::West,
        _ if near(r, EDGE) => ResizeDirection::East,
        _ => return None,
    })
}

fn cursor(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::ResizeNorth,
        ResizeDirection::South => CursorIcon::ResizeSouth,
        ResizeDirection::East => CursorIcon::ResizeEast,
        ResizeDirection::West => CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
    }
}

/// The border and resize handles. Call after everything else is drawn, so
/// the handles sit above the floating windows along the edges.
pub fn edges(ctx: &egui::Context, radius: u8, border: Color32) {
    if maximized(ctx) {
        return;
    }
    let window = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("window_border")));
    painter.rect_stroke(window, radius, Stroke::new(1.0, border), egui::StrokeKind::Inside);

    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let Some(direction) = edge_at(window, pos) else {
        return;
    };
    // An area over just the edge under the pointer, on top, so the press
    // goes to the resize and not to whatever is drawn beneath it.
    let grab = egui::Rect::from_center_size(pos, vec2(2.0 * EDGE, 2.0 * EDGE)).intersect(window);
    egui::Area::new(Id::new("window_resize"))
        .order(Order::Foreground)
        .fixed_pos(grab.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(grab.size(), Sense::drag());
            let resp = resp.on_hover_cursor(cursor(direction));
            if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))
    }

    #[test]
    fn edges_and_corners_resize_the_right_way() {
        let w = window();
        assert_eq!(edge_at(w, egui::pos2(400.0, 2.0)), Some(ResizeDirection::North));
        assert_eq!(edge_at(w, egui::pos2(400.0, 598.0)), Some(ResizeDirection::South));
        assert_eq!(edge_at(w, egui::pos2(1.0, 300.0)), Some(ResizeDirection::West));
        assert_eq!(edge_at(w, egui::pos2(799.0, 300.0)), Some(ResizeDirection::East));
        assert_eq!(edge_at(w, egui::pos2(2.0, 10.0)), Some(ResizeDirection::NorthWest));
        assert_eq!(edge_at(w, egui::pos2(790.0, 598.0)), Some(ResizeDirection::SouthEast));
    }

    #[test]
    fn the_inside_and_the_outside_do_not_resize() {
        let w = window();
        assert_eq!(edge_at(w, egui::pos2(400.0, 300.0)), None);
        assert_eq!(edge_at(w, egui::pos2(10.0, 10.0)), None);
        assert_eq!(edge_at(w, egui::pos2(-1.0, 300.0)), None);
    }
}
