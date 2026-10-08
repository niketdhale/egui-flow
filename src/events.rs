//! Things that happened during a frame.

use egui::{Pos2, Response, Vec2};

use crate::state::PulseDirection;
use crate::types::*;

/// An interaction the application may want to react to.
#[derive(Clone, Debug)]
pub enum FlowEvent<N, E> {
    /// A node was clicked.
    NodeClicked(NodeId),
    /// An edge was clicked.
    EdgeClicked(EdgeId),
    /// The empty canvas was clicked (flow coordinates).
    PaneClicked(Pos2),
    /// One or more nodes moved this frame.
    NodesDragged(Vec<NodeId>),
    /// A node drag ended; these nodes were being moved.
    NodesDragStopped(Vec<NodeId>),
    /// A new edge was created by dragging between handles.
    Connected(EdgeId),
    /// An existing edge's end was dragged onto another handle. The edge already
    /// carries `new`; mirror it into your own model.
    Reconnected {
        /// The edge that moved.
        edge: EdgeId,
        /// Its endpoints before.
        old: Connection,
        /// Its endpoints now.
        new: Connection,
    },
    /// A node's size changed by dragging a resize grip. `finished` is set on
    /// the last event, when the pointer is released.
    NodeResized {
        /// The node being resized.
        node: NodeId,
        /// Its new size, in flow units.
        size: Vec2,
        /// `true` on the last event, when the pointer is released.
        finished: bool,
    },
    /// A group was collapsed or expanded with its header toggle.
    GroupToggled {
        /// The group.
        node: NodeId,
        /// Whether it is now collapsed.
        collapsed: bool,
    },
    /// A node was dropped into a group, or out of one. `parent` is the group it is
    /// now in (`None` for the top level).
    ParentChanged {
        /// The node that moved.
        node: NodeId,
        /// The group it is now in, or `None` for the top level.
        parent: Option<NodeId>,
    },
    /// An animated [`auto_layout_animated`](crate::FlowState::auto_layout_animated) arrived.
    LayoutFinished,
    /// Ctrl/Cmd+Z. Handled for you by [`Editor::process`](crate::Editor::process).
    UndoRequested,
    /// Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y.
    RedoRequested,
    /// Ctrl/Cmd+C.
    CopyRequested,
    /// Ctrl/Cmd+X.
    CutRequested,
    /// Ctrl/Cmd+V.
    PasteRequested,
    /// Ctrl/Cmd+D.
    DuplicateRequested,
    /// A pulse finished its leg along `edge`. `tag` is [`PulseStyle::tag`](crate::PulseStyle::tag).
    PulseArrived {
        /// The edge the pulse travelled along.
        edge: EdgeId,
        /// The pulse's [`tag`](crate::PulseStyle::tag).
        tag: u64,
        /// Which way it travelled.
        direction: PulseDirection,
    },
    /// A connection drag was released without creating an edge. Useful for "drop to add node".
    ConnectionDropped {
        /// The node the drag started from.
        node: NodeId,
        /// The handle the drag started from.
        handle: HandleId,
        /// Where it was released, in flow coordinates.
        pos: Pos2,
    },
    /// Delete key removed these (already removed from the state; edges
    /// orphaned by node removal are included).
    Deleted {
        /// The deleted nodes.
        nodes: Vec<Node<N>>,
        /// The deleted edges, including those removed with their nodes.
        edges: Vec<Edge<E>>,
    },
    /// The selection changed.
    SelectionChanged {
        /// Selected nodes.
        nodes: Vec<NodeId>,
        /// Selected edges.
        edges: Vec<EdgeId>,
    },
    /// The viewport panned or zoomed.
    ViewportChanged(Viewport),
}

/// Result of [`Flow::show`](crate::Flow::show).
pub struct FlowResponse<N, E> {
    /// Response of the empty canvas, e.g. for `.context_menu(..)`.
    pub pane: Response,
    /// Per-node responses, e.g. for context menus or tooltips.
    pub nodes: Vec<(NodeId, Response)>,
    /// What happened this frame.
    pub events: Vec<FlowEvent<N, E>>,
}

impl<N, E> FlowResponse<N, E> {
    /// The response for node `id`, if it is on the canvas.
    pub fn node_response(&self, id: NodeId) -> Option<&Response> {
        self.nodes.iter().find(|(n, _)| *n == id).map(|(_, r)| r)
    }
}
