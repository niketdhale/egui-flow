//! Canvas configuration.

use crate::theme::FlowTheme;
use crate::types::EdgeKind;

/// Pattern drawn behind the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Background {
    /// No background.
    None,
    #[default]
    /// A grid of dots.
    Dots,
    /// A grid of lines.
    Lines,
    /// A grid of crosses.
    Cross,
}

/// When the connection dots on nodes are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HandleVisibility {
    /// Always visible (React Flow's behaviour).
    #[default]
    Always,
    /// Fade in while the pointer is near the node, the node is selected or a
    /// connection is being dragged. Edges still attach at the same points.
    OnHover,
    /// Never drawn, and connecting by dragging is unavailable. Existing edges
    /// still render and can be created from code.
    Hidden,
}

/// What deleting a group does to its members.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupDelete {
    /// The members go with the group.
    #[default]
    DeleteMembers,
    /// Only the group goes; its members stay where they are, one level up.
    KeepMembers,
}

/// Behaviour switches for [`Flow`](crate::Flow). Every field has a sensible
/// default, so override with struct-update syntax:
/// `FlowOptions { minimap: true, ..Default::default() }`.
#[derive(Clone, Debug)]
pub struct FlowOptions {
    /// Pattern drawn behind the graph.
    pub background: Background,
    /// Background pattern spacing in flow units.
    pub background_gap: f32,
    /// Snap dragged nodes to a grid of this size (flow units).
    pub snap_to_grid: Option<f32>,
    /// Smallest zoom allowed.
    pub min_zoom: f32,
    /// Largest zoom allowed.
    pub max_zoom: f32,
    /// Mouse wheel zooms (like React Flow). When false the wheel pans and
    /// only ctrl+wheel / pinch zooms.
    pub zoom_on_scroll: bool,
    /// Show the minimap.
    pub minimap: bool,
    /// Show the zoom buttons.
    pub controls: bool,
    /// Routing for edges that do not set their own `kind`.
    pub default_edge_kind: EdgeKind,
    /// `Step` and `SmoothStep` edges detour around nodes that the plain route would cross
    /// (see [`edge_path_around`](crate::edge_path_around)). Costs a search for each edge that
    /// crosses a node.
    pub avoid_nodes: bool,
    /// Nodes can be dragged.
    pub nodes_draggable: bool,
    /// Handles can start and receive connections.
    pub nodes_connectable: bool,
    /// When to draw the connection dots on nodes.
    pub handle_visibility: HandleVisibility,
    /// Lay text out again at the zoomed size so it stays sharp when zoomed in
    /// (and clean when zoomed out). Off, text is the 1x raster stretched by the
    /// zoom, which is cheaper but soft above 1x.
    pub crisp_text: bool,
    /// Nodes that are removed fade out instead of vanishing (with `animate`). Costs a
    /// copy of every visible node's shapes each frame, so turn it off for very large graphs.
    pub node_exit_animation: bool,
    /// Canvas colours; the default keeps the ones derived from egui's visuals.
    pub theme: FlowTheme,
    /// Dropping a node on a group puts it in that group; dropping it outside its
    /// group takes it out. Off, membership only changes through the API.
    pub group_drop: bool,
    /// Height of a group's header strip. Only the header (and the collapsed group) is
    /// grabbable, so empty space inside a group still pans and box-selects.
    pub group_header_height: f32,
    /// Nodes and edges can be selected by clicking and box-selecting.
    pub elements_selectable: bool,
    /// Delete/Backspace removes the selection.
    pub delete_key: bool,
    /// What the Delete key does to the members of a selected group.
    pub group_delete: GroupDelete,
    /// Allow an edge from a node to itself.
    pub allow_self_loops: bool,
    /// Frame all nodes on the first frame.
    pub fit_view_on_init: bool,
    /// Margin around the graph for fit-view, in screen pixels.
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
    /// While dragging nodes, snap their edges and centres to other nodes' and
    /// draw guide lines.
    pub alignment_guides: bool,
    /// Snap distance for `alignment_guides`, in screen pixels.
    pub guide_threshold: f32,
    /// Arrow keys nudge the selected nodes (1 unit, or 10 with Shift).
    pub keyboard_nudge: bool,
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
            avoid_nodes: false,
            nodes_draggable: true,
            nodes_connectable: true,
            handle_visibility: HandleVisibility::Always,
            crisp_text: true,
            node_exit_animation: true,
            theme: FlowTheme::default(),
            group_drop: true,
            group_header_height: 30.0,
            elements_selectable: true,
            delete_key: true,
            group_delete: GroupDelete::DeleteMembers,
            allow_self_loops: false,
            fit_view_on_init: false,
            fit_view_padding: 50.0,
            animate: true,
            view_transition: 0.35,
            connection_radius: 20.0,
            edges_reconnectable: true,
            nodes_resizable: true,
            alignment_guides: false,
            guide_threshold: 6.0,
            keyboard_nudge: true,
            keyboard_shortcuts: true,
            highlight_connected: false,
        }
    }
}
