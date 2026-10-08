//! Undo/redo history and a clipboard, driven by the canvas's events.

use std::collections::HashMap;

use egui::{Vec2, vec2};

use crate::events::FlowEvent;
use crate::state::{Clipboard, FlowState};
use crate::types::{Edge, Node};

struct Snapshot<N, E> {
    nodes: Vec<Node<N>>,
    edges: Vec<Edge<E>>,
}

impl<N: Clone, E: Clone> Snapshot<N, E> {
    fn take(state: &FlowState<N, E>) -> Self {
        Self {
            nodes: state.nodes.clone(),
            edges: state.edges.clone(),
        }
    }

    /// Restore the snapshot. Items that exist both now and in the snapshot keep
    /// their current selection, so undo never changes what you have selected;
    /// items it brings back keep the selection they had.
    fn apply(&self, state: &mut FlowState<N, E>) {
        let node_sel: HashMap<_, _> = state.nodes.iter().map(|n| (n.id, n.selected)).collect();
        let edge_sel: HashMap<_, _> = state.edges.iter().map(|e| (e.id, e.selected)).collect();
        state.nodes = self.nodes.clone();
        state.edges = self.edges.clone();
        for n in &mut state.nodes {
            if let Some(&sel) = node_sel.get(&n.id) {
                n.selected = sel;
            }
        }
        for e in &mut state.edges {
            if let Some(&sel) = edge_sel.get(&e.id) {
                e.selected = sel;
            }
        }
    }
}

impl<N: Clone, E: Clone> Clone for Snapshot<N, E> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
        }
    }
}

/// Undo/redo and copy/paste for a [`FlowState`].
///
/// Create it once from the initial state and call [`process`](Self::process)
/// with each frame's events:
///
/// ```ignore
/// let mut editor = Editor::new(&state);
/// // every frame
/// let out = Flow::new("graph").show(ui, &mut state, &mut viewer);
/// editor.process(&mut state, &out.events);
/// ```
///
/// A history step is recorded after a node drag, a connect, reconnect, delete,
/// resize, paste, cut or duplicate. Changes you make yourself (editing node
/// data, adding nodes in code) are not seen; call [`commit`](Self::commit)
/// after them to make them undoable.
pub struct Editor<N, E> {
    undo: Vec<Snapshot<N, E>>,
    redo: Vec<Snapshot<N, E>>,
    present: Snapshot<N, E>,
    clipboard: Option<Clipboard<N, E>>,
    pastes: u32,
    /// Oldest steps are dropped beyond this many.
    pub limit: usize,
    /// How far each successive paste is shifted, in flow units.
    pub paste_offset: Vec2,
}

