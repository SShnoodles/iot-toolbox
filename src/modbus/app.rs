use super::display::DisplayFormat;
use anyhow::{bail, Error, Result};
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
    ReadCoils,              // 01
    ReadDiscrete,           // 02
    ReadHolding,            // 03
    ReadInput,              // 04
    WriteSingleCoil,        // 05
    WriteSingleRegister,    // 06
    WriteMultipleCoils,     // 0F
    WriteMultipleRegisters, // 10
}

impl ModbusFunction {
    fn is_read(self) -> bool {
        matches!(
            self,
            Self::ReadCoils | Self::ReadDiscrete | Self::ReadHolding | Self::ReadInput
        )
    }

    fn label(self) -> &'static str {
        match self {
            Self::ReadCoils => "01 Read Coils (0x)",
            Self::ReadDiscrete => "02 Read Discrete Inputs (1x)",
            Self::ReadHolding => "03 Read Holding Registers (4x)",
            Self::ReadInput => "04 Read Input Registers (3x)",
            Self::WriteSingleCoil => "05 Write Single Coil (0x)",
            Self::WriteSingleRegister => "06 Write Single Register (4x)",
            Self::WriteMultipleCoils => "15 Write Multiple Coils (0x)",
            Self::WriteMultipleRegisters => "16 Write Multiple Registers (4x)",
        }
    }

    fn max_read_quantity(self) -> u16 {
        match self {
            Self::ReadCoils | Self::ReadDiscrete => 2000,
            Self::ReadHolding | Self::ReadInput => 125,
            _ => 1,
        }
    }
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
    pub write_values: String,

    pub view_rows: usize,
    pub display_format: DisplayFormat,

    pub data: Vec<u16>,

    pub logs: Vec<String>,
    pub scroll_to_bottom: bool,

    pub rx: Option<Receiver<Vec<u16>>>,
    pub rt: tokio::runtime::Runtime,
    pub stop_tx: Option<Sender<()>>,
    pub status_rx: Option<Receiver<String>>,

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
            write_values: "0".to_string(),

            view_rows: 10,
            display_format: DisplayFormat::Signed,

            data: Vec::new(),

            logs: Vec::new(),
            scroll_to_bottom: false,

            rx: None,
            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            stop_tx: None,
            status_rx: None,

            status: "Disconnected".to_string(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            self.ui_connection(ui);

            self.ui_slave(ui);

            if self.function.is_read() {
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
            } else {
                self.stop_auto_poll();
                self.ui_write(ui);
            }

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

        if let Some(status_rx) = &self.status_rx {
            while let Ok(status) = status_rx.try_recv() {
                self.status = status;
            }
        }
    }

    fn ui_status(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::bottom("modbus_status").show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Status: ").color(egui::Color32::RED));
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
                    .selected_text(self.function.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadCoils,
                            ModbusFunction::ReadCoils.label(),
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadDiscrete,
                            ModbusFunction::ReadDiscrete.label(),
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadHolding,
                            ModbusFunction::ReadHolding.label(),
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::ReadInput,
                            ModbusFunction::ReadInput.label(),
                        );
                        ui.separator();
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::WriteSingleCoil,
                            ModbusFunction::WriteSingleCoil.label(),
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::WriteSingleRegister,
                            ModbusFunction::WriteSingleRegister.label(),
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::WriteMultipleCoils,
                            ModbusFunction::WriteMultipleCoils.label(),
                        );
                        ui.selectable_value(
                            &mut self.function,
                            ModbusFunction::WriteMultipleRegisters,
                            ModbusFunction::WriteMultipleRegisters.label(),
                        );
                    });

                ui.label("Address");
                ui.add(egui::DragValue::new(&mut self.address));

                if self.function.is_read() {
                    ui.label("Quantity");
                    ui.add(
                        egui::DragValue::new(&mut self.quantity)
                            .range(1..=self.function.max_read_quantity()),
                    );
                }
            });
        });
    }

    fn ui_write(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Write").strong());

            ui.horizontal(|ui| {
                ui.label("Value(s)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.write_values)
                        .desired_width(f32::INFINITY)
                        .hint_text("Comma or space separated; decimal or 0x hex"),
                );
            });

            let hint = match self.function {
                ModbusFunction::WriteSingleCoil => {
                    "Enter one coil value: 0/1, off/on, or false/true."
                }
                ModbusFunction::WriteSingleRegister => {
                    "Enter one register value from 0 to 65535 (or 0x0000 to 0xFFFF)."
                }
                ModbusFunction::WriteMultipleCoils => {
                    "Enter 1 to 1968 coil values: 0/1, off/on, or false/true."
                }
                ModbusFunction::WriteMultipleRegisters => {
                    "Enter 1 to 123 register values from 0 to 65535."
                }
                _ => "",
            };
            ui.label(egui::RichText::new(hint).weak().small());

            let send_button =
                egui::Button::new(egui::RichText::new("Send Write").color(egui::Color32::BLUE));
            if ui
                .add_enabled(!self.status.starts_with("Sending "), send_button)
                .clicked()
            {
                self.send_write();
            }
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
                if ui
                    .button(egui::RichText::new("▶ Start Auto Poll").color(egui::Color32::BLUE))
                    .clicked()
                {
                    self.start_auto_poll();
                }
            } else {
                if ui
                    .button(egui::RichText::new("⏹ Stop Auto Poll").color(egui::Color32::RED))
                    .clicked()
                {
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
                        ui.label("Index");
                        ui.label("Address");
                        ui.label("Raw");
                        ui.label("Value");
                        ui.end_row();

                        for row in rows.iter_mut() {
                            ui.label(row.index.to_string());
                            ui.label(row.address.to_string());

                            ui.label(
                                row.raw
                                    .iter()
                                    .map(|v| format!("{:04X}", v))
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            );

                            ui.label(row.format.format(&row.raw));
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
        if self.stop_tx.is_some() || !self.function.is_read() {
            return;
        }

        let (data_tx, data_rx) = channel::<Vec<u16>>();
        let (stop_tx, stop_rx) = channel::<()>();
        let (status_tx, status_rx) = channel::<String>();

        self.rx = Some(data_rx);
        self.stop_tx = Some(stop_tx);
        self.status_rx = Some(status_rx);

        let ip = self.tcp_ip.clone();
        let port = self.tcp_port;
        let slave = self.slave_id;
        let addr = self.address;
        let function = self.function;
        let qty = self.quantity.clamp(1, function.max_read_quantity());
        self.quantity = qty;

        self.status = "Auto Poll started...".into();
        self.logs.push("Auto Poll started (1s)".into());
        self.scroll_to_bottom = true;

        self.rt.spawn(async move {
            if stop_rx.try_recv().is_ok() {
                return;
            }

            match Self::modbus_read_by_function(ip.clone(), port, slave, function, addr, qty).await
            {
                Ok(data) => {
                    let _ = data_tx.send(data);
                }
                Err(e) => {
                    let _ = status_tx.send(format!("Read error: {}", e));
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });
    }

    pub fn stop_auto_poll(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
            self.status = "Auto Poll stopped".into();
            self.scroll_to_bottom = true;
        }

        self.rx = None;
    }

    fn send_write(&mut self) {
        if self.status.starts_with("Sending ") {
            return;
        }

        let values = match Self::parse_write_values(self.function, &self.write_values) {
            Ok(values) => values,
            Err(error) => {
                self.status = format!("Invalid write value: {error}");
                return;
            }
        };

        let (status_tx, status_rx) = channel::<String>();
        self.status_rx = Some(status_rx);

        let ip = self.tcp_ip.clone();
        let port = self.tcp_port;
        let slave = self.slave_id;
        let function = self.function;
        let address = self.address;
        let value_count = values.len();

        self.status = format!("Sending {}...", function.label());

        self.rt.spawn(async move {
            let status =
                match Self::modbus_write_by_function(ip, port, slave, function, address, values)
                    .await
                {
                    Ok(()) => format!(
                        "Write successful: {} value(s) at address {}",
                        value_count, address
                    ),
                    Err(error) => format!("Write error: {error}"),
                };
            let _ = status_tx.send(status);
        });
    }

    async fn modbus_read_by_function(
        ip: String,
        port: u16,
        slave_id: u8,
        function: ModbusFunction,
        address: u16,
        quantity: u16,
    ) -> Result<Vec<u16>, Error> {
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

            _ => bail!("{} is not a read function", function.label()),
        };

        Ok(data)
    }

    async fn modbus_write_by_function(
        ip: String,
        port: u16,
        slave_id: u8,
        function: ModbusFunction,
        address: u16,
        values: Vec<u16>,
    ) -> Result<(), Error> {
        let socket_addr: SocketAddr = format!("{}:{}", ip, port).parse()?;

        let mut ctx = tcp::connect(socket_addr).await?;
        ctx.set_slave(Slave(slave_id));

        match function {
            ModbusFunction::WriteSingleCoil => {
                let [value] = values.as_slice() else {
                    bail!("{} requires exactly one value", function.label());
                };
                ctx.write_single_coil(address, *value != 0).await??;
            }
            ModbusFunction::WriteSingleRegister => {
                let [value] = values.as_slice() else {
                    bail!("{} requires exactly one value", function.label());
                };
                ctx.write_single_register(address, *value).await??;
            }
            ModbusFunction::WriteMultipleCoils => {
                let coils: Vec<bool> = values.into_iter().map(|value| value != 0).collect();
                ctx.write_multiple_coils(address, &coils).await??;
            }
            ModbusFunction::WriteMultipleRegisters => {
                ctx.write_multiple_registers(address, &values).await??;
            }
            _ => bail!("{} is not a write function", function.label()),
        }

        Ok(())
    }

    fn parse_write_values(function: ModbusFunction, input: &str) -> Result<Vec<u16>, Error> {
        let tokens: Vec<&str> = input
            .split(|character: char| {
                character.is_ascii_whitespace() || character == ',' || character == ';'
            })
            .filter(|token| !token.is_empty())
            .collect();

        if tokens.is_empty() {
            bail!("at least one value is required");
        }

        let (is_coil, min_count, max_count) = match function {
            ModbusFunction::WriteSingleCoil => (true, 1, 1),
            ModbusFunction::WriteSingleRegister => (false, 1, 1),
            ModbusFunction::WriteMultipleCoils => (true, 1, 1968),
            ModbusFunction::WriteMultipleRegisters => (false, 1, 123),
            _ => bail!("{} is not a write function", function.label()),
        };

        if tokens.len() < min_count || tokens.len() > max_count {
            if min_count == max_count {
                bail!("{} requires exactly one value", function.label());
            }
            bail!(
                "{} accepts between {} and {} values",
                function.label(),
                min_count,
                max_count
            );
        }

        tokens
            .into_iter()
            .map(|token| {
                if is_coil {
                    match token.to_ascii_lowercase().as_str() {
                        "0" | "off" | "false" => Ok(0),
                        "1" | "on" | "true" => Ok(1),
                        _ => bail!("'{token}' is not a valid coil value"),
                    }
                } else if let Some(hex) = token
                    .strip_prefix("0x")
                    .or_else(|| token.strip_prefix("0X"))
                {
                    u16::from_str_radix(hex, 16)
                        .map_err(|_| anyhow::anyhow!("'{token}' is not a valid 16-bit value"))
                } else {
                    token
                        .parse::<u16>()
                        .map_err(|_| anyhow::anyhow!("'{token}' is not a valid 16-bit value"))
                }
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::{ModbusFunction, ModbusTool};

    #[test]
    fn parses_decimal_and_hex_register_values() {
        let values = ModbusTool::parse_write_values(
            ModbusFunction::WriteMultipleRegisters,
            "1, 0x00FF 65535;0X10",
        )
        .unwrap();

        assert_eq!(values, vec![1, 255, 65535, 16]);
    }

    #[test]
    fn parses_named_coil_values() {
        let values = ModbusTool::parse_write_values(
            ModbusFunction::WriteMultipleCoils,
            "0, 1 off ON false true",
        )
        .unwrap();

        assert_eq!(values, vec![0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn rejects_more_than_one_value_for_single_write() {
        let error = ModbusTool::parse_write_values(ModbusFunction::WriteSingleRegister, "10, 20")
            .unwrap_err();

        assert!(error.to_string().contains("exactly one value"));
    }

    #[test]
    fn rejects_invalid_register_and_coil_values() {
        assert!(
            ModbusTool::parse_write_values(ModbusFunction::WriteSingleRegister, "65536").is_err()
        );
        assert!(ModbusTool::parse_write_values(ModbusFunction::WriteSingleCoil, "2").is_err());
    }

    #[test]
    fn rejects_write_limits_and_read_functions() {
        let too_many_registers = vec!["0"; 124].join(",");
        assert!(ModbusTool::parse_write_values(
            ModbusFunction::WriteMultipleRegisters,
            &too_many_registers,
        )
        .is_err());
        assert!(ModbusTool::parse_write_values(ModbusFunction::ReadHolding, "1").is_err());
    }
}
