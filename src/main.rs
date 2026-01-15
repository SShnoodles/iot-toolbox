mod serial;
mod modbus;

use serial::app::SerialTool;
use eframe::egui::{self};

const APP_FULL: &str = concat!("IoT Toolbox", " ", "V1.0.0");

fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        APP_FULL,
        options,
        Box::new(|_cc| Ok(Box::new(AppState::default()))),
    );
}

#[derive(PartialEq, Default)]
enum MainTab {
    #[default]
    Serial,
    Modbus,
}

struct AppState {
    tab: MainTab,
    serial: SerialTool,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            tab: MainTab::Serial,
            serial: SerialTool::new()
        }
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.tab == MainTab::Serial, "Serial")
                    .clicked()
                {
                    self.tab = MainTab::Serial;
                }

                if ui
                    .selectable_label(self.tab == MainTab::Modbus, "Modbus")
                    .clicked()
                {
                    self.tab = MainTab::Modbus;
                }
            });
        });

        match self.tab {
            MainTab::Serial => self.serial.ui(ctx),
            MainTab::Modbus => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label("Modbus UI not implemented yet");
                    });
                });
            }
        }
    }
}
