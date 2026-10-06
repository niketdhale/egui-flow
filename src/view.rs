//! The canvas widget.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use egui::emath::TSTransform;
use egui::{
    Align, Align2, Area, Color32, FontId, Frame, Id, Key, LayerId, Layout, Order, Painter,
    PointerButton, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
    pos2, vec2,
};

use crate::events::{FlowEvent, FlowResponse};
use crate::geometry::{
    align_to, dist_to_path, edge_path, end_direction, point_at, start_direction,
};
use crate::icons::Icon;
use crate::options::{Background, FlowOptions, HandleVisibility};
use crate::state::{ConnectDrag, FlowState, NodeDrag, PulseDirection, PulseShape, ResizeDrag};
use crate::types::*;
use crate::viewer::FlowViewer;

/// Nodes may grow up to this wide; use `ui.set_max_width` in `node_ui` to
/// constrain further.
const MAX_NODE_WIDTH: f32 = 400.0;
const HANDLE_RADIUS: f32 = 5.0;
const GROUP_TOGGLE: f32 = 20.0;
const EDGE_HIT_PX: f32 = 8.0;
const NODE_FADE_SECS: f64 = 0.25;
const HOVER_EASE_SECS: f32 = 0.12;

/// A node-graph canvas. Build it each frame and call [`Flow::show`].
pub struct Flow {
    id: Id,
    opts: FlowOptions,
}

type Handles = HashMap<NodeId, Vec<Handle>>;

impl Flow {
    pub fn new(id_salt: impl Hash) -> Self {
        Self {
            id: Id::new(id_salt),
            opts: FlowOptions::default(),
        }
    }

    pub fn options(mut self, opts: FlowOptions) -> Self {
        self.opts = opts;
        self
    }

    pub fn background(mut self, background: Background) -> Self {
        self.opts.background = background;
        self
    }

    pub fn minimap(mut self, on: bool) -> Self {
        self.opts.minimap = on;
        self
    }

    pub fn controls(mut self, on: bool) -> Self {
        self.opts.controls = on;
        self
    }

    pub fn snap_to_grid(mut self, grid: f32) -> Self {
        self.opts.snap_to_grid = Some(grid);
        self
    }

    /// Dim everything not connected to the selected or hovered node.
    pub fn highlight_connected(mut self, on: bool) -> Self {
        self.opts.highlight_connected = on;
        self
    }

    /// Colours for the canvas (see [`FlowTheme`](crate::FlowTheme)).
    pub fn theme(mut self, theme: crate::theme::FlowTheme) -> Self {
        self.opts.theme = theme;
        self
    }

    /// Re-lay out text at the zoomed size so it stays sharp (default on).
    pub fn crisp_text(mut self, on: bool) -> Self {
        self.opts.crisp_text = on;
        self
    }

    pub fn edge_kind(mut self, kind: EdgeKind) -> Self {
        self.opts.default_edge_kind = kind;
        self
    }

