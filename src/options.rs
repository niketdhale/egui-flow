//! Canvas configuration.

use crate::types::EdgeKind;

/// Pattern drawn behind the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Background {
    None,
    #[default]
    Dots,
    Lines,
    Cross,
}

/// Behaviour switches for [`Flow`](crate::Flow). Every field has a sensible
/// default, so override with struct-update syntax:
/// `FlowOptions { minimap: true, ..Default::default() }`.
#[derive(Clone, Debug)]
pub struct FlowOptions {
    pub background: Background,
    /// Background pattern spacing in flow units.
    pub background_gap: f32,
    /// Snap dragged nodes to a grid of this size (flow units).
    pub snap_to_grid: Option<f32>,
    pub min_zoom: f32,
    pub max_zoom: f32,
    /// Mouse wheel zooms (like React Flow). When false the wheel pans and
    /// only ctrl+wheel / pinch zooms.
    pub zoom_on_scroll: bool,
    pub minimap: bool,
    pub controls: bool,
    pub default_edge_kind: EdgeKind,
    pub nodes_draggable: bool,
    pub nodes_connectable: bool,
    pub elements_selectable: bool,
    /// Delete/Backspace removes the selection.
    pub delete_key: bool,
    pub allow_self_loops: bool,
    /// Frame all nodes on the first frame.
    pub fit_view_on_init: bool,
    pub fit_view_padding: f32,
    /// Master switch for UI animation: eased view transitions, node fade-in
    /// and hover easing. Turn off for reduced-motion users. Edges you marked
    /// `animated` and explicit [`pulse_edge`](crate::FlowState::pulse_edge)
    /// calls are unaffected.
    pub animate: bool,
    /// Duration in seconds of eased view transitions (zoom buttons, animated fit).
    pub view_transition: f32,
    /// How close (flow units) a dragged connection must get to a handle to snap to it.
    pub connection_radius: f32,
    /// Drag the ends of a selected edge to another handle.
    pub edges_reconnectable: bool,
    /// Show resize grips on selected nodes (see [`Node::resizable`](crate::Node::resizable)).
    pub nodes_resizable: bool,
    /// Emit undo/redo/copy/cut/paste/duplicate request events for Ctrl/Cmd+Z, Shift+Z / Y,
    /// C, X, V, D while the pointer is over the canvas. Feed them to an
    /// [`Editor`](crate::Editor).
    pub keyboard_shortcuts: bool,
    /// While a node is selected or hovered, dim everything not connected to it.
    pub highlight_connected: bool,
}

impl Default for FlowOptions {
    fn default() -> Self {
        Self {
            background: Background::Dots,
            background_gap: 20.0,
            snap_to_grid: None,
            min_zoom: 0.1,
            max_zoom: 4.0,
            zoom_on_scroll: true,
            minimap: false,
            controls: true,
            default_edge_kind: EdgeKind::Bezier,
            nodes_draggable: true,
            nodes_connectable: true,
            elements_selectable: true,
            delete_key: true,
            allow_self_loops: false,
            fit_view_on_init: false,
            fit_view_padding: 50.0,
            animate: true,
            view_transition: 0.35,
            connection_radius: 20.0,
            edges_reconnectable: true,
            nodes_resizable: true,
            keyboard_shortcuts: true,
            highlight_connected: false,
        }
    }
}
