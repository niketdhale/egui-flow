# egui-flow

A [React Flow](https://reactflow.dev/)-style node-graph canvas for [egui](https://github.com/emilk/egui), in pure Rust.

```sh
cargo run -p egui-flow --example basic
```

## Features

| React Flow | egui-flow |
|---|---|
| Pan / zoom viewport | drag background or middle mouse to pan, wheel / pinch to zoom (zoom-to-cursor), `fit_view()` |
| Custom nodes | implement `FlowViewer::node_ui` with any egui widgets |
| Handles | `FlowViewer::handles` — any number per node, on any side, source or target |
| Connecting | drag handle → handle, snapping, live validation (`can_connect`), `Esc` cancels |
| Edge types | `Bezier`, `Straight`, `Step`, `SmoothStep`; labels, arrowheads, per-edge `color` / `width` |
| Animated edges | `edge.animated = true` marches dashes (`animation_speed`, negative reverses) |
| Particles along edges | `state.pulse_edge(id, PulseStyle::default())` sends a dot source → target; `pulse_edge_reverse` or `PulseStyle { direction, delay, label, .. }` for the other way, a start delay and a hover label; `state.max_pulses_per_edge` / `pulse_overflow` control the cap |
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

## Animation

Everything above that moves can be disabled at once with `FlowOptions { animate: false, .. }` (reduced motion): view transitions jump, nodes appear instantly and hover easing is skipped. Edges you marked `animated` and explicit `pulse_edge` calls are your own choice and keep running. Pulses are capped at 8 in flight per edge so a burst of events can't pile up.

## Notes

* Content scales with zoom by rasterising at 1× and transforming, so text is soft when zoomed in far (same as `egui::Scene`).
* Selectable labels are disabled inside nodes so dragging on text moves the node; re-enable in `node_ui` if needed.
* No node exit animation (removed nodes vanish immediately) or per-edge dash patterns.
* Not yet implemented: re-connecting existing edges by dragging their ends, node resizing, nested/grouped nodes, auto-layout.
