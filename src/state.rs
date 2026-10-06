//! The graph model owned by the application.

use std::collections::{HashMap, HashSet};

use egui::{Color32, Pos2, Rect, Vec2, vec2};

use crate::types::*;

/// Transient pointer interaction, kept between frames.
pub(crate) struct ConnectDrag {
    pub node: NodeId,
    pub handle: HandleId,
    /// Set when an existing edge's end is being dragged to a new handle; the
    /// `node`/`handle` are then the edge's fixed end.
    pub reconnecting: Option<EdgeId>,
}

#[derive(Default)]
pub(crate) struct Interaction {
    pub node_drag: Option<NodeDrag>,
    pub box_select: Option<Pos2>,
    pub connecting: Option<ConnectDrag>,
    pub resize: Option<ResizeDrag>,
    pub guides: Vec<crate::geometry::Guide>,
}

pub(crate) struct ResizeDrag {
    pub node: NodeId,
    pub start: Vec2,
    pub accum: Vec2,
}

pub(crate) struct NodeDrag {
    /// Every node that moves: the dragged nodes and everything inside dragged groups.
    pub origins: Vec<(NodeId, Pos2)>,
    /// The dragged nodes themselves (not their group members).
    pub roots: Vec<NodeId>,
    pub accum: Vec2,
}

/// Which way a pulse travels along its edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PulseDirection {
    /// From the edge's source to its target.
    #[default]
    Forward,
    /// From the edge's target back to its source.
    Reverse,
}

/// Shape of a pulse's head.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PulseShape {
    #[default]
    Circle,
    Square,
    Diamond,
    /// A triangle pointing along the direction of travel.
    Arrow,
}

/// Speed profile of a pulse along its edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PulseEasing {
    Linear,
    /// Slow start and end (smoothstep).
    #[default]
    EaseInOut,
}

impl PulseEasing {
    pub(crate) fn apply(self, t: f32) -> f32 {
        match self {
            PulseEasing::Linear => t,
            PulseEasing::EaseInOut => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// What [`FlowState::pulse_edge`] does when an edge is already at
/// [`FlowState::max_pulses_per_edge`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PulseOverflow {
    /// Reject the new pulse (`pulse_edge` returns `false`).
    #[default]
    Drop,
    /// Remove the oldest pulse on that edge to make room.
    ReplaceOldest,
}

/// Nodes and the edges between them, copied out of a [`FlowState`] by
/// [`copy_selected`](FlowState::copy_selected).
#[derive(Clone, Debug)]
pub struct Clipboard<N, E> {
    pub nodes: Vec<Node<N>>,
    pub edges: Vec<Edge<E>>,
}

/// Appearance of a [`FlowState::pulse_edge`] particle.
#[derive(Clone, Debug)]
pub struct PulseStyle {
    /// `None` uses the selection colour.
    pub color: Option<Color32>,
    /// Particle radius in flow units.
    pub radius: f32,
    /// Seconds to travel along the edge.
    pub duration: f32,
    /// Direction of travel.
    pub direction: PulseDirection,
    /// Seconds to wait before the pulse appears, so several legs of a route
    /// can be sequenced.
    pub delay: f32,
    /// Shown next to the pulse while the pointer hovers it.
    pub label: Option<String>,
    /// Head shape.
    pub shape: PulseShape,
    /// Speed profile.
    pub easing: PulseEasing,
    /// Number of fading copies drawn behind the head (0 for none).
    pub trail: u8,
    /// Your own identifier, returned in [`FlowEvent::PulseArrived`](crate::FlowEvent::PulseArrived).
    pub tag: u64,
}

impl Default for PulseStyle {
    fn default() -> Self {
        Self {
            color: None,
            radius: 4.0,
            duration: 0.8,
            direction: PulseDirection::Forward,
            delay: 0.0,
            label: None,
            shape: PulseShape::Circle,
            easing: PulseEasing::EaseInOut,
            trail: 3,
            tag: 0,
        }
    }
}

pub(crate) struct ActivePulse {
    pub edge: EdgeId,
    pub style: PulseStyle,
    /// Set on the first frame the pulse is drawn.
    pub start: Option<f64>,
}

pub(crate) struct ViewAnim {
    pub from: Viewport,
    pub to: Viewport,
    pub duration: f32,
    pub start: Option<f64>,
}

/// Default for [`FlowState::max_pulses_per_edge`].
const DEFAULT_MAX_PULSES_PER_EDGE: usize = 8;

/// Nodes, edges, viewport and selection. Mutate freely between frames; pass
/// to [`Flow::show`](crate::Flow::show) every frame.
pub struct FlowState<N, E> {
    pub nodes: Vec<Node<N>>,
    pub edges: Vec<Edge<E>>,
    pub viewport: Viewport,
    /// Most pulses (including delayed ones) allowed in flight on one edge, so a
    /// burst of traffic cannot grow the queue without bound.
    pub max_pulses_per_edge: usize,
    /// What to do with a pulse beyond `max_pulses_per_edge`.
    pub pulse_overflow: PulseOverflow,
    next_node: u64,
    next_edge: u64,
    pub(crate) fit_frames: u8,
    pub(crate) initialized: bool,
    /// True while a frame runs: node positions are then absolute (see `groups.rs`).
    pub(crate) flat: bool,
    pub(crate) layout_anim: Option<crate::layout::LayoutAnim>,
    pub(crate) interaction: Interaction,
    pub(crate) pulses: Vec<ActivePulse>,
    pub(crate) view_anim: Option<ViewAnim>,
    pub(crate) fit_anim: Option<f32>,
    pub(crate) known_nodes: HashSet<NodeId>,
    pub(crate) appear: HashMap<NodeId, f64>,
}

impl<N, E> Default for FlowState<N, E> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            viewport: Viewport::default(),
            max_pulses_per_edge: DEFAULT_MAX_PULSES_PER_EDGE,
            pulse_overflow: PulseOverflow::Drop,
            next_node: 1,
            next_edge: 1,
            fit_frames: 0,
            initialized: false,
            flat: false,
            layout_anim: None,
            interaction: Interaction::default(),
            pulses: Vec::new(),
            view_anim: None,
            fit_anim: None,
            known_nodes: HashSet::new(),
            appear: HashMap::new(),
        }
    }
}

