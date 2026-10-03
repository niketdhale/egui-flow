# egui-flow

[![CI](https://github.com/niketdhale/egui-flow/actions/workflows/ci.yml/badge.svg)](https://github.com/niketdhale/egui-flow/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![egui 0.33](https://img.shields.io/badge/egui-0.33-orange.svg)](https://github.com/emilk/egui)
[![Rust edition 2024](https://img.shields.io/badge/rust-edition%202024-dea584.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/)

A [React Flow](https://reactflow.dev/)-style node-graph canvas for [egui](https://github.com/emilk/egui), in pure Rust.

```sh
cargo run --example basic   # node graph canvas
cargo run --example icons   # built-in icon gallery
```

## Features

| React Flow | egui-flow |
|---|---|
| Pan / zoom viewport | drag background or middle mouse to pan, wheel / pinch to zoom (zoom-to-cursor), `fit_view()` |
| Custom nodes | implement `FlowViewer::node_ui` with any egui widgets |
| Handles | `FlowViewer::handles` — any number per node, on any side, source or target |
| Connecting | drag handle → handle, snapping, live validation (`can_connect`), `Esc` cancels |
| Edge types | `Bezier`, `Straight`, `Step`, `SmoothStep`; labels, arrowheads, per-edge `color` / `width` |
| Line style and colour | `edge.line_style = LineStyle::{Solid, Dashed, Dotted, Custom { dash, gap }}`, `edge.color`, `edge.width`; `EdgeKind::Straight` for a direct line |
| Animated edges | `edge.animated = true` marches dashes (`animation_speed`, negative reverses) |
| Icons | built-in `Icon` set (check, close, plus, minus, chevrons, triangles, arrows) via `icon(ui, Icon::Check, 14.0)` / `icon_button(..)`; painter-drawn, so no font, SVG or asset is needed and they follow the text colour |
| Particles along edges | `state.pulse_edge(id, PulseStyle::default())` sends a dot source → target |
| Pulse direction, delay, label | `PulseStyle { direction: PulseDirection::Reverse, delay, label, .. }` or `pulse_edge_reverse`; delays let you sequence the legs of a route; the label shows on hover |
| Pulse limits | `state.max_pulses_per_edge` (default 8) and `state.pulse_overflow` (`Drop` or `ReplaceOldest`) |
| `fitView({ duration })`, zoom easing | `state.fit_view_animated(secs)`, `state.animate_viewport(vp, secs)`; zoom/fit buttons ease; user input cancels |
| Node enter transition | nodes added after the first frame fade in |
| Selection | click, shift-click, shift-drag box select, `Delete`/`Backspace` removes |
| `snapToGrid` | `FlowOptions::snap_to_grid` |
| `<Background>` | `Dots`, `Lines`, `Cross`, `None` |
| `<MiniMap>` / `<Controls>` | `FlowOptions::{minimap, controls}` |
| `onConnect`, `onNodesDelete`, … | `FlowEvent` values returned from `show` |

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
use egui_flow::{PulseDirection, PulseOverflow, PulseStyle};

// A frame travelling Engine → CAN1, then CAN1 → Gateway half a second later.
state.pulse_edge(engine_to_can1, PulseStyle { label: Some("0x1A0".into()), ..Default::default() });
state.pulse_edge(can1_to_gateway, PulseStyle { delay: 0.5, ..Default::default() });
// Bus → receiver, against the edge's own direction.
state.pulse_edge_reverse(bus_to_ecu, PulseStyle::default());

state.max_pulses_per_edge = 16;
state.pulse_overflow = PulseOverflow::ReplaceOldest; // newest pulse wins under heavy traffic
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
cargo test --all-features
```

## Animation

Everything above that moves can be disabled at once with `FlowOptions { animate: false, .. }` (reduced motion): view transitions jump, nodes appear instantly and hover easing is skipped. Edges you marked `animated` and explicit `pulse_edge` calls are your own choice and keep running. Pulses are capped per edge (8 by default, see above) so a burst of events can't pile up.

## Notes

* Content scales with zoom by rasterising at 1× and transforming, so text is soft when zoomed in far (same as `egui::Scene`).
* Selectable labels are disabled inside nodes so dragging on text moves the node; re-enable in `node_ui` if needed.
* No node exit animation (removed nodes vanish immediately) or per-edge dash patterns.
* Not yet implemented: re-connecting existing edges by dragging their ends, node resizing, nested/grouped nodes, auto-layout.
