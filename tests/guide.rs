//! The code in docs/getting-started.md must keep compiling against the public API.
#![allow(dead_code, unused_variables)]

use egui_flow::{
    Background, EdgeKind, Editor, Flow, FlowEvent, FlowOptions, FlowState, FlowTheme, FlowViewer,
    Handle, HandleId, HandleVisibility, LayoutOptions, Node, PulseStyle, Side,
};

struct Viewer;
impl FlowViewer<String, ()> for Viewer {
    fn node_ui(&mut self, ui: &mut egui::Ui, node: &mut Node<String>) {
        ui.text_edit_singleline(&mut node.data);
    }

    fn handles(&self, _node: &Node<String>) -> Vec<Handle> {
        vec![
            Handle::target(Handle::DEFAULT_TARGET, Side::Left),
            Handle::source(HandleId(10), Side::Right).with_offset(0.3),
            Handle::source(HandleId(11), Side::Right).with_offset(0.7),
            Handle::target(HandleId(12), Side::Top).along(),
        ]
    }
}

#[test]
fn the_guide_snippets_compile_and_run() {
    let mut state: FlowState<String, ()> = FlowState::new();
    let a = state.add_node(egui::pos2(0.0, 0.0), "a".to_string());
    let b = state.add_node(egui::pos2(250.0, 60.0), "b".to_string());
    let edge = state.connect(a, b, ()).unwrap();

    let options = FlowOptions {
        background: Background::Lines,
        default_edge_kind: EdgeKind::SmoothStep,
        handle_visibility: HandleVisibility::OnHover,
        alignment_guides: true,
        minimap: true,
        theme: FlowTheme::blueprint(),
        ..Default::default()
    };
    let mut editor = Editor::new(&state);

    let group = state.add_group(
        egui::pos2(0.0, 0.0),
        egui::vec2(320.0, 220.0),
        "g".to_string(),
    );
    let child = state.add_node(egui::pos2(20.0, 60.0), "c".to_string());
    state.set_parent(child, Some(group));
    state.auto_layout_animated(&LayoutOptions::default(), 0.5);
    state.fit_view_animated(0.4);
    state.pulse_edge(
        edge,
        PulseStyle {
            label: Some("0x1A4".into()),
            ..Default::default()
        },
    );

    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let out = Flow::new("graph")
                .options(options.clone())
                .show(ui, &mut state, &mut Viewer);
            editor.process(&mut state, &out.events);
            for event in out.events {
                if let FlowEvent::Connected(edge) = event {}
            }
        });
    });
}