impl<N, E> FlowState<N, E> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node with a fresh id.
    pub fn add_node(&mut self, position: Pos2, data: N) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        self.nodes.push(Node::new(id, position, data));
        id
    }

    /// Add a pre-built node, keeping its id. The id counter is advanced past it.
    /// Returns `false` (and drops the node) if the id is already taken.
    pub fn insert_node(&mut self, node: Node<N>) -> bool {
        if self.node(node.id).is_some() {
            return false;
        }
        self.next_node = self.next_node.max(node.id.0 + 1);
        self.nodes.push(node);
        true
    }

    pub fn node(&self, id: NodeId) -> Option<&Node<N>> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node<N>> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn edge(&self, id: EdgeId) -> Option<&Edge<E>> {
        self.edges.iter().find(|e| e.id == id)
    }

    pub fn edge_mut(&mut self, id: EdgeId) -> Option<&mut Edge<E>> {
        self.edges.iter_mut().find(|e| e.id == id)
    }

    /// Add an edge between two nodes' handles. Fails if either node is
    /// missing or an identical edge already exists. This does *not* check
    /// handle kinds; interactive connections are validated by the canvas.
    pub fn add_edge(&mut self, conn: Connection, data: E) -> Option<EdgeId> {
        if self.node(conn.source).is_none() || self.node(conn.target).is_none() {
            return None;
        }
        if self.edges.iter().any(|e| e.connection() == conn) {
            return None;
        }
        let id = EdgeId(self.next_edge);
        self.next_edge += 1;
        self.edges.push(Edge {
            id,
            source: conn.source,
            source_handle: conn.source_handle,
            target: conn.target,
            target_handle: conn.target_handle,
            data,
            kind: None,
            label: None,
            line_style: LineStyle::Solid,
            animated: false,
            arrow: false,
            arrow_style: ArrowStyle::Triangle,
            arrow_at_source: false,
            label_style: EdgeLabelStyle::default(),
            color: None,
            width: None,
            animation_speed: 20.0,
            selected: false,
            deletable: true,
        });
        Some(id)
    }

    /// Connect the default source handle of `from` to the default target
    /// handle of `to`.
    pub fn connect(&mut self, from: NodeId, to: NodeId, data: E) -> Option<EdgeId> {
        self.add_edge(
            Connection {
                source: from,
                source_handle: Handle::DEFAULT_SOURCE,
                target: to,
                target_handle: Handle::DEFAULT_TARGET,
            },
            data,
        )
    }

    /// Remove a node and every edge attached to it.
    pub fn remove_node(&mut self, id: NodeId) -> Option<(Node<N>, Vec<Edge<E>>)> {
        let idx = self.nodes.iter().position(|n| n.id == id)?;
        // Members of a removed group move up a level, staying where they are.
        let parent = self.nodes[idx].parent;
        for child in self.children(id) {
            self.reparent_keep_visual(child, parent);
        }
        let idx = self.nodes.iter().position(|n| n.id == id)?;
        let node = self.nodes.remove(idx);
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.edges)
            .into_iter()
            .partition(|e| e.source == id || e.target == id);
        self.edges = kept;
        Some((node, gone))
    }

    pub fn remove_edge(&mut self, id: EdgeId) -> Option<Edge<E>> {
        let idx = self.edges.iter().position(|e| e.id == id)?;
        Some(self.edges.remove(idx))
    }

    pub fn selected_nodes(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|n| n.selected)
            .map(|n| n.id)
            .collect()
    }

    pub fn selected_edges(&self) -> Vec<EdgeId> {
        self.edges
            .iter()
            .filter(|e| e.selected)
            .map(|e| e.id)
            .collect()
    }

    pub fn clear_selection(&mut self) {
        self.nodes.iter_mut().for_each(|n| n.selected = false);
        self.edges.iter_mut().for_each(|e| e.selected = false);
    }

    pub fn select_all(&mut self) {
        self.nodes.iter_mut().for_each(|n| n.selected = true);
        self.edges.iter_mut().for_each(|e| e.selected = true);
    }

    /// Remove every selected, deletable node and edge (plus edges orphaned by
    /// node removal). Returns what was removed.
    pub fn delete_selected(&mut self) -> (Vec<Node<N>>, Vec<Edge<E>>) {
        let mut node_ids: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected && n.deletable)
            .map(|n| n.id)
            .collect();
        // Deleting a group deletes what is inside it (ungroup it first to keep the members).
        for id in node_ids.clone() {
            for d in self.descendants(id) {
                if self.node(d).is_some_and(|n| n.deletable) && !node_ids.contains(&d) {
                    node_ids.push(d);
                }
            }
        }
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for id in node_ids {
            if let Some((n, es)) = self.remove_node(id) {
                nodes.push(n);
                edges.extend(es);
            }
        }
        let edge_ids: Vec<_> = self
            .edges
            .iter()
            .filter(|e| e.selected && e.deletable)
            .map(|e| e.id)
            .collect();
        for id in edge_ids {
            edges.extend(self.remove_edge(id));
        }
        (nodes, edges)
    }

    /// Smallest rect containing every node, if there are any.
    /// Smallest rect containing every visible node (members of collapsed groups
    /// are not visible), in flow space.
    pub fn bounds(&self) -> Option<Rect> {
        let hidden = self.hidden_nodes();
        self.nodes
            .iter()
            .filter(|n| !hidden.contains(&n.id))
            .filter_map(|n| self.abs_rect(n.id))
            .reduce(|a, b| a.union(b))
    }

    /// Re-frame the viewport around all nodes on the next frames.
    pub fn fit_view(&mut self) {
        // Two frames: the first lays nodes out and measures them, the second
        // fits using the measured sizes.
        self.fit_frames = 2;
    }

    /// Re-frame the viewport around all nodes with an eased transition
    /// (`seconds`). Unlike [`fit_view`](Self::fit_view) this assumes node
    /// sizes are already measured, so call it after the first frame.
    pub fn fit_view_animated(&mut self, seconds: f32) {
        self.fit_anim = Some(seconds);
    }

    /// Ease the viewport to `to` over `seconds`. Any pan or zoom by the user
    /// cancels the transition. Jumps instead when
    /// [`FlowOptions::animate`](crate::FlowOptions::animate) is off.
    pub fn animate_viewport(&mut self, to: Viewport, seconds: f32) {
        self.view_anim = Some(ViewAnim {
            from: self.viewport,
            to,
            duration: seconds,
            start: None,
        });
    }

    /// Where the viewport is heading: the target of a running transition,
    /// else the current viewport.
    pub fn target_viewport(&self) -> Viewport {
        self.view_anim
            .as_ref()
            .map(|a| a.to)
            .unwrap_or(self.viewport)
    }

    /// Send a particle along `edge`, by default from source to target (see
    /// [`PulseStyle::direction`] and [`PulseStyle::delay`]). Returns `false` if
    /// the edge doesn't exist, or it is at [`max_pulses_per_edge`](Self::max_pulses_per_edge)
    /// and [`pulse_overflow`](Self::pulse_overflow) is `Drop`.
    pub fn pulse_edge(&mut self, edge: EdgeId, style: PulseStyle) -> bool {
        if self.edge(edge).is_none() {
            return false;
        }
        let on_edge = self.pulses.iter().filter(|p| p.edge == edge).count();
        if on_edge >= self.max_pulses_per_edge.max(1) {
            if self.pulse_overflow == PulseOverflow::Drop {
                return false;
            }
            if let Some(i) = self.pulses.iter().position(|p| p.edge == edge) {
                self.pulses.remove(i);
            }
        }
        self.pulses.push(ActivePulse {
            edge,
            style,
            start: None,
        });
        true
    }

    /// Send a pulse along a multi-hop route starting at node `start`, one edge
    /// after another, each leg starting when the previous one arrives (after
    /// `style.delay`). The direction of each leg is worked out from the route,
    /// so a frame can go ECU → bus → gateway → bus without you choosing
    /// forward or reverse per edge.
    ///
    /// Returns `false`, queuing nothing, if the edges don't form a connected
    /// chain from `start` or any of them is missing.
    pub fn pulse_route(&mut self, start: NodeId, edges: &[EdgeId], style: PulseStyle) -> bool {
        let mut legs = Vec::with_capacity(edges.len());
        let mut at = start;
        for &id in edges {
            let Some(e) = self.edge(id) else {
                return false;
            };
            if e.source == at {
                legs.push((id, PulseDirection::Forward));
                at = e.target;
            } else if e.target == at {
                legs.push((id, PulseDirection::Reverse));
                at = e.source;
            } else {
                return false;
            }
        }
        let leg_time = style.duration.max(1e-3);
        for (i, (id, direction)) in legs.into_iter().enumerate() {
            let leg = PulseStyle {
                direction,
                delay: style.delay + leg_time * i as f32,
                ..style.clone()
            };
            self.pulse_edge(id, leg);
        }
        true
    }

    /// Like [`pulse_edge`](Self::pulse_edge) but travelling from target to source.
    pub fn pulse_edge_reverse(&mut self, edge: EdgeId, mut style: PulseStyle) -> bool {
        style.direction = PulseDirection::Reverse;
        self.pulse_edge(edge, style)
    }

    /// The viewport that frames `bounds` inside a canvas of `canvas_size`.
    pub fn viewport_for(
        bounds: Rect,
        canvas_size: Vec2,
        padding: f32,
        zoom_range: (f32, f32),
    ) -> Viewport {
        let w = (bounds.width() + 2.0 * padding).max(1.0);
        let h = (bounds.height() + 2.0 * padding).max(1.0);
        let zoom = (canvas_size.x / w)
            .min(canvas_size.y / h)
            .clamp(zoom_range.0, zoom_range.1);
        let center = bounds.center();
        Viewport {
            zoom,
            pan: canvas_size / 2.0 - vec2(center.x, center.y) * zoom,
        }
    }

    pub(crate) fn selection_snapshot(&self) -> (Vec<NodeId>, Vec<EdgeId>) {
        (self.selected_nodes(), self.selected_edges())
    }
}