    /// Fill the available space with the canvas and run one frame.
    pub fn show<N, E: Default, V: FlowViewer<N, E>>(
        self,
        ui: &mut Ui,
        state: &mut FlowState<N, E>,
        viewer: &mut V,
    ) -> FlowResponse<N, E> {
        let o = self.opts;
        let id = ui.make_persistent_id(self.id);
        let (canvas, _) = ui.allocate_exact_size(ui.available_size_before_wrap(), Sense::hover());
        let mut events = Vec::new();
        let selection_before = state.selection_snapshot();
        let viewport_before = state.viewport;

        let now = ui.input(|i| i.time);
        let first_frame = !state.initialized;
        // Inside a frame every position is in flow space; `unflatten` restores
        // group-relative positions before returning.
        state.flatten();
        state.sort_by_depth();
        let hidden = state.hidden_nodes();

        // --- fit view ------------------------------------------------------
        if o.fit_view_on_init && first_frame {
            state.fit_frames = 2;
        }
        state.initialized = true;
        if state.fit_frames > 0 {
            if let Some(bounds) = state.bounds() {
                state.viewport = FlowState::<N, E>::viewport_for(
                    bounds,
                    canvas.size(),
                    o.fit_view_padding,
                    (o.min_zoom, o.max_zoom.min(1.5)),
                );
                ui.ctx().request_repaint();
            }
            state.fit_frames -= 1;
        }
        if let Some(seconds) = state.fit_anim.take()
            && let Some(bounds) = state.bounds()
        {
            let target = FlowState::<N, E>::viewport_for(
                bounds,
                canvas.size(),
                o.fit_view_padding,
                (o.min_zoom, o.max_zoom.min(1.5)),
            );
            state.animate_viewport(target, seconds);
        }

        // --- eased view transition -----------------------------------------
        if let Some(a) = state.view_anim.as_mut() {
            let start = *a.start.get_or_insert(now);
            let t = if o.animate {
                (((now - start) as f32) / a.duration.max(1e-3)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let e = 1.0 - (1.0 - t).powi(3);
            let (from, to) = (a.from, a.to);
            state.viewport = Viewport {
                pan: from.pan + (to.pan - from.pan) * e,
                zoom: from.zoom * (to.zoom / from.zoom).powf(e),
            };
            if t >= 1.0 {
                state.view_anim = None;
            } else {
                ui.ctx().request_repaint();
            }
        }

        // --- node fade-in bookkeeping ---------------------------------------
        state
            .known_nodes
            .retain(|id| state.nodes.iter().any(|n| n.id == *id));
        for n in &state.nodes {
            if state.known_nodes.insert(n.id) && !first_frame && o.animate {
                state.appear.insert(n.id, now);
            }
        }
        let mut alphas: HashMap<NodeId, f32> = HashMap::new();
        state.appear.retain(|id, start| {
            let t = ((now - *start) / NODE_FADE_SECS) as f32;
            if t >= 1.0 {
                return false;
            }
            alphas.insert(*id, t.clamp(0.0, 1.0));
            true
        });
        if !alphas.is_empty() {
            ui.ctx().request_repaint();
        }

        // --- wheel / pinch -------------------------------------------------
        let layer_id = LayerId::new(ui.layer_id().order, id.with("layer"));
        ui.ctx().set_sublayer(ui.layer_id(), layer_id);
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| canvas.contains(*p));
        let over_canvas = hover.is_some_and(|p| {
            ui.ctx()
                .layer_id_at(p)
                .is_none_or(|l| l == layer_id || l == ui.layer_id())
        });
        if let (true, Some(p)) = (over_canvas, hover) {
            let (scroll, zoom_delta) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            let mut factor = zoom_delta;
            if o.zoom_on_scroll {
                factor *= (scroll.y * 0.0015).exp();
            } else if scroll != Vec2::ZERO {
                state.viewport.pan += scroll;
                state.view_anim = None;
            }
            if factor != 1.0 {
                state.view_anim = None;
                let rel = (p - canvas.min).to_pos2();
                state.viewport.zoom_at(rel, factor, o.min_zoom, o.max_zoom);
            }
        }

        // --- scene layer ---------------------------------------------------
        let make_tf = |vp: &Viewport| TSTransform::new(canvas.min.to_vec2() + vp.pan, vp.zoom);
        let mut tf = make_tf(&state.viewport);
        ui.ctx().set_transform_layer(layer_id, tf);
        let mut cui = ui.new_child(
            UiBuilder::new()
                .layer_id(layer_id)
                .max_rect(tf.inverse() * canvas)
                .layout(Layout::top_down(Align::Min)),
        );
        cui.set_clip_rect(tf.inverse() * canvas);

        let pane = cui.interact(
            tf.inverse() * canvas,
            id.with("pane"),
            Sense::click_and_drag(),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        let pointer_scene = |tf: &TSTransform| {
            ui.input(|i| i.pointer.latest_pos())
                .map(|p| tf.inverse() * p)
        };

        if pane.drag_started_by(PointerButton::Primary) && shift && o.elements_selectable {
            // From where the button went down, not where the drag was noticed a few pixels on.
            state.interaction.box_select = ui
                .input(|i| i.pointer.press_origin())
                .map(|p| tf.inverse() * p)
                .or_else(|| pointer_scene(&tf));
        }
        if pane.dragged()
            && state.interaction.box_select.is_none()
            && (pane.dragged_by(PointerButton::Primary) || pane.dragged_by(PointerButton::Middle))
        {
            state.view_anim = None;
            state.viewport.pan += pane.drag_delta() * state.viewport.zoom;
            tf = make_tf(&state.viewport);
            ui.ctx().set_transform_layer(layer_id, tf);
            cui.set_clip_rect(tf.inverse() * canvas);
        }

        let visuals = ui.visuals().clone();
        let painter = ui.painter_at(canvas);
        let theme = o.theme;
        painter.rect_filled(
            canvas,
            0.0,
            theme.background.unwrap_or(visuals.extreme_bg_color),
        );
        paint_background(
            &painter,
            canvas,
            &state.viewport,
            &o,
            theme
                .grid
                .unwrap_or(visuals.widgets.noninteractive.bg_stroke.color),
        );
        let sel_color = theme.selection.unwrap_or(visuals.selection.stroke.color);
        theme.apply_to_node_visuals(&mut cui.style_mut().visuals);

        // --- pass 1: node interaction (before drawing, so drags don't lag) --
        let handles: Handles = state
            .nodes
            .iter()
            .map(|n| (n.id, viewer.handles(n)))
            .collect();

        let node_resps: Vec<(NodeId, Response)> = state
            .nodes
            .iter()
            .filter(|n| !hidden.contains(&n.id))
            .map(|n| {
                let mut grab = n.rect();
                if n.is_group && !n.collapsed {
                    // Only the header: the rest of an open group pans and box-selects.
                    grab.max.y = (grab.min.y + o.group_header_height).min(grab.max.y);
                }
                (
                    n.id,
                    cui.interact(grab, id.with(("node", n.id)), Sense::click_and_drag()),
                )
            })
            .collect();

        let mut bring_to_front = None;
        let mut dragged = Vec::new();
        let mut stopped = Vec::new();
        for (nid, r) in &node_resps {
            let Some(idx) = state.nodes.iter().position(|n| n.id == *nid) else {
                continue;
            };
            if r.clicked() {
                if o.elements_selectable {
                    select_node(state, *nid, shift);
                }
                events.push(FlowEvent::NodeClicked(*nid));
            }
            if r.drag_started_by(PointerButton::Primary) {
                if o.elements_selectable && !state.nodes[idx].selected {
                    select_node(state, *nid, shift);
                }
                if o.nodes_draggable && state.nodes[idx].draggable {
                    let roots: Vec<NodeId> = state
                        .nodes
                        .iter()
                        .filter(|n| n.selected && n.draggable && !hidden.contains(&n.id))
                        .map(|n| n.id)
                        .collect();
                    // A dragged group takes its members with it.
                    let mut moving = roots.clone();
                    for r in &roots {
                        for d in state.descendants(*r) {
                            if !moving.contains(&d) {
                                moving.push(d);
                            }
                        }
                    }
                    let origins = state
                        .nodes
                        .iter()
                        .filter(|n| moving.contains(&n.id))
                        .map(|n| (n.id, n.position))
                        .collect();
                    state.interaction.node_drag = Some(NodeDrag {
                        origins,
                        roots,
                        accum: Vec2::ZERO,
                    });
                }
                bring_to_front = Some(*nid);
            }
            if r.dragged_by(PointerButton::Primary)
                && let Some(d) = &mut state.interaction.node_drag
            {
                d.accum += r.drag_delta();
                let proposed: Vec<(NodeId, Pos2)> = d
                    .origins
                    .iter()
                    .map(|(oid, origin)| (*oid, snap(*origin + d.accum, o.snap_to_grid)))
                    .collect();
                let mut offset = Vec2::ZERO;
                let mut guides = Vec::new();
                if o.alignment_guides {
                    let moving: HashSet<NodeId> = proposed.iter().map(|(i, _)| *i).collect();
                    let group = proposed
                        .iter()
                        .filter_map(|(i, p)| {
                            state
                                .nodes
                                .iter()
                                .find(|n| n.id == *i)
                                .map(|n| Rect::from_min_size(*p, n.size))
                        })
                        .reduce(|a, b| a.union(b));
                    let others: Vec<Rect> = state
                        .nodes
                        .iter()
                        .filter(|n| !moving.contains(&n.id) && !hidden.contains(&n.id))
                        .map(|n| n.rect())
                        .collect();
                    if let Some(group) = group {
                        let threshold = o.guide_threshold / state.viewport.zoom.max(1e-3);
                        (offset, guides) = align_to(group, &others, threshold);
                    }
                }
                state.interaction.guides = guides;
                // Members that must stay in their group stop at its edges.
                let moving_ids: HashSet<NodeId> = proposed.iter().map(|(i, _)| *i).collect();
                let placed: Vec<(NodeId, Pos2)> = proposed
                    .into_iter()
                    .map(|(oid, pos)| {
                        let p = state.clamp_in_group(
                            oid,
                            pos + offset,
                            o.group_header_height,
                            &moving_ids,
                        );
                        (oid, p)
                    })
                    .collect();
                for (oid, pos) in placed {
                    if let Some(n) = state.nodes.iter_mut().find(|n| n.id == oid) {
                        n.position = pos;
                        dragged.push(oid);
                    }
                }
            }
            if r.drag_stopped()
                && let Some(d) = state.interaction.node_drag.take()
            {
                state.interaction.guides.clear();
                stopped.extend(d.origins.iter().map(|(id, _)| *id));
                if o.group_drop {
                    // Dropping on a group puts the node in it; dropping outside takes it out.
                    let moving: HashSet<NodeId> = d.origins.iter().map(|(i, _)| *i).collect();
                    let tops: Vec<NodeId> = d
                        .roots
                        .iter()
                        .copied()
                        .filter(|r| !state.ancestors(*r).iter().any(|a| d.roots.contains(a)))
                        .collect();
                    for root in tops {
                        let Some(centre) = state.node(root).map(|n| n.rect().center()) else {
                            continue;
                        };
                        let target = state
                            .nodes
                            .iter()
                            .filter(|g| {
                                g.is_group
                                    && !g.collapsed
                                    && !moving.contains(&g.id)
                                    && !hidden.contains(&g.id)
                                    && g.rect().contains(centre)
                            })
                            .max_by_key(|g| state.depth(g.id))
                            .map(|g| g.id);
                        let current = state.node(root).and_then(|n| n.parent);
                        if target != current {
                            // Positions are in flow space here, so nothing moves on screen.
                            if let Some(n) = state.node_mut(root) {
                                n.parent = target;
                            }
                            events.push(FlowEvent::ParentChanged {
                                node: root,
                                parent: target,
                            });
                        }
                    }
                }
            }
        }
        if !dragged.is_empty() {
            events.push(FlowEvent::NodesDragged(dragged));
        }
        if !stopped.is_empty() {
            events.push(FlowEvent::NodesDragStopped(stopped));
        }
        if let Some(front) = bring_to_front
            && let Some(i) = state.nodes.iter().position(|n| n.id == front)
        {
            let n = state.nodes.remove(i);
            state.nodes.push(n);
            // Groups stay below their members.
            state.sort_by_depth();
        }

        // --- highlight: dim whatever isn't connected to the focus ----------
        let focus: Option<HashSet<NodeId>> = if o.highlight_connected {
            let mut seeds: HashSet<NodeId> = state
                .nodes
                .iter()
                .filter(|n| n.selected)
                .map(|n| n.id)
                .collect();
            seeds.extend(
                node_resps
                    .iter()
                    .filter(|(_, r)| r.hovered())
                    .map(|(id, _)| *id),
            );
            for e in state.edges.iter().filter(|e| e.selected) {
                seeds.insert(e.source);
                seeds.insert(e.target);
            }
            (!seeds.is_empty()).then_some(seeds)
        } else {
            None
        };
        let edge_dim: Vec<bool> = state
            .edges
            .iter()
            .map(|e| {
                focus
                    .as_ref()
                    .is_some_and(|f| !f.contains(&e.source) && !f.contains(&e.target))
            })
            .collect();
        let related: HashSet<NodeId> = focus
            .as_ref()
            .map(|f| {
                let mut r = f.clone();
                for e in &state.edges {
                    if f.contains(&e.source) || f.contains(&e.target) {
                        r.insert(e.source);
                        r.insert(e.target);
                    }
                }
                r
            })
            .unwrap_or_default();
        let node_dim = |id: NodeId| focus.is_some() && !related.contains(&id);
        const DIM: f32 = 0.25;

        // --- edges: routing, hover, click ---------------------------------
        let rects: HashMap<NodeId, Rect> = state.nodes.iter().map(|n| (n.id, n.rect())).collect();
        let geoms: Vec<(usize, Vec<Pos2>, Side)> = state
            .edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                // An end inside a collapsed group attaches to that group's side instead.
                let anchor = |node: NodeId, handle: HandleId, is_source: bool| {
                    if hidden.contains(&node) {
                        let r = rects.get(&state.collapse_proxy(node))?;
                        return Some(if is_source {
                            (r.right_center(), Side::Right)
                        } else {
                            (r.left_center(), Side::Left)
                        });
                    }
                    let h = find_handle(&handles, node, handle)?;
                    Some((h.position(*rects.get(&node)?), h.side))
                };
                if (hidden.contains(&e.source) || hidden.contains(&e.target))
                    && state.collapse_proxy(e.source) == state.collapse_proxy(e.target)
                {
                    return None; // wholly inside one collapsed group
                }
                let (sp, ss) = anchor(e.source, e.source_handle, true)?;
                let (tp, ts) = anchor(e.target, e.target_handle, false)?;
                let path = edge_path(e.kind.unwrap_or(o.default_edge_kind), sp, ss, tp, ts);
                Some((i, path, ts))
            })
            .collect();

        let pointer_flow = pointer_scene(&tf);
        let hovered_edge = if pane.hovered() && o.elements_selectable {
            pointer_flow.and_then(|p| {
                geoms
                    .iter()
                    .map(|(i, path, _)| (*i, dist_to_path(p, path)))
                    .filter(|(_, d)| *d * state.viewport.zoom <= EDGE_HIT_PX)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            })
        } else {
            None
        };
        if pane.clicked() {
            if let Some(i) = hovered_edge {
                let eid = state.edges[i].id;
                if !shift {
                    state.clear_selection();
                }
                state.edges[i].selected = !shift || !state.edges[i].selected;
                events.push(FlowEvent::EdgeClicked(eid));
            } else {
                if !shift && o.elements_selectable {
                    state.clear_selection();
                }
                if let Some(p) = pointer_flow {
                    events.push(FlowEvent::PaneClicked(p));
                }
            }
        }

        // --- pass 2a: draw edges -------------------------------------------
        let cp = cui.painter().clone();
        let mut order: Vec<_> = geoms.iter().collect();
        order.sort_by_key(|(i, _, _)| state.edges[*i].selected);
        let reconnecting_edge = state
            .interaction
            .connecting
            .as_ref()
            .and_then(|c| c.reconnecting);
        for (i, path, _) in order {
            let e = &state.edges[*i];
            if reconnecting_edge == Some(e.id) {
                continue;
            }
            let hovered = hovered_edge == Some(*i);
            let default_color = theme.edge.unwrap_or_else(|| {
                visuals
                    .widgets
                    .noninteractive
                    .fg_stroke
                    .color
                    .gamma_multiply(0.7)
            });
            let color = if e.selected {
                sel_color
            } else {
                e.color.unwrap_or(default_color)
            };
            let color = if edge_dim[*i] {
                color.gamma_multiply(DIM)
            } else {
                color
            };
            let base_width = e.width.unwrap_or(1.5);
            let target_width = if e.selected || hovered {
                base_width + 1.0
            } else {
                base_width
            };
            let width = if o.animate {
                ui.ctx().animate_value_with_time(
                    id.with(("edge_width", e.id)),
                    target_width,
                    HOVER_EASE_SECS,
                )
            } else {
                target_width
            };
            let stroke = Stroke::new(width, color);
            let pattern = match (e.line_style.pattern(width), e.animated) {
                (None, true) => LineStyle::Dashed.pattern(width),
                (p, _) => p,
            };
            if let Some((dash, gap)) = pattern {
                let offset = if e.animated {
                    // Wrapping keeps f32 precise on long runs.
                    ui.ctx().request_repaint();
                    let period = (dash + gap) as f64;
                    (now * e.animation_speed as f64).rem_euclid(period) as f32
                } else {
                    0.0
                };
                cp.extend(Shape::dashed_line_with_offset(
                    path,
                    stroke,
                    &[dash],
                    &[gap],
                    -offset,
                ));
            } else {
                cp.add(Shape::line(path.clone(), stroke));
            }
            if e.arrow {
                draw_arrow(
                    &cp,
                    *path.last().unwrap(),
                    end_direction(path),
                    color,
                    e.arrow_style,
                );
            }
            if e.arrow_at_source {
                draw_arrow(&cp, path[0], -start_direction(path), color, e.arrow_style);
            }
            if let Some(label) = e.label.as_ref().filter(|_| !edge_dim[*i]) {
                let ls = e.label_style;
                let text_color = ls.color.unwrap_or(visuals.text_color());
                let galley =
                    cp.layout_no_wrap(label.clone(), FontId::proportional(ls.size), text_color);
                let at = point_at(path, ls.position);
                cp.rect_filled(
                    Rect::from_center_size(at, galley.size() + vec2(8.0, 4.0)),
                    3.0,
                    ls.background
                        .unwrap_or(theme.label_background.unwrap_or(visuals.window_fill)),
                );
                cp.galley(at - galley.size() / 2.0, galley, text_color);
            }
        }

        // --- pulses travelling along edges ----------------------------------
        let paths: HashMap<EdgeId, &Vec<Pos2>> = geoms
            .iter()
            .map(|(i, path, _)| (state.edges[*i].id, path))
            .collect();
        let mut hovered_pulse: Option<(Pos2, String)> = None;
        state.pulses.retain_mut(|p| {
            let start = *p.start.get_or_insert(now) + p.style.delay.max(0.0) as f64;
            let Some(path) = paths.get(&p.edge) else {
                return false;
            };
            let t = ((now - start) / p.style.duration.max(1e-3) as f64) as f32;
            if t >= 1.0 {
                events.push(FlowEvent::PulseArrived {
                    edge: p.edge,
                    tag: p.style.tag,
                    direction: p.style.direction,
                });
                return false;
            }
            if t < 0.0 {
                return true; // still waiting out its delay
            }
            let eased = p.style.easing.apply(t);
            let (eased, sign) = match p.style.direction {
                PulseDirection::Forward => (eased, 1.0),
                PulseDirection::Reverse => (1.0 - eased, -1.0),
            };
            let color = p.style.color.unwrap_or(sel_color);
            let r = p.style.radius;
            // Fading trail behind the head, then the head itself.
            for k in (1..=p.style.trail as usize).rev() {
                let back = eased - sign * 0.03 * k as f32;
                if !(0.0..=1.0).contains(&back) {
                    continue;
                }
                cp.circle_filled(
                    point_at(path, back),
                    r * (1.0 - 0.18 * k as f32).max(0.3),
                    color.gamma_multiply((1.0 - 0.25 * k as f32).max(0.1)),
                );
            }
            let head = point_at(path, eased);
            let ahead = point_at(path, (eased + sign * 0.01).clamp(0.0, 1.0));
            let dir = (ahead - head).normalized();
            let dir = if dir.is_finite() { dir } else { Vec2::X };
            draw_pulse_head(&cp, head, dir, r, color, p.style.shape);
            if let Some(label) = &p.style.label {
                let reach = (r + 4.0).max(8.0 / state.viewport.zoom.max(1e-3));
                if pane.hovered()
                    && hovered_pulse.is_none()
                    && let Some(pf) = pointer_scene(&tf)
                    && (pf - head).length() <= reach
                {
                    hovered_pulse = Some((head, label.clone()));
                }
            }
            true
        });
        if let Some((at, label)) = hovered_pulse {
            let galley = cp.layout_no_wrap(label, FontId::proportional(12.0), visuals.text_color());
            let center = at + vec2(0.0, -(galley.size().y + 14.0));
            cp.rect_filled(
                Rect::from_center_size(center, galley.size() + vec2(8.0, 4.0)),
                3.0,
                theme.label_background.unwrap_or(visuals.window_fill),
            );
            cp.galley(center - galley.size() / 2.0, galley, visuals.text_color());
        }
        if !state.pulses.is_empty() {
            ui.ctx().request_repaint();
        }

        // --- pass 2b: node contents ----------------------------------------
        let mut toggles: Vec<(NodeId, bool)> = Vec::new();
        for i in 0..state.nodes.len() {
            if hidden.contains(&state.nodes[i].id) {
                continue;
            }
            let node = &mut state.nodes[i];
            let frame = viewer.node_frame(&cui, node);
            // A collapsed group shrinks to its header.
            let fixed = if node.collapsed {
                None
            } else {
                node.fixed_size
            };
            let content_w = fixed.map_or(MAX_NODE_WIDTH, |f| f.x.max(MAX_NODE_WIDTH));
            let content = Rect::from_min_size(node.position, vec2(content_w, 10_000.0));
            let margin = frame.total_margin().sum();
            let inner = cui.scope_builder(
                UiBuilder::new()
                    .id_salt(("flow_node", node.id))
                    .max_rect(content),
                |ui| {
                    // Selectable labels would swallow the drag that should move the node.
                    ui.style_mut().interaction.selectable_labels = false;
                    let a = alphas.get(&node.id).copied().unwrap_or(1.0)
                        * if node_dim(node.id) { DIM } else { 1.0 };
                    if a < 1.0 {
                        ui.set_opacity(a);
                    }
                    frame.show(ui, |ui| {
                        if let Some(f) = fixed {
                            let inner = (f - margin).max(Vec2::ZERO);
                            ui.set_width(inner.x);
                            ui.set_min_height(inner.y);
                        }
                        viewer.node_ui(ui, node)
                    })
                },
            );
            let rect = inner.inner.response.rect;
            node.size = rect.size();
            if node.is_group {
                // Collapse / expand toggle in the header's top-right corner.
                let toggle = Rect::from_min_size(
                    pos2(rect.max.x - GROUP_TOGGLE - 6.0, rect.min.y + 5.0),
                    Vec2::splat(GROUP_TOGGLE),
                );
                let tr = cui.interact(toggle, id.with(("group_toggle", node.id)), Sense::click());
                let alpha = alphas.get(&node.id).copied().unwrap_or(1.0);
                if tr.hovered() {
                    cp.rect_filled(
                        toggle,
                        4.0,
                        visuals.widgets.hovered.bg_fill.gamma_multiply(alpha),
                    );
                }
                let (icon, tint) = if node.collapsed {
                    (Icon::ChevronRight, visuals.text_color())
                } else {
                    (Icon::ChevronDown, visuals.weak_text_color())
                };
                icon.paint(&cp, toggle.shrink(4.0), tint.gamma_multiply(alpha));
                if tr.clicked() {
                    toggles.push((node.id, node.collapsed));
                }
            }
            if node.selected {
                cp.rect_stroke(
                    rect,
                    6.0,
                    Stroke::new(
                        2.0_f32,
                        sel_color.gamma_multiply(alphas.get(&node.id).copied().unwrap_or(1.0)),
                    ),
                    StrokeKind::Outside,
                );
            }
        }

        for (group, was_collapsed) in toggles {
            state.set_collapsed(group, !was_collapsed);
            events.push(FlowEvent::GroupToggled {
                node: group,
                collapsed: !was_collapsed,
            });
        }

        // --- resize grips on selected nodes (below handles, which win overlaps)
        if o.nodes_resizable {
            let mut resized: Vec<(NodeId, Vec2, bool)> = Vec::new();
            for n in state
                .nodes
                .iter_mut()
                .filter(|n| n.selected && n.resizable && !n.collapsed && !hidden.contains(&n.id))
            {
                let r = n.rect();
                let c = 14.0;
                let zones = [
                    (
                        0u8,
                        Rect::from_center_size(r.max, Vec2::splat(c)),
                        vec2(1.0, 1.0),
                    ),
                    (
                        1,
                        Rect::from_min_max(
                            pos2(r.max.x - 4.0, r.min.y + c),
                            pos2(r.max.x + 4.0, r.max.y - c),
                        ),
                        vec2(1.0, 0.0),
                    ),
                    (
                        2,
                        Rect::from_min_max(
                            pos2(r.min.x + c, r.max.y - 4.0),
                            pos2(r.max.x - c, r.max.y + 4.0),
                        ),
                        vec2(0.0, 1.0),
                    ),
                ];
                for (k, zone, axis) in zones {
                    let zr = cui.interact(zone, id.with(("resize", n.id, k)), Sense::drag());
                    if zr.hovered() || zr.dragged() {
                        ui.ctx().set_cursor_icon(match k {
                            0 => egui::CursorIcon::ResizeNwSe,
                            1 => egui::CursorIcon::ResizeHorizontal,
                            _ => egui::CursorIcon::ResizeVertical,
                        });
                    }
                    if zr.drag_started() {
                        state.interaction.resize = Some(ResizeDrag {
                            node: n.id,
                            start: n.size,
                            accum: Vec2::ZERO,
                        });
                    }
                    let Some(drag) = state.interaction.resize.as_mut().filter(|d| d.node == n.id)
                    else {
                        continue;
                    };
                    if zr.dragged() {
                        drag.accum += zr.drag_delta() * axis;
                        let mut size = drag.start + drag.accum;
                        if let Some(g) = o.snap_to_grid.filter(|g| *g > 0.0) {
                            size = vec2((size.x / g).round() * g, (size.y / g).round() * g);
                        }
                        size = size.max(n.min_size);
                        if let Some(max) = n.max_size {
                            size = size.min(max);
                        }
                        if n.fixed_size != Some(size) {
                            n.fixed_size = Some(size);
                            resized.push((n.id, size, false));
                        }
                    }
                    if zr.drag_stopped() {
                        state.interaction.resize = None;
                        resized.push((n.id, n.fixed_size.unwrap_or(n.size), true));
                    }
                }
                // Corner grip marker.
                let g = cui.painter();
                for d in [4.0_f32, 8.0] {
                    g.line_segment(
                        [
                            pos2(r.max.x - d, r.max.y - 1.5),
                            pos2(r.max.x - 1.5, r.max.y - d),
                        ],
                        Stroke::new(1.5_f32, sel_color),
                    );
                }
            }
            for (node, size, finished) in resized {
                events.push(FlowEvent::NodeResized {
                    node,
                    size,
                    finished,
                });
            }
        }

        // --- pass 2c: handles + connection drag -----------------------------
        let rects: HashMap<NodeId, Rect> = state.nodes.iter().map(|n| (n.id, n.rect())).collect();
        let connecting = state
            .interaction
            .connecting
            .as_ref()
            .map(|c| (c.node, c.handle));
        let mut all_handles: Vec<(NodeId, Handle, Pos2)> = Vec::new();
        for n in state.nodes.iter().filter(|n| !hidden.contains(&n.id)) {
            let connectable = o.nodes_connectable
                && n.connectable
                && o.handle_visibility != HandleVisibility::Hidden;
            let show_handles = match o.handle_visibility {
                HandleVisibility::Always => true,
                HandleVisibility::Hidden => false,
                HandleVisibility::OnHover => {
                    let near = pointer_flow
                        .is_some_and(|p| rects[&n.id].expand(HANDLE_RADIUS + 10.0).contains(p));
                    near || n.selected
                        || connecting.is_some()
                        || state.interaction.node_drag.is_some()
                }
            };
            let visibility = if o.animate {
                ui.ctx().animate_value_with_time(
                    id.with(("handle_visibility", n.id)),
                    f32::from(show_handles),
                    HOVER_EASE_SECS,
                )
            } else {
                f32::from(show_handles)
            };
            for h in handles.get(&n.id).into_iter().flatten() {
                let pos = h.position(rects[&n.id]);
                all_handles.push((n.id, *h, pos));
                let hit = Rect::from_center_size(pos, Vec2::splat(2.0 * (HANDLE_RADIUS + 5.0)));
                let hr = cui.interact(
                    hit,
                    id.with(("handle", n.id, h.id)),
                    if connectable {
                        Sense::drag()
                    } else {
                        Sense::hover()
                    },
                );
                if connectable && hr.drag_started() {
                    state.interaction.connecting = Some(ConnectDrag {
                        node: n.id,
                        handle: h.id,
                        reconnecting: None,
                    });
                }
                let valid_target = connecting.is_some_and(|from| {
                    connection_between(state, &handles, &*viewer, &o, from, (n.id, h.id)).is_some()
                });
                let active = hr.hovered() || connecting == Some((n.id, h.id));
                let target_r = if active || valid_target {
                    HANDLE_RADIUS + 1.5
                } else {
                    HANDLE_RADIUS
                };
                let r = if o.animate {
                    ui.ctx().animate_value_with_time(
                        id.with(("handle_radius", n.id, h.id)),
                        target_r,
                        HOVER_EASE_SECS,
                    )
                } else {
                    target_r
                };
                let alpha = alphas.get(&n.id).copied().unwrap_or(1.0)
                    * if node_dim(n.id) { DIM } else { 1.0 }
                    * visibility;
                if alpha < 0.01 {
                    continue;
                }
                let fill = if active || valid_target {
                    sel_color
                } else {
                    theme
                        .handle
                        .unwrap_or(visuals.widgets.inactive.fg_stroke.color)
                }
                .gamma_multiply(alpha);
                cp.circle_filled(pos, r, fill);
                cp.circle_stroke(
                    pos,
                    r,
                    Stroke::new(1.5_f32, visuals.window_fill.gamma_multiply(alpha)),
                );
                if let Some(text) = viewer.handle_label(n, h.id) {
                    let galley = cp.layout_no_wrap(
                        text,
                        FontId::proportional(10.0),
                        visuals.weak_text_color(),
                    );
                    let size = galley.size();
                    let anchor = pos - h.side.dir() * 8.0;
                    let top_left = match h.side {
                        Side::Left => pos2(anchor.x - size.x, anchor.y - size.y / 2.0),
                        Side::Right => pos2(anchor.x, anchor.y - size.y / 2.0),
                        Side::Top => pos2(anchor.x - size.x / 2.0, anchor.y - size.y),
                        Side::Bottom => pos2(anchor.x - size.x / 2.0, anchor.y),
                    };
                    cp.galley(top_left, galley, visuals.weak_text_color());
                }
            }
        }

        // Grips on the ends of selected edges: drag one to another handle.
        if o.edges_reconnectable && o.nodes_connectable && state.interaction.connecting.is_none() {
            let mut start = None;
            for (i, path, _) in &geoms {
                let e = &state.edges[*i];
                if !e.selected {
                    continue;
                }
                let ends = [
                    (path[0], (e.target, e.target_handle), 0u8),
                    (*path.last().unwrap(), (e.source, e.source_handle), 1u8),
                ];
                for (pos, fixed, which) in ends {
                    let hit = Rect::from_center_size(pos, Vec2::splat(2.0 * (HANDLE_RADIUS + 5.0)));
                    let gr = cui.interact(hit, id.with(("grip", e.id, which)), Sense::drag());
                    let r = if gr.hovered() {
                        HANDLE_RADIUS + 4.5
                    } else {
                        HANDLE_RADIUS + 3.0
                    };
                    cp.circle_stroke(pos, r, Stroke::new(2.0_f32, sel_color));
                    if gr.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if gr.drag_started() {
                        start = Some(ConnectDrag {
                            node: fixed.0,
                            handle: fixed.1,
                            reconnecting: Some(e.id),
                        });
                    }
                }
            }
            if start.is_some() {
                state.interaction.connecting = start;
            }
        }

        if let Some(from) = state
            .interaction
            .connecting
            .as_ref()
            .map(|c| (c.node, c.handle))
        {
            let reconnecting = state
                .interaction
                .connecting
                .as_ref()
                .and_then(|c| c.reconnecting);
            ui.ctx().request_repaint();
            let from_handle = find_handle(&handles, from.0, from.1);
            let from_rect = rects.get(&from.0);
            if let (Some(fh), Some(fr), Some(pointer)) =
                (from_handle, from_rect, pointer_scene(&tf))
            {
                let from_pos = fh.position(*fr);
                let candidate = all_handles
                    .iter()
                    .filter(|(_, _, p)| (*p - pointer).length() <= o.connection_radius)
                    .filter_map(|(n, h, p)| {
                        connection_between(state, &handles, &*viewer, &o, from, (*n, h.id))
                            .map(|c| (c, *h, *p))
                    })
                    .min_by(|a, b| {
                        (a.2 - pointer)
                            .length()
                            .total_cmp(&(b.2 - pointer).length())
                    });

                let (end, end_side) = match &candidate {
                    Some((_, h, p)) => (*p, h.side),
                    None => (pointer, opposite(fh.side)),
                };
                let preview_kind = reconnecting
                    .and_then(|eid| state.edge(eid))
                    .and_then(|e| e.kind)
                    .unwrap_or(o.default_edge_kind);
                let (s, ss, t, ts) = if fh.kind == HandleKind::Source {
                    (from_pos, fh.side, end, end_side)
                } else {
                    (end, end_side, from_pos, fh.side)
                };
                cp.add(Shape::line(
                    edge_path(preview_kind, s, ss, t, ts),
                    Stroke::new(
                        2.0_f32,
                        if candidate.is_some() {
                            sel_color
                        } else {
                            sel_color.gamma_multiply(0.5)
                        },
                    ),
                ));

                let released = ui.input(|i| i.pointer.any_released());
                let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
                if released || cancelled {
                    state.interaction.connecting = None;
                    if released && !cancelled {
                        match (candidate, reconnecting) {
                            (Some((conn, _, _)), Some(eid)) => {
                                if let Some(e) = state.edge_mut(eid) {
                                    let old = e.connection();
                                    e.source = conn.source;
                                    e.source_handle = conn.source_handle;
                                    e.target = conn.target;
                                    e.target_handle = conn.target_handle;
                                    events.push(FlowEvent::Reconnected {
                                        edge: eid,
                                        old,
                                        new: conn,
                                    });
                                }
                            }
                            // Dropped on nothing: the edge snaps back.
                            (None, Some(_)) => {}
                            (Some((conn, _, _)), None) => {
                                if let Some(eid) = state.add_edge(conn, E::default()) {
                                    events.push(FlowEvent::Connected(eid));
                                }
                            }
                            (None, None) => events.push(FlowEvent::ConnectionDropped {
                                node: from.0,
                                handle: from.1,
                                pos: pointer,
                            }),
                        }
                    }
                }
            } else {
                state.interaction.connecting = None;
            }
        }

        // --- alignment guides while dragging --------------------------------
        for g in &state.interaction.guides {
            let (a, b) = if g.vertical {
                (pos2(g.at, g.from), pos2(g.at, g.to))
            } else {
                (pos2(g.from, g.at), pos2(g.to, g.at))
            };
            cp.line_segment(
                [a, b],
                Stroke::new(
                    1.0 / state.viewport.zoom.max(1e-3),
                    theme.guide.unwrap_or(Color32::from_rgb(255, 90, 160)),
                ),
            );
        }

        // --- box selection --------------------------------------------------
        if let (Some(start), Some(cur)) = (state.interaction.box_select, pointer_scene(&tf)) {
            let r = Rect::from_two_pos(start, cur);
            cp.rect_filled(r, 0.0, sel_color.gamma_multiply(0.12));
            cp.rect_stroke(r, 0.0, Stroke::new(1.0_f32, sel_color), StrokeKind::Inside);
            if !ui.input(|i| i.pointer.primary_down()) {
                state.interaction.box_select = None;
                state.clear_selection();
                for n in &mut state.nodes {
                    n.selected = !hidden.contains(&n.id)
                        && if n.is_group {
                            // An open group is only picked when the box covers all of it,
                            // so a box drawn inside it selects its members instead.
                            r.contains_rect(n.rect())
                        } else {
                            r.intersects(n.rect())
                        };
                }
            }
        }

        // --- keyboard -------------------------------------------------------
        if o.delete_key
            && hover.is_some()
            && !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
        {
            let (nodes, edges) = state.delete_selected();
            if !nodes.is_empty() || !edges.is_empty() {
                events.push(FlowEvent::Deleted { nodes, edges });
            }
        }

        if o.keyboard_nudge
            && hover.is_some()
            && !ui.ctx().wants_keyboard_input()
            && state.nodes.iter().any(|n| n.selected && n.draggable)
            && o.nodes_draggable
        {
            use egui::Modifiers as M;
            let delta = ui.input_mut(|i| {
                let mut d = Vec2::ZERO;
                for (key, dir) in [
                    (Key::ArrowLeft, vec2(-1.0, 0.0)),
                    (Key::ArrowRight, vec2(1.0, 0.0)),
                    (Key::ArrowUp, vec2(0.0, -1.0)),
                    (Key::ArrowDown, vec2(0.0, 1.0)),
                ] {
                    if i.consume_key(M::SHIFT, key) {
                        d += dir * 10.0;
                    } else if i.consume_key(M::NONE, key) {
                        d += dir;
                    }
                }
                d
            });
            if delta != Vec2::ZERO {
                let ids: Vec<NodeId> = state
                    .nodes
                    .iter()
                    .filter(|n| n.selected && n.draggable && !hidden.contains(&n.id))
                    .map(|n| n.id)
                    .collect();
                // A nudged group takes its members with it.
                let mut moving = ids.clone();
                for r in &ids {
                    for d in state.descendants(*r) {
                        if !moving.contains(&d) {
                            moving.push(d);
                        }
                    }
                }
                for n in state.nodes.iter_mut().filter(|n| moving.contains(&n.id)) {
                    n.position += delta;
                }
                let moving_set: HashSet<NodeId> = moving.iter().copied().collect();
                for id in &moving {
                    if let Some(p) = state.node(*id).map(|n| n.position) {
                        let clamped =
                            state.clamp_in_group(*id, p, o.group_header_height, &moving_set);
                        if let Some(n) = state.node_mut(*id) {
                            n.position = clamped;
                        }
                    }
                }
                events.push(FlowEvent::NodesDragged(ids.clone()));
                events.push(FlowEvent::NodesDragStopped(ids));
            }
        }

        if o.keyboard_shortcuts && hover.is_some() && !ui.ctx().wants_keyboard_input() {
            use egui::Modifiers as M;
            let (undo, redo, copy, cut, paste, dup) = ui.input_mut(|i| {
                let evt =
                    |i: &egui::InputState, f: fn(&egui::Event) -> bool| i.events.iter().any(f);
                let redo = i.consume_key(M::COMMAND | M::SHIFT, Key::Z)
                    || i.consume_key(M::COMMAND, Key::Y);
                let undo = i.consume_key(M::COMMAND, Key::Z);
                let copy =
                    i.consume_key(M::COMMAND, Key::C) || evt(i, |e| matches!(e, egui::Event::Copy));
                let cut =
                    i.consume_key(M::COMMAND, Key::X) || evt(i, |e| matches!(e, egui::Event::Cut));
                let paste = i.consume_key(M::COMMAND, Key::V)
                    || evt(i, |e| matches!(e, egui::Event::Paste(_)));
                let dup = i.consume_key(M::COMMAND, Key::D);
                (undo, redo, copy, cut, paste, dup)
            });
            for (on, ev) in [
                (undo, FlowEvent::UndoRequested),
                (redo, FlowEvent::RedoRequested),
                (copy, FlowEvent::CopyRequested),
                (cut, FlowEvent::CutRequested),
                (paste, FlowEvent::PasteRequested),
                (dup, FlowEvent::DuplicateRequested),
            ] {
                if on {
                    events.push(ev);
                }
            }
        }

        // --- overlays -------------------------------------------------------
        if o.controls {
            controls(ui, id, canvas, state, &o);
        }
        if o.minimap {
            minimap(ui, id, canvas, state, &*viewer, sel_color, &theme);
        }

        let selection_after = state.selection_snapshot();
        if selection_after != selection_before {
            events.push(FlowEvent::SelectionChanged {
                nodes: selection_after.0,
                edges: selection_after.1,
            });
        }
        if state.viewport != viewport_before {
            events.push(FlowEvent::ViewportChanged(state.viewport));
        }

        state.unflatten();

        if o.crisp_text {
            crate::crisp::crisp_text(ui.ctx(), layer_id, tf.scaling);
        }

        FlowResponse {
            pane,
            nodes: node_resps,
            events,
        }
    }
}

