use super::display::DisplayFormat;
use anyhow::{bail, Error, Result};
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use tokio::task::JoinHandle;
use tokio_modbus::prelude::*;

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
    pub fn is_read(self) -> bool {
        matches!(
            self,
            Self::ReadCoils | Self::ReadDiscrete | Self::ReadHolding | Self::ReadInput
        )
    }

    pub fn label(self) -> &'static str {
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

    pub fn max_read_quantity(self) -> u16 {
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
    pub value: String,
}

pub struct ModbusTool {
    pub tcp_ip: String,
    pub tcp_port: u16,

    pub slave_id: u8,
    pub function: ModbusFunction,
    pub address: u16,
    pub quantity: u16,
    pub write_values: String,

    pub view_rows: usize,
    pub display_format: DisplayFormat,

    pub data: Vec<u16>,

    pub rx: Option<Receiver<Vec<u16>>>,
    pub rt: tokio::runtime::Runtime,
    poll_task: Option<JoinHandle<()>>,
    pub status_rx: Option<Receiver<String>>,

    pub status: String,
}

impl ModbusTool {
    pub fn new() -> Self {
        Self {
            tcp_ip: "127.0.0.1".to_string(),
            tcp_port: 502,
            slave_id: 1,
            function: ModbusFunction::ReadHolding,
            address: 0,
            quantity: 10,
            write_values: "0".to_string(),

            view_rows: 10,
            display_format: DisplayFormat::Signed,

            data: Vec::new(),

            rx: None,
            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            poll_task: None,
            status_rx: None,

            status: "Disconnected".to_string(),
        }
    }

    pub fn receive(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.rx {
            while let Ok(data) = rx.try_recv() {
                self.data = data;
                changed = true;
            }
        }
        if let Some(rx) = &self.status_rx {
            while let Ok(status) = rx.try_recv() {
                changed |= self.status != status;
                self.status = status;
            }
        }
        changed
    }

    pub fn is_polling(&self) -> bool {
        self.poll_task.is_some()
    }

    pub fn start_auto_poll(&mut self) {
        if self.is_polling() || !self.function.is_read() {
            return;
        }

        let (data_tx, data_rx) = channel::<Vec<u16>>();
        let (status_tx, status_rx) = channel::<String>();

        self.rx = Some(data_rx);
        self.data.clear();
        self.status_rx = Some(status_rx);

        let ip = self.tcp_ip.clone();
        let port = self.tcp_port;
        let slave = self.slave_id;
        let addr = self.address;
        let function = self.function;
        let qty = self.quantity.clamp(1, function.max_read_quantity());
        self.quantity = qty;

        self.status = "Auto Poll started...".into();

        self.poll_task = Some(self.rt.spawn(async move {
            loop {
                match Self::modbus_read_by_function(ip.clone(), port, slave, function, addr, qty)
                    .await
                {
                    Ok(data) => {
                        let _ = data_tx.send(data);
                        let _ = status_tx.send("Auto Poll running".into());
                    }
                    Err(e) => {
                        let _ = status_tx.send(format!("Read error: {}", e));
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }));
    }

    pub fn stop_auto_poll(&mut self) {
        if let Some(task) = self.poll_task.take() {
            task.abort();
            self.status = "Auto Poll stopped".into();
            self.status_rx = None;
        }

        self.rx = None;
    }

    pub fn is_writing(&self) -> bool {
        self.status.starts_with("Sending ")
    }

    pub fn send_write(&mut self) {
        if self.is_writing() {
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

    pub fn build_rows(
        start_addr: u16,
        regs: &[u16],
        rows: usize,
        format: DisplayFormat,
    ) -> Vec<ModbusRow> {
        let reg_per_row = format.register_count();

        (0..rows)
            .map(|i| {
                let addr = start_addr.wrapping_add((i * reg_per_row) as u16);
                let start = i * reg_per_row;
                let raw = regs.get(start..start + reg_per_row).unwrap_or(&[]).to_vec();

                ModbusRow {
                    index: i,
                    address: addr,
                    value: format.format(&raw),
                    raw,
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
