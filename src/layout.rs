//! Automatic layout: arrange nodes in layers along the direction of their edges.
//!
//! This is a compact layered ("Sugiyama-style") layout:
//!
//! 1. edges that close a cycle are reversed, so the graph is acyclic;
//! 2. every node goes in the layer after its latest predecessor (longest path);
//! 3. the order inside each layer is improved by barycenter sweeps, to cut crossings;
//! 4. layers are placed one after another along the main axis, and the nodes in a
//!    layer are pulled toward the centre of their neighbours without overlapping;
//! 5. parts of the graph that are not connected are stacked one after another.
//!
//! Groups are laid out as one block of their current size, with their members
//! keeping their positions inside. Lay out the inside of a group with
//! [`LayoutOptions::scope`].

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Vec2, pos2, vec2};

use crate::state::FlowState;
use crate::types::NodeId;

/// The way edges point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    /// Sources on the left, targets to the right.
    #[default]
    LeftToRight,
    /// Sources at the top, targets below.
    TopToBottom,
}

/// How [`FlowState::auto_layout`] arranges nodes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutOptions {
    pub direction: LayoutDirection,
    /// Space between one layer and the next.
    pub layer_gap: f32,
    /// Space between nodes in the same layer.
    pub node_gap: f32,
    /// Space between parts of the graph that are not connected.
    pub component_gap: f32,
    /// `None` lays out the top-level nodes; `Some(group)` lays out the members of that
    /// group, then resizes the group to fit them.
    pub scope: Option<NodeId>,
    /// Keep the top-left corner of the result where the current top-left corner is
    /// (top level only). Off, the layout starts at the origin.
    pub keep_origin: bool,
    /// Padding inside a group laid out with `scope`.
    pub group_padding: f32,
    /// Header height of a group laid out with `scope`.
    pub group_header: f32,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::LeftToRight,
            layer_gap: 80.0,
            node_gap: 40.0,
            component_gap: 80.0,
            scope: None,
            keep_origin: true,
            group_padding: 24.0,
            group_header: 30.0,
        }
    }
}

/// A layout that is being animated toward its result.
pub(crate) struct LayoutAnim {
    from: Vec<(NodeId, Pos2)>,
    to: Vec<(NodeId, Pos2)>,
    duration: f32,
    start: Option<f64>,
    refit: Option<(NodeId, f32, f32)>,
}

// ---- the algorithm, on plain indices -------------------------------------------

/// Reverse the edges that close a cycle (found by depth-first search), dropping
/// self-loops and duplicates. The result is acyclic.
fn break_cycles(n: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut succ = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b {
            succ[a].push(b);
        }
    }
    let mut colour = vec![0u8; n]; // 0 new, 1 on the stack, 2 done
    let mut out: HashSet<(usize, usize)> = HashSet::new();
    for root in 0..n {
        if colour[root] != 0 {
            continue;
        }
        colour[root] = 1;
        let mut stack = vec![(root, 0usize)];
        while let Some(&mut (u, ref mut next)) = stack.last_mut() {
            if let Some(&v) = succ[u].get(*next) {
                *next += 1;
                match colour[v] {
                    1 => {
                        out.insert((v, u)); // a back edge: point it the other way
                    }
                    c => {
                        out.insert((u, v));
                        if c == 0 {
                            colour[v] = 1;
                            stack.push((v, 0));
                        }
                    }
                }
            } else {
                colour[u] = 2;
                stack.pop();
            }
        }
    }
    let mut out: Vec<_> = out.into_iter().collect();
    out.sort_unstable();
    out
}

/// Layer of every node: one more than its latest predecessor (`dag` must be acyclic).
fn layers_of(n: usize, dag: &[(usize, usize)]) -> Vec<usize> {
    let mut indeg = vec![0usize; n];
    let mut succ = vec![Vec::new(); n];
    for &(a, b) in dag {
        indeg[b] += 1;
        succ[a].push(b);
    }
    let mut layer = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        for &v in &succ[u] {
            layer[v] = layer[v].max(layer[u] + 1);
            indeg[v] -= 1;
            if indeg[v] == 0 {
                queue.push(v);
            }
        }
    }
    layer
}

