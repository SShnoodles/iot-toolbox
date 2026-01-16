use anyhow::Result;
use serialport::{DataBits, Parity, StopBits};
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};
use tokio_modbus::client::Client;
use tokio_modbus::prelude::*;

#[derive(PartialEq)]
pub enum ModbusMode {
    Tcp,
    Rtu,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ModbusFunction {
    ReadCoils,    // 01
    ReadDiscrete, // 02
    ReadHolding,  // 03
    ReadInput,    // 04
}

pub struct ModbusTool {
    pub mode: ModbusMode,
    pub connected: bool,

    pub tcp_ip: String,
    pub tcp_port: u16,

    pub available_ports: Vec<String>,
    pub selected_port: Option<String>,
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,

    pub slave_id: u8,
    pub function: ModbusFunction,
    pub address: u16,
    pub quantity: u16,

    pub view_rows: usize,       // 10 / 20 / 50
    pub display_format: String, // DEC / HEX / BIN

    pub data: Vec<u16>,

    pub logs: Vec<String>,
    pub scroll_to_bottom: bool,

    pub auto_poll: bool,
    pub rx: Option<Receiver<Vec<u16>>>,
    pub rt: tokio::runtime::Runtime,
}

impl ModbusTool {
    pub fn new() -> Self {
        let available_ports = serialport::available_ports()
            .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
            .unwrap_or_default();

        Self {
            mode: ModbusMode::Tcp,
            connected: false,

            // ===== TCP =====
            tcp_ip: "127.0.0.1".to_string(),
            tcp_port: 502,

            // ===== RTU =====
            available_ports,
            selected_port: None,
            baud_rate: 9600,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,

            // ===== Slave =====
            slave_id: 1,
            function: ModbusFunction::ReadHolding,
            address: 0,
            quantity: 10,

            view_rows: 10,
            display_format: "DEC".to_string(),

            data: Vec::new(),

            logs: Vec::new(),
            scroll_to_bottom: false,

            auto_poll: false,
            rx: None,

            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            self.ui_connection(ui);
            ui.separator();

            self.ui_slave(ui);
            ui.separator();

            self.ui_view(ui);
            ui.separator();

            self.ui_table(ui);
            ui.separator();

            self.ui_logs(ui);
        });

        if let Some(rx) = &self.rx {
            while let Ok(data) = rx.try_recv() {
                self.data = data;
                self.logs.push(format!("RX {} registers", self.data.len()));
                self.scroll_to_bottom = true;
            }
        }

        self.handle_auto_poll();
    }

