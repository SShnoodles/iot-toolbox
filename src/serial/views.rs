use eframe::egui;

use super::app::{SendFormat, SerialTool};

pub fn render_main_view(app: &mut SerialTool, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            render_settings_panel(app, ui);
            render_communication_panel(app, ui);
        });
    });

    render_status_bar(app, ctx);
}

fn render_settings_panel(app: &mut SerialTool, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        if ui.button("Refresh Port").clicked() {
            app.refresh_ports();
        }
        egui::ComboBox::from_label("Port")
            .selected_text(
                app.selected_port
                    .clone()
                    .unwrap_or_else(|| "Select a port".to_string()),
            )
            .show_ui(ui, |ui| {
                for port in &app.available_ports {
                    ui.selectable_value(
                        &mut app.selected_port,
                        Some(port.port_name.clone()),
                        &port.port_name,
                    );
                }
            });

        ui.label("Baud Rate");
        ui.add(egui::Slider::new(&mut app.baud_rate, 9600..=115200));

        ui.label("Data Bits");
        ui.radio_value(&mut app.data_bits, serialport::DataBits::Eight, "8");
        ui.radio_value(&mut app.data_bits, serialport::DataBits::Seven, "7");

        ui.label("Parity");
        ui.radio_value(&mut app.parity, serialport::Parity::None, "None");
        ui.radio_value(&mut app.parity, serialport::Parity::Odd, "Odd");
        ui.radio_value(&mut app.parity, serialport::Parity::Even, "Even");

        ui.label("Stop Bits");
        ui.radio_value(&mut app.stop_bits, serialport::StopBits::One, "1");
        ui.radio_value(&mut app.stop_bits, serialport::StopBits::Two, "2");

        if ui.button("Connect").clicked() {
            app.connect();
        }
        if ui.button("Disconnect").clicked() {
            app.disconnect();
        }
    });
}

fn render_communication_panel(app: &mut SerialTool, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Send Format:");
            egui::ComboBox::from_label("Format")
                .selected_text(match app.send_format {
                    SendFormat::ASCII => "ASCII",
                    SendFormat::Hex => "Hex",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.send_format, SendFormat::ASCII, "ASCII");
                    ui.selectable_value(&mut app.send_format, SendFormat::Hex, "Hex");
                });
        });
        ui.horizontal(|ui| {
            ui.add_sized([ui.available_width() - 50.0, 0.0], egui::TextEdit::multiline(&mut app.input_text));
            if ui.button("Send").clicked() {
                app.send_data();
            }
        });

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for log in &app.logs {
                ui.label(log);
            }
        });
    });
}

fn render_status_bar(app: &mut SerialTool, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("Status Bar").show(ctx, |ui| {
        ui.label(&app.status);
    });
}