fn draw_arrow(cp: &Painter, tip: Pos2, dir: Vec2, color: Color32, style: ArrowStyle) {
    let normal = vec2(-dir.y, dir.x);
    match style {
        ArrowStyle::Triangle => {
            let base = tip - dir * 10.0;
            cp.add(Shape::convex_polygon(
                vec![tip, base + normal * 5.0, base - normal * 5.0],
                color,
                Stroke::NONE,
            ));
        }
        ArrowStyle::Open => {
            let base = tip - dir * 9.0;
            let stroke = Stroke::new(1.8_f32, color);
            cp.line_segment([base + normal * 5.0, tip], stroke);
            cp.line_segment([base - normal * 5.0, tip], stroke);
        }
        ArrowStyle::Circle => {
            cp.circle_filled(tip - dir * 4.0, 4.0, color);
        }
        ArrowStyle::Diamond => {
            cp.add(Shape::convex_polygon(
                vec![
                    tip,
                    tip - dir * 7.0 + normal * 4.5,
                    tip - dir * 14.0,
                    tip - dir * 7.0 - normal * 4.5,
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}

fn draw_pulse_head(cp: &Painter, pos: Pos2, dir: Vec2, r: f32, color: Color32, shape: PulseShape) {
    let normal = vec2(-dir.y, dir.x);
    match shape {
        PulseShape::Circle => {
            cp.circle_filled(pos, r, color);
        }
        PulseShape::Square => {
            cp.rect_filled(
                Rect::from_center_size(pos, Vec2::splat(r * 1.8)),
                1.0,
                color,
            );
        }
        PulseShape::Diamond => {
            cp.add(Shape::convex_polygon(
                vec![
                    pos + dir * r * 1.4,
                    pos + normal * r,
                    pos - dir * r * 1.4,
                    pos - normal * r,
                ],
                color,
                Stroke::NONE,
            ));
        }
        PulseShape::Arrow => {
            cp.add(Shape::convex_polygon(
                vec![
                    pos + dir * r * 1.6,
                    pos - dir * r + normal * r * 1.1,
                    pos - dir * r * 0.4,
                    pos - dir * r - normal * r * 1.1,
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}

fn select_node<N, E>(state: &mut FlowState<N, E>, id: NodeId, additive: bool) {
    if additive {
        if let Some(n) = state.node_mut(id) {
            n.selected = !n.selected;
        }
    } else {
        state.clear_selection();
        if let Some(n) = state.node_mut(id) {
            n.selected = true;
        }
    }
}

fn snap(p: Pos2, grid: Option<f32>) -> Pos2 {
    match grid {
        Some(g) if g > 0.0 => pos2((p.x / g).round() * g, (p.y / g).round() * g),
        _ => p,
    }
}

fn opposite(s: Side) -> Side {
    match s {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
    }
}

fn find_handle(handles: &Handles, node: NodeId, id: HandleId) -> Option<Handle> {
    handles.get(&node)?.iter().find(|h| h.id == id).copied()
}

/// The oriented connection for a drag from `from` to `to`, if it is allowed.
fn connection_between<N, E, V: FlowViewer<N, E>>(
    state: &FlowState<N, E>,
    handles: &Handles,
    viewer: &V,
    o: &FlowOptions,
    from: (NodeId, HandleId),
    to: (NodeId, HandleId),
) -> Option<Connection> {
    let fh = find_handle(handles, from.0, from.1)?;
    let th = find_handle(handles, to.0, to.1)?;
    if fh.kind == th.kind {
        return None;
    }
    let conn = if fh.kind == HandleKind::Source {
        Connection {
            source: from.0,
            source_handle: from.1,
            target: to.0,
            target_handle: to.1,
        }
    } else {
        Connection {
            source: to.0,
            source_handle: to.1,
            target: from.0,
            target_handle: from.1,
        }
    };
    if !o.allow_self_loops && conn.source == conn.target {
        return None;
    }
    if !state.node(to.0)?.connectable || !o.nodes_connectable {
        return None;
    }
    if state.edges.iter().any(|e| e.connection() == conn) {
        return None;
    }
    viewer.can_connect(&conn).then_some(conn)
}

fn paint_background(p: &Painter, canvas: Rect, vp: &Viewport, o: &FlowOptions, color: Color32) {
    let gap = o.background_gap * vp.zoom;
    if o.background == Background::None || gap < 5.0 {
        return;
    }
    let off = vec2(vp.pan.x.rem_euclid(gap), vp.pan.y.rem_euclid(gap));
    let start = canvas.min + off;
    let cols = ((canvas.max.x - start.x) / gap).ceil().max(0.0) as usize + 1;
    let rows = ((canvas.max.y - start.y) / gap).ceil().max(0.0) as usize + 1;
    let stroke = Stroke::new(1.0_f32, color.gamma_multiply(0.6));
    match o.background {
        Background::Dots => {
            let r = (vp.zoom * 1.0).clamp(0.8, 2.0);
            for c in 0..cols {
                for r_ in 0..rows {
                    p.circle_filled(start + vec2(c as f32 * gap, r_ as f32 * gap), r, color);
                }
            }
        }
        Background::Lines => {
            for c in 0..cols {
                let x = start.x + c as f32 * gap;
                p.line_segment([pos2(x, canvas.min.y), pos2(x, canvas.max.y)], stroke);
            }
            for r_ in 0..rows {
                let y = start.y + r_ as f32 * gap;
                p.line_segment([pos2(canvas.min.x, y), pos2(canvas.max.x, y)], stroke);
            }
        }
        Background::Cross => {
            let a = (gap * 0.12).clamp(2.0, 5.0);
            for c in 0..cols {
                for r_ in 0..rows {
                    let m = start + vec2(c as f32 * gap, r_ as f32 * gap);
                    p.line_segment([m - vec2(a, 0.0), m + vec2(a, 0.0)], stroke);
                    p.line_segment([m - vec2(0.0, a), m + vec2(0.0, a)], stroke);
                }
            }
        }
        Background::None => {}
    }
}

/// Zoom in / out / fit buttons, bottom-left.
fn controls<N, E>(ui: &Ui, id: Id, canvas: Rect, state: &mut FlowState<N, E>, o: &FlowOptions) {
    Area::new(id.with("controls"))
        .order(Order::Foreground)
        .movable(false)
        .fixed_pos(canvas.left_bottom() + vec2(12.0, -12.0))
        .pivot(Align2::LEFT_BOTTOM)
        .show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
                let center = (canvas.size() / 2.0).to_pos2();
                if ui.button("+").on_hover_text("Zoom in").clicked() {
                    let mut to = state.target_viewport();
                    to.zoom_at(center, 1.25, o.min_zoom, o.max_zoom);
                    state.animate_viewport(to, o.view_transition);
                }
                if ui.button("\u{2212}").on_hover_text("Zoom out").clicked() {
                    let mut to = state.target_viewport();
                    to.zoom_at(center, 0.8, o.min_zoom, o.max_zoom);
                    state.animate_viewport(to, o.view_transition);
                }
                if ui.button("Fit").on_hover_text("Fit view").clicked() {
                    state.fit_view_animated(o.view_transition);
                }
            });
        });
}

/// Overview of the whole graph, bottom-right. Click or drag to recentre.
fn minimap<N, E, V: FlowViewer<N, E>>(
    ui: &Ui,
    id: Id,
    canvas: Rect,
    state: &mut FlowState<N, E>,
    viewer: &V,
    accent: Color32,
    theme: &crate::theme::FlowTheme,
) {
    const SIZE: Vec2 = vec2(160.0, 110.0);
    let hidden_nodes = state.hidden_nodes();
    Area::new(id.with("minimap"))
        .order(Order::Foreground)
        .movable(false)
        .fixed_pos(canvas.right_bottom() - vec2(12.0, 12.0))
        .pivot(Align2::RIGHT_BOTTOM)
        .show(ui.ctx(), |ui| {
            let (rect, resp) = ui.allocate_exact_size(SIZE, Sense::click_and_drag());
            let v = ui.visuals();
            let painter = ui.painter_at(rect);
            painter.rect(
                rect,
                4.0,
                theme
                    .minimap_background
                    .unwrap_or(v.window_fill)
                    .gamma_multiply(0.92),
                v.widgets.noninteractive.bg_stroke,
                StrokeKind::Inside,
            );

            let vp = state.viewport;
            let view =
                Rect::from_min_max(vp.to_flow(Pos2::ZERO), vp.to_flow(canvas.size().to_pos2()));
            let mut bounds = view;
            if let Some(b) = state.bounds() {
                bounds = bounds.union(b);
            }
            bounds = bounds.expand(bounds.size().max_elem() * 0.05 + 1.0);
            let scale = (rect.width() / bounds.width()).min(rect.height() / bounds.height());
            let to_mini = |p: Pos2| rect.center() + (p - bounds.center()) * scale;
            let to_rect = |r: Rect| Rect::from_min_max(to_mini(r.min), to_mini(r.max));

            for n in state.nodes.iter().filter(|n| !hidden_nodes.contains(&n.id)) {
                let color = viewer
                    .minimap_color(n)
                    .unwrap_or_else(|| v.widgets.inactive.fg_stroke.color.gamma_multiply(0.6));
                painter.rect_filled(to_rect(n.rect()), 1.0, color);
            }
            painter.rect_stroke(
                to_rect(view),
                1.0,
                Stroke::new(1.5_f32, accent),
                StrokeKind::Inside,
            );

            if (resp.clicked() || resp.dragged())
                && let Some(p) = resp.interact_pointer_pos()
            {
                let target = bounds.center() + (p - rect.center()) / scale;
                state.viewport.pan = canvas.size() / 2.0 - target.to_vec2() * vp.zoom;
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PulseStyle;
    use egui::{Context, Event, RawInput, Shape};

    struct V;
    impl FlowViewer<&'static str, ()> for V {
        fn node_ui(&mut self, ui: &mut Ui, node: &mut Node<&'static str>) {
            ui.label(node.data);
        }
    }

    fn run_frame(
        ctx: &Context,
        state: &mut FlowState<&'static str, ()>,
        events: Vec<Event>,
        fit: bool,
    ) -> Vec<FlowEvent<&'static str, ()>> {
        run_frame_at(ctx, state, events, fit, None, true)
    }

    fn run_frame_at(
        ctx: &Context,
        state: &mut FlowState<&'static str, ()>,
        events: Vec<Event>,
        fit: bool,
        time: Option<f64>,
        animate: bool,
    ) -> Vec<FlowEvent<&'static str, ()>> {
        let opts = FlowOptions {
            minimap: true,
            fit_view_on_init: fit,
            animate,
            ..Default::default()
        };
        run_frame_with(ctx, state, events, time, opts)
    }

    fn run_frame_with(
        ctx: &Context,
        state: &mut FlowState<&'static str, ()>,
        events: Vec<Event>,
        time: Option<f64>,
        opts: FlowOptions,
    ) -> Vec<FlowEvent<&'static str, ()>> {
        run_full(ctx, state, events, time, opts).0
    }

    /// Run one frame, returning the events and every shape that was painted.
    fn run_full(
        ctx: &Context,
        state: &mut FlowState<&'static str, ()>,
        events: Vec<Event>,
        time: Option<f64>,
        opts: FlowOptions,
    ) -> (Vec<FlowEvent<&'static str, ()>>, Vec<Shape>) {
        run_full_mods(ctx, state, events, time, opts, egui::Modifiers::NONE)
    }

    fn run_full_mods(
        ctx: &Context,
        state: &mut FlowState<&'static str, ()>,
        events: Vec<Event>,
        time: Option<f64>,
        opts: FlowOptions,
        modifiers: egui::Modifiers,
    ) -> (Vec<FlowEvent<&'static str, ()>>, Vec<Shape>) {
        let mut out = Vec::new();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events,
            time,
            modifiers,
            ..Default::default()
        };
        let full = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(Frame::NONE)
                .show(ctx, |ui| {
                    let opts = opts.clone();
                    out = Flow::new("t").options(opts).show(ui, state, &mut V).events;
                });
        });
        let mut shapes = Vec::new();
        for clipped in full.shapes {
            flatten(clipped.shape, &mut shapes);
        }
        (out, shapes)
    }

    fn flatten(shape: Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(v) => v.into_iter().for_each(|s| flatten(s, out)),
            other => out.push(other),
        }
    }

    /// Two nodes joined by a straight edge, settled over a few frames.
    fn straight_pair(
        edit: impl FnOnce(&mut Edge<()>),
    ) -> (
        Context,
        FlowState<&'static str, ()>,
        Vec<Shape>,
        (Pos2, Pos2),
    ) {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 100.0), "b");
        let e = s.connect(a, b, ()).unwrap();
        {
            let edge = s.edge_mut(e).unwrap();
            edge.kind = Some(EdgeKind::Straight);
            edit(edge);
        }
        let opts = FlowOptions {
            minimap: false,
            controls: false,
            ..Default::default()
        };
        let mut shapes = Vec::new();
        for _ in 0..4 {
            shapes = run_full(&ctx, &mut s, vec![], None, opts.clone()).1;
        }
        let from =
            Handle::source(Handle::DEFAULT_SOURCE, Side::Right).position(s.node(a).unwrap().rect());
        let to =
            Handle::target(Handle::DEFAULT_TARGET, Side::Left).position(s.node(b).unwrap().rect());
        (ctx, s, shapes, (from, to))
    }

    #[test]
    fn every_arrow_style_draws_at_the_target_and_optionally_the_source() {
        let none = straight_pair(|_| {}).2.len();
        // (style, shapes one arrowhead adds)
        for (style, adds) in [
            (ArrowStyle::Triangle, 1),
            (ArrowStyle::Open, 2),
            (ArrowStyle::Circle, 1),
            (ArrowStyle::Diamond, 1),
        ] {
            let target = straight_pair(|e| {
                e.arrow = true;
                e.arrow_style = style;
            })
            .2
            .len();
            let both = straight_pair(|e| {
                e.arrow = true;
                e.arrow_at_source = true;
                e.arrow_style = style;
            })
            .2
            .len();
            let source_only = straight_pair(|e| {
                e.arrow_at_source = true;
                e.arrow_style = style;
            })
            .2
            .len();
            assert_eq!(target - none, adds, "{style:?} at target");
            assert_eq!(both - none, 2 * adds, "{style:?} at both ends");
            assert_eq!(source_only - none, adds, "{style:?} at source only");
        }
    }

    #[test]
    fn arrowheads_sit_on_the_handles_they_point_at() {
        let (_, _, shapes, (from, to)) = straight_pair(|e| {
            e.arrow = true;
            e.arrow_at_source = true;
        });
        let tips: Vec<Pos2> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Path(p) if p.closed => Some(p.points.clone()),
                _ => None,
            })
            .filter(|pts| pts.len() == 3)
            .map(|pts| pts[0])
            .collect();
        assert!(
            tips.iter().any(|t| (*t - to).length() < 0.5),
            "target tip: {tips:?} vs {to:?}"
        );
        assert!(
            tips.iter().any(|t| (*t - from).length() < 0.5),
            "source tip: {tips:?} vs {from:?}"
        );
    }

    fn text_shape<'a>(shapes: &'a [Shape], text: &str) -> Option<&'a egui::epaint::TextShape> {
        shapes.iter().find_map(|s| match s {
            Shape::Text(t) if t.galley.text() == text => Some(t),
            _ => None,
        })
    }

    #[test]
    fn edge_label_follows_its_style() {
        let fill = Color32::from_rgb(10, 20, 30);
        let ink = Color32::from_rgb(200, 150, 50);
        let (_, _, shapes, (from, to)) = straight_pair(|e| {
            e.label = Some("RPM".into());
            e.label_style = EdgeLabelStyle {
                position: 0.7,
                size: 11.0,
                color: Some(ink),
                background: Some(fill),
            };
        });
        let want = from + (to - from) * 0.7;
        let t = text_shape(&shapes, "RPM").expect("label drawn");
        let centre = t.pos + t.galley.size() / 2.0;
        assert!((centre - want).length() < 1.5, "{centre:?} vs {want:?}");
        let section = &t.galley.job.sections[0].format;
        assert_eq!((section.color, section.font_id.size), (ink, 11.0));
        let bg = shapes.iter().find_map(|s| match s {
            Shape::Rect(r) if r.fill == fill => Some(r.rect),
            _ => None,
        });
        let bg = bg.expect("label background drawn");
        assert!((bg.center() - want).length() < 1.5, "{bg:?}");
        assert!(bg.contains_rect(Rect::from_center_size(centre, t.galley.size())));
    }

    #[test]
    fn edge_label_defaults_to_the_middle_in_the_theme_colours() {
        let (ctx, _, shapes, (from, to)) = straight_pair(|e| e.label = Some("mid".into()));
        let t = text_shape(&shapes, "mid").expect("label drawn");
        let centre = t.pos + t.galley.size() / 2.0;
        assert!((centre - from.lerp(to, 0.5)).length() < 1.5);
        let section = &t.galley.job.sections[0].format;
        assert_eq!(section.color, ctx.style().visuals.text_color());
        assert_eq!(section.font_id.size, 12.0);
    }

    #[test]
    fn alignment_guides_are_painted_while_dragging_and_not_after() {
        let guide = Color32::from_rgb(255, 90, 160);
        let painted = |shapes: &[Shape]| {
            shapes
                .iter()
                .any(|s| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == guide))
        };
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 250.0), "b");
        let opts = FlowOptions {
            alignment_guides: true,
            ..Default::default()
        };
        for _ in 0..3 {
            run_frame_with(&ctx, &mut s, vec![], None, opts.clone());
        }
        let top = s.node(a).unwrap().position.y;
        let grab = s.node(b).unwrap().rect().center();
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame_with(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(grab)],
            None,
            opts.clone(),
        );
        run_frame_with(&ctx, &mut s, vec![btn(grab, true)], None, opts.clone());
        let far = grab + vec2(-20.0, -140.0); // not within range of anything
        let near = grab + vec2(-20.0, top + 4.0 - 250.0);
        let mut last = Vec::new();
        for i in 1..=4 {
            let p = grab + (far - grab) * (i as f32 / 4.0);
            last = run_full(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(p)],
                None,
                opts.clone(),
            )
            .1;
        }
        assert!(!painted(&last), "no guide while nothing is in range");
        for i in 1..=4 {
            let p = far + (near - far) * (i as f32 / 4.0);
            last = run_full(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(p)],
                None,
                opts.clone(),
            )
            .1;
        }
        assert!(painted(&last), "guide painted when snapped");
        run_frame_with(&ctx, &mut s, vec![btn(near, false)], None, opts.clone());
        let (_, after) = run_full(&ctx, &mut s, vec![], None, opts.clone());
        assert!(!painted(&after), "guide gone after release");

        // With the option off nothing snaps or paints, even at the same spot.
        let off = FlowOptions::default();
        let mut s2 = FlowState::new();
        s2.add_node(pos2(50.0, 100.0), "a");
        let b2 = s2.add_node(pos2(400.0, 250.0), "b");
        for _ in 0..3 {
            run_frame_with(&ctx, &mut s2, vec![], None, off.clone());
        }
        run_frame_with(
            &ctx,
            &mut s2,
            vec![Event::PointerMoved(grab)],
            None,
            off.clone(),
        );
        run_frame_with(&ctx, &mut s2, vec![btn(grab, true)], None, off.clone());
        for i in 1..=6 {
            let p = grab + (near - grab) * (i as f32 / 6.0);
            last = run_full(
                &ctx,
                &mut s2,
                vec![Event::PointerMoved(p)],
                None,
                off.clone(),
            )
            .1;
        }
        assert!(!painted(&last));
        assert!((s2.node(b2).unwrap().position.y - (top + 4.0)).abs() < 1.5);
    }

    /// Lay one labelled node out at `zoom` and return the label's layout font size
    /// and its on-screen width.
    fn label_at_zoom(zoom: f32, crisp: bool) -> (f32, f32, Color32) {
        let ctx = Context::default();
        let mut s = FlowState::new();
        s.add_node(pos2(40.0, 40.0), "hello world");
        s.viewport.zoom = zoom;
        let opts = FlowOptions {
            minimap: false,
            controls: false,
            crisp_text: crisp,
            ..Default::default()
        };
        let mut shapes = Vec::new();
        for _ in 0..4 {
            shapes = run_full(&ctx, &mut s, vec![], None, opts.clone()).1;
        }
        let t = text_shape(&shapes, "hello world").expect("node text drawn");
        let f = &t.galley.job.sections[0].format;
        (f.font_id.size, t.galley.rect.width(), f.color)
    }

    #[test]
    fn zoomed_text_is_laid_out_at_the_zoomed_size_and_lands_at_the_same_screen_size() {
        let (base_size, base_width, _) = label_at_zoom(1.0, true);
        for zoom in [2.0_f32, 3.0, 0.5] {
            let (size, width, _) = label_at_zoom(zoom, true);
            let want = base_size * crate::crisp::text_scale(zoom);
            assert!(
                (size - want).abs() < 0.01,
                "zoom {zoom}: layout size {size}, want {want}"
            );
            // On screen it is `zoom` times the 1x width, whichever way it was laid out.
            let ratio = width / base_width;
            assert!(
                (ratio - zoom).abs() / zoom < 0.04,
                "zoom {zoom}: width ratio {ratio}"
            );
        }
    }

    #[test]
    fn text_is_left_alone_at_zoom_one_and_when_crisp_text_is_off() {
        let (base, _, _) = label_at_zoom(1.0, true);
        assert_eq!(label_at_zoom(1.0, false).0, base);
        // Off: the 1x layout is simply stretched.
        assert_eq!(label_at_zoom(2.0, false).0, base);
        assert!(label_at_zoom(2.0, true).0 > base * 1.9);
    }

    #[test]
    fn crisp_text_keeps_the_text_colour() {
        let (_, _, plain) = label_at_zoom(2.0, false);
        let (_, _, crisp) = label_at_zoom(2.0, true);
        assert_eq!(plain, crisp);
    }

    #[test]
    fn pulse_shapes_paint_a_head_in_the_pulse_colour() {
        let ink = Color32::from_rgb(250, 120, 10);
        for (shape, closed_path) in [
            (crate::PulseShape::Circle, false),
            (crate::PulseShape::Square, false),
            (crate::PulseShape::Diamond, true),
            (crate::PulseShape::Arrow, true),
        ] {
            let ctx = Context::default();
            let mut s = FlowState::new();
            let a = s.add_node(pos2(0.0, 0.0), "a");
            let b = s.add_node(pos2(300.0, 100.0), "b");
            let e = s.connect(a, b, ()).unwrap();
            let opts = FlowOptions::default();
            run_full(&ctx, &mut s, vec![], Some(0.0), opts.clone());
            s.pulse_edge(
                e,
                PulseStyle {
                    duration: 1.0,
                    color: Some(ink),
                    shape,
                    trail: 0,
                    ..Default::default()
                },
            );
            run_full(&ctx, &mut s, vec![], Some(1.0), opts.clone());
            let (_, shapes) = run_full(&ctx, &mut s, vec![], Some(1.5), opts);
            let painted = shapes.iter().any(|sh| match sh {
                Shape::Path(p) => closed_path && p.closed && p.fill == ink,
                Shape::Circle(c) => shape == crate::PulseShape::Circle && c.fill == ink,
                Shape::Rect(r) => shape == crate::PulseShape::Square && r.fill == ink,
                _ => false,
            });
            assert!(painted, "{shape:?} head painted");
        }
    }

    #[test]
    fn renders_measures_and_fits() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(0.0, 0.0), "a");
        let b = s.add_node(pos2(400.0, 300.0), "b");
        s.connect(a, b, ());
        for _ in 0..4 {
            run_frame(&ctx, &mut s, vec![], true);
        }
        // Size was measured from content rather than the default estimate.
        assert_ne!(s.nodes[0].size, vec2(150.0, 40.0));
        // Fit view framed both nodes inside the 800x600 canvas.
        for n in &s.nodes {
            let p = s.viewport.to_screen(n.position);
            assert!(
                p.x >= 0.0 && p.y >= 0.0 && p.x <= 800.0 && p.y <= 600.0,
                "{p:?}"
            );
        }
    }

    #[test]
    fn dragging_a_node_moves_it() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        s.viewport = Viewport::default();
        let a = s.add_node(pos2(100.0, 100.0), "node");
        for _ in 0..2 {
            run_frame(&ctx, &mut s, vec![], false);
        }
        let start = s.node(a).unwrap().rect().center();
        let mut moved = start;
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame(&ctx, &mut s, vec![Event::PointerMoved(start)], false);
        run_frame(&ctx, &mut s, vec![btn(start, true)], false);
        for step in 1..=5 {
            moved = start + vec2(20.0 * step as f32, 10.0 * step as f32);
            run_frame(&ctx, &mut s, vec![Event::PointerMoved(moved)], false);
        }
        run_frame(&ctx, &mut s, vec![btn(moved, false)], false);
        let n = s.node(a).unwrap();
        assert!(
            n.position.x > 150.0 && n.position.y > 130.0,
            "{:?}",
            n.position
        );
        assert!(n.selected);
    }

    #[test]
    fn dragging_between_handles_creates_an_edge() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 250.0), "b");
        for _ in 0..3 {
            run_frame(&ctx, &mut s, vec![], false);
        }
        let from =
            Handle::source(Handle::DEFAULT_SOURCE, Side::Right).position(s.node(a).unwrap().rect());
        let to =
            Handle::target(Handle::DEFAULT_TARGET, Side::Left).position(s.node(b).unwrap().rect());
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame(&ctx, &mut s, vec![Event::PointerMoved(from)], false);
        run_frame(&ctx, &mut s, vec![btn(from, true)], false);
        let mut events = Vec::new();
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            events.extend(run_frame(&ctx, &mut s, vec![Event::PointerMoved(p)], false));
        }
        events.extend(run_frame(&ctx, &mut s, vec![btn(to, false)], false));
        assert_eq!(s.edges.len(), 1, "{events:?}");
        assert_eq!((s.edges[0].source, s.edges[0].target), (a, b));
        assert!(events.iter().any(|e| matches!(e, FlowEvent::Connected(_))));
    }

    /// Drag from `from` to `to` over a few frames and release; returns events.
    fn drag(
        ctx: &Context,
        s: &mut FlowState<&'static str, ()>,
        from: Pos2,
        to: Pos2,
    ) -> Vec<FlowEvent<&'static str, ()>> {
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame(ctx, s, vec![Event::PointerMoved(from)], false);
        run_frame(ctx, s, vec![btn(from, true)], false);
        let mut events = Vec::new();
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            events.extend(run_frame(ctx, s, vec![Event::PointerMoved(p)], false));
        }
        events.extend(run_frame(ctx, s, vec![btn(to, false)], false));
        events
    }

    fn reconnect_setup() -> (
        Context,
        FlowState<&'static str, ()>,
        (NodeId, NodeId, NodeId),
        EdgeId,
    ) {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 100.0), "b");
        let c = s.add_node(pos2(400.0, 300.0), "c");
        let e = s.connect(a, b, ()).unwrap();
        for _ in 0..3 {
            run_frame(&ctx, &mut s, vec![], false);
        }
        s.edge_mut(e).unwrap().selected = true;
        run_frame(&ctx, &mut s, vec![], false);
        (ctx, s, (a, b, c), e)
    }

    fn target_pos(s: &FlowState<&'static str, ()>, n: NodeId) -> Pos2 {
        Handle::target(Handle::DEFAULT_TARGET, Side::Left).position(s.node(n).unwrap().rect())
    }

    #[test]
    fn dragging_an_edge_end_reconnects_it() {
        let (ctx, mut s, (a, _, c), e) = reconnect_setup();
        let (from, to) = (target_pos(&s, b_of(&s, e)), target_pos(&s, c));
        let events = drag(&ctx, &mut s, from, to);
        let edge = s.edge(e).unwrap();
        assert_eq!((edge.source, edge.target), (a, c), "{events:?}");
        assert_eq!(s.edges.len(), 1);
        assert!(events.iter().any(|ev| matches!(
            ev,
            FlowEvent::Reconnected { edge, old, new }
                if *edge == e && old.target != new.target && new.target == c
        )));
    }

    fn b_of(s: &FlowState<&'static str, ()>, e: EdgeId) -> NodeId {
        s.edge(e).unwrap().target
    }

    #[test]
    fn dragging_the_corner_grip_resizes_a_selected_node() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(100.0, 100.0), "a");
        for _ in 0..3 {
            run_frame(&ctx, &mut s, vec![], false);
        }
        s.node_mut(a).unwrap().selected = true;
        run_frame(&ctx, &mut s, vec![], false);
        let before = s.node(a).unwrap().size;
        let corner = s.node(a).unwrap().rect().max;

        let events = drag(&ctx, &mut s, corner, corner + vec2(60.0, 40.0));
        let n = s.node(a).unwrap();
        assert!(n.fixed_size.is_some(), "{events:?}");
        assert!(
            (n.size.x - (before.x + 60.0)).abs() < 2.0,
            "{:?} vs {before:?}",
            n.size
        );
        assert!(n.size.y > before.y + 30.0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, FlowEvent::NodeResized { finished: true, .. }))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            FlowEvent::NodeResized {
                finished: false,
                ..
            }
        )));
    }

    #[test]
    fn resizing_respects_min_size_and_unselected_nodes_have_no_grips() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(100.0, 100.0), "a");
        s.node_mut(a).unwrap().min_size = vec2(120.0, 80.0);
        for _ in 0..3 {
            run_frame(&ctx, &mut s, vec![], false);
        }
        // Not selected: dragging the corner moves the node instead of resizing.
        let corner = s.node(a).unwrap().rect().max;
        drag(
            &ctx,
            &mut s,
            corner - vec2(2.0, 2.0),
            corner + vec2(30.0, 30.0),
        );
        assert!(s.node(a).unwrap().fixed_size.is_none());

        s.node_mut(a).unwrap().selected = true;
        run_frame(&ctx, &mut s, vec![], false);
        let corner = s.node(a).unwrap().rect().max;
        drag(&ctx, &mut s, corner, corner - vec2(200.0, 200.0));
        let n = s.node(a).unwrap();
        assert_eq!(n.fixed_size, Some(vec2(120.0, 80.0)));
    }

    #[test]
    fn keyboard_shortcuts_become_events() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        s.add_node(pos2(100.0, 100.0), "a");
        for _ in 0..2 {
            run_frame(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(pos2(400.0, 300.0))],
                false,
            );
        }
        let key = |key, modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let ctrl = egui::Modifiers::COMMAND;
        let shift_ctrl = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        for (ev, want) in [
            (key(egui::Key::Z, ctrl), 0),
            (key(egui::Key::Z, shift_ctrl), 1),
            (key(egui::Key::C, ctrl), 2),
            (key(egui::Key::V, ctrl), 3),
            (key(egui::Key::D, ctrl), 4),
        ] {
            let out = run_frame(&ctx, &mut s, vec![ev], false);
            let hit = out.iter().position(|e| {
                matches!(
                    (want, e),
                    (0, FlowEvent::UndoRequested)
                        | (1, FlowEvent::RedoRequested)
                        | (2, FlowEvent::CopyRequested)
                        | (3, FlowEvent::PasteRequested)
                        | (4, FlowEvent::DuplicateRequested)
                )
            });
            assert!(hit.is_some(), "shortcut {want}: {out:?}");
            assert_eq!(out.len(), 1, "exactly one request: {out:?}");
        }
    }

    #[test]
    fn arrow_keys_nudge_selected_nodes() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(100.0, 100.0), "a");
        let b = s.add_node(pos2(300.0, 100.0), "b");
        s.node_mut(a).unwrap().selected = true;
        for _ in 0..2 {
            run_frame(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(pos2(600.0, 400.0))],
                false,
            );
        }
        let key = |key, modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let out = run_frame(
            &ctx,
            &mut s,
            vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)],
            false,
        );
        assert_eq!(s.node(a).unwrap().position, pos2(101.0, 100.0));
        assert!(
            out.iter()
                .any(|e| matches!(e, FlowEvent::NodesDragStopped(ids) if ids == &[a]))
        );
        run_frame(
            &ctx,
            &mut s,
            vec![key(egui::Key::ArrowDown, egui::Modifiers::SHIFT)],
            false,
        );
        assert_eq!(s.node(a).unwrap().position, pos2(101.0, 110.0));
        assert_eq!(
            s.node(b).unwrap().position,
            pos2(300.0, 100.0),
            "unselected stays"
        );
    }

    #[test]
    fn dragging_near_another_node_snaps_to_its_edge() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 250.0), "b");
        let opts = FlowOptions {
            alignment_guides: true,
            ..Default::default()
        };
        for _ in 0..3 {
            run_frame_with(&ctx, &mut s, vec![], None, opts.clone());
        }
        let top = s.node(a).unwrap().position.y;
        let grab = s.node(b).unwrap().rect().center();
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame_with(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(grab)],
            None,
            opts.clone(),
        );
        run_frame_with(&ctx, &mut s, vec![btn(grab, true)], None, opts.clone());
        // Move so b's top lands 4 units below a's top: inside the 6px threshold.
        let to = grab + vec2(-20.0, top + 4.0 - 250.0);
        for i in 1..=6 {
            let p = grab + (to - grab) * (i as f32 / 6.0);
            run_frame_with(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(p)],
                None,
                opts.clone(),
            );
        }
        assert_eq!(
            s.node(b).unwrap().position.y,
            top,
            "snapped onto a's top edge"
        );
        assert!(!s.interaction.guides.is_empty(), "guide line is shown");
        run_frame_with(&ctx, &mut s, vec![btn(to, false)], None, opts);
        assert!(s.interaction.guides.is_empty(), "guides clear on release");
    }

    // ---- groups ---------------------------------------------------------------

    struct Scene {
        ctx: Context,
        s: FlowState<&'static str, ()>,
        group: NodeId,
        child: NodeId,
        outside: NodeId,
    }

    /// A 320x220 group at (100, 100) with one member, plus a node outside it.
    fn group_scene() -> Scene {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let group = s.add_group(pos2(100.0, 100.0), vec2(320.0, 220.0), "group");
        let child = s.add_node(pos2(20.0, 60.0), "child");
        s.node_mut(child).unwrap().parent = Some(group);
        let outside = s.add_node(pos2(560.0, 150.0), "outside");
        for _ in 0..4 {
            run_frame(&ctx, &mut s, vec![], false);
        }
        Scene {
            ctx,
            s,
            group,
            child,
            outside,
        }
    }

    fn centre(s: &FlowState<&'static str, ()>, n: NodeId) -> Pos2 {
        s.abs_rect(n).unwrap().center()
    }

    /// A point on a group's header, clear of its title text and toggle.
    fn header_point(s: &FlowState<&'static str, ()>, g: NodeId) -> Pos2 {
        let r = s.abs_rect(g).unwrap();
        pos2(r.center().x, r.min.y + 15.0)
    }

    fn near(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 2.5
    }

    #[test]
    fn dragging_a_groups_header_moves_its_members_with_it() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            outside,
        } = group_scene();
        let (g0, c0, o0) = (
            s.abs_position(group).unwrap(),
            s.abs_position(child).unwrap(),
            s.abs_position(outside).unwrap(),
        );
        let rel0 = s.node(child).unwrap().position;
        let from = header_point(&s, group);
        drag(&ctx, &mut s, from, from + vec2(60.0, 40.0));
        let d = vec2(60.0, 40.0);
        assert!(
            near(s.abs_position(group).unwrap(), g0 + d),
            "{:?}",
            s.abs_position(group)
        );
        assert!(
            near(s.abs_position(child).unwrap(), c0 + d),
            "member came along"
        );
        assert_eq!(
            s.node(child).unwrap().position,
            rel0,
            "its position inside the group is unchanged"
        );
        assert_eq!(s.abs_position(outside), Some(o0));
        // Groups stay below their members even after being brought to the front.
        let order: Vec<_> = s.nodes.iter().map(|n| n.id).collect();
        assert!(order.iter().position(|n| *n == group) < order.iter().position(|n| *n == child));
    }

    #[test]
    fn dragging_a_member_inside_its_group_moves_only_the_member() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            ..
        } = group_scene();
        let (g0, rel0) = (
            s.abs_position(group).unwrap(),
            s.node(child).unwrap().position,
        );
        let from = centre(&s, child);
        let events = drag(&ctx, &mut s, from, from + vec2(30.0, 20.0));
        assert_eq!(s.abs_position(group), Some(g0));
        let rel = s.node(child).unwrap().position;
        assert!(
            near(
                rel.to_vec2().to_pos2(),
                (rel0 + vec2(30.0, 20.0)).to_vec2().to_pos2()
            ),
            "{rel:?}"
        );
        assert_eq!(s.node(child).unwrap().parent, Some(group));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, FlowEvent::ParentChanged { .. })),
            "still in the group: {events:?}"
        );
    }

    #[test]
    fn dropping_a_node_on_a_group_puts_it_in_the_group() {
        let Scene {
            ctx,
            mut s,
            group,
            outside,
            ..
        } = group_scene();
        let from = centre(&s, outside);
        let target = pos2(300.0, 270.0); // inside the group, away from its member
        let events = drag(&ctx, &mut s, from, target);
        assert!(
            events.iter().any(|e| matches!(e, FlowEvent::ParentChanged { node, parent: Some(g) } if *node == outside && *g == group)),
            "{events:?}"
        );
        assert_eq!(s.node(outside).unwrap().parent, Some(group));
        assert!(
            near(centre(&s, outside), target),
            "it stays where it was dropped: {:?}",
            centre(&s, outside)
        );
        let rel = s.node(outside).unwrap().position;
        assert!(near(
            rel.to_vec2().to_pos2() + s.abs_position(group).unwrap().to_vec2(),
            s.abs_position(outside).unwrap()
        ));
    }

    #[test]
    fn dragging_a_member_out_of_its_group_releases_it() {
        let Scene {
            ctx, mut s, child, ..
        } = group_scene();
        let from = centre(&s, child);
        let target = pos2(700.0, 450.0);
        let events = drag(&ctx, &mut s, from, target);
        assert!(
            events.iter().any(
                |e| matches!(e, FlowEvent::ParentChanged { node, parent: None } if *node == child)
            ),
            "{events:?}"
        );
        assert_eq!(s.node(child).unwrap().parent, None);
        assert!(near(centre(&s, child), target));
    }

    #[test]
    fn group_drop_can_be_turned_off() {
        let Scene {
            ctx,
            mut s,
            outside,
            ..
        } = group_scene();
        let opts = FlowOptions {
            group_drop: false,
            ..Default::default()
        };
        let from = centre(&s, outside);
        let to = pos2(300.0, 270.0);
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame_with(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(from)],
            None,
            opts.clone(),
        );
        run_frame_with(&ctx, &mut s, vec![btn(from, true)], None, opts.clone());
        let mut events = Vec::new();
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            events.extend(run_frame_with(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(p)],
                None,
                opts.clone(),
            ));
        }
        events.extend(run_frame_with(
            &ctx,
            &mut s,
            vec![btn(to, false)],
            None,
            opts,
        ));
        assert_eq!(s.node(outside).unwrap().parent, None);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, FlowEvent::ParentChanged { .. }))
        );
    }

    /// Click the collapse toggle in the top-right of group `g`.
    fn click_toggle(
        ctx: &Context,
        s: &mut FlowState<&'static str, ()>,
        g: NodeId,
    ) -> Vec<FlowEvent<&'static str, ()>> {
        let r = s.abs_rect(g).unwrap();
        let at = pos2(r.max.x - 16.0, r.min.y + 15.0);
        let btn = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame(ctx, s, vec![Event::PointerMoved(at)], false);
        run_frame(ctx, s, vec![btn(true)], false);
        run_frame(ctx, s, vec![btn(false)], false)
    }

    #[test]
    fn the_header_toggle_collapses_a_group_and_hides_its_members() {
        let Scene {
            ctx, mut s, group, ..
        } = group_scene();
        let opts = FlowOptions::default();
        let shown = |s: &mut FlowState<&'static str, ()>| {
            text_shape(&run_full(&ctx, s, vec![], None, opts.clone()).1, "child").is_some()
        };
        assert!(shown(&mut s), "member drawn while the group is open");
        let events = click_toggle(&ctx, &mut s, group);
        assert!(
            events.iter().any(
                |e| matches!(e, FlowEvent::GroupToggled { node, collapsed: true } if *node == group)
            ),
            "{events:?}"
        );
        assert!(s.node(group).unwrap().collapsed);
        run_frame(&ctx, &mut s, vec![], false);
        assert!(!shown(&mut s), "member hidden once collapsed");
        // A collapsed group shrinks to its header.
        assert!(
            s.node(group).unwrap().size.y < 100.0,
            "{:?}",
            s.node(group).unwrap().size
        );

        // And back.
        let events = click_toggle(&ctx, &mut s, group);
        assert!(events.iter().any(|e| matches!(
            e,
            FlowEvent::GroupToggled {
                collapsed: false,
                ..
            }
        )));
        run_frame(&ctx, &mut s, vec![], false);
        assert!(shown(&mut s));
    }

    #[test]
    fn edges_to_hidden_members_attach_to_the_collapsed_group() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            outside,
        } = group_scene();
        let ink = Color32::from_rgb(1, 2, 3);
        let e = s.connect(child, outside, ()).unwrap();
        {
            let edge = s.edge_mut(e).unwrap();
            edge.kind = Some(EdgeKind::Straight);
            edge.color = Some(ink);
            edge.width = Some(3.0);
        }
        let opts = FlowOptions::default();
        let edge_points = |shapes: &[Shape]| {
            shapes.iter().find_map(|sh| match sh {
                Shape::Path(p) if !p.closed && p.points.len() == 2 => match &p.stroke.color {
                    egui::epaint::ColorMode::Solid(c) if *c == ink => {
                        Some((p.points[0], p.points[1]))
                    }
                    _ => None,
                },
                _ => None,
            })
        };
        for _ in 0..3 {
            run_frame_with(&ctx, &mut s, vec![], None, opts.clone());
        }
        s.set_collapsed(group, true);
        let mut shapes = Vec::new();
        for _ in 0..4 {
            shapes = run_full(&ctx, &mut s, vec![], None, opts.clone()).1;
        }
        let (start, end) = edge_points(&shapes).expect("edge still drawn");
        let g = s.abs_rect(group).unwrap();
        assert!(
            near(start, g.right_center()),
            "starts at the group's right side: {start:?} vs {:?}",
            g.right_center()
        );
        let out = s.abs_rect(outside).unwrap();
        assert!(
            near(end, out.left_center()),
            "{end:?} vs {:?}",
            out.left_center()
        );
    }

    #[test]
    fn edges_wholly_inside_a_collapsed_group_are_not_drawn() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            ..
        } = group_scene();
        let other = s.add_node(pos2(180.0, 120.0), "other");
        s.node_mut(other).unwrap().parent = Some(group);
        let ink = Color32::from_rgb(9, 8, 7);
        let e = s.connect(child, other, ()).unwrap();
        s.edge_mut(e).unwrap().color = Some(ink);
        let drawn = |shapes: &[Shape]| {
            shapes.iter().any(|sh| matches!(sh, Shape::Path(p) if matches!(&p.stroke.color, egui::epaint::ColorMode::Solid(c) if *c == ink)))
        };
        let opts = FlowOptions::default();
        let mut shapes = Vec::new();
        for _ in 0..3 {
            shapes = run_full(&ctx, &mut s, vec![], None, opts.clone()).1;
        }
        assert!(drawn(&shapes), "drawn while the group is open");
        s.set_collapsed(group, true);
        for _ in 0..3 {
            shapes = run_full(&ctx, &mut s, vec![], None, opts.clone()).1;
        }
        assert!(!drawn(&shapes), "internal edge hidden when collapsed");
    }

    #[test]
    fn a_box_drawn_inside_a_group_selects_members_not_the_group() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            outside,
        } = group_scene();
        let shift = egui::Modifiers::SHIFT;
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: shift,
        };
        let c = s.abs_rect(child).unwrap();
        let opts = FlowOptions::default();
        let drag_box = |s: &mut FlowState<&'static str, ()>, from: Pos2, to: Pos2| {
            run_full_mods(
                &ctx,
                s,
                vec![Event::PointerMoved(from)],
                None,
                opts.clone(),
                shift,
            );
            run_full_mods(&ctx, s, vec![btn(from, true)], None, opts.clone(), shift);
            for i in 1..=6 {
                let p = from + (to - from) * (i as f32 / 6.0);
                run_full_mods(
                    &ctx,
                    s,
                    vec![Event::PointerMoved(p)],
                    None,
                    opts.clone(),
                    shift,
                );
            }
            run_full_mods(&ctx, s, vec![btn(to, false)], None, opts.clone(), shift);
        };
        // Starts on the group's empty body, which is not grabbable, and covers only the member.
        drag_box(
            &mut s,
            pos2(c.min.x - 10.0, c.min.y - 10.0),
            pos2(c.max.x + 10.0, c.max.y + 10.0),
        );
        assert_eq!(s.selected_nodes(), vec![child]);
        // Covering the whole group (and the node beside it) picks the group too.
        drag_box(&mut s, pos2(90.0, 90.0), pos2(700.0, 340.0));
        let picked: HashSet<_> = s.selected_nodes().into_iter().collect();
        assert!(
            picked.contains(&group) && picked.contains(&child) && picked.contains(&outside),
            "{picked:?}"
        );
    }

    #[test]
    fn a_constrained_member_cannot_be_dragged_out_of_its_group() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            ..
        } = group_scene();
        s.node_mut(child).unwrap().constrain_to_parent = true;
        let from = centre(&s, child);
        let events = drag(&ctx, &mut s, from, pos2(760.0, 520.0)); // far outside
        let (g, c) = (s.abs_rect(group).unwrap(), s.abs_rect(child).unwrap());
        assert!(g.contains_rect(c), "{c:?} escaped {g:?}");
        assert!(
            (c.max.x - g.max.x).abs() < 0.5 && (c.max.y - g.max.y).abs() < 0.5,
            "pushed into the corner: {c:?}"
        );
        assert_eq!(s.node(child).unwrap().parent, Some(group));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, FlowEvent::ParentChanged { .. }))
        );
        // Dragging toward the top stops below the header.
        let from = centre(&s, child);
        drag(&ctx, &mut s, from, pos2(from.x, -200.0));
        let c = s.abs_rect(child).unwrap();
        assert!((c.min.y - (g.min.y + 30.0)).abs() < 0.5, "{c:?}");
    }

    #[test]
    fn arrow_keys_stop_a_constrained_member_at_the_group_edge() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            ..
        } = group_scene();
        s.node_mut(child).unwrap().constrain_to_parent = true;
        s.clear_selection();
        s.node_mut(child).unwrap().selected = true;
        run_frame(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(pos2(650.0, 500.0))],
            false,
        );
        let key = |k| Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        };
        for _ in 0..40 {
            run_frame(&ctx, &mut s, vec![key(egui::Key::ArrowRight)], false);
        }
        let (g, c) = (s.abs_rect(group).unwrap(), s.abs_rect(child).unwrap());
        assert!((c.max.x - g.max.x).abs() < 0.5, "{c:?} vs {g:?}");
    }

    #[test]
    fn nudging_a_group_moves_its_members_too() {
        let Scene {
            ctx,
            mut s,
            group,
            child,
            ..
        } = group_scene();
        let (g0, c0) = (
            s.abs_position(group).unwrap(),
            s.abs_position(child).unwrap(),
        );
        s.clear_selection();
        s.node_mut(group).unwrap().selected = true;
        run_frame(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(pos2(650.0, 500.0))],
            false,
        );
        run_frame(
            &ctx,
            &mut s,
            vec![Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::SHIFT,
            }],
            false,
        );
        assert_eq!(s.abs_position(group), Some(g0 + vec2(10.0, 0.0)));
        assert_eq!(s.abs_position(child), Some(c0 + vec2(10.0, 0.0)));
    }

    // ---- theme ---------------------------------------------------------------

    fn solid(c: &egui::epaint::ColorMode) -> Option<Color32> {
        match c {
            egui::epaint::ColorMode::Solid(c) => Some(*c),
            _ => None,
        }
    }

    /// Render two nodes joined by a straight edge with `theme`, returning the shapes.
    fn themed_pair(theme: crate::FlowTheme, select_a: bool) -> Vec<Shape> {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(60.0, 100.0), "alpha");
        let b = s.add_node(pos2(400.0, 100.0), "beta");
        let e = s.connect(a, b, ()).unwrap();
        s.edge_mut(e).unwrap().kind = Some(EdgeKind::Straight);
        s.edge_mut(e).unwrap().label = Some("lbl".into());
        s.node_mut(a).unwrap().selected = select_a;
        let opts = FlowOptions {
            theme,
            minimap: true,
            controls: false,
            ..Default::default()
        };
        let mut shapes = Vec::new();
        for _ in 0..4 {
            shapes = run_full(&ctx, &mut s, vec![], None, opts.clone()).1;
        }
        shapes
    }

    #[test]
    fn the_default_theme_paints_exactly_what_it_did_before() {
        let plain = themed_pair(crate::FlowTheme::default(), true);
        let ctx = Context::default();
        let visuals = ctx.style().visuals.clone();
        // Canvas fill is egui's extreme background, grid dots are the noninteractive stroke.
        assert!(
            plain.iter().any(|sh| matches!(sh, Shape::Rect(r) if r.fill == visuals.extreme_bg_color && r.rect.width() >= 799.0)),
            "canvas fill"
        );
        assert!(
            plain.iter().any(|sh| matches!(sh, Shape::Circle(c) if c.fill == visuals.widgets.noninteractive.bg_stroke.color)),
            "grid dots"
        );
        assert!(
            plain
                .iter()
                .any(|sh| matches!(sh, Shape::Rect(r) if r.fill == visuals.window_fill)),
            "node frame fill"
        );
    }

    #[test]
    fn every_theme_colour_reaches_the_canvas() {
        let theme = crate::FlowTheme {
            background: Some(Color32::from_rgb(1, 1, 1)),
            grid: Some(Color32::from_rgb(2, 2, 2)),
            edge: Some(Color32::from_rgb(3, 3, 3)),
            selection: Some(Color32::from_rgb(4, 4, 4)),
            handle: Some(Color32::from_rgb(5, 5, 5)),
            label_background: Some(Color32::from_rgb(6, 6, 6)),
            minimap_background: Some(Color32::from_rgb(7, 7, 7)),
            node_fill: Some(Color32::from_rgb(8, 8, 8)),
            node_stroke: Some(Color32::from_rgb(9, 9, 9)),
            text: Some(Color32::from_rgb(10, 10, 10)),
            ..Default::default()
        };
        let shapes = themed_pair(theme, true);
        let c = |v: u8| Color32::from_rgb(v, v, v);
        let has = |f: &dyn Fn(&Shape) -> bool| shapes.iter().any(f);

        assert!(
            has(&|s| matches!(s, Shape::Rect(r) if r.fill == c(1) && r.rect.width() >= 799.0)),
            "background"
        );
        assert!(
            has(&|s| matches!(s, Shape::Circle(ci) if ci.fill == c(2))),
            "grid dots"
        );
        assert!(
            has(
                &|s| matches!(s, Shape::Path(p) if !p.closed && p.points.len() == 2 && solid(&p.stroke.color) == Some(c(3)))
            ),
            "edge"
        );
        assert!(
            has(
                &|s| matches!(s, Shape::Rect(r) if r.stroke.color == c(4) && r.stroke.width >= 2.0)
            ),
            "selection outline"
        );
        assert!(
            has(&|s| matches!(s, Shape::Circle(ci) if ci.fill == c(5))),
            "idle handle"
        );
        assert!(
            has(&|s| matches!(s, Shape::Rect(r) if r.fill == c(6))),
            "edge label background"
        );
        assert!(
            has(
                &|s| matches!(s, Shape::Rect(r) if (r.rect.size() - vec2(160.0, 110.0)).length() < 0.5
                && r.fill.to_srgba_unmultiplied()[..3].iter().all(|v| v.abs_diff(7) <= 1))
            ),
            "minimap panel (it fades in, so compare the colour without alpha)"
        );
        assert!(
            has(&|s| matches!(s, Shape::Rect(r) if r.fill == c(8))),
            "node fill"
        );
        assert!(
            has(&|s| matches!(s, Shape::Rect(r) if r.stroke.color == c(9))),
            "node outline"
        );
        let alpha = text_shape(&shapes, "alpha").expect("node text");
        // A plain label leaves its colour to the text shape's fallback.
        assert_eq!(alpha.fallback_color, c(10), "node text");
    }

    #[test]
    fn the_guide_colour_is_themeable() {
        let theme = crate::FlowTheme {
            guide: Some(Color32::from_rgb(11, 12, 13)),
            ..Default::default()
        };
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 250.0), "b");
        let opts = FlowOptions {
            alignment_guides: true,
            theme,
            ..Default::default()
        };
        for _ in 0..3 {
            run_frame_with(&ctx, &mut s, vec![], None, opts.clone());
        }
        let top = s.node(a).unwrap().position.y;
        let grab = s.node(b).unwrap().rect().center();
        let near = grab + vec2(-20.0, top + 4.0 - 250.0);
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame_with(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(grab)],
            None,
            opts.clone(),
        );
        run_frame_with(&ctx, &mut s, vec![btn(grab, true)], None, opts.clone());
        let mut shapes = Vec::new();
        for i in 1..=6 {
            let p = grab + (near - grab) * (i as f32 / 6.0);
            shapes = run_full(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(p)],
                None,
                opts.clone(),
            )
            .1;
        }
        assert!(shapes.iter().any(|sh| matches!(sh, Shape::LineSegment { stroke, .. } if stroke.color == Color32::from_rgb(11, 12, 13))));
        assert!(!shapes.iter().any(|sh| matches!(sh, Shape::LineSegment { stroke, .. } if stroke.color == Color32::from_rgb(255, 90, 160))), "the default pink is gone");
    }

    #[test]
    fn preset_themes_render_with_their_background() {
        for t in [
            crate::FlowTheme::dark(),
            crate::FlowTheme::light(),
            crate::FlowTheme::blueprint(),
        ] {
            let shapes = themed_pair(t, false);
            assert!(shapes.iter().any(|sh| matches!(sh, Shape::Rect(r) if Some(r.fill) == t.background && r.rect.width() >= 799.0)));
        }
    }

    #[test]
    fn dropping_an_edge_end_on_nothing_snaps_back() {
        let (ctx, mut s, (_a, b, _c), e) = reconnect_setup();
        let from = target_pos(&s, b);
        let events = drag(&ctx, &mut s, from, pos2(650.0, 520.0));
        assert_eq!(s.edge(e).unwrap().target, b, "{events:?}");
        assert!(!events.iter().any(|ev| matches!(
            ev,
            FlowEvent::Reconnected { .. } | FlowEvent::ConnectionDropped { .. }
        )));
    }

    #[test]
    fn pulse_arrival_is_reported_with_its_tag() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(0.0, 0.0), "a");
        let b = s.add_node(pos2(300.0, 100.0), "b");
        let e = s.connect(a, b, ()).unwrap();
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), true);
        s.pulse_edge_reverse(
            e,
            PulseStyle {
                duration: 0.5,
                tag: 42,
                shape: crate::PulseShape::Arrow,
                ..Default::default()
            },
        );
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.0), true);
        let mid = run_frame_at(&ctx, &mut s, vec![], false, Some(1.25), true);
        assert!(
            !mid.iter()
                .any(|ev| matches!(ev, FlowEvent::PulseArrived { .. }))
        );
        let end = run_frame_at(&ctx, &mut s, vec![], false, Some(2.0), true);
        assert!(end.iter().any(|ev| matches!(
            ev,
            FlowEvent::PulseArrived { edge, tag: 42, direction: PulseDirection::Reverse }
                if *edge == e
        )));
    }

    #[test]
    fn viewport_transition_eases_then_lands_on_target() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        s.add_node(pos2(0.0, 0.0), "a");
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), true);
        let to = Viewport {
            pan: vec2(200.0, 100.0),
            zoom: 2.0,
        };
        s.animate_viewport(to, 0.4);
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.0), true);
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.2), true);
        assert!(
            s.viewport.zoom > 1.0 && s.viewport.zoom < 2.0,
            "{:?}",
            s.viewport
        );
        run_frame_at(&ctx, &mut s, vec![], false, Some(2.0), true);
        assert_eq!(s.viewport, to);
        assert!(s.view_anim.is_none());
    }

    #[test]
    fn viewport_transition_jumps_when_animation_is_off() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        s.add_node(pos2(0.0, 0.0), "a");
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), false);
        let to = Viewport {
            pan: vec2(50.0, 50.0),
            zoom: 1.5,
        };
        s.animate_viewport(to, 5.0);
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.1), false);
        assert_eq!(s.viewport, to);
    }

    #[test]
    fn pulses_travel_then_expire() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(0.0, 0.0), "a");
        let b = s.add_node(pos2(300.0, 100.0), "b");
        let e = s.connect(a, b, ()).unwrap();
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), true);
        assert!(s.pulse_edge(
            e,
            PulseStyle {
                duration: 0.5,
                ..Default::default()
            }
        ));
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.0), true);
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.25), true);
        assert_eq!(s.pulses.len(), 1, "still in flight mid-way");
        run_frame_at(&ctx, &mut s, vec![], false, Some(2.0), true);
        assert!(s.pulses.is_empty(), "expired after its duration");
    }

    #[test]
    fn delayed_pulse_waits_before_expiring() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(0.0, 0.0), "a");
        let b = s.add_node(pos2(300.0, 100.0), "b");
        let e = s.connect(a, b, ()).unwrap();
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), true);
        s.pulse_edge_reverse(
            e,
            PulseStyle {
                duration: 0.5,
                delay: 1.0,
                ..Default::default()
            },
        );
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.0), true);
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.8), true);
        assert_eq!(s.pulses.len(), 1, "still waiting out its delay");
        run_frame_at(&ctx, &mut s, vec![], false, Some(2.6), true);
        assert!(s.pulses.is_empty());
    }

    #[test]
    fn pulse_on_removed_edge_is_dropped() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(0.0, 0.0), "a");
        let b = s.add_node(pos2(300.0, 100.0), "b");
        let e = s.connect(a, b, ()).unwrap();
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), true);
        s.pulse_edge(e, PulseStyle::default());
        s.remove_edge(e);
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.1), true);
        assert!(s.pulses.is_empty());
    }

    #[test]
    fn nodes_added_later_fade_in_but_initial_ones_do_not() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        s.add_node(pos2(0.0, 0.0), "first");
        run_frame_at(&ctx, &mut s, vec![], false, Some(0.0), true);
        assert!(s.appear.is_empty(), "initial nodes appear instantly");
        let late = s.add_node(pos2(100.0, 0.0), "late");
        run_frame_at(&ctx, &mut s, vec![], false, Some(1.0), true);
        assert!(s.appear.contains_key(&late));
        run_frame_at(&ctx, &mut s, vec![], false, Some(2.0), true);
        assert!(s.appear.is_empty(), "fade finished");
    }

    fn run_frame_opts(
        ctx: &Context,
        state: &mut FlowState<&'static str, ()>,
        events: Vec<Event>,
        opts: FlowOptions,
        time: Option<f64>,
    ) -> Vec<FlowEvent<&'static str, ()>> {
        run_frame_with(ctx, state, events, time, opts)
    }

    /// Drag from `from` to `to` with the primary button, one frame per step.
    fn drag_between(
        ctx: &Context,
        s: &mut FlowState<&'static str, ()>,
        opts: &FlowOptions,
        from: Pos2,
        to: Pos2,
    ) {
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let mut frame = |events| run_frame_opts(ctx, s, events, opts.clone(), None);
        frame(vec![Event::PointerMoved(from)]);
        frame(vec![btn(from, true)]);
        for i in 1..=6 {
            frame(vec![Event::PointerMoved(
                from + (to - from) * (i as f32 / 6.0),
            )]);
        }
        frame(vec![btn(to, false)]);
        frame(vec![]);
    }

    fn handle_pos(s: &FlowState<&'static str, ()>, node: NodeId, h: Handle) -> Pos2 {
        h.position(s.node(node).unwrap().rect())
    }

    #[test]
    fn one_handle_can_have_many_connections() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 50.0), "b");
        let c = s.add_node(pos2(400.0, 300.0), "c");
        let opts = FlowOptions::default();
        for _ in 0..3 {
            run_frame_opts(&ctx, &mut s, vec![], opts.clone(), None);
        }
        let src = Handle::source(Handle::DEFAULT_SOURCE, Side::Right);
        let tgt = Handle::target(Handle::DEFAULT_TARGET, Side::Left);
        for to in [b, c] {
            let (from_p, to_p) = (handle_pos(&s, a, src), handle_pos(&s, to, tgt));
            drag_between(&ctx, &mut s, &opts, from_p, to_p);
        }
        assert_eq!(s.edges.len(), 2, "both wires leave the same source handle");
        // A second wire between the same pair is rejected as a duplicate.
        let (from_p, to_p) = (handle_pos(&s, a, src), handle_pos(&s, b, tgt));
        drag_between(&ctx, &mut s, &opts, from_p, to_p);
        assert_eq!(s.edges.len(), 2);
    }

    #[test]
    fn many_wires_can_share_one_target_handle() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 50.0), "a");
        let b = s.add_node(pos2(50.0, 300.0), "b");
        let c = s.add_node(pos2(450.0, 150.0), "c");
        let opts = FlowOptions::default();
        for _ in 0..3 {
            run_frame_opts(&ctx, &mut s, vec![], opts.clone(), None);
        }
        let src = Handle::source(Handle::DEFAULT_SOURCE, Side::Right);
        let tgt = Handle::target(Handle::DEFAULT_TARGET, Side::Left);
        for from in [a, b] {
            let (from_p, to_p) = (handle_pos(&s, from, src), handle_pos(&s, c, tgt));
            drag_between(&ctx, &mut s, &opts, from_p, to_p);
        }
        assert_eq!(s.edges.len(), 2);
        assert!(s.edges.iter().all(|e| e.target == c));
    }

    #[test]
    fn hidden_handles_cannot_start_a_connection() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 100.0), "b");
        let opts = FlowOptions {
            handle_visibility: HandleVisibility::Hidden,
            ..Default::default()
        };
        for _ in 0..3 {
            run_frame_opts(&ctx, &mut s, vec![], opts.clone(), None);
        }
        let src = Handle::source(Handle::DEFAULT_SOURCE, Side::Right);
        let tgt = Handle::target(Handle::DEFAULT_TARGET, Side::Left);
        let (from_p, to_p) = (handle_pos(&s, a, src), handle_pos(&s, b, tgt));
        drag_between(&ctx, &mut s, &opts, from_p, to_p);
        assert!(s.edges.is_empty());
        // Code can still create the edge, and it still renders without panicking.
        s.connect(a, b, ());
        run_frame_opts(&ctx, &mut s, vec![], opts, None);
        assert_eq!(s.edges.len(), 1);
    }

    #[test]
    fn on_hover_handles_still_connect() {
        let ctx = Context::default();
        let mut s = FlowState::new();
        let a = s.add_node(pos2(50.0, 100.0), "a");
        let b = s.add_node(pos2(400.0, 100.0), "b");
        let opts = FlowOptions {
            handle_visibility: HandleVisibility::OnHover,
            animate: false,
            ..Default::default()
        };
        for _ in 0..3 {
            run_frame_opts(&ctx, &mut s, vec![], opts.clone(), None);
        }
        let src = Handle::source(Handle::DEFAULT_SOURCE, Side::Right);
        let tgt = Handle::target(Handle::DEFAULT_TARGET, Side::Left);
        let (from_p, to_p) = (handle_pos(&s, a, src), handle_pos(&s, b, tgt));
        drag_between(&ctx, &mut s, &opts, from_p, to_p);
        assert_eq!(s.edges.len(), 1);
    }
}