/// Reorder the nodes inside each layer to cut edge crossings (barycenter sweeps).
fn reduce_crossings(layers: &mut [Vec<usize>], dag: &[(usize, usize)], n: usize) {
    let mut preds = vec![Vec::new(); n];
    let mut succs = vec![Vec::new(); n];
    for &(a, b) in dag {
        preds[b].push(a);
        succs[a].push(b);
    }
    let mut layer_of = vec![0usize; n];
    for (l, layer) in layers.iter().enumerate() {
        for &v in layer {
            layer_of[v] = l;
        }
    }
    // Where a node sits in its layer, as a fraction, so nodes in different layers compare.
    let frac = |layers: &[Vec<usize>], v: usize| {
        let layer = &layers[layer_of[v]];
        let at = layer.iter().position(|&x| x == v).unwrap_or(0);
        (at as f32 + 0.5) / layer.len() as f32
    };
    let sweep = |layers: &mut [Vec<usize>], l: usize, neighbours: &[Vec<usize>]| {
        let key: HashMap<usize, f32> = layers[l]
            .iter()
            .map(|&v| {
                let ns = &neighbours[v];
                let k = if ns.is_empty() {
                    frac(layers, v)
                } else {
                    ns.iter().map(|&u| frac(layers, u)).sum::<f32>() / ns.len() as f32
                };
                (v, k)
            })
            .collect();
        let at: HashMap<usize, usize> =
            layers[l].iter().enumerate().map(|(i, &v)| (v, i)).collect();
        layers[l].sort_by(|a, b| key[a].total_cmp(&key[b]).then(at[a].cmp(&at[b])));
    };
    for _ in 0..4 {
        for l in 1..layers.len() {
            sweep(layers, l, &preds);
        }
        for l in (0..layers.len().saturating_sub(1)).rev() {
            sweep(layers, l, &succs);
        }
    }
}

/// Centres for the nodes of one layer, in `order`, as close to `desired` as they can be
/// without overlapping or changing order (isotonic regression by pooling adjacent
/// violators).
fn place_layer(order: &[usize], extent: &[f32], desired: &[f32], gap: f32) -> Vec<f32> {
    let m = order.len();
    let mut offset = vec![0.0f32; m];
    for i in 1..m {
        offset[i] = offset[i - 1] + (extent[order[i - 1]] + extent[order[i]]) / 2.0 + gap;
    }
    let mut blocks: Vec<(f32, usize)> = Vec::new(); // (sum, count)
    for i in 0..m {
        blocks.push((desired[i] - offset[i], 1));
        while blocks.len() >= 2 {
            let (a, b) = (blocks[blocks.len() - 2], blocks[blocks.len() - 1]);
            if a.0 / a.1 as f32 > b.0 / b.1 as f32 {
                blocks.truncate(blocks.len() - 2);
                blocks.push((a.0 + b.0, a.1 + b.1));
            } else {
                break;
            }
        }
    }
    let mut out = Vec::with_capacity(m);
    for (sum, count) in blocks {
        out.extend(std::iter::repeat_n(sum / count as f32, count));
    }
    out.iter().zip(&offset).map(|(z, o)| z + o).collect()
}

