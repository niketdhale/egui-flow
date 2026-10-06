//! A CAN-gateway style graph showing edge reconnecting, route pulses and
//! connected-node highlighting. Run with: `cargo run --example gateway`
//!
//! * select an edge, then drag the ring on either end to another handle
//! * "Send frame" animates ECU → bus → gateway → bus → ECU as one route
//! * select a node and drag its corner or edges to resize it
//! * Ctrl/Cmd+Z / Shift+Z undo and redo, Ctrl/Cmd+C / V / X / D copy, paste, cut, duplicate
//! * drag a node near another to see alignment guides; arrow keys nudge selected nodes
//! * the Powertrain group: drag its header, collapse it, drop nodes in or out; "Group" wraps the selection
//! * members of Powertrain are constrained: they cannot be dragged out of it
//! * "Auto layout" arranges the graph by its edges (tick "Vertical" for top to bottom)
//! * the Theme menu restyles the canvas; deleted nodes fade out
//! * zoom in far: text stays sharp (untick "Crisp text" to compare)
//! * "Highlight connected" dims everything unrelated to the selected/hovered node

use egui::{Color32, Ui};
use egui_flow::{
    ArrowStyle, Background, EdgeId, EdgeKind, Editor, Flow, FlowEvent, FlowOptions, FlowState,
    FlowTheme, FlowViewer, Handle, LayoutDirection, LayoutOptions, LineStyle, Node, NodeId,
    PulseShape, PulseStyle, Side,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    Ecu,
    Bus,
    Gateway,
    Group,
}

#[derive(Clone)]
pub struct Data {
    pub role: Role,
    pub name: &'static str,
}

struct Viewer;

impl FlowViewer<Data, ()> for Viewer {
    fn node_ui(&mut self, ui: &mut Ui, node: &mut Node<Data>) {
        let text = egui::RichText::new(node.data.name).strong();
        match node.data.role {
            // Leave room on the right for the collapse toggle the canvas draws.
            Role::Group => {
                ui.horizontal(|ui| {
                    ui.label(text.color(Color32::from_rgb(170, 190, 230)));
                    ui.add_space(28.0);
                })
                .response
            }
            Role::Ecu => ui.label(text),
            Role::Bus => ui.label(text.color(Color32::from_rgb(230, 180, 70))),
            Role::Gateway => ui.label(text.color(Color32::from_rgb(120, 190, 255))),
        };
    }

    fn node_frame(&self, ui: &Ui, node: &Node<Data>) -> egui::Frame {
        if node.data.role == Role::Group {
            return egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(70, 100, 160, 34))
                .stroke(egui::Stroke::new(1.5_f32, Color32::from_rgb(90, 120, 190)))
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(12, 6));
        }
        let accent = match node.data.role {
            Role::Group => unreachable!(),
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

    fn handles(&self, node: &Node<Data>) -> Vec<Handle> {
        if node.data.role == Role::Group {
            return Vec::new();
        }
        vec![
            Handle::target(Handle::DEFAULT_TARGET, Side::Left),
            Handle::source(Handle::DEFAULT_SOURCE, Side::Right),
        ]
    }

    fn minimap_color(&self, node: &Node<Data>) -> Option<Color32> {
        Some(match node.data.role {
            Role::Group => Color32::from_rgb(90, 120, 190),
            Role::Ecu => Color32::from_rgb(110, 160, 110),
            Role::Bus => Color32::from_rgb(200, 150, 50),
            Role::Gateway => Color32::from_rgb(90, 150, 220),
        })
    }
}

