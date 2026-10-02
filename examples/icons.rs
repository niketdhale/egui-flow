//! Gallery of the built-in icons. Run with: `cargo run --example icons`

use egui_flow::{Icon, icon, icon_button};

struct App {
    clicks: u32,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("egui-flow icons");
                egui::widgets::global_theme_preference_switch(ui);
            });
            ui.label("Painter-drawn: no font or image assets, tinted with the text colour.");
            ui.separator();

            egui::Grid::new("icons")
                .num_columns(5)
                .spacing([24.0, 10.0])
                .show(ui, |ui| {
                    ui.strong("Name");
                    for size in [12.0, 16.0, 24.0, 40.0] {
                        ui.strong(format!("{size}px"));
                    }
                    ui.end_row();
                    for i in Icon::ALL {
                        ui.monospace(format!("{i:?}"));
                        for size in [12.0, 16.0, 24.0, 40.0] {
                            icon(ui, i, size);
                        }
                        ui.end_row();
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("icon_button:");
                for i in [Icon::Plus, Icon::Minus, Icon::Check, Icon::Close] {
                    if icon_button(ui, i, 16.0).clicked() {
                        self.clicks += 1;
                    }
                }
                ui.label(format!("clicked {} times", self.clicks));
            });
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "egui-flow icons",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(App { clicks: 0 }))),
    )
}
