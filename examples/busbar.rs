//! Bus bars, routing around nodes and group rules.
//!
//! Run with: `cargo run --example busbar`
//!
//! * Drag from just outside the bar's top or bottom edge, or onto it: wires attach where you
//!   drop them (`Handle::along`), and several can share the bar.
//! * Tick "Route around nodes" to make step edges go round the Gateway.
//! * The Cluster group refuses the bar (`FlowViewer::can_join_group`); try dragging the bar in.
//! * Select the Cluster and press Delete, with "Keep members" on or off (`group_delete`).

use egui_flow::{
    Connection, EdgeKind, Flow, FlowEvent, FlowOptions, FlowState, FlowViewer, GroupDelete, Handle,
    HandleId, Node, Side,
};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Bus,
    Feeder,
    Load,
    Plain,
    Group,
}

type Data = (&'static str, Kind);

struct Viewer;

impl FlowViewer<Data, ()> for Viewer {
    fn node_ui(&mut self, ui: &mut egui::Ui, node: &mut Node<Data>) {
        ui.label(node.data.0);
    }

    fn handles(&self, node: &Node<Data>) -> Vec<Handle> {
        match node.data.1 {
            // One handle per side, each taking any number of wires anywhere along it.
            Kind::Bus => vec![
                Handle::target(HandleId(0), Side::Top).along(),
                Handle::source(HandleId(1), Side::Bottom).along(),
            ],
            Kind::Feeder => vec![Handle::source(HandleId(1), Side::Bottom)],
            Kind::Load => vec![Handle::target(HandleId(0), Side::Top)],
            Kind::Plain => vec![
                Handle::target(Handle::DEFAULT_TARGET, Side::Left),
                Handle::source(Handle::DEFAULT_SOURCE, Side::Right),
            ],
            Kind::Group => vec![],
        }
    }

    /// The bus bar belongs at the top level, so the cluster refuses it.
    fn can_join_group(&self, node: &Node<Data>, _group: &Node<Data>) -> bool {
        node.data.1 != Kind::Bus
    }
}

struct App {
    state: FlowState<Data, ()>,
    avoid: bool,
    keep_members: bool,
}

impl App {
    fn new() -> Self {
        let mut state: FlowState<Data, ()> = FlowState::new();
        let bus = state.add_node(egui::pos2(100.0, 230.0), ("Powertrain bus", Kind::Bus));
        state.node_mut(bus).unwrap().fixed_size = Some(egui::vec2(460.0, 24.0));

        let wire = |state: &mut FlowState<Data, ()>, c: Connection, so, to| {
            let e = state.add_edge_at(c, so, to, ()).unwrap();
            let edge = state.edge_mut(e).unwrap();
            edge.kind = Some(EdgeKind::SmoothStep);
            edge.arrow = true;
        };
        for (i, (name, at)) in [("Engine", 0.12), ("Brake", 0.42), ("Gearbox", 0.9)]
            .into_iter()
            .enumerate()
        {
            let n = state.add_node(
                egui::pos2(60.0 + i as f32 * 190.0, 60.0),
                (name, Kind::Feeder),
            );
            let c = Connection {
                source: n,
                source_handle: HandleId(1),
                target: bus,
                target_handle: HandleId(0),
            };
            wire(&mut state, c, None, Some(at));
        }
        for (i, (name, at)) in [("Dashboard", 0.25), ("Diag tool", 0.55), ("Logger", 0.8)]
            .into_iter()
            .enumerate()
        {
            let n = state.add_node(
                egui::pos2(80.0 + i as f32 * 190.0, 380.0),
                (name, Kind::Load),
            );
            let c = Connection {
                source: bus,
                source_handle: HandleId(1),
                target: n,
                target_handle: HandleId(0),
            };
            wire(&mut state, c, Some(at), None);
        }

        // A source, a wall and a target in a row, joined by a step edge.
        let a = state.add_node(egui::pos2(640.0, 150.0), ("Source", Kind::Plain));
        let gateway = state.add_node(egui::pos2(740.0, 90.0), ("Gateway", Kind::Plain));
        state.node_mut(gateway).unwrap().fixed_size = Some(egui::vec2(110.0, 190.0));
        let b = state.add_node(egui::pos2(920.0, 150.0), ("Target", Kind::Plain));
        let e = state.connect(a, b, ()).unwrap();
        let edge = state.edge_mut(e).unwrap();
        edge.kind = Some(EdgeKind::SmoothStep);
        edge.arrow = true;

        let cluster = state.add_group(
            egui::pos2(640.0, 330.0),
            egui::vec2(300.0, 160.0),
            ("Cluster", Kind::Group),
        );
        let inside = state.add_node(egui::pos2(30.0, 60.0), ("Speedo", Kind::Plain));
        state.set_parent(inside, Some(cluster));

        Self {
            state,
            avoid: true,
            keep_members: true,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.avoid, "Route around nodes");
                ui.checkbox(
                    &mut self.keep_members,
                    "Keep members when a group is deleted",
                );
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let options = FlowOptions {
                    avoid_nodes: self.avoid,
                    group_delete: if self.keep_members {
                        GroupDelete::KeepMembers
                    } else {
                        GroupDelete::DeleteMembers
                    },
                    fit_view_on_init: true,
                    ..Default::default()
                };
                let out =
                    Flow::new("busbar")
                        .options(options)
                        .show(ui, &mut self.state, &mut Viewer);
                for event in out.events {
                    if let FlowEvent::Connected(e) = event
                        && let Some(edge) = self.state.edge_mut(e)
                    {
                        edge.kind = Some(EdgeKind::SmoothStep);
                        edge.arrow = true;
                    }
                }
            });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "egui-flow: bus bars",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(App::new()))),
    )
}
