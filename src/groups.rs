//! Groups: nodes that contain other nodes.
//!
//! A node's `parent` makes its `position` relative to that group, so moving a group moves
//! everything inside it. To keep the canvas code simple, [`Flow::show`](crate::Flow::show)
//! converts every position to flow space when a frame starts (`flatten`) and back to
//! group-relative when it ends (`unflatten`); outside a frame the helpers here walk the
//! parent chain.

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Rect, Vec2, pos2, vec2};

use crate::state::FlowState;
use crate::types::{Node, NodeId};

/// Nesting deeper than this (or a cycle) stops the walk up the parent chain.
const MAX_DEPTH: usize = 64;

type Index<'a, N> = HashMap<NodeId, &'a Node<N>>;

fn abs_in<N>(index: &Index<'_, N>, id: NodeId) -> Option<Pos2> {
    let node = index.get(&id)?;
    let mut pos = node.position;
    let mut cur = node.parent;
    for _ in 0..MAX_DEPTH {
        let Some(parent) = cur.and_then(|p| index.get(&p)) else {
            break;
        };
        pos += parent.position.to_vec2();
        cur = parent.parent;
    }
    Some(pos)
}

fn ancestors_in<N>(index: &Index<'_, N>, id: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut cur = index.get(&id).and_then(|n| n.parent);
    while let Some(p) = cur {
        if out.len() >= MAX_DEPTH || p == id || out.contains(&p) || !index.contains_key(&p) {
            break;
        }
        out.push(p);
        cur = index[&p].parent;
    }
    out
}

