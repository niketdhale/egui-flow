//! Plain data types: ids, nodes, edges, handles and the viewport.

use egui::{Color32, Pos2, Rect, Vec2, pos2, vec2};

macro_rules! id_type {
    ($(#[$m:meta])* $name:ident($inner:ty)) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name(pub $inner);
    };
}

id_type!(
    /// Identifies a node within one [`FlowState`](crate::FlowState).
    NodeId(u64)
);
id_type!(
    /// Identifies an edge within one [`FlowState`](crate::FlowState).
    EdgeId(u64)
);
id_type!(
    /// Identifies a handle (connection port) within one node.
    HandleId(u32)
);

/// Which side of a node a handle sits on. Also gives the direction an edge
/// leaves/enters the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Side {
    /// The left side.
    Left,
    /// The right side.
    Right,
    /// The top side.
    Top,
    /// The bottom side.
    Bottom,
}

impl Side {
    /// Outward unit direction from the node.
    pub fn dir(self) -> Vec2 {
        match self {
            Side::Left => vec2(-1.0, 0.0),
            Side::Right => vec2(1.0, 0.0),
            Side::Top => vec2(0.0, -1.0),
            Side::Bottom => vec2(0.0, 1.0),
        }
    }

    /// Whether this is `Left` or `Right`.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }
}

/// Whether a handle starts or ends a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HandleKind {
    /// Wires leave from a source.
    Source,
    /// Wires arrive at a target.
    Target,
}

/// A connection port on a node.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Handle {
    /// Identifies the handle within its node.
    pub id: HandleId,
    /// Source or target.
    pub kind: HandleKind,
    /// Which side of the node it is on.
    pub side: Side,
    /// Position along `side`, `0.0..=1.0` (default `0.5`, centred).
    pub offset: f32,
    /// Accept wires anywhere along `side`, not just at `offset`. Each wire remembers where it
    /// landed ([`Edge::source_offset`] / [`Edge::target_offset`]). Made for bus bars.
    #[cfg_attr(feature = "serde", serde(default))]
    pub along: bool,
}

impl Handle {
    /// Id of the default target handle (left side).
    pub const DEFAULT_TARGET: HandleId = HandleId(0);
    /// Id of the default source handle (right side).
    pub const DEFAULT_SOURCE: HandleId = HandleId(1);

    /// A source handle on `side`, centred.
    pub fn source(id: HandleId, side: Side) -> Self {
        Self {
            id,
            kind: HandleKind::Source,
            side,
            offset: 0.5,
            along: false,
        }
    }

    /// A target handle on `side`, centred.
    pub fn target(id: HandleId, side: Side) -> Self {
        Self {
            id,
            kind: HandleKind::Target,
            side,
            offset: 0.5,
            along: false,
        }
    }

    /// Place the handle `offset` (`0.0..=1.0`) along its side.
    pub fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset.clamp(0.0, 1.0);
        self
    }

    /// Accept wires anywhere along this side. See [`Handle::along`](Self#structfield.along).
    pub fn along(mut self) -> Self {
        self.along = true;
        self
    }

    /// Where this handle sits for a node occupying `rect`.
    pub fn position(&self, rect: Rect) -> Pos2 {
        self.position_at(rect, None)
    }

    /// Like [`position`](Self::position), but a wire's own `offset` wins on an `along` handle.
    pub fn position_at(&self, rect: Rect, offset: Option<f32>) -> Pos2 {
        let o = offset
            .filter(|_| self.along)
            .map_or(self.offset, |o| o.clamp(0.0, 1.0));
        match self.side {
            Side::Left => pos2(rect.min.x, rect.min.y + rect.height() * o),
            Side::Right => pos2(rect.max.x, rect.min.y + rect.height() * o),
            Side::Top => pos2(rect.min.x + rect.width() * o, rect.min.y),
            Side::Bottom => pos2(rect.min.x + rect.width() * o, rect.max.y),
        }
    }
}