/// Lay out one connected part. `main` and `cross` are the node sizes along and across
/// the layer direction. Returns each node's start along the main axis and across it,
/// and the extents of the result.
fn layout_component(
    main: &[f32],
    cross: &[f32],
    edges: &[(usize, usize)],
    layer_gap: f32,
    node_gap: f32,
) -> (Vec<f32>, Vec<f32>, f32, f32) {
    let n = main.len();
    let dag = break_cycles(n, edges);
    let layer = layers_of(n, &dag);
    let depth = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for v in 0..n {
        layers[layer[v]].push(v);
    }
    reduce_crossings(&mut layers, &dag, n);

    // Along the main axis: layers one after another, nodes centred in their layer.
    let layer_main: Vec<f32> = layers
        .iter()
        .map(|l| l.iter().map(|&v| main[v]).fold(0.0, f32::max))
        .collect();
    let mut layer_start = vec![0.0f32; depth];
    for l in 1..depth {
        layer_start[l] = layer_start[l - 1] + layer_main[l - 1] + layer_gap;
    }
    let main_extent = layer_start
        .last()
        .zip(layer_main.last())
        .map_or(0.0, |(s, m)| s + m);
    let main_start: Vec<f32> = (0..n)
        .map(|v| layer_start[layer[v]] + (layer_main[layer[v]] - main[v]) / 2.0)
        .collect();

    // Across it: stack each layer centred, then pull nodes toward their neighbours.
    let mut centre = vec![0.0f32; n];
    for l in &layers {
        let total: f32 =
            l.iter().map(|&v| cross[v]).sum::<f32>() + node_gap * l.len().saturating_sub(1) as f32;
        let mut y = -total / 2.0;
        for &v in l {
            centre[v] = y + cross[v] / 2.0;
            y += cross[v] + node_gap;
        }
    }
    let mut preds = vec![Vec::new(); n];
    let mut succs = vec![Vec::new(); n];
    for &(a, b) in &dag {
        preds[b].push(a);
        succs[a].push(b);
    }
    for pass in 0..8 {
        let down = pass % 2 == 0;
        let order: Vec<usize> = if down {
            (1..depth).collect()
        } else {
            (0..depth.saturating_sub(1)).rev().collect()
        };
        for l in order {
            let neighbours = if down { &preds } else { &succs };
            let desired: Vec<f32> = layers[l]
                .iter()
                .map(|&v| {
                    let ns = &neighbours[v];
                    if ns.is_empty() {
                        centre[v]
                    } else {
                        ns.iter().map(|&u| centre[u]).sum::<f32>() / ns.len() as f32
                    }
                })
                .collect();
            let placed = place_layer(&layers[l], cross, &desired, node_gap);
            for (&v, c) in layers[l].iter().zip(placed) {
                centre[v] = c;
            }
        }
    }
    let top = (0..n)
        .map(|v| centre[v] - cross[v] / 2.0)
        .fold(f32::INFINITY, f32::min);
    let cross_start: Vec<f32> = (0..n).map(|v| centre[v] - cross[v] / 2.0 - top).collect();
    let cross_extent = (0..n)
        .map(|v| cross_start[v] + cross[v])
        .fold(0.0, f32::max);
    (main_start, cross_start, main_extent, cross_extent)
}

// ---- on the graph --------------------------------------------------------------

impl<N, E> FlowState<N, E> {
    /// Where [`auto_layout`](Self::auto_layout) would put each node, without moving
    /// anything: `(node, position)` in the coordinates of `options.scope` (flow space for
    /// the top level, relative to the group otherwise). Uses the nodes' last measured
    /// sizes, so lay out after the nodes have been shown once.
    pub fn layout_positions(&self, options: &LayoutOptions) -> Vec<(NodeId, Pos2)> {
        let hidden = self.hidden_nodes();
        let blocks: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.parent == options.scope && !hidden.contains(&n.id))
            .map(|n| n.id)
            .collect();
        if blocks.is_empty() {
            return Vec::new();
        }
        let index: HashMap<NodeId, usize> =
            blocks.iter().enumerate().map(|(i, id)| (*id, i)).collect();

        // Every node belongs to the block it is, or is inside.
        let by_id: HashMap<NodeId, &crate::types::Node<N>> =
            self.nodes.iter().map(|n| (n.id, n)).collect();
        let mut block_of: HashMap<NodeId, usize> = HashMap::new();
        for n in &self.nodes {
            let mut cur = Some(n.id);
            for _ in 0..64 {
                let Some(c) = cur else { break };
                if let Some(&b) = index.get(&c) {
                    block_of.insert(n.id, b);
                    break;
                }
                cur = by_id.get(&c).and_then(|x| x.parent);
            }
        }
        let size = |i: usize| -> Vec2 {
            let n = by_id[&blocks[i]];
            let s = if n.collapsed {
                n.size
            } else {
                n.fixed_size.unwrap_or(n.size)
            };
            vec2(s.x.max(1.0), s.y.max(1.0))
        };

