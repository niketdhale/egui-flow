# Getting started with egui-flow

This guide takes you from an empty window to an editable graph. Every snippet uses the public API; the [README](../README.md) lists every feature and `cargo run --example gateway` shows most of them together.

## 1. Add the crate

egui-flow is not on crates.io yet; depend on a tagged release:

```toml
[dependencies]
egui-flow = { git = "https://github.com/niketdhale/egui-flow", tag = "v0.3.0" }
eframe = "0.33"   # or any egui 0.33 host
```

Optional: `features = ["serde"]` to serialize nodes, edges and the viewport.

## 2. The three pieces

| Piece | What it is | You |
|---|---|---|
| `FlowState<N, E>` | the graph: nodes, edges, viewport, selection | own it, mutate it between frames |
| `FlowViewer<N, E>` | how a node looks and which connections are allowed | implement it |
| `Flow` | the canvas widget | call `Flow::new("id").show(ui, &mut state, &mut viewer)` every frame |

`N` is the data you store on each node, `E` the data on each edge (use `()` if you have none).

```rust
use egui_flow::{Flow, FlowEvent, FlowState, FlowViewer, Node};

struct Viewer;
impl FlowViewer<String, ()> for Viewer {
    fn node_ui(&mut self, ui: &mut egui::Ui, node: &mut Node<String>) {
        ui.text_edit_singleline(&mut node.data); // any egui widget works
    }
}

struct App {
    state: FlowState<String, ()>,
}

impl App {
    fn new() -> Self {
        let mut state = FlowState::new();
        let a = state.add_node(egui::pos2(0.0, 0.0), "a".to_string());
        let b = state.add_node(egui::pos2(250.0, 60.0), "b".to_string());
        state.connect(a, b, ());
        Self { state }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let out = Flow::new("graph").show(ui, &mut self.state, &mut Viewer);
            for event in out.events {
                if let FlowEvent::Connected(edge) = event { /* mirror into your model */ }
            }
        });
    }
}
```

That already gives you pan (drag the background), zoom (wheel), dragging nodes, connecting handles, selecting and deleting.

## 3. Reacting to what the user does

`show` returns `out.events`, a list of `FlowEvent`s for the frame: `Connected`, `Reconnected`, `NodesDragStopped`, `Deleted`, `SelectionChanged`, `ParentChanged`, `PulseArrived` and so on. The state is already updated when you see them; use the events to mirror changes into your own model or to trigger work.

`out.pane` and `out.nodes` are egui `Response`s, so you can attach context menus or tooltips.

## 4. Options

`Flow::options(FlowOptions { .. })` takes a struct with a default for every field, so override only what you need:

```rust
use egui_flow::{Background, EdgeKind, FlowOptions, HandleVisibility};

let options = FlowOptions {
    background: Background::Lines,
    default_edge_kind: EdgeKind::SmoothStep,
    handle_visibility: HandleVisibility::OnHover,
    alignment_guides: true,
    minimap: true,
    ..Default::default()
};
```

## 5. Handles and wires

By default a node has a target on the left and a source on the right. Return your own from `FlowViewer::handles`; each has an id, a side and an offset along that side:

```rust
use egui_flow::{Handle, HandleId, Side};

fn handles(&self, _node: &Node<String>) -> Vec<Handle> {
    vec![
        Handle::target(Handle::DEFAULT_TARGET, Side::Left),
        Handle::source(HandleId(10), Side::Right).with_offset(0.3),
        Handle::source(HandleId(11), Side::Right).with_offset(0.7),
    ]
}
```

- A handle takes any number of wires; only an identical connection is refused.
- Add `.along()` to let wires land anywhere on a side (bus bars). See `cargo run --example busbar`.
- Veto connections with `FlowViewer::can_connect`.
- Style an edge through `state.edge_mut(id)`: `kind`, `line_style`, `color`, `width`, `arrow`, `label`, `animated`.

## 6. Undo, redo, copy and paste

`Editor` keeps the history and clipboard. Feed it the events and it handles Ctrl/Cmd+Z, Shift+Z, C, X, V and D:

```rust
let mut editor = Editor::new(&state);          // once
// every frame
let out = Flow::new("graph").show(ui, &mut state, &mut viewer);
editor.process(&mut state, &out.events);
```

Changes you make in code (editing node data, changing an edge's style) are not seen: call `editor.commit(&state)` afterwards to make them undoable.

## 7. Groups

```rust
let group = state.add_group(egui::pos2(0.0, 0.0), egui::vec2(320.0, 220.0), data);
let child = state.add_node(egui::pos2(20.0, 60.0), data);   // relative to the group
state.set_parent(child, Some(group));
```

Dragging a group's header moves its members, the header toggle collapses it, and dropping a node on a group adopts it (`FlowOptions::group_drop`). Refuse drops with `FlowViewer::can_join_group`; keep a deleted group's members with `FlowOptions::group_delete`; keep a member inside with `Node::constrained()`.

## 8. Layout, themes and animation

```rust
use egui_flow::{FlowTheme, LayoutOptions};

state.auto_layout_animated(&LayoutOptions::default(), 0.5); // glides into layered order
state.fit_view_animated(0.4);

let options = FlowOptions { theme: FlowTheme::blueprint(), ..Default::default() };
```

Pulses animate along edges: `state.pulse_edge(edge, PulseStyle { label: Some("0x1A4".into()), .. })` and `state.pulse_route(start, &edges, style)` for several hops; `FlowEvent::PulseArrived` fires when a leg ends.

## 9. Testing your integration

The canvas runs headless in egui's `Context::run`, so you can drive it with synthetic pointer events and inspect the painted `Shape`s. The tests in `src/view.rs` do exactly that and are the best examples.

## Where next

- [README](../README.md): every feature, with screenshots and the tour.
- `cargo doc --open`: the API reference.
- Examples: `basic`, `gateway`, `busbar`, `icons`.
- [CHANGELOG](../CHANGELOG.md): what changed in each release.
