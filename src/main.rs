mod modbus;
mod serial;

use eframe::egui::{self};
use serial::app::SerialTool;

use crate::modbus::app::ModbusTool;

const APP_FULL: &str = concat!("IoT Toolbox", " ", "V1.0.0");

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]), // 720p
        ..Default::default()
    };
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
    modbus: ModbusTool,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            tab: MainTab::Serial,
            serial: SerialTool::new(),
            modbus: ModbusTool::new(),
        }
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, MainTab::Serial, "Serial");
                ui.selectable_value(&mut self.tab, MainTab::Modbus, "Modbus");
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            MainTab::Serial => self.serial.ui(ctx),
            MainTab::Modbus => self.modbus.ui(ui),
        });
    }
}
