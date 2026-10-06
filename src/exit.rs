//! Fade-out for nodes that disappear.
//!
//! A removed node has no data left to draw, so while enabled the canvas keeps a copy of
//! the shapes each visible node was last painted with. When a node vanishes, that copy
//! (a "ghost") is drawn with a fading opacity for a short while.

use std::sync::Arc;

use egui::epaint::PathStroke;
use egui::{Color32, Shape, Stroke};

/// A removed node, fading out.
pub(crate) struct Ghost {
    pub shapes: Vec<Shape>,
    /// Set on the first frame the ghost is drawn.
    pub start: Option<f64>,
}

fn dim(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

fn dim_stroke(s: &mut Stroke, a: f32) {
    s.color = dim(s.color, a);
}

fn dim_path_stroke(s: &mut PathStroke, a: f32) {
    if let egui::epaint::ColorMode::Solid(c) = &mut s.color {
        *c = dim(*c, a);
    }
}

/// Multiply the opacity of everything in `shape` by `a` (0 = invisible).
pub(crate) fn fade_shape(shape: &mut Shape, a: f32) {
    match shape {
        Shape::Text(t) => t.opacity_factor *= a,
        Shape::Rect(r) => {
            r.fill = dim(r.fill, a);
            dim_stroke(&mut r.stroke, a);
        }
        Shape::Circle(c) => {
            c.fill = dim(c.fill, a);
            dim_stroke(&mut c.stroke, a);
        }
        Shape::Ellipse(e) => {
            e.fill = dim(e.fill, a);
            dim_stroke(&mut e.stroke, a);
        }
        Shape::LineSegment { stroke, .. } => dim_stroke(stroke, a),
        Shape::Path(p) => {
            p.fill = dim(p.fill, a);
            dim_path_stroke(&mut p.stroke, a);
        }
        Shape::QuadraticBezier(b) => {
            b.fill = dim(b.fill, a);
            dim_path_stroke(&mut b.stroke, a);
        }
        Shape::CubicBezier(b) => {
            b.fill = dim(b.fill, a);
            dim_path_stroke(&mut b.stroke, a);
        }
        Shape::Mesh(m) => {
            for v in &mut Arc::make_mut(m).vertices {
                v.color = dim(v.color, a);
            }
        }
        Shape::Vec(v) => v.iter_mut().for_each(|s| fade_shape(s, a)),
        Shape::Noop | Shape::Callback(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect, pos2, vec2};

    #[test]
    fn fading_scales_every_colour_and_recurses() {
        let red = Color32::from_rgb(200, 100, 50);
        let mut rect = Shape::rect_filled(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
            0.0,
            red,
        );
        fade_shape(&mut rect, 0.5);
        let Shape::Rect(r) = &rect else { panic!() };
        assert_eq!(r.fill, red.gamma_multiply(0.5));

        let mut group = Shape::Vec(vec![
            Shape::circle_filled(Pos2::ZERO, 4.0, red),
            Shape::line_segment([Pos2::ZERO, pos2(5.0, 5.0)], Stroke::new(2.0_f32, red)),
            Shape::line(vec![Pos2::ZERO, pos2(5.0, 5.0)], Stroke::new(2.0_f32, red)),
        ]);
        fade_shape(&mut group, 0.25);
        let Shape::Vec(v) = &group else { panic!() };
        assert!(matches!(&v[0], Shape::Circle(c) if c.fill == red.gamma_multiply(0.25)));
        assert!(
            matches!(&v[1], Shape::LineSegment { stroke, .. } if stroke.color == red.gamma_multiply(0.25))
        );
        assert!(
            matches!(&v[2], Shape::Path(p) if matches!(p.stroke.color, egui::epaint::ColorMode::Solid(c) if c == red.gamma_multiply(0.25)))
        );

        // Fully faded is fully transparent.
        let mut gone = Shape::circle_filled(Pos2::ZERO, 4.0, red);
        fade_shape(&mut gone, 0.0);
        assert!(matches!(&gone, Shape::Circle(c) if c.fill == Color32::TRANSPARENT));
        // Nothing to do for a no-op.
        fade_shape(&mut Shape::Noop, 0.5);
    }
}