impl Handle {
    /// The two ends of this handle's side of `rect`.
    pub fn side_segment(&self, rect: Rect) -> [Pos2; 2] {
        let (a, b) = (
            Handle {
                offset: 0.0,
                ..*self
            },
            Handle {
                offset: 1.0,
                ..*self
            },
        );
        [a.position(rect), b.position(rect)]
    }

    /// The `offset` (`0.0..=1.0`) of the point on this handle's side nearest to `p`.
    pub fn offset_near(&self, rect: Rect, p: Pos2) -> f32 {
        let [a, b] = self.side_segment(rect);
        let ab = b - a;
        if ab.length_sq() == 0.0 {
            return 0.5;
        }
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    }
}

/// Shape of an edge's arrowhead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ArrowStyle {
    /// A filled triangle.
    #[default]
    Triangle,
    /// Two strokes forming a chevron.
    Open,
    /// A filled dot.
    Circle,
    /// A filled diamond.
    Diamond,
}

/// Where and how an edge's label is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EdgeLabelStyle {
    /// Position along the edge: `0.0` at the source, `1.0` at the target.
    pub position: f32,
    /// Font size in flow units.
    pub size: f32,
    /// `None` uses the theme's text colour.
    pub color: Option<Color32>,
    /// Fill behind the text; `None` uses the theme's window fill.
    pub background: Option<Color32>,
}

impl Default for EdgeLabelStyle {
    fn default() -> Self {
        Self {
            position: 0.5,
            size: 12.0,
            color: None,
            background: None,
        }
    }
}

/// Stroke pattern of an edge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LineStyle {
    /// A continuous line.
    #[default]
    Solid,
    /// Short dashes.
    Dashed,
    /// Dots, sized to the line width.
    Dotted,
    /// Custom dash and gap lengths in flow units.
    Custom {
        /// Dash length.
        dash: f32,
        /// Gap length.
        gap: f32,
    },
}

impl LineStyle {
    /// `(dash, gap)` lengths for a line of `width`, or `None` when solid.
    pub fn pattern(self, width: f32) -> Option<(f32, f32)> {
        let w = width.max(1.0);
        match self {
            LineStyle::Solid => None,
            LineStyle::Dashed => Some((6.0, 4.0)),
            LineStyle::Dotted => Some((w, 2.0 * w + 2.0)),
            LineStyle::Custom { dash, gap } => Some((dash.max(0.5), gap.max(0.5))),
        }
    }
}

/// How an edge is routed between its two handles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EdgeKind {
    /// Smooth cubic bezier (React Flow's default).
    #[default]
    Bezier,
    /// A direct line.
    Straight,
    /// Right-angled routing.
    Step,
    /// Right-angled routing with rounded corners.
    SmoothStep,
}

/// A node on the canvas. `data` is yours; the library only reads/writes the
/// other fields.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Node<D> {
    /// Identifies the node.
    pub id: NodeId,
    /// Top-left corner in flow coordinates.
    pub position: Pos2,
    /// Size in flow coordinates. Measured from the rendered content every
    /// frame, so this is only an initial estimate for the first frame.
    pub size: Vec2,
    /// Size set by resizing (or by you). `None` lets the content decide. The
    /// width is applied exactly, the height as a minimum, so a node never
    /// shrinks below its content.
    pub fixed_size: Option<Vec2>,
    /// Smallest size a resize drag can reach.
    pub min_size: Vec2,
    /// Largest size a resize drag can reach.
    pub max_size: Option<Vec2>,
    /// Show resize grips when the node is selected.
    pub resizable: bool,
    /// The group this node sits in. `position` is then relative to that group's
    /// top-left corner, so moving the group moves its members. While
    /// [`Flow::show`](crate::Flow::show) runs (including in your viewer callbacks)
    /// positions are in flow space; use
    /// [`FlowState::abs_position`](crate::FlowState::abs_position) outside it.
    pub parent: Option<NodeId>,
    /// A container for other nodes (see [`FlowState::set_parent`](crate::FlowState::set_parent)).
    /// Groups draw behind their members and can be collapsed.
    pub is_group: bool,
    /// Keep this node inside its group while it is dragged or nudged: it stops at the
    /// group's edges (and below its header) instead of leaving. Has no effect on a node
    /// without a group. Moving the group itself, the API, and undo are not constrained.
    pub constrain_to_parent: bool,
    /// For groups: hide the members and show just the group's header. Edges to hidden
    /// members attach to the group instead.
    pub collapsed: bool,
    /// Your data, shown by [`FlowViewer::node_ui`](crate::FlowViewer::node_ui).
    pub data: D,
    /// Currently selected.
    pub selected: bool,
    /// Can be dragged (needs `FlowOptions::nodes_draggable`).
    pub draggable: bool,
    /// Can take part in connections.
    pub connectable: bool,
    /// Can be removed with the Delete key.
    pub deletable: bool,
}

