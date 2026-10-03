//! Keeps text sharp when the canvas is zoomed.
//!
//! The canvas is drawn into a layer that egui scales with a transform, so glyphs
//! rasterised at 1x are stretched and go soft above 1x zoom. After the frame has
//! been drawn, every text shape in that layer is laid out again at the zoomed font
//! size and then shrunk back, so the layer transform lands it on screen at exactly
//! the pixel size it was rasterised at.
//!
//! The scale is rounded to eighth-octave steps (about 9% apart): continuous zooming
//! would otherwise rasterise a new font size every frame and fill the glyph atlas.
//! Between steps the text is stretched by at most about 4%.

use std::collections::HashMap;
use std::sync::Arc;

use egui::emath::TSTransform;
use egui::epaint::text::LayoutJob;
use egui::epaint::{Galley, TextShape};
use egui::layers::ShapeIdx;
use egui::{Context, LayerId, Shape};

const STEPS_PER_OCTAVE: f32 = 8.0;

/// The zoom that text is laid out at: `zoom` rounded to a step. `1.0` for zooms
/// at or next to 1, where nothing needs doing.
pub(crate) fn text_scale(zoom: f32) -> f32 {
    if !zoom.is_finite() || zoom <= 0.0 {
        return 1.0;
    }
    2f32.powf((zoom.log2() * STEPS_PER_OCTAVE).round() / STEPS_PER_OCTAVE)
}

/// The same text with every size multiplied by `s`.
pub(crate) fn scaled_job(job: &LayoutJob, s: f32) -> LayoutJob {
    let mut job = job.clone();
    for section in &mut job.sections {
        let f = &mut section.format;
        f.font_id.size *= s;
        f.extra_letter_spacing *= s;
        if let Some(h) = &mut f.line_height {
            *h *= s;
        }
        f.expand_bg *= s;
        f.underline.width *= s;
        f.strikethrough.width *= s;
        section.leading_space *= s;
    }
    job.wrap.max_width *= s; // stays infinite if it was
    job.first_row_min_height *= s;
    job
}

fn collect(shape: &Shape, out: &mut HashMap<*const Galley, Arc<Galley>>) {
    match shape {
        Shape::Text(t) => {
            out.entry(Arc::as_ptr(&t.galley))
                .or_insert_with(|| t.galley.clone());
        }
        Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

fn replace(shape: &mut Shape, map: &HashMap<*const Galley, Arc<Galley>>, s: f32) {
    match shape {
        Shape::Text(t) => {
            if let Some(new) = map.get(&Arc::as_ptr(&t.galley)) {
                swap_galley(t, new.clone(), s);
            }
        }
        Shape::Vec(v) => v.iter_mut().for_each(|x| replace(x, map, s)),
        _ => {}
    }
}

/// Use `galley` (laid out `s` times too large) in place of the old one, shrunk
/// back so the shape keeps its size and position in flow space.
fn swap_galley(t: &mut TextShape, galley: Arc<Galley>, s: f32) {
    let pos = t.pos;
    t.galley = galley;
    t.transform(TSTransform::from_scaling(1.0 / s));
    t.pos = pos;
}

/// Re-lay out every text shape painted into `layer` this frame for a canvas at
/// `zoom`. Call after everything is drawn and before the frame ends.
pub(crate) fn crisp_text(ctx: &Context, layer: LayerId, zoom: f32) {
    let s = text_scale(zoom);
    if (s - 1.0).abs() < 1e-3 {
        return;
    }
    // The context lock is not re-entrant, so: gather, lay out, then replace.
    let mut found = HashMap::new();
    ctx.graphics_mut(|g| {
        if let Some(list) = g.get(layer) {
            list.all_entries()
                .for_each(|c| collect(&c.shape, &mut found));
        }
    });
    if found.is_empty() {
        return;
    }
    let scaled: HashMap<*const Galley, Arc<Galley>> = ctx.fonts_mut(|fonts| {
        found
            .iter()
            .map(|(ptr, g)| (*ptr, fonts.layout_job(scaled_job(&g.job, s))))
            .collect()
    });
    ctx.graphics_mut(|g| {
        if let Some(list) = g.get_mut(layer) {
            let count = list.all_entries().len();
            for i in 0..count {
                list.mutate_shape(ShapeIdx(i), |c| replace(&mut c.shape, &scaled, s));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::text::{LayoutSection, TextFormat};
    use egui::{Color32, FontId};

    #[test]
    fn scale_is_one_at_one_and_snaps_to_steps() {
        assert_eq!(text_scale(1.0), 1.0);
        assert!((text_scale(2.0) - 2.0).abs() < 1e-5);
        assert!((text_scale(0.5) - 0.5).abs() < 1e-5);
        assert!((text_scale(1.1) - 2f32.powf(1.0 / 8.0)).abs() < 1e-5);
        // Nonsense zooms fall back to "do nothing".
        for z in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(text_scale(z), 1.0, "{z}");
        }
    }

    #[test]
    fn stretching_between_steps_stays_within_about_four_percent() {
        let mut z = 0.1_f32;
        while z < 8.0 {
            let residual = z / text_scale(z);
            assert!(
                (0.955..=1.046).contains(&residual),
                "zoom {z} -> {residual}"
            );
            z *= 1.013;
        }
    }

    #[test]
    fn scaled_job_scales_every_size() {
        let mut job = LayoutJob {
            text: "hi".into(),
            ..Default::default()
        };
        job.sections.push(LayoutSection {
            leading_space: 4.0,
            byte_range: 0..2,
            format: TextFormat {
                font_id: FontId::proportional(14.0),
                extra_letter_spacing: 1.0,
                line_height: Some(20.0),
                color: Color32::RED,
                ..Default::default()
            },
        });
        job.wrap.max_width = 100.0;
        job.first_row_min_height = 10.0;
        let s = scaled_job(&job, 2.0);
        let f = &s.sections[0].format;
        assert_eq!(f.font_id.size, 28.0);
        assert_eq!((f.extra_letter_spacing, f.line_height), (2.0, Some(40.0)));
        assert_eq!(
            (s.sections[0].leading_space, s.wrap.max_width),
            (8.0, 200.0)
        );
        assert_eq!(s.first_row_min_height, 20.0);
        assert_eq!(f.color, Color32::RED, "colour is untouched");
        assert_eq!(s.text, "hi");

        // An unlimited width stays unlimited.
        job.wrap.max_width = f32::INFINITY;
        assert!(scaled_job(&job, 2.0).wrap.max_width.is_infinite());
    }
}
