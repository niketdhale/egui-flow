# Changelog

All notable changes to egui-flow are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-10-08

### Added
- **Connect anywhere along a side**: `Handle::along()` makes a handle accept wires at any point of
  its side; the landing point is stored in `Edge::{source_offset, target_offset}`, several wires may
  share the handle, and reconnecting moves only the dragged end. `Handle::{position_at,
  side_segment, offset_near}` and `FlowState::add_edge_at`. Made for bus bars.
- `FlowOptions::avoid_nodes` and `edge_path_around`: `Step` and `SmoothStep` edges route around the
  nodes in their way (off by default).
- `FlowViewer::can_join_group`: refuse a drop into a group before it happens, so it emits no event
  and never reaches an `Editor`.
- `FlowOptions::group_delete` (`GroupDelete::{DeleteMembers, KeepMembers}`) and
  `FlowState::delete_selected_with`.
- `PulseStyle::label_mode` (`PulseLabelMode::{OnHover, Always}`); pulse labels now move out of the
  way of edge labels and of each other.

### Documentation
- Every public item is documented and `#![warn(missing_docs)]` keeps it that way.
- `docs/getting-started.md`, a guide from an empty window to an editable graph; its code is
  compiled by `tests/guide.rs`.
- New `busbar` example: bus bars, routing around nodes and group rules.

### Changed
- `Edge` has two new fields (`source_offset`, `target_offset`) and `Handle` one (`along`); code that
  builds them with a struct literal needs them or `..`.

## [0.2.0] - 2026-10-06

### Added
- **Auto-layout**: `FlowState::{layout_positions, auto_layout, auto_layout_animated}` with
  `LayoutOptions` and `LayoutDirection` (`LeftToRight`, `TopToBottom`). A compact layered layout:
  cycles are broken, nodes are layered by longest path, crossings are reduced by barycenter sweeps,
  nodes are placed without overlapping and pulled toward their neighbours, and unconnected parts are
  stacked. A group is laid out as one block; `LayoutOptions::scope` lays out the inside of a group
  and refits it. `auto_layout_animated` glides there and sends `FlowEvent::LayoutFinished` (which the
  `Editor` records); starting a drag cancels it.
- **Node exit animation** (`FlowOptions::node_exit_animation`, on by default, needs `animate`): a removed
  node fades out where it was instead of vanishing. The canvas keeps a copy of each visible node's
  last painted shapes to do it, so turn it off for very large graphs.