impl<D> Node<D> {
    /// A node at `position` carrying `data`, with default behaviour.
    pub fn new(id: NodeId, position: Pos2, data: D) -> Self {
        Self {
            id,
            position,
            size: vec2(150.0, 40.0),
            fixed_size: None,
            min_size: vec2(60.0, 30.0),
            max_size: None,
            resizable: true,
            parent: None,
            constrain_to_parent: false,
            is_group: false,
            collapsed: false,
            data,
            selected: false,
            draggable: true,
            connectable: true,
            deletable: true,
        }
    }

    /// Make this node a group of the given size.
    pub fn group(mut self, size: Vec2) -> Self {
        self.is_group = true;
        self.fixed_size = Some(size);
        self
    }

    /// Keep this node inside its group when dragged (see [`constrain_to_parent`](Self::constrain_to_parent)).
    pub fn constrained(mut self) -> Self {
        self.constrain_to_parent = true;
        self
    }

    /// Place this node inside `parent` (its `position` is then relative to the parent).
    pub fn in_group(mut self, parent: NodeId) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Start with a fixed size instead of sizing to the content.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.fixed_size = Some(size);
        self
    }

    /// The node's rectangle in flow coordinates (relative to its group, if it has one).
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.position, self.size)
    }
}

/// A connection between a source handle and a target handle.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Edge<D> {
    /// Identifies the edge.
    pub id: EdgeId,
    /// The node the wire leaves.
    pub source: NodeId,
    /// The handle it leaves from.
    pub source_handle: HandleId,
    /// The node the wire arrives at.
    pub target: NodeId,
    /// The handle it arrives at.
    pub target_handle: HandleId,
    /// Where along an [`along`](Handle::along) source handle this wire leaves (`0.0..=1.0`).
    pub source_offset: Option<f32>,
    /// Where along an [`along`](Handle::along) target handle this wire arrives.
    pub target_offset: Option<f32>,
    /// Your data.
    pub data: D,
    /// Overrides the canvas default routing.
    pub kind: Option<EdgeKind>,
    /// Text drawn on the edge; see `label_style`.
    pub label: Option<String>,
    /// Stroke pattern (solid, dashed, dotted, custom).
    pub line_style: LineStyle,
    /// Draw marching dashes. Uses `line_style`'s pattern, or dashes if solid.
    pub animated: bool,
    /// Draw an arrowhead at the target.
    pub arrow: bool,
    /// Shape of the arrowhead(s).
    pub arrow_style: ArrowStyle,
    /// Also draw an arrowhead at the source (for two-way links).
    pub arrow_at_source: bool,
    /// Where and how `label` is drawn.
    pub label_style: EdgeLabelStyle,
    /// Stroke colour; `None` uses the theme's edge colour.
    pub color: Option<Color32>,
    /// Stroke width in flow units; `None` uses 1.5.
    pub width: Option<f32>,
    /// Speed of the marching dashes of an `animated` edge, in flow units per
    /// second. Negative values run from target to source.
    pub animation_speed: f32,
    /// Currently selected.
    pub selected: bool,
    /// Can be removed with the Delete key.
    pub deletable: bool,
}

