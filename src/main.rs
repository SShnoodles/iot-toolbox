mod serial;
mod modbus;

use serial::app::SerialTool;
use eframe::egui::{self, CentralPanel, Context, Ui};
use modbus::app::ModbusTool;

struct DebuggerApp {
    selected_tab: Tab,
}

#[derive(PartialEq)]
enum Tab {
    Serial,
    Modbus,
}

impl DebuggerApp {
    fn new() -> Self {
        Self {
            selected_tab: Tab::Serial,
        }
    }
}

impl eframe::App for DebuggerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui: &mut Ui| {
                if ui.add(egui::SelectableLabel::new(self.selected_tab == Tab::Serial, "Serial")).clicked() {
                    self.selected_tab = Tab::Serial;
                }
                if ui.add(egui::SelectableLabel::new(self.selected_tab == Tab::Modbus, "Modbus")).clicked() {
                    self.selected_tab = Tab::Modbus;
                }
            });

            ui.separator();

            match self.selected_tab {
                Tab::Serial => SerialTool::new().views(ctx, ui),
                Tab::Modbus => ModbusTool::new().views(ctx, ui),
            }
        });
    }
}


fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "IoT Debugging Tool",
        options,
        Box::new(|_cc| Ok(Box::new(DebuggerApp::new()))),
    );
}