impl<N: Clone, E: Clone> Editor<N, E> {
    /// An editor whose history starts at the current contents of `state`.
    pub fn new(state: &FlowState<N, E>) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            present: Snapshot::take(state),
            clipboard: None,
            pastes: 0,
            limit: 100,
            paste_offset: vec2(24.0, 24.0),
        }
    }

    /// Record the state's current contents as a new undo step.
    pub fn commit(&mut self, state: &FlowState<N, E>) {
        let prev = std::mem::replace(&mut self.present, Snapshot::take(state));
        self.undo.push(prev);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Whether there is a step to [`undo`](Self::undo).
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is a step to [`redo`](Self::redo).
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Step back. Returns `false` if there is nothing to undo.
    pub fn undo(&mut self, state: &mut FlowState<N, E>) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.present, prev);
        self.redo.push(current);
        self.present.apply(state);
        true
    }

    /// Step forward again. Returns `false` if there is nothing to redo.
    pub fn redo(&mut self, state: &mut FlowState<N, E>) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.present, next);
        self.undo.push(current);
        self.present.apply(state);
        true
    }

    /// Copy the selection into the editor's clipboard.
    pub fn copy(&mut self, state: &FlowState<N, E>) -> bool {
        self.pastes = 0;
        self.clipboard = state.copy_selected();
        self.clipboard.is_some()
    }

    /// Paste the clipboard, shifted a little further on each call.
    pub fn paste(&mut self, state: &mut FlowState<N, E>) -> bool {
        let Some(cb) = &self.clipboard else {
            return false;
        };
        self.pastes += 1;
        state.paste(cb, self.paste_offset * self.pastes as f32);
        self.commit(state);
        true
    }

    /// React to a frame's events: record history steps, and carry out the
    /// undo, redo, copy, cut, paste and duplicate shortcuts. Returns `true`
    /// if the state changed.
    pub fn process(&mut self, state: &mut FlowState<N, E>, events: &[FlowEvent<N, E>]) -> bool {
        let mut changed = false;
        for event in events {
            match event {
                FlowEvent::NodesDragStopped(_)
                | FlowEvent::Connected(_)
                | FlowEvent::Reconnected { .. }
                | FlowEvent::ParentChanged { .. }
                | FlowEvent::LayoutFinished
                | FlowEvent::GroupToggled { .. }
                | FlowEvent::Deleted { .. }
                | FlowEvent::NodeResized { finished: true, .. } => {
                    self.commit(state);
                    changed = true;
                }
                FlowEvent::UndoRequested => changed |= self.undo(state),
                FlowEvent::RedoRequested => changed |= self.redo(state),
                FlowEvent::CopyRequested => {
                    self.copy(state);
                }
                FlowEvent::CutRequested => {
                    if self.copy(state) {
                        state.delete_selected();
                        self.commit(state);
                        changed = true;
                    }
                }
                FlowEvent::PasteRequested => changed |= self.paste(state),
                FlowEvent::DuplicateRequested => {
                    let copies = state.duplicate_selected(self.paste_offset);
                    if !copies.is_empty() {
                        self.commit(state);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn state() -> FlowState<&'static str, ()> {
        let mut s = FlowState::new();
        s.add_node(pos2(0.0, 0.0), "a");
        s
    }

    #[test]
    fn undo_and_redo_walk_through_commits() {
        let mut s = state();
        let mut ed = Editor::new(&s);
        assert!(!ed.undo(&mut s), "nothing to undo yet");

        s.add_node(pos2(100.0, 0.0), "b");
        ed.commit(&s);
        s.add_node(pos2(200.0, 0.0), "c");
        ed.commit(&s);
        assert_eq!(s.nodes.len(), 3);

        assert!(ed.undo(&mut s));
        assert_eq!(s.nodes.len(), 2);
        assert!(ed.undo(&mut s));
        assert_eq!(s.nodes.len(), 1);
        assert!(!ed.can_undo() && ed.can_redo());

        assert!(ed.redo(&mut s));
        assert!(ed.redo(&mut s));
        assert_eq!(s.nodes.len(), 3);
        assert!(!ed.redo(&mut s));
    }

    #[test]
    fn wire_style_edits_are_undoable_after_a_commit() {
        use crate::{ArrowStyle, LineStyle};
        let mut s = state();
        let b = s.add_node(pos2(100.0, 0.0), "b");
        let a = s.nodes[0].id;
        let e = s.connect(a, b, ()).unwrap();
        s.edge_mut(e).unwrap().target_offset = Some(0.3);
        let mut ed = Editor::new(&s);

        let edge = s.edge_mut(e).unwrap();
        edge.line_style = LineStyle::Dashed;
        edge.color = Some(egui::Color32::RED);
        edge.width = Some(4.0);
        edge.arrow = true;
        edge.arrow_style = ArrowStyle::Diamond;
        ed.commit(&s);

        assert!(ed.undo(&mut s));
        let edge = s.edge(e).unwrap();
        assert_eq!(edge.line_style, LineStyle::Solid);
        assert_eq!((edge.color, edge.width, edge.arrow), (None, None, false));
        assert_eq!(edge.target_offset, Some(0.3), "landing points ride along");
        assert!(ed.redo(&mut s));
        assert_eq!(s.edge(e).unwrap().line_style, LineStyle::Dashed);
        assert_eq!(s.edge(e).unwrap().arrow_style, ArrowStyle::Diamond);
    }

    #[test]
    fn undo_and_redo_leave_the_current_selection_alone() {
        let mut s = state();
        let a = s.nodes[0].id;
        let b = s.add_node(pos2(100.0, 0.0), "b");
        let e = s.connect(a, b, ()).unwrap();
        s.node_mut(a).unwrap().selected = true;
        let mut ed = Editor::new(&s); // snapshot: a selected
        s.nodes[0].position = pos2(5.0, 5.0);
        ed.commit(&s);

        // Select something else, then undo the move.
        s.clear_selection();
        s.node_mut(b).unwrap().selected = true;
        s.edge_mut(e).unwrap().selected = true;
        assert!(ed.undo(&mut s));
        assert_eq!(
            s.node(a).unwrap().position,
            pos2(0.0, 0.0),
            "the move was undone"
        );
        assert!(!s.node(a).unwrap().selected, "a stays deselected");
        assert!(s.node(b).unwrap().selected && s.edge(e).unwrap().selected);
        assert!(ed.redo(&mut s));
        assert!(s.node(b).unwrap().selected && !s.node(a).unwrap().selected);
    }

    #[test]
    fn undoing_a_delete_brings_the_node_back_with_its_old_selection() {
        let mut s = state();
        let a = s.nodes[0].id;
        s.node_mut(a).unwrap().selected = true;
        let mut ed = Editor::new(&s);
        s.delete_selected();
        ed.commit(&s);
        assert!(s.nodes.is_empty());
        assert!(ed.undo(&mut s));
        assert_eq!(s.nodes.len(), 1);
        assert!(
            s.node(a).unwrap().selected,
            "restored node is selected again"
        );
    }

    #[test]
    fn group_changes_are_undoable() {
        let mut s = state();
        let a = s.nodes[0].id;
        let g = s.add_group(pos2(100.0, 100.0), egui::vec2(200.0, 100.0), "g");
        let mut ed = Editor::new(&s);

        // Dropping `a` into the group, as the canvas reports it.
        s.set_parent(a, Some(g));
        assert!(ed.process(
            &mut s,
            &[FlowEvent::ParentChanged {
                node: a,
                parent: Some(g)
            }]
        ));
        assert_eq!(s.node(a).unwrap().parent, Some(g));
        // Collapsing it is a step of its own.
        s.set_collapsed(g, true);
        assert!(ed.process(
            &mut s,
            &[FlowEvent::GroupToggled {
                node: g,
                collapsed: true
            }]
        ));
        assert!(s.is_hidden(a));

        assert!(ed.undo(&mut s));
        assert!(!s.node(g).unwrap().collapsed && !s.is_hidden(a));
        assert!(ed.undo(&mut s));
        assert_eq!(s.node(a).unwrap().parent, None);
        assert_eq!(s.abs_position(a), Some(pos2(0.0, 0.0)), "back where it was");
        assert!(ed.redo(&mut s));
        assert_eq!(s.node(a).unwrap().parent, Some(g));
    }

    #[test]
    fn a_finished_layout_is_one_undo_step() {
        let mut s = state();
        let a = s.nodes[0].id;
        let b = s.add_node(pos2(5.0, 5.0), "b");
        s.node_mut(a).unwrap().size = egui::vec2(50.0, 30.0);
        s.node_mut(b).unwrap().size = egui::vec2(50.0, 30.0);
        s.connect(a, b, ());
        let mut ed = Editor::new(&s);
        let before: Vec<_> = s.nodes.iter().map(|n| n.position).collect();
        s.auto_layout(&crate::LayoutOptions::default());
        assert_ne!(
            s.nodes.iter().map(|n| n.position).collect::<Vec<_>>(),
            before
        );
        assert!(ed.process(&mut s, &[FlowEvent::LayoutFinished]));
        assert!(ed.undo(&mut s));
        assert_eq!(
            s.nodes.iter().map(|n| n.position).collect::<Vec<_>>(),
            before
        );
        assert!(ed.redo(&mut s));
        assert_ne!(
            s.nodes.iter().map(|n| n.position).collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn a_new_commit_clears_redo_and_limit_drops_oldest() {
        let mut s = state();
        let mut ed = Editor::new(&s);
        ed.limit = 2;
        for i in 0..4 {
            s.add_node(pos2(i as f32, 0.0), "x");
            ed.commit(&s);
        }
        assert_eq!(ed.undo.len(), 2);
        ed.undo(&mut s);
        assert!(ed.can_redo());
        s.add_node(pos2(9.0, 9.0), "y");
        ed.commit(&s);
        assert!(!ed.can_redo());
    }

    #[test]
    fn process_handles_shortcut_events() {
        let mut s = state();
        let mut ed = Editor::new(&s);
        let a = s.nodes[0].id;
        s.node_mut(a).unwrap().selected = true;

        // Duplicate, then undo it, then redo it.
        assert!(ed.process(&mut s, &[FlowEvent::DuplicateRequested]));
        assert_eq!(s.nodes.len(), 2);
        assert!(ed.process(&mut s, &[FlowEvent::UndoRequested]));
        assert_eq!(s.nodes.len(), 1);
        assert!(ed.process(&mut s, &[FlowEvent::RedoRequested]));
        assert_eq!(s.nodes.len(), 2);

        // Copy + paste twice shifts each paste further.
        s.clear_selection();
        s.node_mut(a).unwrap().selected = true;
        ed.process(&mut s, &[FlowEvent::CopyRequested]);
        ed.process(
            &mut s,
            &[FlowEvent::PasteRequested, FlowEvent::PasteRequested],
        );
        assert_eq!(s.nodes.len(), 4);
        let xs: Vec<_> = s.nodes.iter().map(|n| n.position.x).collect();
        assert_eq!(&xs[2..], &[24.0, 48.0]);

        // Cut removes the selection (the second paste) and can be undone.
        ed.process(&mut s, &[FlowEvent::CutRequested]);
        assert_eq!(s.nodes.len(), 3);
        ed.process(&mut s, &[FlowEvent::UndoRequested]);
        assert_eq!(s.nodes.len(), 4);
    }

    #[test]
    fn drag_and_delete_events_record_steps() {
        let mut s = state();
        let mut ed = Editor::new(&s);
        s.nodes[0].position = pos2(50.0, 50.0);
        let id = s.nodes[0].id;
        ed.process(&mut s, &[FlowEvent::NodesDragStopped(vec![id])]);
        assert!(ed.undo(&mut s));
        assert_eq!(s.nodes[0].position, pos2(0.0, 0.0));
    }
}