- **`FlowTheme`** (`FlowOptions::theme`, `Flow::theme`): colours for the canvas, edges, selection, handles,
  alignment guides, labels, minimap and the default node frame and text, all optional so
  `FlowTheme::default()` keeps the colours derived from egui's visuals. Presets `FlowTheme::{dark, light,
  blueprint}`.
- **`Node::constrain_to_parent`** (and `Node::constrained()`): a member stops at its group's edges, and
  below its header, while it is dragged or nudged, instead of leaving.
- **Groups** (sub-flows): `Node::{parent, is_group, collapsed}` and `Node::{group, in_group}`. A node's
  `position` is relative to its group; groups nest. `FlowState::{add_group, set_parent, children,
  descendants, ancestors, depth, abs_position, abs_rect, set_collapsed, fit_group, group_selected,
  ungroup, hidden_nodes, is_hidden, collapse_proxy}`.
  On the canvas: groups draw behind their members, only the header grabs (so the rest of an open
  group still pans and box-selects), a header toggle collapses a group (`FlowEvent::GroupToggled`;
  edges to hidden members attach to the group, edges wholly inside it are not drawn), dropping a
  node on or off a group changes its membership (`FlowEvent::ParentChanged`,
  `FlowOptions::group_drop`), and dragging, nudging, copying and deleting a group take its members
  along. `FlowOptions::group_header_height`. `Editor` records both events, so they undo.
  A box select picks an open group only when the box covers all of it.
- **Crisp text when zoomed** (`FlowOptions::crisp_text`, on by default; `Flow::crisp_text`). The canvas is
  drawn into a scaled layer, so text used to be the 1x raster stretched by the zoom and went soft
  above 1x. Text is now laid out again at the zoomed font size and shrunk back, so glyphs are
  rasterised at (almost exactly) their on-screen size. The scale is quantised to eighth-octave steps
  to avoid filling the glyph atlas while zooming, so text is stretched by at most about 4%.
- `FlowOptions::handle_visibility` (`HandleVisibility::{Always, OnHover, Hidden}`): hide the
  connection dots on nodes. `OnHover` fades them in near the node, while it is selected or dragged,
  and while a connection is being dragged; `Hidden` never draws them and disables
  drag-to-connect. The default, `Always`, keeps the previous behaviour.
- Tests that one handle accepts any number of wires (as source and as target) and that exact
  duplicates are still rejected.

### Changed
- `FlowState::bounds` is in flow space whatever group a node is in, and ignores members of collapsed
  groups. `FlowResponse::nodes` lists only visible nodes.
- `FlowState::remove_node` on a group moves its members up a level, in place; `delete_selected` on a
  group deletes its deletable members too (`ungroup` first to keep them).
- Copying a group copies its members; pasting remaps their membership. A copied member whose group is
  not copied is pasted at the top level, where it was.
- While `Flow::show` runs, positions are in flow space (they are group-relative again when it returns).
  Only code that reads `node.position` inside `FlowViewer` callbacks for a node in a group can notice.

### Fixed
- A Shift-drag box select started a few pixels after the press (where the drag was noticed); it now
  starts exactly where the button went down.

## [0.1.0] - 2026-10-03

First release. egui-flow is a [React Flow](https://reactflow.dev/)-style node-graph canvas for
[egui](https://github.com/emilk/egui) 0.33, in pure Rust. While the version is below 1.0 the API
may still change in a minor release; such changes will be called out here.

### Canvas and nodes
- Pan (drag the background or middle mouse) and zoom (wheel or pinch, zoom-to-cursor), `fit_view`
  and eased `fit_view_animated` / `animate_viewport` transitions.
- Draggable nodes with any egui content via `FlowViewer::node_ui`; nodes fade in when added.
- Node **resizing**: corner and edge grips on selected nodes; `Node::{fixed_size, min_size,
  max_size, resizable}`, `FlowOptions::nodes_resizable`, `FlowEvent::NodeResized`.
- **Alignment guides** while dragging (`FlowOptions::alignment_guides`) and **arrow-key nudging**
  (`FlowOptions::keyboard_nudge`, Shift for 10 units).
- Click, Shift-click and Shift-drag box selection; Delete or Backspace removes the selection.
- Dots, lines or cross background, snap to grid, minimap, zoom controls.
- **Highlight connected** (`FlowOptions::highlight_connected`) dims everything not connected to the
  selected or hovered node.

### Handles and edges
- Any number of handles per node, on any side and offset, source or target.
- Connect by dragging handle to handle, with snapping and live validation (`can_connect`).
- **Reconnect** an edge by dragging the ring on either end of a selected edge to another handle
  (`FlowEvent::Reconnected`, `FlowOptions::edges_reconnectable`).
- Edge kinds `Bezier`, `Straight`, `Step` and `SmoothStep`; per-edge `color`, `width`, animated
  marching dashes.
- **Line styles**: `LineStyle::{Solid, Dashed, Dotted, Custom { dash, gap }}`.
- **Arrowheads**: `ArrowStyle::{Triangle, Open, Circle, Diamond}`, at the target and optionally the
  source for two-way links.
- **Edge labels** with `EdgeLabelStyle { position, size, color, background }`.

### Pulses
- `FlowState::pulse_edge` and `pulse_edge_reverse`; `PulseStyle` with `direction`, `delay`, hover
  `label`, `shape`, `easing`, `trail` and `tag`.
- `FlowState::pulse_route` animates a multi-hop route leg by leg, working out forward or reverse
  for each edge.
- `FlowEvent::PulseArrived` when a leg ends.
- Configurable `max_pulses_per_edge` and `pulse_overflow` (`Drop` or `ReplaceOldest`).

### Editing
- `Editor`: undo/redo history and clipboard driven by the canvas events (Ctrl/Cmd+Z, Shift+Z / Y,
  C, X, V, D via `FlowOptions::keyboard_shortcuts`). Undo and redo leave the current selection
  alone.
- `FlowState::{copy_selected, paste, duplicate_selected}`; edges between copied nodes come along
  and ids are remapped.

### Icons
- Built-in painter-drawn icon set (`Icon`, `icon`, `icon_button`) that needs no font or asset and
  never renders as an empty box.

### Other
- `FlowEvent` values returned from `Flow::show` for everything the application needs to mirror.
- Optional `serde` feature for nodes, edges, ids and the viewport.
- `basic`, `gateway` and `icons` examples; README with a feature tour.
- CI on Linux, plus library tests on Windows and macOS; render tests inspect the painted shapes.

### Notes for users of earlier commits
Anyone pinned to a commit from before this release (for example `57f39cb`) should know:
- `PulseStyle` is no longer `Copy` (it holds a `String` label); it is still `Clone`.
- `Node`, `Edge`, `FlowOptions` and `PulseStyle` gained fields. They all have defaults, but code
  that builds them with a full struct literal needs the new fields or `..Default::default()`.

[0.3.0]: https://github.com/niketdhale/egui-flow/releases/tag/v0.3.0
[0.2.0]: https://github.com/niketdhale/egui-flow/releases/tag/v0.2.0
[0.1.0]: https://github.com/niketdhale/egui-flow/releases/tag/v0.1.0
