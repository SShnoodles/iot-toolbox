use eframe::egui;
use serialport::{self, SerialPort, SerialPortInfo};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use super::utils::{hex_to_bytes, bytes_to_hex_string};
use super::views::render_main_view;

#[derive(PartialEq)]
pub enum SendFormat {
    ASCII,
    Hex,
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
    pub send_format: SendFormat
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
            send_format: SendFormat::ASCII,
        }
    }

    pub fn refresh_ports(&mut self) {
        self.available_ports = serialport::available_ports().unwrap_or_default();
    }

    pub fn connect(&mut self) {
        if let Some(port_name) = &self.selected_port {
            match serialport::new(port_name, self.baud_rate)
                .data_bits(self.data_bits)
                .parity(self.parity)
                .stop_bits(self.stop_bits)
                .timeout(Duration::from_millis(10))
                .open()
            {
                Ok(port) => {
                    self.status = format!("Connected to {}", port_name);
                    self.port = Some(Arc::new(Mutex::new(port)));
                }
                Err(e) => {
                    self.status = format!("Connection failed: {}", e);
                }
            }
        } else {
            self.status = "Please select a serial port".to_string();
        }
    }

    pub fn disconnect(&mut self) {
        self.port = None;
        self.status = "Disconnected".to_string();
    }

    pub fn send_data(&mut self) {
        let bytes_to_send = match self.send_format {
            SendFormat::ASCII => self.input_text.clone().into_bytes(),
            SendFormat::Hex => hex_to_bytes(&self.input_text)
        };
        if let Some(port) = &self.port {
            let mut port = port.lock().unwrap();
            if let Err(e) = port.write(&bytes_to_send) {
                self.logs.push(format!("Send failed: {}", e));
            } else {
                self.logs.push(format!("TX: {}", self.input_text));
            }
            self.input_text.clear();
        } else {
            self.logs.push("Not connected to serial port".to_string());
        }
    }

    pub fn receive_data(&mut self) {
        if let Some(port) = &self.port {
            let mut port = port.lock().unwrap();
            let mut buffer = [0; 1024];
            match port.read(&mut buffer) {
                Ok(bytes_read) => {
                    if bytes_read > 0 {
                        let received_hex = bytes_to_hex_string(&buffer[..bytes_read]);
                        self.logs.push(format!("RX: {}", received_hex));
                    }
                }
                Err(_) => {}
            }
        }
    }
}

impl eframe::App for SerialTool {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::light());
        render_main_view(self, ctx);
        self.receive_data();
    }
}