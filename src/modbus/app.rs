use anyhow::Result;
use serialport::{DataBits, Parity, StopBits};
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
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

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum DisplayFormat {
    Signed,
    Unsigned,
    Hex,
    Binary,
    Long,
    LongInverse,
    Float,
    FloatInverse,
    Double,
    DoubleInverse,
}

pub struct ModbusRow {
    pub addr: u16,
    pub raw: Vec<u16>, // original register
    pub display_format: DisplayFormat,
    pub display_value: String,
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

    pub view_rows: usize, // 10 / 20 / 50
    pub display_format: DisplayFormat,

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
            display_format: DisplayFormat::Signed,

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

            self.ui_slave(ui);

            self.ui_view(ui);

            self.ui_table(ui);

            // self.ui_logs(ui);
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
            ui.label(egui::RichText::new("Connection").strong());

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, ModbusMode::Tcp, "TCP");
                // TODO
                // ui.selectable_value(&mut self.mode, ModbusMode::Rtu, "RTU");
            });

            ui.separator();

            match self.mode {
                ModbusMode::Tcp => self.ui_tcp(ui),
                ModbusMode::Rtu => self.ui_rtu(ui),
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("Connect").color(egui::Color32::BLUE))
                    .clicked()
                {
                    self.logs.push("Connecting...".into());
                    self.scroll_to_bottom = true;
                }
                if ui
                    .button(egui::RichText::new("Disconnect").color(egui::Color32::RED))
                    .clicked()
                {
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
            egui::ComboBox::from_id_salt("rtu_port")
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
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Slave").strong());

            ui.horizontal(|ui| {
                ui.label("Slave ID");
                ui.add(egui::DragValue::new(&mut self.slave_id).range(1..=247));

                ui.label("Function");
                egui::ComboBox::from_id_salt("func")
                    .selected_text(format!("{:?}", self.function))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadCoils,
                            "01 Read Coils(0x)",
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadDiscrete,
                            "02 Read Discrete Inputs(1x)",
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadHolding,
                            "03 Read Holding Registers(4x)",
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadInput,
                            "04 Read Input Registers(3x)",
                        );
                    });

                ui.label("Address");
                ui.add(egui::DragValue::new(&mut self.address));

                ui.label("Quantity");
                ui.add(egui::DragValue::new(&mut self.quantity).range(1..=125));
            });

            // ui.horizontal(|ui| {
            //     ui.label("Address");
            //     ui.add(egui::DragValue::new(&mut self.address));

            //     ui.label("Quantity");
            //     ui.add(egui::DragValue::new(&mut self.quantity).range(1..=125));
            // });
        });
    }

    fn ui_view(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("View").strong());

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Row: ").strong());
                ui.radio_value(&mut self.view_rows, 10, "10");
                ui.radio_value(&mut self.view_rows, 20, "20");
                ui.radio_value(&mut self.view_rows, 50, "50");

                ui.label(egui::RichText::new("Display: ").strong());
                egui::ComboBox::from_id_salt("display")
                    .selected_text(self.display_format.label())
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::Signed,
                            "Signed",
                        );
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::Unsigned,
                            "Unsigned",
                        );
                        ui.selectable_value(&mut self.display_format, DisplayFormat::Hex, "Hex");
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::Binary,
                            "Binary",
                        );
                        ui.selectable_value(&mut self.display_format, DisplayFormat::Long, "Long");
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::LongInverse,
                            "Long Inverse",
                        );
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::Float,
                            "Float",
                        );
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::FloatInverse,
                            "Float Inverse",
                        );
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::Double,
                            "Double",
                        );
                        ui.selectable_value(
                            &mut self.display_format,
                            DisplayFormat::DoubleInverse,
                            "Double Inverse",
                        );
                    });
            });
        });

        ui.horizontal(|ui: &mut egui::Ui| {
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
                            ui.label("Row\\Addr");
                            for i in 0..self.quantity {
                                ui.label(format!("{}", self.address + i));
                            }
                            ui.end_row();

                            for row in 0..self.view_rows {
                                ui.label(row.to_string());

                                for col in 0..self.quantity {
                                    let idx = row * self.quantity as usize + col as usize;
                                    let v = self.data.get(idx).copied().unwrap_or(0);

                                    let txt = Self::format_value(&[v], self.display_format);
                                    ui.label(txt.to_string());
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

        let mut ctx = tcp::connect(socket_addr).await?;
        ctx.set_slave(Slave(slave_id));

        let data: Vec<u16> = match function {
            ModbusFunction::ReadCoils => {
                let response = ctx.read_coils(address, quantity).await??;
                response.into_iter().map(|b| b as u16).collect()
            }

            ModbusFunction::ReadDiscrete => {
                let response = ctx.read_discrete_inputs(address, quantity).await??;
                response.into_iter().map(|b| b as u16).collect()
            }

            ModbusFunction::ReadHolding => {
                let response = ctx.read_holding_registers(address, quantity).await??;
                response.into_iter().map(|r| r as u16).collect()
            }

            ModbusFunction::ReadInput => {
                let response = ctx.read_input_registers(address, quantity).await??;
                response.into_iter().map(|r| r as u16).collect()
            }
        };

        Ok(data)
    }

    pub fn format_value(raw: &[u16], fmt: DisplayFormat) -> String {
        match fmt {
            DisplayFormat::Signed => {
                let v = raw.get(0).copied().unwrap_or(0) as i16;
                v.to_string()
            }
            DisplayFormat::Unsigned => raw.get(0).copied().unwrap_or(0).to_string(),
            DisplayFormat::Hex => {
                format!("0x{:04X}", raw.get(0).copied().unwrap_or(0))
            }
            DisplayFormat::Binary => {
                format!("{:016b}", raw.get(0).copied().unwrap_or(0))
            }
            DisplayFormat::Long => {
                if raw.len() >= 2 {
                    let v = ((raw[0] as u32) << 16) | raw[1] as u32;
                    (v as i32).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::LongInverse => {
                if raw.len() >= 2 {
                    let v = ((raw[1] as u32) << 16) | raw[0] as u32;
                    (v as i32).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::Float => {
                if raw.len() >= 2 {
                    let bits = ((raw[0] as u32) << 16) | raw[1] as u32;
                    f32::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::FloatInverse => {
                if raw.len() >= 2 {
                    let bits = ((raw[1] as u32) << 16) | raw[0] as u32;
                    f32::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::Double | DisplayFormat::DoubleInverse => {
                if raw.len() >= 4 {
                    let bits = if fmt == DisplayFormat::Double {
                        ((raw[0] as u64) << 48)
                            | ((raw[1] as u64) << 32)
                            | ((raw[2] as u64) << 16)
                            | (raw[3] as u64)
                    } else {
                        ((raw[3] as u64) << 48)
                            | ((raw[2] as u64) << 32)
                            | ((raw[1] as u64) << 16)
                            | (raw[0] as u64)
                    };
                    f64::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
        }
    }
}

impl DisplayFormat {
    pub fn label(&self) -> &'static str {
        match self {
            DisplayFormat::Signed => "Signed",
            DisplayFormat::Unsigned => "Unsigned",
            DisplayFormat::Hex => "Hex",
            DisplayFormat::Binary => "Binary",
            DisplayFormat::Long => "Long",
            DisplayFormat::LongInverse => "Long Inverse",
            DisplayFormat::Float => "Float",
            DisplayFormat::FloatInverse => "Float Inverse",
            DisplayFormat::Double => "Double",
            DisplayFormat::DoubleInverse => "Double Inverse",
        }
    }

    pub const ALL: [DisplayFormat; 10] = [
        DisplayFormat::Signed,
        DisplayFormat::Unsigned,
        DisplayFormat::Hex,
        DisplayFormat::Binary,
        DisplayFormat::Long,
        DisplayFormat::LongInverse,
        DisplayFormat::Float,
        DisplayFormat::FloatInverse,
        DisplayFormat::Double,
        DisplayFormat::DoubleInverse,
    ];
}
