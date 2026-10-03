//! A CAN-gateway style graph showing edge reconnecting, route pulses and
//! connected-node highlighting. Run with: `cargo run --example gateway`
//!
//! * select an edge, then drag the ring on either end to another handle
//! * "Send frame" animates ECU → bus → gateway → bus → ECU as one route
//! * "Highlight connected" dims everything unrelated to the selected/hovered node

use egui::{Color32, Ui};
use egui_flow::{
    Background, EdgeId, EdgeKind, Flow, FlowEvent, FlowOptions, FlowState, FlowViewer, Handle,
    LineStyle, Node, NodeId, PulseShape, PulseStyle, Side,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    Ecu,
    Bus,
    Gateway,
}

pub struct Data {
    pub role: Role,
    pub name: &'static str,
}

struct Viewer;

impl FlowViewer<Data, ()> for Viewer {
    fn node_ui(&mut self, ui: &mut Ui, node: &mut Node<Data>) {
        let text = egui::RichText::new(node.data.name).strong();
        match node.data.role {
            Role::Ecu => ui.label(text),
            Role::Bus => ui.label(text.color(Color32::from_rgb(230, 180, 70))),
            Role::Gateway => ui.label(text.color(Color32::from_rgb(120, 190, 255))),
        };
    }

    fn node_frame(&self, ui: &Ui, node: &Node<Data>) -> egui::Frame {
        let accent = match node.data.role {
            Role::Ecu => Color32::from_rgb(110, 160, 110),
            Role::Bus => Color32::from_rgb(200, 150, 50),
            Role::Gateway => Color32::from_rgb(90, 150, 220),
        };
        egui::Frame::new()
            .fill(ui.visuals().window_fill)
            .stroke(egui::Stroke::new(1.5_f32, accent))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(14, 8))
    }

    fn handles(&self, _node: &Node<Data>) -> Vec<Handle> {
        vec![
            Handle::target(Handle::DEFAULT_TARGET, Side::Left),
            Handle::source(Handle::DEFAULT_SOURCE, Side::Right),
        ]
    }

    fn minimap_color(&self, node: &Node<Data>) -> Option<Color32> {
        Some(match node.data.role {
            Role::Ecu => Color32::from_rgb(110, 160, 110),
            Role::Bus => Color32::from_rgb(200, 150, 50),
            Role::Gateway => Color32::from_rgb(90, 150, 220),
        })
    }
}

pub struct App {
    pub state: FlowState<Data, ()>,
    pub highlight: bool,
    pub engine: NodeId,
    pub route: Vec<EdgeId>,
    pub brake_edge: EdgeId,
    pub nodes: Vec<(&'static str, NodeId)>,
    pub arrived: u32,
    pub log: String,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut s = FlowState::new();
        let add = |s: &mut FlowState<Data, ()>, role, name, x, y| {
            s.add_node(egui::pos2(x, y), Data { role, name })
        };
        let engine = add(&mut s, Role::Ecu, "Engine", 0.0, 40.0);
        let brake = add(&mut s, Role::Ecu, "Brake", 0.0, 200.0);
        let can1 = add(&mut s, Role::Bus, "CAN1", 190.0, 120.0);
        let gw = add(&mut s, Role::Gateway, "Gateway", 380.0, 120.0);
        let can2 = add(&mut s, Role::Bus, "CAN2", 590.0, 120.0);
        let dash = add(&mut s, Role::Ecu, "Dashboard", 770.0, 120.0);

        let wire = |s: &mut FlowState<Data, ()>, a, b, kind| {
            let id = s.connect(a, b, ()).unwrap();
            let e = s.edge_mut(id).unwrap();
            e.kind = Some(kind);
            e.arrow = true;
            e.color = Some(Color32::from_rgb(150, 150, 170));
            id
        };
        let e1 = wire(&mut s, engine, can1, EdgeKind::SmoothStep);
        let brake_edge = wire(&mut s, brake, can1, EdgeKind::SmoothStep);
        let e3 = wire(&mut s, can1, gw, EdgeKind::Straight);
        let e4 = wire(&mut s, gw, can2, EdgeKind::Straight);
        let e5 = wire(&mut s, can2, dash, EdgeKind::Straight);
        s.edge_mut(e3).unwrap().line_style = LineStyle::Dashed;
        s.edge_mut(e4).unwrap().line_style = LineStyle::Dashed;
        s.fit_view();
        Self {
            state: s,
            highlight: false,
            engine,
            route: vec![e1, e3, e4, e5],
            brake_edge,
            nodes: vec![
                ("engine", engine),
                ("brake", brake),
                ("can1", can1),
                ("gw", gw),
                ("can2", can2),
                ("dash", dash),
            ],
            arrived: 0,
            log: "select an edge, then drag a ring on its end to another handle".into(),
        }
    }

    pub fn send_frame(&mut self, duration: f32, tag: u64) {
        self.state.pulse_route(
            self.engine,
            &self.route,
            PulseStyle {
                duration,
                shape: PulseShape::Arrow,
                color: Some(Color32::from_rgb(255, 190, 60)),
                radius: 5.0,
                label: Some("0x1A0 RPM".into()),
                tag,
                ..Default::default()
            },
        );
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Send frame").clicked() {
                    self.send_frame(0.9, 1);
                }
                ui.checkbox(&mut self.highlight, "Highlight connected");
                ui.separator();
                ui.label(format!("frames delivered: {}", self.arrived));
            });
        });
        egui::TopBottomPanel::bottom("log").show(ctx, |ui| {
            ui.label(&self.log);
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let opts = FlowOptions {
                    background: Background::Dots,
                    highlight_connected: self.highlight,
                    fit_view_on_init: true,
                    ..Default::default()
                };
                let out = Flow::new("gateway")
                    .options(opts)
                    .show(ui, &mut self.state, &mut Viewer);
                for event in out.events {
                    match event {
                        FlowEvent::Reconnected { new, .. } => {
                            self.log = format!("edge now runs {:?} -> {:?}", new.source, new.target)
                        }
                        FlowEvent::PulseArrived { tag: 1, edge, .. }
                            if Some(&edge) == self.route.last() =>
                        {
                            self.arrived += 1
                        }
                        _ => {}
                    }
                }
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "egui-flow gateway",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(App::new()))),
    )
}
