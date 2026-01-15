use eframe::egui;
use serialport::{self, SerialPort, SerialPortInfo};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use super::utils::{parse_hex_string, bytes_to_hex_string, now_timestamp};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};


#[derive(Clone, Copy, PartialEq)]
pub enum SendFormat {
    Hex,
    Ascii,
}

pub struct SerialTool {
    // Serial port settings
    pub available_ports: Vec<SerialPortInfo>,
    pub selected_port: Option<String>,
    pub baud_rate: u32,
    pub data_bits: serialport::DataBits,
    pub parity: serialport::Parity,
    pub stop_bits: serialport::StopBits,
    // Logs
    pub logs: Vec<String>,
    // Input field
    pub input_text: String,
    // Connection status
    pub status: String,
    // Serial port connection
    pub port: Option<Arc<Mutex<Box<dyn SerialPort>>>>,
    pub send_format: SendFormat,
    // Receiver
    rx: Option<Receiver<Vec<u8>>>,
    read_running: Arc<AtomicBool>,
}

impl SerialTool {
    pub fn new() -> Self {
        let available_ports = serialport::available_ports().unwrap_or_default();
        SerialTool {
            available_ports,
            selected_port: None,
            baud_rate: 9600,
            data_bits: serialport::DataBits::Eight,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,
            logs: vec![],
            input_text: String::new(),
            status: "Disconnected".to_string(),
            port: None,
            send_format: SendFormat::Hex,
            rx: None,
            read_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        // receive message
        if let Some(rx) = &self.rx {
            while let Ok(data) = rx.try_recv() {
                let ts = now_timestamp();

                let display = match self.send_format {
                    SendFormat::Hex => bytes_to_hex_string(&data),
                    SendFormat::Ascii => {
                        String::from_utf8_lossy(&data)
                            .replace('\r', "\\r")
                            .replace('\n', "\\n")
                    }
                };

                self.logs.push(format!("{} RX <- {}", ts, display));

                ctx.request_repaint();
            }
        }
        // bottom
        egui::TopBottomPanel::bottom("serial_status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.monospace(&self.status);
            });
        });

        // center
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // config
                egui::Frame::group(ui.style())
                    .show(ui, |ui| {
                        self.ui_config(ui);
                    });

                ui.add_space(6.0);

                // send
                egui::Frame::group(ui.style())
                    .show(ui, |ui| {
                        ui.label("Send");
                        self.ui_sender(ui);
                    });

                ui.add_space(6.0);

                // log
                let available_height = ui.available_height();

                egui::Frame::group(ui.style())
                    .show(ui, |ui| {
                        ui.set_min_height(available_height);
                        self.ui_logs(ui);
                    });
            });
        });
    }

    pub fn ui_config(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        // -------------------------------
        // Refresh + Port
        // -------------------------------
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("Refresh Ports").color(egui::Color32::BLUE)).clicked() {
                self.available_ports = serialport::available_ports().unwrap_or_default();
                self.selected_port = Some("Select Port".to_string());
            }

            egui::ComboBox::from_label("")
                .width(220.0)
                .selected_text(
                    self.selected_port
                        .clone()
                        .unwrap_or_else(|| "Select Port".into()),
                )
                .show_ui(ui, |ui| {
                    for p in &self.available_ports {
                        if ui
                            .selectable_label(
                                self.selected_port.as_deref() == Some(&p.port_name),
                                &p.port_name,
                            )
                            .clicked()
                        {
                            self.selected_port = Some(p.port_name.clone());
                        }
                    }
                });
        });

        ui.add_space(6.0);

        // -------------------------------
        // Baud + DataBits + Parity + StopBits
        // -------------------------------
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Baud rate:").strong());
            ui.add(
                egui::DragValue::new(&mut self.baud_rate)
                    .speed(100)
                    .range(1200..=921600),
            );

            ui.separator();

            ui.label(egui::RichText::new("Data bits:").strong());
            ui.radio_value(&mut self.data_bits, serialport::DataBits::Five, "5");
            ui.radio_value(&mut self.data_bits, serialport::DataBits::Six, "6");
            ui.radio_value(&mut self.data_bits, serialport::DataBits::Seven, "7");
            ui.radio_value(&mut self.data_bits, serialport::DataBits::Eight, "8");

            ui.label(egui::RichText::new("Parity:").strong());
            ui.radio_value(&mut self.parity, serialport::Parity::None, "None");
            ui.radio_value(&mut self.parity, serialport::Parity::Odd, "Odd");
            ui.radio_value(&mut self.parity, serialport::Parity::Even, "Even");

            ui.separator();

            ui.label(egui::RichText::new("Stop bits:").strong());
            ui.radio_value(&mut self.stop_bits, serialport::StopBits::One, "1");
            ui.radio_value(&mut self.stop_bits, serialport::StopBits::Two, "2");
        });

        ui.add_space(4.0);

        // -------------------------------
        // Connect / Disconnect
        // -------------------------------
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("Connect").color(egui::Color32::BLUE)).clicked() {
                self.connect();
            }
            if ui.button(egui::RichText::new("Disconnect").color(egui::Color32::RED)).clicked() {
                self.disconnect();
            }
        });
    }

    pub fn ui_sender(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.radio_value(&mut self.send_format, SendFormat::Hex, "HEX");
            ui.radio_value(&mut self.send_format, SendFormat::Ascii, "ASCII");

            ui.separator();

            ui.add_sized(
                [ui.available_width() - 140.0, 24.0],
                egui::TextEdit::multiline(&mut self.input_text)
                    .hint_text(match self.send_format {
                        SendFormat::Hex => "48 65 6C 6C 6F",
                        SendFormat::Ascii => "Hello",
                    }),
            );

            if ui.button(egui::RichText::new("Send").color(egui::Color32::BLUE)).clicked() {
                self.send();
            }
        });
    }

    pub fn ui_logs(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        ui.label("Logs");

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for log in &self.logs {
                    ui.monospace(log);
                }
            });
    }

    pub fn connect(&mut self) {
        let Some(port_name) = &self.selected_port else {
            self.status = "No port selected".into();
            return;
        };

        match serialport::new(port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .timeout(Duration::from_millis(100))
            .open()
        {
            Ok(port) => {
                let port = Arc::new(Mutex::new(port));

                let (tx, rx) = mpsc::channel();

                Self::start_read_thread(
                    port.clone(),
                    tx,
                    self.read_running.clone(),
                );

                self.port = Some(port);
                self.rx = Some(rx);

                self.status = format!("Connected: {}", port_name);
                self.logs.push("Connected".into());
            }
            Err(e) => {
                self.status = format!("Connect failed: {e}");
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.read_running.store(false, Ordering::SeqCst);

        self.port = None;
        self.rx = None;

        self.status = "Disconnected".into();
        self.logs.push("Disconnected".into());
    }

    pub fn send(&mut self) {
        let Some(port) = &self.port else {
            self.logs.push("TX -- Not connected".into());
            return;
        };

        let bytes = match self.send_format {
            SendFormat::Hex => match parse_hex_string(&self.input_text) {
                Ok(b) => b,
                Err(e) => {
                    self.logs.push(format!("TX -- HEX error: {}", e));
                    return;
                }
            },
            SendFormat::Ascii => self.input_text.as_bytes().to_vec(),
        };

        let mut port = port.lock().unwrap();
        if let Err(e) = port.write_all(&bytes) {
            self.logs.push(format!("TX -- Send failed: {}", e));
            return;
        }

        let ts = now_timestamp();
        let display = match self.send_format {
            SendFormat::Hex => bytes_to_hex_string(&bytes),
            SendFormat::Ascii => String::from_utf8_lossy(&bytes).to_string(),
        };

        self.logs.push(format!("{} TX -> {}", ts, display));
    }

    pub fn start_read_thread(port: Arc<Mutex<Box<dyn SerialPort>>>, tx: Sender<Vec<u8>>, running: Arc<AtomicBool>) {
        running.store(true, Ordering::SeqCst);

        thread::spawn(move || {
            let mut buf = [0u8; 256];
            let mut frame: Vec<u8> = Vec::new();

            // setting timeout
            let frame_timeout = Duration::from_millis(30);
            let mut last_recv = Instant::now();

            while running.load(Ordering::SeqCst) {
                let n = {
                    let mut port = match port.lock() {
                        Ok(p) => p,
                        Err(_) => break,
                    };

                    match port.read(&mut buf) {
                        Ok(n) => n,
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => 0,
                        Err(_) => break,
                    }
                };

                if n > 0 {
                    frame.extend_from_slice(&buf[..n]);
                    last_recv = Instant::now();
                    continue;
                }

                // timeout -> finish
                if !frame.is_empty() && last_recv.elapsed() >= frame_timeout {
                    let completed = std::mem::take(&mut frame);
                    let _ = tx.send(completed);
                }

                thread::sleep(Duration::from_millis(2));
            }

            // reissue residual data
            if !frame.is_empty() {
                let _ = tx.send(frame);
            }
        });
    }
    
}