    fn ui_connection(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Connection");

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, ModbusMode::Tcp, "TCP");
                ui.selectable_value(&mut self.mode, ModbusMode::Rtu, "RTU");
            });

            ui.separator();

            match self.mode {
                ModbusMode::Tcp => self.ui_tcp(ui),
                ModbusMode::Rtu => self.ui_rtu(ui),
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Connect").clicked() {
                    self.logs.push("Connecting...".into());
                    self.scroll_to_bottom = true;
                }
                if ui.button("Disconnect").clicked() {
                    self.logs.push("Disconnected".into());
                    self.scroll_to_bottom = true;
                }
            });
        });
    }

    fn ui_tcp(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("IP");
            ui.text_edit_singleline(&mut self.tcp_ip);

            ui.label("Port");
            ui.add(egui::DragValue::new(&mut self.tcp_port));
        });
    }

    fn ui_rtu(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Port");
            egui::ComboBox::from_id_source("rtu_port")
                .selected_text(
                    self.selected_port
                        .clone()
                        .unwrap_or_else(|| "Select".into()),
                )
                .show_ui(ui, |ui| {
                    for p in &self.available_ports {
                        ui.selectable_value(&mut self.selected_port, Some(p.clone()), p);
                    }
                });

            ui.label("Baud");
            ui.add(egui::DragValue::new(&mut self.baud_rate));
        });

        ui.horizontal(|ui| {
            ui.radio_value(&mut self.data_bits, serialport::DataBits::Eight, "8");
            ui.radio_value(&mut self.parity, serialport::Parity::None, "N");
            ui.radio_value(&mut self.parity, serialport::Parity::Even, "E");
            ui.radio_value(&mut self.parity, serialport::Parity::Odd, "O");
            ui.radio_value(&mut self.stop_bits, serialport::StopBits::One, "1");
            ui.radio_value(&mut self.stop_bits, serialport::StopBits::Two, "2");
        });
    }

    fn ui_slave(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Slave");

            ui.horizontal(|ui| {
                ui.label("Slave ID");
                ui.add(egui::DragValue::new(&mut self.slave_id).clamp_range(1..=247));

                ui.label("Function");
                egui::ComboBox::from_id_source("func")
                    .selected_text(format!("{:?}", self.function))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadCoils,
                            "01 Read Coils",
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadHolding,
                            "03 Read Holding",
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadInput,
                            "04 Read Input",
                        );
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Address");
                ui.add(egui::DragValue::new(&mut self.address));

                ui.label("Quantity");
                ui.add(egui::DragValue::new(&mut self.quantity).clamp_range(1..=125));
            });
        });
    }

    fn ui_view(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("View");

            ui.horizontal(|ui| {
                ui.radio_value(&mut self.view_rows, 10, "10");
                ui.radio_value(&mut self.view_rows, 20, "20");
                ui.radio_value(&mut self.view_rows, 50, "50");

                egui::ComboBox::from_id_source("display")
                    .selected_text(&self.display_format)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.display_format, "DEC".into(), "DEC");
                        ui.selectable_value(&mut self.display_format, "HEX".into(), "HEX");
                        ui.selectable_value(&mut self.display_format, "BIN".into(), "BIN");
                    });
            });
        });

        ui.horizontal(|ui| {
            if ui.button("Read Once").clicked() {
                self.start_read_once();
            }

            ui.checkbox(&mut self.auto_poll, "Auto Poll (1s)");
        });
    }

    fn ui_table(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("modbus_table")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Row");
                            for i in 0..self.quantity {
                                ui.label(format!("Addr {}", self.address + i));
                            }
                            ui.end_row();

                            for row in 0..self.view_rows {
                                ui.label(row.to_string());

                                for col in 0..self.quantity {
                                    let idx = row * self.quantity as usize + col as usize;
                                    let v = self.data.get(idx).copied().unwrap_or(0);

                                    let txt = match self.display_format.as_str() {
                                        "HEX" => format!("{:04X}", v),
                                        "BIN" => format!("{:016b}", v),
                                        _ => v.to_string(),
                                    };

                                    ui.label(txt);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn ui_logs(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.scroll_to_bottom)
                .show(ui, |ui| {
                    for log in &self.logs {
                        ui.label(log);
                    }
                });

            self.scroll_to_bottom = false;
        });
    }

    fn start_read_once(&mut self) {
        let (tx, rx) = channel();
        self.rx = Some(rx);

        let ip = self.tcp_ip.clone();
        let port = self.tcp_port;
        let slave = self.slave_id;
        let addr = self.address;
        let qty: u16 = self.quantity;
        let function = self.function;

        self.logs.push("TX Read Holding Registers".into());
        self.scroll_to_bottom = true;

        self.rt.spawn(async move {
            match Self::modbus_read_by_function(ip, port, slave, function, addr, qty).await {
                Ok(data) => {
                    let _ = tx.send(data);
                }
                Err(e) => {
                    eprintln!("Modbus error: {:?}", e);
                }
            }
        });
    }

    fn handle_auto_poll(&mut self) {
        if self.auto_poll && self.rx.is_none() {
            self.start_auto_poll();
        }

        if !self.auto_poll && self.rx.is_some() {
            self.rx = None; // stop
            self.logs.push("Auto Poll stopped".into());
        }
    }

    fn start_auto_poll(&mut self) {
        let (tx, rx) = channel();
        self.rx = Some(rx);

        let ip = self.tcp_ip.clone();
        let port = self.tcp_port;
        let slave = self.slave_id;
        let addr = self.address;
        let qty = self.quantity;
        let function = self.function;

        self.logs.push("Auto Poll started (1s)".into());
        self.scroll_to_bottom = true;

        self.rt.spawn(async move {
            loop {
                match Self::modbus_read_by_function(ip.clone(), port, slave, function, addr, qty)
                    .await
                {
                    Ok(data) => {
                        if tx.send(data).is_err() {
                            break; // stop
                        }
                    }
                    Err(_) => {}
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    async fn modbus_read_by_function(
        ip: String,
        port: u16,
        slave_id: u8,
        function: ModbusFunction,
        address: u16,
        quantity: u16,
    ) -> Result<Vec<u16>> {
        let socket_addr: SocketAddr = format!("{}:{}", ip, port).parse()?;

        let ctx = tcp::connect(socket_addr).await?;
        ctx.set_slave(Slave(slave_id));

        let data: Vec<u16> = match function {
            ModbusFunction::ReadCoils => {
                let response = ctx.read_coils(address, quantity).await?;
                response.into_iter().map(|b| b.into() as u16).collect()
            }

            ModbusFunction::ReadDiscrete => {
                let response = ctx.read_discrete_inputs(address, quantity).await?;
                response.into_iter().map(|b| b.into() as u16).collect()
            }

            ModbusFunction::ReadHolding => {
                let response = ctx.read_holding_registers(address, quantity).await?;
                response.into_iter().map(|r| r.into() as u16).collect()
            }

            ModbusFunction::ReadInput => {
                let response = ctx.read_input_registers(address, quantity).await?;
                response.into_iter().map(|r| r.into() as u16).collect()
            }
        };

        Ok(data)
    }
}