impl<D> Edge<D> {
    /// This edge's endpoints.
    pub fn connection(&self) -> Connection {
        Connection {
            source: self.source,
            source_handle: self.source_handle,
            target: self.target,
            target_handle: self.target_handle,
        }
    }
}

/// The endpoints of a (proposed or existing) edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Connection {
    /// The node the wire leaves.
    pub source: NodeId,
    /// The handle it leaves from.
    pub source_handle: HandleId,
    /// The node the wire arrives at.
    pub target: NodeId,
    /// The handle it arrives at.
    pub target_handle: HandleId,
}

/// Pan and zoom. A flow-space point `p` is drawn at
/// `canvas.min + pan + p * zoom` on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Viewport {
    /// Where the flow origin is on screen, in pixels.
    pub pan: Vec2,
    /// Scale, `1.0` is actual size.
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// Screen position (relative to the canvas top-left) of a flow point.
    pub fn to_screen(&self, p: Pos2) -> Pos2 {
        pos2(p.x * self.zoom + self.pan.x, p.y * self.zoom + self.pan.y)
    }

    /// Flow point under a canvas-relative screen position.
    pub fn to_flow(&self, p: Pos2) -> Pos2 {
        pos2(
            (p.x - self.pan.x) / self.zoom,
            (p.y - self.pan.y) / self.zoom,
        )
    }

    /// Zoom by `factor` keeping the canvas-relative point `anchor` fixed.
    pub fn zoom_at(&mut self, anchor: Pos2, factor: f32, min: f32, max: f32) {
        let new_zoom = (self.zoom * factor).clamp(min, max);
        let flow = self.to_flow(anchor);
        self.zoom = new_zoom;
        self.pan = anchor.to_vec2() - flow.to_vec2() * new_zoom;
    }
}

#[cfg(test)]
mod line_style_tests {
    use super::*;

    #[test]
    fn line_style_patterns() {
        assert_eq!(LineStyle::Solid.pattern(2.0), None);
        assert_eq!(LineStyle::Dashed.pattern(2.0), Some((6.0, 4.0)));
        let (dash, gap) = LineStyle::Dotted.pattern(2.0).unwrap();
        assert!(dash <= 2.0 && gap > dash, "dots are short with wider gaps");
        assert_eq!(
            LineStyle::Custom {
                dash: 0.0,
                gap: 3.0
            }
            .pattern(1.0),
            Some((0.5, 3.0))
        );
    }
}

#[cfg(test)]
mod along_tests {
    use super::*;

    #[test]
    fn along_handles_follow_the_wires_offset_and_plain_ones_ignore_it() {
        let rect = Rect::from_min_size(pos2(100.0, 50.0), vec2(200.0, 20.0));
        let plain = Handle::target(HandleId(0), Side::Top);
        let along = plain.along();
        assert_eq!(plain.position_at(rect, Some(0.1)), plain.position(rect));
        assert_eq!(along.position_at(rect, Some(0.25)), pos2(150.0, 50.0));
        assert_eq!(along.position_at(rect, None), pos2(200.0, 50.0));
        assert_eq!(
            along.position_at(rect, Some(9.0)),
            pos2(300.0, 50.0),
            "clamped"
        );
    }

    #[test]
    fn offset_near_projects_onto_the_side() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 20.0));
        let top = Handle::target(HandleId(0), Side::Top).along();
        assert_eq!(top.offset_near(rect, pos2(50.0, -30.0)), 0.25);
        assert_eq!(top.offset_near(rect, pos2(-80.0, 5.0)), 0.0);
        let right = Handle::source(HandleId(1), Side::Right).along();
        assert_eq!(right.offset_near(rect, pos2(260.0, 15.0)), 0.75);
    }
}