impl<N, E> FlowState<N, E> {
    fn index(&self) -> Index<'_, N> {
        self.nodes.iter().map(|n| (n.id, n)).collect()
    }

    /// The groups `id` is inside, nearest first.
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        ancestors_in(&self.index(), id)
    }

    /// The nodes directly inside group `id`.
    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|n| n.parent == Some(id) && n.id != id)
            .map(|n| n.id)
            .collect()
    }

    /// Everything inside group `id`, at any depth.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut by_parent: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for n in &self.nodes {
            if let Some(p) = n.parent.filter(|p| *p != n.id) {
                by_parent.entry(p).or_default().push(n.id);
            }
        }
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            for &c in by_parent.get(&cur).into_iter().flatten() {
                if c != id && !out.contains(&c) && out.len() < self.nodes.len() {
                    out.push(c);
                    stack.push(c);
                }
            }
        }
        out
    }

    /// How many groups deep `id` is (`0` for a top-level node).
    pub fn depth(&self, id: NodeId) -> usize {
        self.ancestors(id).len()
    }

    /// Top-left of the node in flow space, whatever group it is in.
    pub fn abs_position(&self, id: NodeId) -> Option<Pos2> {
        if self.flat {
            return self.node(id).map(|n| n.position);
        }
        abs_in(&self.index(), id)
    }

    /// The node's rectangle in flow space (from its last measured size).
    pub fn abs_rect(&self, id: NodeId) -> Option<Rect> {
        let size = self.node(id)?.size;
        Some(Rect::from_min_size(self.abs_position(id)?, size))
    }

    /// Nodes inside a collapsed group, which are not drawn.
    pub fn hidden_nodes(&self) -> HashSet<NodeId> {
        if self.nodes.iter().all(|n| n.parent.is_none()) {
            return HashSet::new();
        }
        let index = self.index();
        self.nodes
            .iter()
            .filter(|n| {
                ancestors_in(&index, n.id)
                    .iter()
                    .any(|a| index.get(a).is_some_and(|g| g.collapsed))
            })
            .map(|n| n.id)
            .collect()
    }

    pub fn is_hidden(&self, id: NodeId) -> bool {
        let index = self.index();
        ancestors_in(&index, id)
            .iter()
            .any(|a| index.get(a).is_some_and(|g| g.collapsed))
    }

    /// The node an edge to `id` is drawn to: `id` itself, or, when it is hidden inside
    /// collapsed groups, the outermost collapsed group.
    pub fn collapse_proxy(&self, id: NodeId) -> NodeId {
        let index = self.index();
        ancestors_in(&index, id)
            .into_iter()
            .rfind(|a| index.get(a).is_some_and(|g| g.collapsed))
            .unwrap_or(id)
    }

    /// Set a node's position from a flow-space one, whatever group it is in.
    fn set_abs_position(&mut self, id: NodeId, abs: Pos2) {
        let offset = if self.flat {
            Vec2::ZERO
        } else {
            let index = self.index();
            index
                .get(&id)
                .and_then(|n| n.parent)
                .and_then(|p| abs_in(&index, p))
                .map_or(Vec2::ZERO, |p| p.to_vec2())
        };
        if let Some(n) = self.node_mut(id) {
            n.position = abs - offset;
        }
    }

    /// Move a node into `parent` (or out, with `None`) without it moving on screen.
    pub(crate) fn reparent_keep_visual(&mut self, id: NodeId, parent: Option<NodeId>) {
        let Some(abs) = self.abs_position(id) else {
            return;
        };
        if let Some(n) = self.node_mut(id) {
            n.parent = parent;
        }
        self.set_abs_position(id, abs);
    }

    /// Put `id` in `parent` (a group), or at the top level with `None`, keeping it where
    /// it is on screen. Returns `false`, changing nothing, if either node is missing,
    /// `parent` is not a group, or the move would put a group inside itself.
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) -> bool {
        if self.node(id).is_none() {
            return false;
        }
        if let Some(p) = parent {
            let ok = p != id
                && self.node(p).is_some_and(|g| g.is_group)
                && !self.descendants(id).contains(&p);
            if !ok {
                return false;
            }
        }
        self.reparent_keep_visual(id, parent);
        true
    }

    /// Add an empty group of `size` at `position`.
    pub fn add_group(&mut self, position: Pos2, size: Vec2, data: N) -> NodeId {
        let id = self.add_node(position, data);
        if let Some(n) = self.node_mut(id) {
            n.is_group = true;
            n.fixed_size = Some(size);
        }
        id
    }

    /// Collapse or expand a group. Collapsing deselects what it hides. Returns
    /// `false` if `id` is not a group.
    pub fn set_collapsed(&mut self, id: NodeId, collapsed: bool) -> bool {
        if !self.node(id).is_some_and(|n| n.is_group) {
            return false;
        }
        if let Some(n) = self.node_mut(id) {
            n.collapsed = collapsed;
        }
        if collapsed {
            for d in self.descendants(id) {
                if let Some(n) = self.node_mut(d) {
                    n.selected = false;
                }
            }
        }
        true
    }

    /// Resize and move group `id` to hug its members: `padding` on every side, plus
    /// `header` more along the top for the title. Members stay where they are on
    /// screen. Uses the members' last measured sizes. Returns `false` for a group
    /// with no members.
    pub fn fit_group(&mut self, id: NodeId, padding: f32, header: f32) -> bool {
        let kids = self.children(id);
        let rects: Vec<(NodeId, Pos2)> = kids
            .iter()
            .filter_map(|k| self.abs_position(*k).map(|p| (*k, p)))
            .collect();
        let Some(bounds) = kids
            .iter()
            .filter_map(|k| self.abs_rect(*k))
            .reduce(|a, b| a.union(b))
        else {
            return false;
        };
        let top_left = bounds.min - vec2(padding, padding + header);
        let size = bounds.size() + vec2(2.0 * padding, 2.0 * padding + header);
        self.set_abs_position(id, top_left);
        if let Some(g) = self.node_mut(id) {
            g.fixed_size = Some(size);
            g.size = size;
        }
        for (k, abs) in rects {
            self.set_abs_position(k, abs);
        }
        true
    }

    /// Wrap the selected nodes in a new group (see [`fit_group`](Self::fit_group) for
    /// `padding` and `header`). Only top-level members of the selection that share
    /// the first one's group are wrapped. The new group is selected. `None` if
    /// nothing suitable is selected.
    pub fn group_selected(&mut self, data: N, padding: f32, header: f32) -> Option<NodeId> {
        let hidden = self.hidden_nodes();
        let selected: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected && !hidden.contains(&n.id))
            .map(|n| n.id)
            .collect();
        // Skip nodes whose group is selected too: they travel with it.
        let tops: Vec<NodeId> = selected
            .iter()
            .copied()
            .filter(|id| !self.ancestors(*id).iter().any(|a| selected.contains(a)))
            .collect();
        let parent = self.node(*tops.first()?)?.parent;
        let members: Vec<NodeId> = tops
            .into_iter()
            .filter(|id| self.node(*id).is_some_and(|n| n.parent == parent))
            .collect();
        let bounds = members
            .iter()
            .filter_map(|m| self.abs_rect(*m))
            .reduce(|a, b| a.union(b))?;
        let group = self.add_group(bounds.min, bounds.size(), data);
        // Sit just above the members in the node order is not needed: the canvas sorts by depth.
        self.reparent_keep_visual(group, parent);
        for m in &members {
            self.reparent_keep_visual(*m, Some(group));
        }
        self.fit_group(group, padding, header);
        self.clear_selection();
        if let Some(g) = self.node_mut(group) {
            g.selected = true;
        }
        Some(group)
    }

    /// Dissolve a group: its members move up a level, staying where they are, and the
    /// group (with the edges attached to it) is removed. Returns `false` if `id` is
    /// not a group.
    pub fn ungroup(&mut self, id: NodeId) -> bool {
        if !self.node(id).is_some_and(|n| n.is_group) {
            return false;
        }
        self.remove_node(id).is_some()
    }

    /// Where a node that wants to be at `pos` (flow space) ends up: `pos` itself, or
    /// the nearest spot inside its group (below the `header`) if it is constrained.
    /// Nodes whose group is among `moving` are left alone: they move with it.
    pub(crate) fn clamp_in_group(
        &self,
        id: NodeId,
        pos: Pos2,
        header: f32,
        moving: &HashSet<NodeId>,
    ) -> Pos2 {
        let Some(node) = self.node(id).filter(|n| n.constrain_to_parent) else {
            return pos;
        };
        let Some(parent) = node.parent.filter(|p| !moving.contains(p)) else {
            return pos;
        };
        let (Some(group), Some(rect)) = (self.node(parent), self.abs_rect(parent)) else {
            return pos;
        };
        let top = if group.is_group && !group.collapsed {
            header.min(rect.height())
        } else {
            0.0
        };
        let (min, max) = (pos2(rect.min.x, rect.min.y + top), rect.max);
        // A node bigger than the room is pinned to the top-left.
        pos2(
            pos.x.clamp(min.x, (max.x - node.size.x).max(min.x)),
            pos.y.clamp(min.y, (max.y - node.size.y).max(min.y)),
        )
    }

    /// Convert every position to flow space, for the duration of a frame.
    pub(crate) fn flatten(&mut self) {
        if self.flat {
            return;
        }
        if self.nodes.iter().any(|n| n.parent.is_some()) {
            let index = self.index();
            let abs: HashMap<NodeId, Pos2> = self
                .nodes
                .iter()
                .filter_map(|n| abs_in(&index, n.id).map(|p| (n.id, p)))
                .collect();
            for n in &mut self.nodes {
                if let Some(p) = abs.get(&n.id) {
                    n.position = *p;
                }
            }
        }
        self.flat = true;
    }

    /// Undo [`flatten`](Self::flatten): positions become relative to the (possibly
    /// moved) group again. Members whose group no longer exists become top-level.
    pub(crate) fn unflatten(&mut self) {
        if !self.flat {
            return;
        }
        let abs: HashMap<NodeId, Pos2> = self.nodes.iter().map(|n| (n.id, n.position)).collect();
        for n in &mut self.nodes {
            match n.parent {
                Some(p) if p != n.id => match abs.get(&p) {
                    Some(pp) => n.position -= pp.to_vec2(),
                    None => n.parent = None,
                },
                Some(_) => n.parent = None,
                None => {}
            }
        }
        self.flat = false;
    }

    /// Order nodes so groups come before their members (and so are drawn and hit-tested
    /// below them); otherwise the order is kept.
    pub(crate) fn sort_by_depth(&mut self) {
        if self.nodes.iter().all(|n| n.parent.is_none()) {
            return;
        }
        let index = self.index();
        let depth: HashMap<NodeId, usize> = self
            .nodes
            .iter()
            .map(|n| (n.id, ancestors_in(&index, n.id).len()))
            .collect();
        self.nodes.sort_by_key(|n| depth[&n.id]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    type S = FlowState<&'static str, ()>;

    /// A 300x200 group at (100, 100) holding `a` at (20, 50) and `b` at (150, 60),
    /// each 40x30 as if already measured.
    fn grouped() -> (S, NodeId, NodeId, NodeId) {
        let mut s: S = FlowState::new();
        let g = s.add_group(pos2(100.0, 100.0), vec2(300.0, 200.0), "g");
        let a = s.add_node(pos2(0.0, 0.0), "a");
        let b = s.add_node(pos2(0.0, 0.0), "b");
        for (n, rel) in [(a, pos2(20.0, 50.0)), (b, pos2(150.0, 60.0))] {
            let node = s.node_mut(n).unwrap();
            node.size = vec2(40.0, 30.0);
            node.position = rel;
            node.parent = Some(g);
        }
        s.node_mut(g).unwrap().size = vec2(300.0, 200.0);
        (s, g, a, b)
    }

    #[test]
    fn positions_inside_a_group_are_relative_to_it() {
        let (mut s, g, a, _) = grouped();
        assert_eq!(s.abs_position(a), Some(pos2(120.0, 150.0)));
        assert_eq!(s.abs_rect(a).unwrap().max, pos2(160.0, 180.0));
        // Moving the group moves what is in it.
        s.node_mut(g).unwrap().position = pos2(500.0, 0.0);
        assert_eq!(s.abs_position(a), Some(pos2(520.0, 50.0)));
        assert_eq!(
            s.node(a).unwrap().position,
            pos2(20.0, 50.0),
            "relative position is unchanged"
        );
    }

    #[test]
    fn nested_groups_add_up_and_report_their_depth() {
        let (mut s, g, a, _) = grouped();
        let inner = s.add_group(pos2(10.0, 40.0), vec2(120.0, 100.0), "inner");
        assert!(s.set_parent(inner, Some(g)));
        assert!(s.set_parent(a, Some(inner)));
        assert_eq!(s.depth(a), 2);
        assert_eq!(s.ancestors(a), vec![inner, g]);
        assert_eq!(s.descendants(g).len(), 3);
        assert_eq!(
            s.abs_position(a),
            Some(pos2(120.0, 150.0)),
            "still where it was"
        );
    }

    #[test]
    fn set_parent_keeps_the_node_where_it_is_and_rejects_bad_moves() {
        let (mut s, g, a, _) = grouped();
        let loose = s.add_node(pos2(700.0, 50.0), "loose");
        assert!(s.set_parent(loose, Some(g)));
        assert_eq!(s.node(loose).unwrap().position, pos2(600.0, -50.0));
        assert_eq!(s.abs_position(loose), Some(pos2(700.0, 50.0)));
        assert!(s.set_parent(loose, None));
        assert_eq!(s.node(loose).unwrap().position, pos2(700.0, 50.0));

        assert!(!s.set_parent(loose, Some(a)), "a plain node is not a group");
        assert!(!s.set_parent(g, Some(g)), "not inside itself");
        let inner = s.add_group(pos2(0.0, 0.0), vec2(50.0, 50.0), "inner");
        s.set_parent(inner, Some(g));
        assert!(!s.set_parent(g, Some(inner)), "no cycles");
        assert!(!s.set_parent(NodeId(999), None));
        assert_eq!(s.node(g).unwrap().parent, None);
    }

    #[test]
    fn flatten_and_unflatten_round_trip_and_follow_a_moved_group() {
        let (mut s, g, a, _) = grouped();
        s.flatten();
        assert_eq!(
            s.node(a).unwrap().position,
            pos2(120.0, 150.0),
            "flow space inside a frame"
        );
        assert_eq!(
            s.abs_position(a),
            Some(pos2(120.0, 150.0)),
            "no double counting while flat"
        );
        // Move the group (and, as the canvas does, its members) by 30,10.
        for n in [g, a] {
            s.node_mut(n).unwrap().position += vec2(30.0, 10.0);
        }
        s.unflatten();
        assert_eq!(s.node(g).unwrap().position, pos2(130.0, 110.0));
        assert_eq!(
            s.node(a).unwrap().position,
            pos2(20.0, 50.0),
            "relative offset kept"
        );

        // A member whose group vanished becomes top-level.
        s.flatten();
        s.nodes.retain(|n| n.id != g);
        s.unflatten();
        assert_eq!(s.node(a).unwrap().parent, None);
    }

    #[test]
    fn hidden_nodes_and_edge_proxies_follow_collapsing() {
        let (mut s, g, a, b) = grouped();
        let out = s.add_node(pos2(600.0, 100.0), "out");
        assert!(s.hidden_nodes().is_empty());
        s.node_mut(a).unwrap().selected = true;
        assert!(s.set_collapsed(g, true));
        assert_eq!(s.hidden_nodes(), HashSet::from([a, b]));
        assert!(
            !s.node(a).unwrap().selected,
            "collapsing deselects what it hides"
        );
        assert_eq!(s.collapse_proxy(a), g);
        assert_eq!(s.collapse_proxy(out), out);
        assert!(s.is_hidden(b) && !s.is_hidden(g));
        assert!(!s.set_collapsed(out, true), "only groups collapse");

        // Nested: the outermost collapsed group is the proxy.
        let inner = s.add_group(pos2(0.0, 0.0), vec2(80.0, 80.0), "inner");
        s.set_parent(inner, Some(g));
        s.set_parent(a, Some(inner));
        s.set_collapsed(inner, true);
        assert_eq!(s.collapse_proxy(a), g);
        s.set_collapsed(g, false);
        assert_eq!(s.collapse_proxy(a), inner);
    }

    #[test]
    fn bounds_ignore_hidden_nodes() {
        let (mut s, g, _, _) = grouped();
        s.node_mut(g).unwrap().size = vec2(300.0, 200.0);
        let open = s.bounds().unwrap();
        assert_eq!(
            open,
            Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 200.0))
        );
        // Put a member far outside the group's box, then hide it by collapsing.
        let far = s.children(g)[0];
        s.node_mut(far).unwrap().position = pos2(900.0, 900.0);
        assert!(s.bounds().unwrap().max.x > 900.0);
        s.set_collapsed(g, true);
        assert_eq!(s.bounds().unwrap(), open);
    }

    #[test]
    fn fit_group_hugs_the_members_without_moving_them() {
        let (mut s, g, a, b) = grouped();
        let (pa, pb) = (s.abs_position(a).unwrap(), s.abs_position(b).unwrap());
        assert!(s.fit_group(g, 10.0, 30.0));
        // Members span x 120..290, y 150..190; padding 10, header 30 above.
        let grp = s.node(g).unwrap();
        assert_eq!(s.abs_position(g), Some(pos2(110.0, 110.0)));
        assert_eq!(grp.fixed_size, Some(vec2(190.0, 90.0)));
        assert_eq!(s.abs_position(a), Some(pa));
        assert_eq!(s.abs_position(b), Some(pb));
        let empty = s.add_group(pos2(0.0, 0.0), vec2(10.0, 10.0), "e");
        assert!(!s.fit_group(empty, 5.0, 5.0), "nothing to hug");
    }

    #[test]
    fn group_selected_wraps_and_ungroup_dissolves() {
        let mut s: S = FlowState::new();
        let a = s.add_node(pos2(100.0, 100.0), "a");
        let b = s.add_node(pos2(300.0, 160.0), "b");
        let c = s.add_node(pos2(900.0, 900.0), "c");
        for n in [a, b, c] {
            s.node_mut(n).unwrap().size = vec2(50.0, 30.0);
        }
        s.connect(a, b, ());
        assert_eq!(s.group_selected("g", 20.0, 30.0), None, "nothing selected");
        s.node_mut(a).unwrap().selected = true;
        s.node_mut(b).unwrap().selected = true;
        let g = s.group_selected("g", 20.0, 30.0).unwrap();
        assert!(s.node(g).unwrap().is_group);
        assert_eq!(s.children(g).len(), 2);
        assert_eq!(s.node(c).unwrap().parent, None, "unselected stays out");
        assert_eq!(
            s.selected_nodes(),
            vec![g],
            "the new group is the selection"
        );
        assert_eq!(s.abs_position(a), Some(pos2(100.0, 100.0)));
        let grp = s.abs_rect(g).unwrap();
        assert!(grp.contains(pos2(100.0, 100.0)) && grp.contains(pos2(349.0, 189.0)));
        assert_eq!(grp.min, pos2(80.0, 50.0), "20 padding, 30 header above");

        assert!(s.ungroup(g));
        assert!(s.node(g).is_none());
        assert_eq!(s.node(a).unwrap().parent, None);
        assert_eq!(s.abs_position(a), Some(pos2(100.0, 100.0)));
        assert_eq!(s.edges.len(), 1, "edges between members survive");
        assert!(!s.ungroup(a), "a plain node cannot be ungrouped");
    }

    #[test]
    fn deleting_a_group_deletes_its_members_but_ungroup_keeps_them() {
        let (mut s, g, a, b) = grouped();
        let out = s.add_node(pos2(700.0, 0.0), "out");
        s.connect(a, out, ());
        s.node_mut(g).unwrap().selected = true;
        let (nodes, edges) = s.delete_selected();
        let gone: HashSet<_> = nodes.iter().map(|n| n.id).collect();
        assert_eq!(gone, HashSet::from([g, a, b]));
        assert_eq!(edges.len(), 1);
        assert_eq!(s.nodes.len(), 1);

        // Removing just the group node frees its members, in place.
        let (mut s, g, a, _) = grouped();
        let before = s.abs_position(a);
        s.remove_node(g);
        assert_eq!(s.node(a).unwrap().parent, None);
        assert_eq!(s.abs_position(a), before);

        // A member that cannot be deleted is freed rather than deleted.
        let (mut s, g, a, b) = grouped();
        s.node_mut(a).unwrap().deletable = false;
        s.node_mut(g).unwrap().selected = true;
        s.delete_selected();
        assert!(s.node(a).is_some() && s.node(b).is_none() && s.node(g).is_none());
        assert_eq!(s.node(a).unwrap().parent, None);
    }

    #[test]
    fn keep_members_deletes_only_the_group() {
        use crate::GroupDelete;
        let (mut s, g, a, b) = grouped();
        let out = s.add_node(pos2(700.0, 0.0), "out");
        s.connect(a, out, ());
        s.connect(g, out, ());
        let (before_a, before_b) = (s.abs_position(a), s.abs_position(b));
        s.node_mut(g).unwrap().selected = true;
        let (nodes, edges) = s.delete_selected_with(GroupDelete::KeepMembers);
        assert_eq!(nodes.iter().map(|n| n.id).collect::<Vec<_>>(), vec![g]);
        assert_eq!(edges.len(), 1, "only the group's own wire goes");
        assert!(s.node(a).is_some() && s.node(b).is_some());
        assert_eq!(s.node(a).unwrap().parent, None);
        assert_eq!((s.abs_position(a), s.abs_position(b)), (before_a, before_b));
        assert_eq!(s.edges.len(), 1, "the member's wire survives");
    }

    #[test]
    fn copying_a_group_brings_its_members_and_remaps_membership() {
        let (mut s, g, a, b) = grouped();
        s.connect(a, b, ());
        s.node_mut(g).unwrap().selected = true;
        let cb = s.copy_selected().unwrap();
        assert_eq!((cb.nodes.len(), cb.edges.len()), (3, 1));

        let new = s.paste(&cb, vec2(0.0, 300.0));
        assert_eq!(new.len(), 3);
        assert_eq!(s.nodes.len(), 6);
        let new_group = *new.iter().find(|n| s.node(**n).unwrap().is_group).unwrap();
        assert_eq!(
            s.selected_nodes(),
            vec![new_group],
            "only the top-level copy is selected"
        );
        assert_eq!(s.abs_position(new_group), Some(pos2(100.0, 400.0)));
        let kids = s.children(new_group);
        assert_eq!(kids.len(), 2);
        let first = s.node(kids[0]).unwrap();
        assert!(first.position == pos2(20.0, 50.0) || first.position == pos2(150.0, 60.0));
        assert!(
            kids.iter()
                .all(|k| !new.contains(&g) && s.node(*k).unwrap().parent == Some(new_group))
        );
        let pasted = s.edges.last().unwrap();
        assert!(kids.contains(&pasted.source) && kids.contains(&pasted.target));
    }

    #[test]
    fn copying_only_a_member_pastes_it_at_top_level_where_it_was() {
        let (mut s, _, a, _) = grouped();
        s.node_mut(a).unwrap().selected = true;
        let cb = s.copy_selected().unwrap();
        assert_eq!(cb.nodes.len(), 1);
        assert_eq!(cb.nodes[0].parent, None);
        assert_eq!(cb.nodes[0].position, pos2(120.0, 150.0));
        let new = s.paste(&cb, vec2(10.0, 10.0));
        assert_eq!(s.abs_position(new[0]), Some(pos2(130.0, 160.0)));
    }

    #[test]
    fn a_constrained_member_is_clamped_inside_its_group_below_the_header() {
        let (mut s, g, a, _) = grouped(); // group abs (100,100)..(400,300), a is 40x30
        let none = HashSet::new();
        // Unconstrained members go anywhere.
        assert_eq!(
            s.clamp_in_group(a, pos2(999.0, -50.0), 30.0, &none),
            pos2(999.0, -50.0)
        );
        s.node_mut(a).unwrap().constrain_to_parent = true;
        let clamp = |s: &S, p| s.clamp_in_group(a, p, 30.0, &none);
        assert_eq!(
            clamp(&s, pos2(150.0, 200.0)),
            pos2(150.0, 200.0),
            "inside is untouched"
        );
        assert_eq!(
            clamp(&s, pos2(0.0, 0.0)),
            pos2(100.0, 130.0),
            "top-left, below the header"
        );
        assert_eq!(
            clamp(&s, pos2(999.0, 999.0)),
            pos2(360.0, 270.0),
            "bottom-right keeps the node's size inside"
        );
        // A member whose group is moving along with it is left alone.
        assert_eq!(
            s.clamp_in_group(a, pos2(0.0, 0.0), 30.0, &HashSet::from([g])),
            pos2(0.0, 0.0)
        );
        // A collapsed group has no header strip to keep clear of.
        s.node_mut(g).unwrap().collapsed = true;
        s.node_mut(g).unwrap().size = vec2(300.0, 200.0);
        assert_eq!(clamp(&s, pos2(0.0, 0.0)), pos2(100.0, 100.0));
        // Bigger than the room: pinned to the top-left.
        s.node_mut(g).unwrap().collapsed = false;
        s.node_mut(a).unwrap().size = vec2(500.0, 500.0);
        assert_eq!(clamp(&s, pos2(250.0, 250.0)), pos2(100.0, 130.0));
    }

    #[test]
    fn depth_sorting_puts_groups_before_their_members_and_keeps_the_rest() {
        let (mut s, g, a, b) = grouped();
        let loose = s.add_node(pos2(0.0, 0.0), "loose");
        s.nodes.reverse(); // loose, b, a, g
        s.sort_by_depth();
        let order: Vec<_> = s.nodes.iter().map(|n| n.id).collect();
        assert!(order.iter().position(|x| *x == g) < order.iter().position(|x| *x == a));
        assert!(order.iter().position(|x| *x == g) < order.iter().position(|x| *x == b));
        // Same depth keeps its relative order: loose, g, then b before a (as reversed).
        assert_eq!(order, vec![loose, g, b, a]);
    }
}
