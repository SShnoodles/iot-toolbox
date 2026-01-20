use super::display::DisplayFormat;
use anyhow::Result;
use serialport::{DataBits, Parity, StopBits};
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};
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

pub struct ModbusRow {
    pub index: usize,
    pub address: u16,
    pub raw: Vec<u16>, // original
    pub format: DisplayFormat,
    pub value: String,
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

    pub view_rows: usize,
    pub display_format: DisplayFormat,

    pub data: Vec<u16>,

    pub logs: Vec<String>,
    pub scroll_to_bottom: bool,

    pub rx: Option<Receiver<Vec<u16>>>,
    pub rt: tokio::runtime::Runtime,
    pub stop_tx: Option<Sender<()>>,

    pub status: String,
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

            rx: None,
            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            stop_tx: None,

            status: "Disconnected".to_string(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            self.ui_connection(ui);

            self.ui_slave(ui);

            self.ui_view(ui);

            self.ui_table(
                ui,
                &mut Self::build_rows(
                    self.address,
                    &self.data,
                    self.view_rows,
                    self.display_format,
                ),
            );

            // self.ui_logs(ui);
            self.ui_status(ui);
        });

        if let Some(rx) = &self.rx {
            while let Ok(data) = rx.try_recv() {
                self.data = data;
                self.logs.push(format!("RX {} registers", self.data.len()));
                self.scroll_to_bottom = true;
            }
        }
    }

    fn ui_status(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::bottom("modbus_status").show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.monospace(&self.status);
            });
        });
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
            let running = self.stop_tx.is_some();

            if !running {
                if ui.button("▶ Start Auto Poll").clicked() {
                    self.start_auto_poll();
                }
            } else {
                if ui.button("⏹ Stop Auto Poll").clicked() {
                    self.stop_auto_poll();
                }
            }
        });
    }

    pub fn ui_table(&mut self, ui: &mut egui::Ui, rows: &mut Vec<ModbusRow>) {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Grid::new("modbus_table")
                    .striped(true)
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        ui.label("#");
                        ui.label("Address");
                        ui.label("Raw");
                        ui.label("Format");
                        ui.label("Value");
                        ui.end_row();

                        for row in rows.iter_mut() {
                            ui.label(row.index.to_string());
                            ui.label(format!("{}", row.address));

                            ui.label(
                                row.raw
                                    .iter()
                                    .map(|v| format!("{:04X}", v))
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            );

                            egui::ComboBox::from_id_salt(format!("fmt_{}", row.index))
                                .selected_text(row.format.label())
                                .show_ui(ui, |ui| {
                                    for f in DisplayFormat::ALL {
                                        if ui
                                            .selectable_value(&mut row.format, f, f.label())
                                            .clicked()
                                        {
                                            row.value = row.format.format(&row.raw);
                                        }
                                    }
                                });

                            ui.label(&row.value);
                            ui.end_row();
                        }
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

    fn start_auto_poll(&mut self) {
        if self.stop_tx.is_some() {
            return;
        }

        let (data_tx, data_rx) = channel::<Vec<u16>>();
        let (stop_tx, stop_rx) = channel::<()>();

        self.rx = Some(data_rx);
        self.stop_tx = Some(stop_tx);

        let ip = self.tcp_ip.clone();
        let port = self.tcp_port;
        let slave = self.slave_id;
        let addr = self.address;
        let qty = self.quantity;
        let function = self.function;

        self.status = "Auto Poll started...".into();
        self.logs.push("Auto Poll started (1s)".into());
        self.scroll_to_bottom = true;

        self.rt.spawn(async move {
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                match Self::modbus_read_by_function(ip.clone(), port, slave, function, addr, qty)
                    .await
                {
                    Ok(data) => {
                        let _ = data_tx.send(data);
                    }
                    Err(_) => {}
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    pub fn stop_auto_poll(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }

        self.rx = None;

        self.status = "Auto Poll stopped".into();
        self.logs.push("Auto Poll stopped".into());
        self.scroll_to_bottom = true;
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

    fn build_rows(
        start_addr: u16,
        regs: &[u16],
        rows: usize,
        format: DisplayFormat,
    ) -> Vec<ModbusRow> {
        let reg_per_row = format.register_count();

        (0..rows)
            .map(|i| {
                let addr = start_addr + (i * reg_per_row) as u16;
                let start = i * reg_per_row;
                let raw = regs.get(start..start + reg_per_row).unwrap_or(&[]).to_vec();

                ModbusRow {
                    index: i,
                    address: addr,
                    raw: raw.clone(),
                    format,
                    value: format.format(&raw),
                }
            })
            .collect()
    }
}
