//! # egui-flow
//!
//! A [React Flow](https://reactflow.dev/)-style node-graph canvas for
//! [egui](https://github.com/emilk/egui).
//!
//! * pan (drag the background / middle mouse) and zoom (wheel, pinch)
//! * draggable nodes with arbitrary egui content
//! * connectable handles with live validation and snapping
//! * bezier / straight / step / smooth-step edges, labels, arrows, animation,
//!   per-edge colour, width and line style (solid, dashed, dotted, custom)
//! * click, shift-click and box (shift-drag) selection, Delete to remove
//! * dots / lines / cross background, minimap, zoom controls
//! * animated dashed edges, travelling pulses, eased view transitions, node fade-in
//! * built-in vector [`Icon`]s (check, chevrons, triangles, ...) that need no font
//! * resizable nodes, undo/redo and copy/paste ([`Editor`])
//! * groups / sub-flows: nesting, collapse, drag in and out, constrained members
//!   ([`FlowViewer::can_join_group`], [`GroupDelete`])
//! * auto layout ([`LayoutOptions`], [`FlowState::auto_layout_animated`]) and
//!   [`FlowTheme`] colour presets
//! * bus bars: [`Handle::along`] handles take wires anywhere along a side, and
//!   [`FlowOptions::avoid_nodes`] routes step edges around nodes
//! * crisp text at high zoom, node exit animation, pulse labels ([`PulseLabelMode`])
//! * events for everything the application needs to mirror
//!
//! New here? Read the
//! [getting-started guide](https://github.com/niketdhale/egui-flow/blob/main/docs/getting-started.md).
//! The [README](https://github.com/niketdhale/egui-flow#readme) covers each feature, and
//! `cargo run --example gateway` shows most of them together.
//!
//! ```no_run
//! use egui_flow::{Flow, FlowState, FlowViewer, Node};
//!
//! struct Viewer;
//! impl FlowViewer<String, ()> for Viewer {
//!     fn node_ui(&mut self, ui: &mut egui::Ui, node: &mut Node<String>) {
//!         ui.text_edit_singleline(&mut node.data);
//!     }
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut FlowState<String, ()>) {
//!     let out = Flow::new("graph").show(ui, state, &mut Viewer);
//!     for event in out.events { /* react */ }
//! }
//! ```

#![warn(missing_docs)]

mod crisp;
mod editor;
mod events;
mod exit;
mod geometry;
mod groups;
mod icons;
pub mod layout;
mod options;
mod state;
mod theme;
mod types;
mod view;
mod viewer;

pub use editor::Editor;
pub use events::{FlowEvent, FlowResponse};
pub use geometry::{edge_path, edge_path_around, point_at};
pub use icons::{Icon, icon, icon_button};
pub use layout::{LayoutDirection, LayoutOptions};
pub use options::{Background, FlowOptions, GroupDelete, HandleVisibility};
pub use state::{
    Clipboard, FlowState, PulseDirection, PulseEasing, PulseLabelMode, PulseOverflow, PulseShape,
    PulseStyle,
};
pub use theme::FlowTheme;
pub use types::*;
pub use view::Flow;
pub use viewer::FlowViewer;