impl<N: Clone, E: Clone> FlowState<N, E> {
    /// Copy the selected nodes and every edge between two of them. `None` if
    /// no node is selected.
    pub fn copy_selected(&self) -> Option<Clipboard<N, E>> {
        let hidden = self.hidden_nodes();
        let mut ids: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected && !hidden.contains(&n.id))
            .map(|n| n.id)
            .collect();
        if ids.is_empty() {
            return None;
        }
        // A copied group brings everything inside it.
        for id in ids.clone() {
            for d in self.descendants(id) {
                if !ids.contains(&d) {
                    ids.push(d);
                }
            }
        }
        let set: HashSet<NodeId> = ids.iter().copied().collect();
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| set.contains(&n.id))
            .map(|n| {
                let mut copy = n.clone();
                // A member whose group is not copied becomes a top-level node, where it is.
                if !copy.parent.is_some_and(|p| set.contains(&p)) {
                    copy.position = self.abs_position(n.id).unwrap_or(n.position);
                    copy.parent = None;
                }
                copy
            })
            .collect();
        let edges = self
            .edges
            .iter()
            .filter(|e| set.contains(&e.source) && set.contains(&e.target))
            .cloned()
            .collect();
        Some(Clipboard { nodes, edges })
    }

    /// Insert a copy of `clipboard`, with fresh ids and the edges and group
    /// membership re-pointed at the new nodes. Top-level pasted nodes are moved by
    /// `offset` and become the selection (members of pasted groups move with their
    /// group). Returns every new node's id.
    pub fn paste(&mut self, clipboard: &Clipboard<N, E>, offset: Vec2) -> Vec<NodeId> {
        self.clear_selection();
        let mut map = HashMap::new();
        for n in &clipboard.nodes {
            map.insert(n.id, NodeId(self.next_node));
            self.next_node += 1;
        }
        let mut new_ids = Vec::new();
        for n in &clipboard.nodes {
            let mut node = n.clone();
            node.id = map[&n.id];
            node.parent = n.parent.and_then(|p| map.get(&p).copied());
            if node.parent.is_none() {
                node.position += offset;
                node.selected = true;
            } else {
                node.selected = false;
            }
            new_ids.push(node.id);
            self.nodes.push(node);
        }
        for e in &clipboard.edges {
            let (Some(&source), Some(&target)) = (map.get(&e.source), map.get(&e.target)) else {
                continue;
            };
            let mut edge = e.clone();
            edge.id = EdgeId(self.next_edge);
            self.next_edge += 1;
            edge.source = source;
            edge.target = target;
            edge.selected = false;
            self.edges.push(edge);
        }
        new_ids
    }

    /// Copy the selection and paste it straight away, shifted by `offset`.
    pub fn duplicate_selected(&mut self, offset: Vec2) -> Vec<NodeId> {
        match self.copy_selected() {
            Some(cb) => self.paste(&cb, offset),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn two_nodes() -> (FlowState<(), ()>, NodeId, NodeId) {
        let mut s = FlowState::new();
        let a = s.add_node(pos2(0.0, 0.0), ());
        let b = s.add_node(pos2(200.0, 0.0), ());
        (s, a, b)
    }

    #[test]
    fn connect_rejects_duplicates_and_missing() {
        let (mut s, a, b) = two_nodes();
        assert!(s.connect(a, b, ()).is_some());
        assert!(s.connect(a, b, ()).is_none());
        assert!(s.connect(a, NodeId(99), ()).is_none());
    }

    #[test]
    fn removing_node_removes_its_edges() {
        let (mut s, a, b) = two_nodes();
        s.connect(a, b, ());
        let (_, gone) = s.remove_node(a).unwrap();
        assert_eq!(gone.len(), 1);
        assert!(s.edges.is_empty());
    }

    #[test]
    fn delete_selected_respects_deletable() {
        let (mut s, a, b) = two_nodes();
        s.connect(a, b, ());
        s.select_all();
        s.node_mut(b).unwrap().deletable = false;
        let (nodes, edges) = s.delete_selected();
        assert_eq!(nodes.len(), 1);
        assert_eq!(edges.len(), 1);
        assert_eq!(s.nodes.len(), 1);
    }

    #[test]
    fn insert_node_advances_id_counter() {
        let mut s: FlowState<(), ()> = FlowState::new();
        assert!(s.insert_node(Node::new(NodeId(10), pos2(0.0, 0.0), ())));
        assert!(!s.insert_node(Node::new(NodeId(10), pos2(0.0, 0.0), ())));
        assert_eq!(s.add_node(pos2(0.0, 0.0), ()), NodeId(11));
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut vp = Viewport {
            pan: vec2(30.0, 10.0),
            zoom: 1.0,
        };
        let anchor = pos2(100.0, 80.0);
        let before = vp.to_flow(anchor);
        vp.zoom_at(anchor, 2.0, 0.1, 4.0);
        let after = vp.to_flow(anchor);
        assert!((before - after).length() < 1e-3);
        assert_eq!(vp.zoom, 2.0);
    }

    #[test]
    fn pulses_are_capped_per_edge_and_need_a_real_edge() {
        let (mut s, a, b) = two_nodes();
        let e = s.connect(a, b, ()).unwrap();
        assert!(!s.pulse_edge(EdgeId(99), PulseStyle::default()));
        for _ in 0..DEFAULT_MAX_PULSES_PER_EDGE {
            assert!(s.pulse_edge(e, PulseStyle::default()));
        }
        assert!(!s.pulse_edge(e, PulseStyle::default()));
    }

    #[test]
    fn pulse_limit_is_configurable_and_can_replace_oldest() {
        let (mut s, a, b) = two_nodes();
        let e = s.connect(a, b, ()).unwrap();
        s.max_pulses_per_edge = 2;
        s.pulse_overflow = PulseOverflow::ReplaceOldest;
        for i in 0..3 {
            let style = PulseStyle {
                label: Some(i.to_string()),
                ..Default::default()
            };
            assert!(s.pulse_edge(e, style));
        }
        assert_eq!(s.pulses.len(), 2);
        assert_eq!(s.pulses[0].style.label.as_deref(), Some("1"));
        assert!(s.pulse_edge_reverse(e, PulseStyle::default()));
        assert_eq!(s.pulses[1].style.direction, PulseDirection::Reverse);
    }

    #[test]
    fn pulse_route_picks_directions_and_chains_delays() {
        let mut s: FlowState<(), ()> = FlowState::new();
        let ecu = s.add_node(pos2(0.0, 0.0), ());
        let bus = s.add_node(pos2(100.0, 0.0), ());
        let gw = s.add_node(pos2(200.0, 0.0), ());
        let up = s.connect(ecu, bus, ()).unwrap(); // ecu -> bus
        let down = s.connect(gw, bus, ()).unwrap(); // gw -> bus (against the route)
        let style = PulseStyle {
            duration: 0.5,
            delay: 0.1,
            ..Default::default()
        };
        assert!(s.pulse_route(ecu, &[up, down], style.clone()));
        assert_eq!(s.pulses.len(), 2);
        assert_eq!(s.pulses[0].style.direction, PulseDirection::Forward);
        assert_eq!(s.pulses[1].style.direction, PulseDirection::Reverse);
        assert!((s.pulses[1].style.delay - 0.6).abs() < 1e-6);

        // A broken chain queues nothing.
        let before = s.pulses.len();
        assert!(!s.pulse_route(gw, &[up], style));
        assert_eq!(s.pulses.len(), before);
    }

    #[test]
    fn copy_paste_remaps_ids_and_edges() {
        let (mut s, a, b) = two_nodes();
        let c = s.add_node(pos2(0.0, 100.0), ());
        s.connect(a, b, ());
        s.connect(b, c, ());
        s.node_mut(a).unwrap().selected = true;
        s.node_mut(b).unwrap().selected = true;
        let cb = s.copy_selected().unwrap();
        assert_eq!(
            (cb.nodes.len(), cb.edges.len()),
            (2, 1),
            "only the inner edge"
        );

        let new = s.paste(&cb, vec2(10.0, 20.0));
        assert_eq!(new.len(), 2);
        assert_eq!(s.nodes.len(), 5);
        assert_eq!(s.edges.len(), 3);
        assert_eq!(s.selected_nodes(), new, "pasted nodes become the selection");
        let pasted = s.edges.last().unwrap();
        assert!(new.contains(&pasted.source) && new.contains(&pasted.target));
        assert_eq!(s.node(new[0]).unwrap().position, pos2(10.0, 20.0));
        assert!(s.edges.iter().map(|e| e.id).collect::<HashSet<_>>().len() == 3);
    }

    #[test]
    fn copy_needs_a_selection() {
        let (s, _, _) = two_nodes();
        assert!(s.copy_selected().is_none());
    }

    #[test]
    fn target_viewport_follows_running_transition() {
        let mut s: FlowState<(), ()> = FlowState::new();
        let to = Viewport {
            pan: vec2(5.0, 5.0),
            zoom: 2.0,
        };
        s.animate_viewport(to, 0.3);
        assert_eq!(s.target_viewport(), to);
    }

    #[test]
    fn fit_viewport_centres_bounds() {
        let b = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 100.0));
        let vp = FlowState::<(), ()>::viewport_for(b, vec2(800.0, 600.0), 0.0, (0.1, 4.0));
        let c = vp.to_screen(b.center());
        assert!((c - pos2(400.0, 300.0)).length() < 1e-3);
    }
}
