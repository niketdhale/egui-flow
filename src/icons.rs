//! Built-in vector icons, drawn with egui's painter.
//!
//! They need no font, image loader or extra dependency, so they never render
//! as empty boxes the way missing font glyphs do, and they scale and tint like
//! text. Use [`Icon::paint`] to draw one at a position, [`icon`] as a plain
//! widget, or [`icon_button`] for a clickable one.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

/// A built-in icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Check,
    Close,
    Plus,
    Minus,
    ChevronUp,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    /// Solid triangle pointing right (collapsed disclosure).
    TriangleRight,
    /// Solid triangle pointing down (expanded disclosure).
    TriangleDown,
    ArrowRight,
    ArrowLeft,
}

impl Icon {
    /// Every icon, e.g. for a gallery.
    pub const ALL: [Icon; 12] = [
        Icon::Check,
        Icon::Close,
        Icon::Plus,
        Icon::Minus,
        Icon::ChevronUp,
        Icon::ChevronDown,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::TriangleRight,
        Icon::TriangleDown,
        Icon::ArrowRight,
        Icon::ArrowLeft,
    ];

    /// The shapes for this icon centred in `rect`, drawn in `color`.
    pub fn shapes(self, rect: Rect, color: Color32) -> Vec<Shape> {
        let c = rect.center();
        let r = rect.width().min(rect.height()) / 2.0;
        let stroke = Stroke::new((r * 0.2).max(1.0), color);
        // Point at (x, y) in -1..=1 icon space.
        let p = |x: f32, y: f32| pos2(c.x + x * r * 0.6, c.y + y * r * 0.6);
        let line = |pts: Vec<Pos2>| Shape::line(pts, stroke);
        let tri = |pts: Vec<Pos2>| Shape::convex_polygon(pts, color, Stroke::NONE);
        match self {
            Icon::Check => vec![line(vec![p(-1.0, 0.1), p(-0.35, 0.75), p(1.0, -0.7)])],
            Icon::Close => vec![
                line(vec![p(-0.8, -0.8), p(0.8, 0.8)]),
                line(vec![p(-0.8, 0.8), p(0.8, -0.8)]),
            ],
            Icon::Plus => vec![
                line(vec![p(-1.0, 0.0), p(1.0, 0.0)]),
                line(vec![p(0.0, -1.0), p(0.0, 1.0)]),
            ],
            Icon::Minus => vec![line(vec![p(-1.0, 0.0), p(1.0, 0.0)])],
            Icon::ChevronUp => vec![line(vec![p(-0.9, 0.4), p(0.0, -0.5), p(0.9, 0.4)])],
            Icon::ChevronDown => vec![line(vec![p(-0.9, -0.4), p(0.0, 0.5), p(0.9, -0.4)])],
            Icon::ChevronLeft => vec![line(vec![p(0.4, -0.9), p(-0.5, 0.0), p(0.4, 0.9)])],
            Icon::ChevronRight => vec![line(vec![p(-0.4, -0.9), p(0.5, 0.0), p(-0.4, 0.9)])],
            Icon::TriangleRight => vec![tri(vec![p(-0.6, -0.9), p(0.9, 0.0), p(-0.6, 0.9)])],
            Icon::TriangleDown => vec![tri(vec![p(-0.9, -0.6), p(0.9, -0.6), p(0.0, 0.9)])],
            Icon::ArrowRight => vec![
                line(vec![p(-1.0, 0.0), p(1.0, 0.0)]),
                line(vec![p(0.2, -0.8), p(1.0, 0.0), p(0.2, 0.8)]),
            ],
            Icon::ArrowLeft => vec![
                line(vec![p(1.0, 0.0), p(-1.0, 0.0)]),
                line(vec![p(-0.2, -0.8), p(-1.0, 0.0), p(-0.2, 0.8)]),
            ],
        }
    }

    /// Draw the icon centred in `rect`.
    pub fn paint(self, painter: &egui::Painter, rect: Rect, color: Color32) {
        painter.extend(self.shapes(rect, color));
    }
}

/// Add an icon of `size` points as a widget, tinted with the text colour.
pub fn icon(ui: &mut Ui, icon: Icon, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        icon.paint(ui.painter(), rect, ui.visuals().text_color());
    }
    response
}

/// A small clickable icon that highlights on hover.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32) -> Response {
    let pad = vec2(4.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size) + pad * 2.0, Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact(&response);
        if response.hovered() || response.has_focus() {
            ui.painter().rect_filled(rect, 4.0, v.bg_fill);
        }
        icon.paint(ui.painter(), rect.shrink2(pad), v.fg_stroke.color);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_draws_inside_its_rect() {
        let rect = Rect::from_center_size(pos2(50.0, 50.0), Vec2::splat(20.0));
        for i in Icon::ALL {
            let shapes = i.shapes(rect, Color32::WHITE);
            assert!(!shapes.is_empty(), "{i:?}");
            for s in shapes {
                assert!(
                    rect.expand(1.0)
                        .contains_rect(s.visual_bounding_rect().shrink(1.5)),
                    "{i:?}"
                );
            }
        }
    }
}