pub struct App {
    pub state: FlowState<Data, ()>,
    pub editor: Editor<Data, ()>,
    pub highlight: bool,
    pub crisp: bool,
    /// 0 egui's own colours, 1 dark, 2 light, 3 blueprint.
    pub theme: usize,
    pub vertical: bool,
    pub engine: NodeId,
    pub route: Vec<EdgeId>,
    pub brake_edge: EdgeId,
    pub nodes: Vec<(&'static str, NodeId)>,
    pub arrived: u32,
    pub log: String,
    /// Where the canvas sits on screen (set each frame; handy for test drivers).
    pub canvas: egui::Rect,
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
        // Not wired up yet: drag from its right handle onto a bus to connect it.
        let diag = add(&mut s, Role::Ecu, "Diag tool", 0.0, 330.0);
        // A group around the two ECUs on the left: positions inside it are relative to it.
        let powertrain = s.add_group(
            egui::pos2(-30.0, -12.0),
            egui::vec2(150.0, 275.0),
            Data {
                role: Role::Group,
                name: "Powertrain",
            },
        );
        s.set_parent(engine, Some(powertrain));
        s.node_mut(engine).unwrap().constrain_to_parent = true;
        s.node_mut(brake).unwrap().constrain_to_parent = true;
        s.set_parent(brake, Some(powertrain));

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
        // Arrowhead shapes, a two-way link and styled, positioned labels.
        s.edge_mut(e1).unwrap().arrow_style = ArrowStyle::Open;
        s.edge_mut(brake_edge).unwrap().arrow_style = ArrowStyle::Circle;
        s.edge_mut(e3).unwrap().arrow_style = ArrowStyle::Diamond;
        let two_way = s.edge_mut(e5).unwrap();
        two_way.arrow_at_source = true;
        for (id, text, pos, color) in [
            (e3, "500 kbit/s", 0.5, Color32::from_rgb(230, 180, 70)),
            (e4, "filtered", 0.5, Color32::from_rgb(120, 190, 255)),
            (e1, "RPM", 0.7, Color32::from_rgb(140, 200, 140)),
        ] {
            let e = s.edge_mut(id).unwrap();
            e.label = Some(text.into());
            e.label_style = egui_flow::EdgeLabelStyle {
                position: pos,
                size: 11.0,
                color: Some(color),
                background: Some(Color32::from_rgb(28, 28, 34)),
            };
        }
        s.fit_view();
        Self {
            editor: Editor::new(&s),
            state: s,
            highlight: false,
            crisp: true,
            theme: 0,
            vertical: false,
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
                ("diag", diag),
                ("powertrain", powertrain),
            ],
            arrived: 0,
            canvas: egui::Rect::NOTHING,
            log: "select a node: drag its corner to resize; Ctrl+C / Ctrl+V copy and paste; Ctrl+Z undoes".into(),
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
                radius: 6.5,
                label: Some("0x1A0 RPM".into()),
                tag,
                ..Default::default()
            },
        );
    }

    pub fn flow_theme(&self) -> FlowTheme {
        match self.theme {
            1 => FlowTheme::dark(),
            2 => FlowTheme::light(),
            3 => FlowTheme::blueprint(),
            _ => FlowTheme::default(),
        }
    }

    /// Arrange the top-level nodes by their edges, gliding there.
    pub fn auto_layout(&mut self) {
        let options = LayoutOptions {
            direction: if self.vertical {
                LayoutDirection::TopToBottom
            } else {
                LayoutDirection::LeftToRight
            },
            ..Default::default()
        };
        self.state.auto_layout_animated(&options, 0.7);
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("layout_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Auto layout").clicked() {
                    self.auto_layout();
                }
                ui.checkbox(&mut self.vertical, "Vertical");
                ui.separator();
                ui.label("Theme:");
                for (i, name) in ["egui", "Dark", "Light", "Blueprint"]
                    .into_iter()
                    .enumerate()
                {
                    ui.selectable_value(&mut self.theme, i, name);
                }
            });
        });
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.editor.can_undo(), egui::Button::new("Undo"))
                    .clicked()
                {
                    self.editor.undo(&mut self.state);
                }
                if ui
                    .add_enabled(self.editor.can_redo(), egui::Button::new("Redo"))
                    .clicked()
                {
                    self.editor.redo(&mut self.state);
                }
                ui.separator();
                if ui.button("Send frame").clicked() {
                    self.send_frame(0.9, 1);
                }
                ui.checkbox(&mut self.highlight, "Highlight connected");
                ui.checkbox(&mut self.crisp, "Crisp text");
                if ui.button("Group").clicked()
                    && self
                        .state
                        .group_selected(
                            Data {
                                role: Role::Group,
                                name: "Group",
                            },
                            24.0,
                            30.0,
                        )
                        .is_some()
                {
                    self.editor.commit(&self.state);
                }
                if ui.button("Ungroup").clicked() {
                    let selected = self.state.selected_nodes();
                    let mut changed = false;
                    for id in selected {
                        changed |= self.state.ungroup(id);
                    }
                    if changed {
                        self.editor.commit(&self.state);
                    }
                }
                ui.separator();
                ui.label("Edges:");
                for (kind, name) in [
                    (EdgeKind::Bezier, "Bezier"),
                    (EdgeKind::Straight, "Straight"),
                    (EdgeKind::Step, "Step"),
                    (EdgeKind::SmoothStep, "Smooth"),
                ] {
                    let current = self.state.edges.iter().all(|e| e.kind == Some(kind));
                    if ui.selectable_label(current, name).clicked() {
                        self.state
                            .edges
                            .iter_mut()
                            .for_each(|e| e.kind = Some(kind));
                    }
                }
                ui.label("Line:");
                for (style, name) in [
                    (LineStyle::Solid, "Solid"),
                    (LineStyle::Dashed, "Dashed"),
                    (LineStyle::Dotted, "Dotted"),
                ] {
                    let current = self.state.edges.iter().all(|e| e.line_style == style);
                    if ui.selectable_label(current, name).clicked() {
                        self.state
                            .edges
                            .iter_mut()
                            .for_each(|e| e.line_style = style);
                    }
                }
                ui.separator();
                ui.label(format!("frames delivered: {}", self.arrived));
            });
        });
        egui::TopBottomPanel::bottom("log").show(ctx, |ui| {
            ui.strong(&self.log);
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.canvas = ui.max_rect();
                let opts = FlowOptions {
                    background: Background::Dots,
                    highlight_connected: self.highlight,
                    crisp_text: self.crisp,
                    theme: self.flow_theme(),
                    alignment_guides: true,
                    minimap: true,
                    fit_view_on_init: true,
                    ..Default::default()
                };
                let out = Flow::new("gateway")
                    .options(opts)
                    .show(ui, &mut self.state, &mut Viewer);
                self.editor.process(&mut self.state, &out.events);
                for event in out.events {
                    match event {
                        // Style edges drawn by hand like the ones built in `new`.
                        FlowEvent::Connected(id) => {
                            if let Some(e) = self.state.edge_mut(id) {
                                e.kind = Some(EdgeKind::SmoothStep);
                                e.arrow = true;
                                e.color = Some(Color32::from_rgb(150, 150, 170));
                            }
                            self.log = "connected".into();
                        }
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
