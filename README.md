# egui-flow

[![CI](https://github.com/niketdhale/egui-flow/actions/workflows/ci.yml/badge.svg)](https://github.com/niketdhale/egui-flow/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/niketdhale/egui-flow)](https://github.com/niketdhale/egui-flow/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![egui 0.33](https://img.shields.io/badge/egui-0.33-orange.svg)](https://github.com/emilk/egui)
[![Rust edition 2024](https://img.shields.io/badge/rust-edition%202024-dea584.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/)

A [React Flow](https://reactflow.dev/)-style node-graph canvas for [egui](https://github.com/emilk/egui), in pure Rust.

![A CAN-gateway graph built with egui-flow: styled edge labels, arrowheads, dashed links and a two-way link](docs/media/overview.png)

```sh
cargo run --example basic   # node graph canvas
cargo run --example gateway # the CAN-gateway demo from the tour below
cargo run --example icons   # built-in icon gallery
```

## Feature tour

One continuous take through the whole library, recorded from the real app ([`examples/gateway`](examples/gateway.rs)):

![A 50-second tour of egui-flow: pan and zoom, drag with alignment guides, connect, reconnect, edge and line styles, route pulses, resize, nudge, highlight, box select, copy and paste, undo and redo, delete](docs/media/tour.gif)

In order, with where to look:

| Caption | What it is |
|---|---|
| Pan and zoom | drag the background, wheel or pinch; `fit_view_animated` |
| Drag nodes, alignment guides | `FlowOptions::alignment_guides`; Ctrl+Z undoes the move |
| Connect | drag from one handle to another, with live validation and snapping |
| Reconnect | select an edge and drag the ring on its end; `FlowEvent::Reconnected` |
| Edge types, line styles | `EdgeKind::{Bezier, Straight, Step, SmoothStep}`, `LineStyle::{Solid, Dashed, Dotted}`, arrowheads, labels |
| Route pulses | `state.pulse_route(..)`; shapes, trails, arrival events |
| Resize | corner and edge grips on a selected node |
| Nudge | arrow keys move the selection (Shift: 10 units) |
| Highlight connected | `FlowOptions::highlight_connected` |
| Box select | Shift-drag |
| Copy, paste, undo, redo | `Editor`; Ctrl/Cmd+C, V, Z, Shift+Z |
| Delete | removes the selection and its edges |

### Icons
Painter-drawn, so they need no font and never render as empty boxes.

![The built-in icons: check, close, plus, minus, chevrons, triangles and arrows](docs/media/icons.png)

## Features

| React Flow | egui-flow |
|---|---|
| Pan / zoom viewport | drag background or middle mouse to pan, wheel / pinch to zoom (zoom-to-cursor), `fit_view()` |
| Custom nodes | implement `FlowViewer::node_ui` with any egui widgets |
| Handles | `FlowViewer::handles` — any number per node, on any side, source or target |
| Connecting | drag handle → handle, snapping, live validation (`can_connect`), `Esc` cancels |
| Many connections | a handle takes any number of wires; only exact duplicates are rejected |
| Hiding handles | `FlowOptions::handle_visibility`: `Always`, `OnHover` (fade in near the node, when selected or while connecting) or `Hidden` |
| Edge types | `Bezier`, `Straight`, `Step`, `SmoothStep`; labels, arrowheads, per-edge `color` / `width` |
| Arrowheads | `edge.arrow = true` with `edge.arrow_style = ArrowStyle::{Triangle, Open, Circle, Diamond}`; `edge.arrow_at_source = true` for two-way links |
| Edge labels | `edge.label` plus `edge.label_style = EdgeLabelStyle { position, size, color, background }`; `position` runs 0.0 (source) to 1.0 (target) |
| Alignment guides | `FlowOptions::alignment_guides` snaps dragged nodes to other nodes' edges and centres and draws guide lines (`guide_threshold` px) |
| Keyboard nudge | arrow keys move selected nodes 1 unit, Shift+arrow 10; `FlowOptions::keyboard_nudge` |
| Line style and colour | `edge.line_style = LineStyle::{Solid, Dashed, Dotted, Custom { dash, gap }}`, `edge.color`, `edge.width`; `EdgeKind::Straight` for a direct line |
| Animated edges | `edge.animated = true` marches dashes (`animation_speed`, negative reverses) |
| Icons | built-in `Icon` set (check, close, plus, minus, chevrons, triangles, arrows) via `icon(ui, Icon::Check, 14.0)` / `icon_button(..)`; painter-drawn, so no font, SVG or asset is needed and they follow the text colour |
| Particles along edges | `state.pulse_edge(id, PulseStyle::default())` sends a dot source → target |
| Resize nodes | select a node and drag its corner or right/bottom edge; `Node::{fixed_size, min_size, max_size, resizable}`, `FlowOptions::nodes_resizable`, snaps to `snap_to_grid`; `FlowEvent::NodeResized { node, size, finished }` |
| Undo / redo | `Editor::new(&state)` then `editor.process(&mut state, &out.events)` each frame; Ctrl/Cmd+Z, Shift+Z / Y; or call `editor.undo(..)` / `redo(..)` from buttons |
| Copy / paste / duplicate | Ctrl/Cmd+C, X, V, D handled by the same `Editor`; or `state.copy_selected()`, `state.paste(&clipboard, offset)`, `state.duplicate_selected(offset)` (edges between copied nodes come along, ids are remapped) |
| Reconnect edges | select an edge, drag the ring on either end to another handle; the edge snaps back if dropped on nothing; `FlowEvent::Reconnected { edge, old, new }`; `FlowOptions::edges_reconnectable` |
| Highlight connected | `FlowOptions::highlight_connected` / `Flow::highlight_connected(true)` dims everything not connected to the selected or hovered node |
| Route pulses | `state.pulse_route(start_node, &[edge, edge, ..], style)` animates a multi-hop route leg by leg, picking forward/reverse per edge |
| Pulse direction, delay, label | `PulseStyle { direction: PulseDirection::Reverse, delay, label, .. }` or `pulse_edge_reverse`; delays let you sequence the legs of a route; the label shows on hover; `shape` (`Circle`/`Square`/`Diamond`/`Arrow`), `easing`, `trail` and `tag`; `FlowEvent::PulseArrived { edge, tag, direction }` fires when a leg ends |
| Pulse limits | `state.max_pulses_per_edge` (default 8) and `state.pulse_overflow` (`Drop` or `ReplaceOldest`) |
| `fitView({ duration })`, zoom easing | `state.fit_view_animated(secs)`, `state.animate_viewport(vp, secs)`; zoom/fit buttons ease; user input cancels |
| Node enter transition | nodes added after the first frame fade in |
| Selection | click, shift-click, shift-drag box select, `Delete`/`Backspace` removes |
| `snapToGrid` | `FlowOptions::snap_to_grid` |
| `<Background>` | `Dots`, `Lines`, `Cross`, `None` |
| `<MiniMap>` / `<Controls>` | `FlowOptions::{minimap, controls}` |
| `onConnect`, `onNodesDelete`, … | `FlowEvent` values returned from `show` |

## Installation

egui-flow is not on crates.io yet; depend on a tagged release from GitHub:

```toml
[dependencies]
egui-flow = { git = "https://github.com/niketdhale/egui-flow", tag = "v0.1.0" }
```

It targets egui 0.33. Releases follow [Semantic Versioning](https://semver.org/) (below 1.0, a minor release may include breaking changes); see the [changelog](CHANGELOG.md) and the [releases page](https://github.com/niketdhale/egui-flow/releases).

## Usage

```rust
use egui_flow::{Flow, FlowEvent, FlowState, FlowViewer, Node};

struct Viewer;
impl FlowViewer<String, ()> for Viewer {
    fn node_ui(&mut self, ui: &mut egui::Ui, node: &mut Node<String>) {
        ui.text_edit_singleline(&mut node.data);
    }
}

// once
let mut state: FlowState<String, ()> = FlowState::new();
let a = state.add_node(egui::pos2(0.0, 0.0), "a".into());
let b = state.add_node(egui::pos2(250.0, 60.0), "b".into());
state.connect(a, b, ());

// every frame
let out = Flow::new("graph").minimap(true).show(ui, &mut state, &mut Viewer);
for event in out.events {
    if let FlowEvent::Connected(edge) = event { /* mirror into your model */ }
}
```

`FlowState` is plain data (`nodes`, `edges`, `viewport`) that you own and may mutate between frames.
Node sizes are measured from the rendered content each frame. Enable the `serde` feature to serialize nodes, edges and the viewport.

## Pulses

```rust
use egui_flow::{PulseDirection, PulseOverflow, PulseShape, PulseStyle};

// A whole route in one call: each leg starts when the previous one arrives, and
// forward/reverse is worked out per edge from the starting node.
state.pulse_route(engine, &[engine_to_can1, can1_to_gateway, gateway_to_can2], PulseStyle {
    shape: PulseShape::Arrow,
    label: Some("0x1A0".into()),
    tag: 7, // comes back in FlowEvent::PulseArrived
    ..Default::default()
});
// Or one leg at a time, with your own delay.
state.pulse_edge(can1_to_gateway, PulseStyle { delay: 0.5, ..Default::default() });
// Bus → receiver, against the edge's own direction.
state.pulse_edge_reverse(bus_to_ecu, PulseStyle::default());

state.max_pulses_per_edge = 16;
state.pulse_overflow = PulseOverflow::ReplaceOldest; // newest pulse wins under heavy traffic
```

## Undo, redo, copy and paste

```rust
use egui_flow::Editor;

// `N` and `E` must be `Clone`.
let mut editor = Editor::new(&state);

// every frame
let out = Flow::new("graph").show(ui, &mut state, &mut viewer);
editor.process(&mut state, &out.events);

if ui.add_enabled(editor.can_undo(), egui::Button::new("Undo")).clicked() {
    editor.undo(&mut state);
}
```

A history step is recorded after a node drag, connect, reconnect, delete, resize, paste, cut or duplicate. Changes you make yourself (editing node data, adding nodes in code) are not seen; call `editor.commit(&state)` after them to make them undoable. The shortcuts only fire while the pointer is over the canvas and no text field has focus; turn them off with `FlowOptions { keyboard_shortcuts: false, .. }`.

## Resizing nodes

Selected nodes show grips on the corner and the right and bottom edges. Dragging sets `node.fixed_size` (width exact, height a minimum, so the content never gets clipped); set it yourself, or build a node with `Node::new(..).with_size(vec2(220.0, 120.0))`. Clear it with `node.fixed_size = None` to go back to sizing from the content. Limit the range with `min_size` / `max_size`, or opt a node out with `resizable = false`.

## Reconnecting and highlighting

```rust
let opts = FlowOptions { highlight_connected: true, ..Default::default() };
let out = Flow::new("graph").options(opts).show(ui, &mut state, &mut viewer);
for event in out.events {
    match event {
        // The edge already carries `new`; mirror it into your own model.
        FlowEvent::Reconnected { edge, old, new } => { /* ... */ }
        FlowEvent::PulseArrived { edge, tag, .. } => { /* a frame reached the end of a leg */ }
        _ => {}
    }
}
```

## Multiple connection points

Give a node several handles from `FlowViewer::handles`, each with its own side and `offset` (`0.0..=1.0` along that side), and select them with `source_handle` / `target_handle` on the edge, so a gateway's wires to CAN1 and CAN2 leave from different points:

```rust
fn handles(&self, _node: &Node<Ecu>) -> Vec<Handle> {
    vec![
        Handle::target(Handle::DEFAULT_TARGET, Side::Left),
        Handle::source(HandleId(10), Side::Right).with_offset(0.3), // CAN1
        Handle::source(HandleId(11), Side::Right).with_offset(0.7), // CAN2
    ]
}
```

## Icons

```rust
use egui_flow::{Icon, icon, icon_button};

icon(ui, Icon::Check, 14.0);
if icon_button(ui, Icon::Close, 14.0).clicked() { /* ... */ }
Icon::TriangleDown.paint(ui.painter(), rect, color); // at a position of your choice
```

The icons are drawn with egui's painter, so they never render as empty boxes like missing font glyphs. `Icon::ALL` lists them.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features      # unit + render tests (they inspect the painted shapes)
cargo test --lib               # without the optional serde feature
cargo build --examples --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

CI runs all of these on Linux, and the library tests on Windows and macOS too.

### Releasing

1. Bump `version` in `Cargo.toml` and add a dated `## [x.y.z] - YYYY-MM-DD` section (and link) to `CHANGELOG.md`; a test fails if they disagree.
2. Merge to `main`.
3. Publish: push the tag (`git tag -a vx.y.z -m "egui-flow x.y.z" && git push origin vx.y.z`), or open **Actions → Release → Run workflow** on `main` and enter `vx.y.z` to have it tag the commit for you.

The Release workflow re-runs the checks, verifies the tag matches `Cargo.toml`, and creates the GitHub Release from the changelog.

If a release was cut with the wrong version, **Actions → Delete release → Run workflow** on `main` removes the release and its tag (type the tag twice to confirm); then release again with the right one.

## Animation

Everything above that moves can be disabled at once with `FlowOptions { animate: false, .. }` (reduced motion): view transitions jump, nodes appear instantly and hover easing is skipped. Edges you marked `animated` and explicit `pulse_edge` calls are your own choice and keep running. Pulses are capped per edge (8 by default, see above) so a burst of events can't pile up.

## Notes

* Content scales with zoom by rasterising at 1× and transforming, so text is soft when zoomed in far (same as `egui::Scene`).
* Selectable labels are disabled inside nodes so dragging on text moves the node; re-enable in `node_ui` if needed.
* Resizing sets a size; it does not make node content scale.
* No node exit animation (removed nodes vanish immediately) or per-edge dash patterns.
* Not yet implemented: nested/grouped nodes, auto-layout.