        let mut seen = HashSet::new();
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for e in &self.edges {
            if let (Some(&a), Some(&b)) = (block_of.get(&e.source), block_of.get(&e.target))
                && a != b
                && seen.insert((a, b))
            {
                edges.push((a, b));
            }
        }

        // Connected parts, in the order of their first node.
        let mut parent: Vec<usize> = (0..blocks.len()).collect();
        fn find(p: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while p[r] != r {
                r = p[r];
            }
            let mut c = x;
            while p[c] != r {
                let next = p[c];
                p[c] = r;
                c = next;
            }
            r
        }
        for &(a, b) in &edges {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[rb.max(ra)] = rb.min(ra);
            }
        }
        let mut comps: Vec<Vec<usize>> = Vec::new();
        let mut comp_of: HashMap<usize, usize> = HashMap::new();
        for i in 0..blocks.len() {
            let root = find(&mut parent, i);
            let c = *comp_of.entry(root).or_insert_with(|| {
                comps.push(Vec::new());
                comps.len() - 1
            });
            comps[c].push(i);
        }

        let horizontal = options.direction == LayoutDirection::LeftToRight;
        let base = if options.scope.is_some() {
            pos2(options.group_padding, options.group_header)
        } else if options.keep_origin {
            let (mut x, mut y) = (f32::INFINITY, f32::INFINITY);
            for id in &blocks {
                let p = by_id[id].position;
                x = x.min(p.x);
                y = y.min(p.y);
            }
            pos2(x, y)
        } else {
            Pos2::ZERO
        };

        let mut out = Vec::with_capacity(blocks.len());
        let mut cursor = 0.0f32; // along the cross axis
        for comp in &comps {
            let local: HashMap<usize, usize> =
                comp.iter().enumerate().map(|(i, &g)| (g, i)).collect();
            let (mut main, mut cross) = (Vec::new(), Vec::new());
            for &g in comp {
                let s = size(g);
                let (m, c) = if horizontal { (s.x, s.y) } else { (s.y, s.x) };
                main.push(m);
                cross.push(c);
            }
            let local_edges: Vec<(usize, usize)> = edges
                .iter()
                .filter_map(|(a, b)| Some((*local.get(a)?, *local.get(b)?)))
                .collect();
            let (m_start, c_start, _m_ext, c_ext) = layout_component(
                &main,
                &cross,
                &local_edges,
                options.layer_gap,
                options.node_gap,
            );
            for (i, &g) in comp.iter().enumerate() {
                let c = cursor + c_start[i];
                let p = if horizontal {
                    vec2(m_start[i], c)
                } else {
                    vec2(c, m_start[i])
                };
                out.push((blocks[g], base + p));
            }
            cursor += c_ext + options.component_gap;
        }
        out
    }

    /// Arrange the nodes in layers along the direction of their edges (see the
    /// [`layout` module](crate::layout) for how). Moves everything at once; use
    /// [`auto_layout_animated`](Self::auto_layout_animated) to glide there. Returns how
    /// many nodes were placed. With [`LayoutOptions::scope`] set, the group is resized to
    /// fit its members afterwards.
    pub fn auto_layout(&mut self, options: &LayoutOptions) -> usize {
        self.layout_anim = None;
        let plan = self.layout_positions(options);
        for (id, pos) in &plan {
            if let Some(n) = self.node_mut(*id) {
                n.position = *pos;
            }
        }
        if let Some(group) = options.scope {
            self.fit_group(group, options.group_padding, options.group_header);
        }
        plan.len()
    }

    /// Like [`auto_layout`](Self::auto_layout), but the nodes glide to their places over
    /// `seconds` while the canvas runs. [`FlowEvent::LayoutFinished`](crate::FlowEvent::LayoutFinished)
    /// is sent when they arrive. Starting to drag a node cancels it. Returns how many nodes
    /// will move.
    pub fn auto_layout_animated(&mut self, options: &LayoutOptions, seconds: f32) -> usize {
        let to = self.layout_positions(options);
        let from: Vec<(NodeId, Pos2)> = to
            .iter()
            .filter_map(|(id, _)| self.node(*id).map(|n| (*id, n.position)))
            .collect();
        let count = to.len();
        self.layout_anim = Some(LayoutAnim {
            from,
            to,
            duration: seconds.max(0.0),
            start: None,
            refit: options
                .scope
                .map(|g| (g, options.group_padding, options.group_header)),
        });
        count
    }

    /// Advance a running animated layout. Returns `true` on the frame it finishes.
    pub(crate) fn step_layout(&mut self, now: f64) -> bool {
        if self.interaction.node_drag.is_some() {
            self.layout_anim = None;
        }
        let Some(anim) = &mut self.layout_anim else {
            return false;
        };
        let start = *anim.start.get_or_insert(now);
        let t = if anim.duration <= 0.0 {
            1.0
        } else {
            ((now - start) / anim.duration as f64).clamp(0.0, 1.0) as f32
        };
        let eased = t * t * (3.0 - 2.0 * t);
        let moves: Vec<(NodeId, Pos2)> = anim
            .from
            .iter()
            .zip(&anim.to)
            .map(|((id, a), (_, b))| (*id, a.lerp(*b, eased)))
            .collect();
        let finished = t >= 1.0;
        let refit = if finished { anim.refit } else { None };
        if finished {
            self.layout_anim = None;
        }
        for (id, p) in moves {
            if let Some(n) = self.node_mut(id) {
                n.position = p;
            }
        }
        if let Some((g, padding, header)) = refit {
            self.fit_group(g, padding, header);
        }
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type S = FlowState<&'static str, ()>;

    /// Nodes of the given sizes, "measured" as if already shown.
    fn graph(sizes: &[(f32, f32)], edges: &[(usize, usize)]) -> (S, Vec<NodeId>) {
        let mut s: S = FlowState::new();
        let ids: Vec<NodeId> = sizes
            .iter()
            .map(|&(w, h)| {
                let id = s.add_node(pos2(0.0, 0.0), "n");
                s.node_mut(id).unwrap().size = vec2(w, h);
                id
            })
            .collect();
        for &(a, b) in edges {
            s.connect(ids[a], ids[b], ());
        }
        (s, ids)
    }

    fn rect(s: &S, id: NodeId) -> egui::Rect {
        s.node(id).unwrap().rect()
    }

    fn assert_no_overlaps(s: &S, ids: &[NodeId]) {
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                assert!(
                    !rect(s, *a).intersects(rect(s, *b)),
                    "{a:?} overlaps {b:?}: {:?} {:?}",
                    rect(s, *a),
                    rect(s, *b)
                );
            }
        }
    }

    #[test]
    fn a_chain_runs_left_to_right_on_one_line() {
        let (mut s, n) = graph(&[(100.0, 40.0); 4], &[(0, 1), (1, 2), (2, 3)]);
        assert_eq!(s.auto_layout(&LayoutOptions::default()), 4);
        for w in n.windows(2) {
            let (a, b) = (rect(&s, w[0]), rect(&s, w[1]));
            assert!(b.min.x >= a.max.x + 79.9, "layer gap kept: {a:?} {b:?}");
            assert!((a.center().y - b.center().y).abs() < 0.01, "same line");
        }
        assert_eq!(
            rect(&s, n[0]).min,
            pos2(0.0, 0.0),
            "the top-left stays where it was"
        );
    }

    #[test]
    fn top_to_bottom_runs_down() {
        let (mut s, n) = graph(&[(100.0, 40.0); 3], &[(0, 1), (1, 2)]);
        s.auto_layout(&LayoutOptions {
            direction: LayoutDirection::TopToBottom,
            ..Default::default()
        });
        for w in n.windows(2) {
            let (a, b) = (rect(&s, w[0]), rect(&s, w[1]));
            assert!(b.min.y >= a.max.y + 79.9);
            assert!((a.center().x - b.center().x).abs() < 0.01);
        }
    }

    #[test]
    fn a_diamond_has_its_middle_nodes_side_by_side_and_the_ends_centred() {
        let (mut s, n) = graph(&[(80.0, 40.0); 4], &[(0, 1), (0, 2), (1, 3), (2, 3)]);
        s.auto_layout(&LayoutOptions::default());
        let r: Vec<_> = n.iter().map(|id| rect(&s, *id)).collect();
        assert!(
            r[1].min.x == r[2].min.x && r[1].min.x > r[0].max.x,
            "B and C share a layer"
        );
        assert!(
            r[1].max.y + 39.9 <= r[2].min.y || r[2].max.y + 39.9 <= r[1].min.y,
            "node gap kept"
        );
        let mid = (r[1].center().y + r[2].center().y) / 2.0;
        assert!(
            (r[0].center().y - mid).abs() < 0.5 && (r[3].center().y - mid).abs() < 0.5,
            "ends centred between them"
        );
        assert_no_overlaps(&s, &n);
    }

    #[test]
    fn cycles_and_self_loops_do_not_hang_and_still_lay_out() {
        let (mut s, n) = graph(
            &[(60.0, 30.0); 4],
            &[(0, 1), (1, 2), (2, 0), (3, 3), (2, 3)],
        );
        assert_eq!(s.auto_layout(&LayoutOptions::default()), 4);
        assert_no_overlaps(&s, &n);
        // Still a left-to-right flow: the reversed back edge is the only one going left.
        let xs: Vec<f32> = n.iter().map(|id| rect(&s, *id).min.x).collect();
        assert!(xs[0] < xs[1] && xs[1] < xs[2], "{xs:?}");
    }

    #[test]
    fn break_cycles_always_gives_an_acyclic_graph() {
        let cases: [&[(usize, usize)]; 4] = [
            &[(0, 1), (1, 0)],
            &[(0, 1), (1, 2), (2, 0)],
            &[(0, 1), (1, 2), (2, 3), (3, 1), (3, 0), (0, 0)],
            &[(0, 1), (0, 2), (1, 3), (2, 3), (3, 4), (4, 2)],
        ];
        for edges in cases {
            let n = 5;
            let dag = break_cycles(n, edges);
            // Kahn's algorithm consumes every node of an acyclic graph.
            let layer = layers_of(n, &dag);
            for &(a, b) in &dag {
                assert!(layer[a] < layer[b], "{edges:?} -> {dag:?}");
            }
            assert!(dag.iter().all(|(a, b)| a != b));
        }
    }

    #[test]
    fn ordering_removes_a_crossing_that_creation_order_would_cause() {
        // A above B; edges A->D and B->C. Created order C, D would cross them.
        let (mut s, n) = graph(&[(80.0, 40.0); 4], &[(0, 3), (1, 2)]);
        let (a, b, c, d) = (n[0], n[1], n[2], n[3]);
        s.auto_layout(&LayoutOptions::default());
        let y = |id| rect(&s, id).center().y;
        // Two separate chains, so no crossing is possible: each pair is level.
        assert!((y(a) - y(d)).abs() < 0.5 && (y(b) - y(c)).abs() < 0.5);

        // One connected graph where order matters: roots R1, R2 feed X, Y crosswise
        // and a common sink joins them.
        let (mut s, n) = graph(&[(80.0, 40.0); 5], &[(0, 3), (1, 2), (2, 4), (3, 4)]);
        s.auto_layout(&LayoutOptions::default());
        let (r1, r2, x, yy) = (n[0], n[1], n[2], n[3]);
        let cy = |id| rect(&s, id).center().y;
        // R1 feeds Y and R2 feeds X, so whichever root is on top has its child on top.
        assert_eq!(cy(r1) < cy(r2), cy(yy) < cy(x), "no crossing");
    }

    #[test]
    fn parts_that_are_not_connected_are_stacked_without_touching() {
        let (mut s, n) = graph(&[(100.0, 40.0); 5], &[(0, 1), (2, 3)]);
        s.auto_layout(&LayoutOptions::default());
        assert_no_overlaps(&s, &n);
        let (first, second) = (
            rect(&s, n[0]).union(rect(&s, n[1])),
            rect(&s, n[2]).union(rect(&s, n[3])),
        );
        assert!(
            second.min.y >= first.max.y + 79.9,
            "component gap: {first:?} {second:?}"
        );
        assert!(
            rect(&s, n[4]).min.y >= second.max.y + 79.9,
            "the lone node comes last"
        );
    }

    #[test]
    fn nodes_of_different_sizes_never_overlap() {
        let sizes = [
            (120.0, 70.0),
            (60.0, 30.0),
            (200.0, 90.0),
            (80.0, 50.0),
            (150.0, 40.0),
            (90.0, 120.0),
            (70.0, 35.0),
        ];
        let edges = [
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 4),
            (2, 4),
            (3, 5),
            (4, 6),
            (5, 6),
            (2, 6),
        ];
        for direction in [LayoutDirection::LeftToRight, LayoutDirection::TopToBottom] {
            let (mut s, n) = graph(&sizes, &edges);
            s.auto_layout(&LayoutOptions {
                direction,
                ..Default::default()
            });
            assert_no_overlaps(&s, &n);
        }
    }

    #[test]
    fn layout_positions_is_a_pure_preview() {
        let (s, n) = graph(&[(100.0, 40.0); 3], &[(0, 1), (1, 2)]);
        let plan = s.layout_positions(&LayoutOptions::default());
        assert_eq!(plan.len(), 3);
        assert!(plan.iter().all(|(_, p)| p.x >= 0.0));
        assert!(
            n.iter()
                .all(|id| s.node(*id).unwrap().position == pos2(0.0, 0.0)),
            "nothing moved"
        );
    }

    #[test]
    fn keep_origin_pins_the_top_left_and_off_starts_at_zero() {
        let (mut s, n) = graph(&[(100.0, 40.0); 2], &[(0, 1)]);
        s.node_mut(n[0]).unwrap().position = pos2(500.0, 300.0);
        s.node_mut(n[1]).unwrap().position = pos2(700.0, 400.0);
        s.auto_layout(&LayoutOptions::default());
        assert_eq!(rect(&s, n[0]).min, pos2(500.0, 300.0));
        s.auto_layout(&LayoutOptions {
            keep_origin: false,
            ..Default::default()
        });
        assert_eq!(rect(&s, n[0]).min, pos2(0.0, 0.0));
    }

    #[test]
    fn a_group_is_one_block_and_keeps_its_members_inside() {
        let (mut s, n) = graph(&[(100.0, 40.0), (100.0, 40.0), (60.0, 30.0)], &[]);
        let g = s.add_group(pos2(0.0, 0.0), vec2(300.0, 200.0), "g");
        s.node_mut(g).unwrap().size = vec2(300.0, 200.0);
        s.set_parent(n[1], Some(g));
        s.node_mut(n[1]).unwrap().position = pos2(40.0, 60.0);
        // The outside node feeds a node inside the group, so the edge goes to the group block.
        s.connect(n[0], n[1], ());
        s.connect(n[1], n[2], ());
        s.auto_layout(&LayoutOptions::default());
        assert_eq!(
            s.node(n[1]).unwrap().position,
            pos2(40.0, 60.0),
            "members keep their place inside"
        );
        let (a, grp, c) = (rect(&s, n[0]), rect(&s, g), rect(&s, n[2]));
        assert!(
            grp.min.x >= a.max.x + 79.9 && c.min.x >= grp.max.x + 79.9,
            "{a:?} {grp:?} {c:?}"
        );
    }

    #[test]
    fn scope_lays_out_the_members_of_one_group_and_refits_it() {
        let (mut s, n) = graph(&[(100.0, 40.0); 3], &[(0, 1), (1, 2)]);
        let outside = s.add_node(pos2(900.0, 900.0), "out");
        let g = s.add_group(pos2(50.0, 50.0), vec2(10.0, 10.0), "g");
        for id in &n {
            s.set_parent(*id, Some(g));
        }
        let opts = LayoutOptions {
            scope: Some(g),
            ..Default::default()
        };
        assert_eq!(s.auto_layout(&opts), 3);
        assert_eq!(
            s.node(outside).unwrap().position,
            pos2(900.0, 900.0),
            "other nodes untouched"
        );
        // Members run left to right inside the group, starting at its padding and header.
        let abs = |id| s.abs_rect(id).unwrap();
        let grp = s.abs_rect(g).unwrap();
        for w in n.windows(2) {
            assert!(abs(w[1]).min.x >= abs(w[0]).max.x + 79.9);
        }
        for id in &n {
            assert!(
                grp.contains_rect(abs(*id)),
                "{grp:?} should contain {:?}",
                abs(*id)
            );
        }
        assert!(abs(n[0]).min.y >= grp.min.y + 30.0, "below the header");
    }

    #[test]
    fn hidden_members_of_collapsed_groups_are_left_alone() {
        let (mut s, n) = graph(&[(100.0, 40.0); 3], &[(0, 1), (1, 2)]);
        let g = s.add_group(pos2(0.0, 0.0), vec2(200.0, 100.0), "g");
        s.node_mut(g).unwrap().size = vec2(200.0, 100.0);
        s.set_parent(n[1], Some(g));
        s.node_mut(n[1]).unwrap().position = pos2(11.0, 22.0);
        s.set_collapsed(g, true);
        let before = s.node(n[1]).unwrap().position;
        s.auto_layout(&LayoutOptions::default());
        assert_eq!(s.node(n[1]).unwrap().position, before);
    }

    #[test]
    fn empty_and_single_node_graphs_are_fine() {
        let mut s: S = FlowState::new();
        assert_eq!(s.auto_layout(&LayoutOptions::default()), 0);
        let (mut s, n) = graph(&[(50.0, 50.0)], &[]);
        assert_eq!(s.auto_layout(&LayoutOptions::default()), 1);
        assert_eq!(rect(&s, n[0]).min, pos2(0.0, 0.0));
    }

    #[test]
    fn an_animated_layout_glides_then_lands_exactly() {
        let (mut s, n) = graph(&[(100.0, 40.0); 3], &[(0, 1), (1, 2)]);
        for (i, id) in n.iter().enumerate() {
            s.node_mut(*id).unwrap().position = pos2(0.0, i as f32 * 10.0);
        }
        let target: HashMap<NodeId, Pos2> = s
            .layout_positions(&LayoutOptions::default())
            .into_iter()
            .collect();
        assert_eq!(s.auto_layout_animated(&LayoutOptions::default(), 1.0), 3);
        assert!(s.layout_anim.is_some());
        assert!(!s.step_layout(10.0), "the first step starts the clock");
        assert_eq!(
            s.node(n[2]).unwrap().position,
            pos2(0.0, 20.0),
            "nothing has moved yet"
        );
        assert!(!s.step_layout(10.5));
        let mid = s.node(n[2]).unwrap().position;
        assert!(
            mid.x > 0.0 && mid.x < target[&n[2]].x,
            "halfway there: {mid:?}"
        );
        assert!(s.step_layout(11.2), "finishes after the duration");
        for id in &n {
            assert_eq!(s.node(*id).unwrap().position, target[id]);
        }
        assert!(s.layout_anim.is_none());
        assert!(!s.step_layout(12.0), "nothing left to do");
    }

    #[test]
    fn starting_a_drag_cancels_an_animated_layout() {
        let (mut s, n) = graph(&[(100.0, 40.0); 2], &[(0, 1)]);
        s.auto_layout_animated(&LayoutOptions::default(), 5.0);
        s.step_layout(1.0);
        s.interaction.node_drag = Some(crate::state::NodeDrag {
            origins: vec![(n[0], pos2(0.0, 0.0))],
            roots: vec![n[0]],
            accum: Vec2::ZERO,
        });
        assert!(!s.step_layout(2.0));
        assert!(s.layout_anim.is_none(), "the drag wins");
    }

    #[test]
    fn auto_layout_replaces_a_running_animation() {
        let (mut s, _) = graph(&[(100.0, 40.0); 2], &[(0, 1)]);
        s.auto_layout_animated(&LayoutOptions::default(), 5.0);
        s.auto_layout(&LayoutOptions::default());
        assert!(s.layout_anim.is_none());
    }
}
