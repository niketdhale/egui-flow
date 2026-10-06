//! Colours for the canvas, so it can match your app without touching egui's global style.

use egui::{Color32, Visuals};

/// Colours for the parts of the canvas egui-flow draws itself. Every field is optional:
/// `None` keeps the colour derived from the current egui [`Visuals`], so
/// `FlowTheme::default()` changes nothing.
///
/// Set it with [`FlowOptions::theme`](crate::FlowOptions::theme) or
/// [`Flow::theme`](crate::Flow::theme). Presets: [`dark`](Self::dark),
/// [`light`](Self::light) and [`blueprint`](Self::blueprint).
///
/// The `node_*` and `text` colours restyle what your viewer draws inside nodes, by
/// overriding the matching egui visuals for the node content only: the default
/// [`FlowViewer::node_frame`](crate::FlowViewer::node_frame) and plain labels pick them
/// up. Colours you set explicitly in `node_ui` or `node_frame` still win.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlowTheme {
    /// The canvas fill.
    pub background: Option<Color32>,
    /// The dots, lines or crosses drawn on it.
    pub grid: Option<Color32>,
    /// Edges that have no colour of their own.
    pub edge: Option<Color32>,
    /// Selection outlines and rings, selected edges, active handles, the box-select
    /// rectangle and the default pulse colour.
    pub selection: Option<Color32>,
    /// Idle connection handles.
    pub handle: Option<Color32>,
    /// Alignment guide lines.
    pub guide: Option<Color32>,
    /// Behind edge labels that have no background of their own.
    pub label_background: Option<Color32>,
    /// The minimap panel.
    pub minimap_background: Option<Color32>,
    /// Fill of the default node frame.
    pub node_fill: Option<Color32>,
    /// Outline of the default node frame.
    pub node_stroke: Option<Color32>,
    /// Default text colour inside nodes.
    pub text: Option<Color32>,
}

const fn rgb(r: u8, g: u8, b: u8) -> Option<Color32> {
    Some(Color32::from_rgb(r, g, b))
}

impl FlowTheme {
    /// Near-black canvas with soft grey edges and a blue selection.
    pub const fn dark() -> Self {
        Self {
            background: rgb(14, 14, 18),
            grid: rgb(52, 52, 64),
            edge: rgb(150, 150, 170),
            selection: rgb(90, 150, 255),
            handle: rgb(170, 170, 185),
            guide: rgb(255, 90, 160),
            label_background: rgb(28, 28, 34),
            minimap_background: rgb(24, 24, 30),
            node_fill: rgb(28, 28, 34),
            node_stroke: rgb(70, 70, 84),
            text: rgb(225, 225, 235),
        }
    }

    /// Off-white canvas with slate edges and a strong blue selection.
    pub const fn light() -> Self {
        Self {
            background: rgb(246, 246, 249),
            grid: rgb(206, 206, 216),
            edge: rgb(110, 110, 130),
            selection: rgb(36, 98, 232),
            handle: rgb(90, 90, 110),
            guide: rgb(226, 40, 120),
            label_background: rgb(255, 255, 255),
            minimap_background: rgb(255, 255, 255),
            node_fill: rgb(255, 255, 255),
            node_stroke: rgb(190, 190, 204),
            text: rgb(34, 34, 48),
        }
    }

    /// Deep blue drawing-board look with pale blue lines and a yellow selection.
    pub const fn blueprint() -> Self {
        Self {
            background: rgb(16, 42, 84),
            grid: rgb(44, 86, 148),
            edge: rgb(170, 205, 255),
            selection: rgb(255, 214, 10),
            handle: rgb(200, 225, 255),
            guide: rgb(255, 140, 0),
            label_background: rgb(14, 36, 72),
            minimap_background: rgb(12, 32, 66),
            node_fill: rgb(22, 54, 104),
            node_stroke: rgb(120, 170, 235),
            text: rgb(230, 240, 255),
        }
    }

    /// Apply the node colours to the visuals used for node content.
    pub(crate) fn apply_to_node_visuals(&self, v: &mut Visuals) {
        if let Some(c) = self.node_fill {
            v.window_fill = c;
        }
        if let Some(c) = self.node_stroke {
            v.widgets.noninteractive.bg_stroke.color = c;
        }
        if let Some(c) = self.text {
            v.widgets.noninteractive.fg_stroke.color = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_theme_changes_nothing() {
        let t = FlowTheme::default();
        assert!(
            [
                t.background,
                t.grid,
                t.edge,
                t.selection,
                t.handle,
                t.guide,
                t.label_background,
                t.minimap_background,
                t.node_fill,
                t.node_stroke,
                t.text
            ]
            .iter()
            .all(Option::is_none)
        );
        let mut v = Visuals::dark();
        let before = v.clone();
        t.apply_to_node_visuals(&mut v);
        assert_eq!(v, before);
    }

    #[test]
    fn presets_set_every_colour_and_differ() {
        for t in [
            FlowTheme::dark(),
            FlowTheme::light(),
            FlowTheme::blueprint(),
        ] {
            assert!(
                [
                    t.background,
                    t.grid,
                    t.edge,
                    t.selection,
                    t.handle,
                    t.guide,
                    t.label_background,
                    t.minimap_background,
                    t.node_fill,
                    t.node_stroke,
                    t.text
                ]
                .iter()
                .all(Option::is_some)
            );
        }
        let (d, l, b) = (
            FlowTheme::dark(),
            FlowTheme::light(),
            FlowTheme::blueprint(),
        );
        assert!(d != l && l != b && d != b);
        // Text must be readable on its node fill, in each preset.
        for t in [d, l, b] {
            let lum =
                |c: Color32| 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
            let diff = (lum(t.text.unwrap()) - lum(t.node_fill.unwrap())).abs();
            assert!(diff > 110.0, "text contrast {diff}");
        }
    }

    #[test]
    fn node_colours_override_only_what_is_set() {
        let mut v = Visuals::dark();
        let before = v.clone();
        let t = FlowTheme {
            node_fill: rgb(1, 2, 3),
            text: rgb(9, 8, 7),
            ..Default::default()
        };
        t.apply_to_node_visuals(&mut v);
        assert_eq!(v.window_fill, Color32::from_rgb(1, 2, 3));
        assert_eq!(
            v.widgets.noninteractive.fg_stroke.color,
            Color32::from_rgb(9, 8, 7)
        );
        assert_eq!(
            v.widgets.noninteractive.bg_stroke,
            before.widgets.noninteractive.bg_stroke
        );
    }
}
