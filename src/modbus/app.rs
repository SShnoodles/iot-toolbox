use super::display::DisplayFormat;
use anyhow::{bail, Error, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
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

struct ReadResult {
    function: ModbusFunction,
    address: u16,
    data: Vec<u16>,
}

pub struct ModbusTool {
    pub tcp_ip: IpAddr,
    pub tcp_port: u16,

    pub slave_id: u8,
    pub function: ModbusFunction,
    pub address: u16,
    pub quantity: u16,
    pub write_values: String,

    pub view_rows: usize,
    pub display_format: DisplayFormat,

    result: Option<ReadResult>,

    rx: Option<Receiver<ReadResult>>,
    pub rt: tokio::runtime::Runtime,
    poll_task: Option<JoinHandle<()>>,
    pub status_rx: Option<Receiver<String>>,

    pub status: String,
    wake: async_channel::Sender<()>,
}

impl ModbusTool {
    pub fn new(wake: async_channel::Sender<()>) -> Self {
        Self {
            tcp_ip: Ipv4Addr::LOCALHOST.into(),
            tcp_port: 502,
            slave_id: 1,
            function: ModbusFunction::ReadHolding,
            address: 0,
            quantity: 10,
            write_values: "0".to_string(),

            view_rows: 10,
            display_format: DisplayFormat::Signed,

            result: None,

            rx: None,
            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            poll_task: None,
            status_rx: None,

            status: "Disconnected".to_string(),
            wake,
        }
    }

    pub fn receive(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.rx {
            while let Ok(data) = rx.try_recv() {
                self.result = Some(data);
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

        let qty = self.quantity.clamp(1, self.function.max_read_quantity());
        if !address_range_fits(self.address, usize::from(qty)) {
            self.status = "Request exceeds address 65535".into();
            return;
        }

        let (data_tx, data_rx) = channel::<ReadResult>();
        let (status_tx, status_rx) = channel::<String>();

        self.rx = Some(data_rx);
        self.result = None;
        self.status_rx = Some(status_rx);

        let ip = self.tcp_ip;
        let port = self.tcp_port;
        let slave = self.slave_id;
        let addr = self.address;
        let function = self.function;
        self.quantity = qty;

        self.status = "Auto Poll started...".into();

        let wake = self.wake.clone();
        self.poll_task = Some(self.rt.spawn(async move {
            loop {
                match timeout(
                    REQUEST_TIMEOUT,
                    Self::modbus_read_by_function(ip, port, slave, function, addr, qty),
                )
                .await
                {
                    Ok(Ok(data)) => {
                        let _ = data_tx.send(ReadResult {
                            function,
                            address: addr,
                            data,
                        });
                        let _ = status_tx.send("Auto Poll running".into());
                    }
                    Ok(Err(e)) => {
                        let _ = status_tx.send(format!("Read error: {e}"));
                    }
                    Err(_) => {
                        let _ = status_tx.send("Read timed out".into());
                    }
                }

                let _ = wake.try_send(());
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

        if !address_range_fits(self.address, values.len()) {
            self.status = "Request exceeds address 65535".into();
            return;
        }

        let (status_tx, status_rx) = channel::<String>();
        self.status_rx = Some(status_rx);

        let ip = self.tcp_ip;
        let port = self.tcp_port;
        let slave = self.slave_id;
        let function = self.function;
        let address = self.address;
        let value_count = values.len();

        self.status = format!("Sending {}...", function.label());

        let wake = self.wake.clone();
        self.rt.spawn(async move {
            let status = match timeout(
                REQUEST_TIMEOUT,
                Self::modbus_write_by_function(ip, port, slave, function, address, values),
            )
            .await
            {
                Ok(Ok(())) => {
                    format!("Write successful: {value_count} value(s) at address {address}",)
                }
                Ok(Err(error)) => format!("Write error: {error}"),
                Err(_) => "Write timed out; device outcome unknown".into(),
            };
            let _ = status_tx.send(status);
            let _ = wake.try_send(());
        });
    }

    async fn modbus_read_by_function(
        ip: IpAddr,
        port: u16,
        slave_id: u8,
        function: ModbusFunction,
        address: u16,
        quantity: u16,
    ) -> Result<Vec<u16>, Error> {
        let socket_addr = SocketAddr::new(ip, port);

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
        ip: IpAddr,
        port: u16,
        slave_id: u8,
        function: ModbusFunction,
        address: u16,
        values: Vec<u16>,
    ) -> Result<(), Error> {
        let socket_addr = SocketAddr::new(ip, port);

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

    pub fn rows(&self) -> Vec<ModbusRow> {
        let (address, data) = self
            .result
            .as_ref()
            .filter(|result| result.function == self.function)
            .map_or((self.address, &[][..]), |result| {
                (result.address, result.data.as_slice())
            });
        Self::build_rows(address, data, self.view_rows, self.display_format)
    }

    fn build_rows(
        start_addr: u16,
        regs: &[u16],
        rows: usize,
        format: DisplayFormat,
    ) -> Vec<ModbusRow> {
        let reg_per_row = format.register_count();

        (0..rows)
            .map_while(|i| {
                let addr = start_addr.checked_add((i * reg_per_row) as u16)?;
                let start = i * reg_per_row;
                let raw = regs.get(start..start + reg_per_row).unwrap_or(&[]).to_vec();

                Some(ModbusRow {
                    index: i,
                    address: addr,
                    value: format.format(&raw),
                    raw,
                })
            })
            .collect()
    }
}

fn address_range_fits(start: u16, count: usize) -> bool {
    count <= usize::from(u16::MAX - start) + 1
}

#[cfg(test)]
mod tests {
    use super::{ModbusFunction, ModbusTool, ReadResult};

    #[test]
    fn rejects_read_and_write_ranges_past_the_last_address() {
        let mut tool = ModbusTool::new(async_channel::bounded(1).0);
        tool.address = u16::MAX;
        tool.quantity = 2;
        tool.start_auto_poll();
        assert!(!tool.is_polling());
        assert_eq!(tool.status, "Request exceeds address 65535");

        tool.function = ModbusFunction::WriteMultipleRegisters;
        tool.write_values = "1, 2".into();
        tool.send_write();
        assert!(!tool.is_writing());
        assert_eq!(tool.status, "Request exceeds address 65535");
        assert!(super::address_range_fits(u16::MAX, 1));
    }

    #[test]
    #[ignore = "requires permission to bind a local TCP socket"]
    fn ipv6_modbus_write_recovers_after_a_silent_peer() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;
        use std::time::{Duration, Instant};

        let listener = TcpListener::bind("[::1]:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let (wake, events) = async_channel::bounded(1);
        let mut tool = ModbusTool::new(wake);
        let endpoint = listener.local_addr().unwrap();
        tool.tcp_ip = endpoint.ip();
        tool.tcp_port = endpoint.port();
        tool.function = ModbusFunction::WriteSingleRegister;
        tool.send_write();
        assert!(tool.is_writing());

        let deadline = Instant::now() + Duration::from_secs(6);
        let accept = || loop {
            match listener.accept() {
                Ok((peer, _)) => {
                    peer.set_nonblocking(false).unwrap();
                    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    break peer;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "write did not connect");
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        };
        let mut peer = accept();
        let mut request = [0; 12];
        peer.read_exact(&mut request).unwrap();
        assert_eq!(request[7], 6);

        // Keep the connection open without returning a Modbus response.
        let wait_for_completion = |tool: &mut ModbusTool| {
            while tool.is_writing() {
                assert!(Instant::now() < deadline, "write stayed pending");
                thread::sleep(Duration::from_millis(10));
                if events.try_recv().is_ok() {
                    tool.receive();
                }
            }
        };
        wait_for_completion(&mut tool);
        assert_eq!(tool.status, "Write timed out; device outcome unknown");
        drop(peer);

        tool.send_write();
        let mut peer = accept();
        peer.read_exact(&mut request).unwrap();
        peer.write_all(&request).unwrap();
        wait_for_completion(&mut tool);
        assert_eq!(tool.status, "Write successful: 1 value(s) at address 0");
    }

    #[test]
    fn read_rows_keep_the_address_of_the_request_that_produced_them() {
        let mut tool = ModbusTool::new(async_channel::bounded(1).0);
        tool.result = Some(ReadResult {
            function: ModbusFunction::ReadHolding,
            address: 100,
            data: vec![42],
        });
        tool.address = 200;
        let rows = tool.rows();
        assert_eq!(rows[0].address, 100);
        assert_eq!(rows[0].raw, [42]);

        tool.function = ModbusFunction::ReadInput;
        assert!(tool.rows()[0].raw.is_empty());
    }

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
