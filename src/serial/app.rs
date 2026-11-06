use super::utils::{bytes_to_hex_string, now_timestamp, parse_hex_string};
use serialport::{self, SerialPort, SerialPortInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    pub flow_control: serialport::FlowControl,
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
            flow_control: serialport::FlowControl::None,
            logs: vec![],
            input_text: String::new(),
            status: "Disconnected".to_string(),
            port: None,
            send_format: SendFormat::Hex,
            rx: None,
            read_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn receive(&mut self) -> bool {
        let before = self.logs.len();
        if let Some(rx) = &self.rx {
            while let Ok(data) = rx.try_recv() {
                let ts = now_timestamp();

                let display = match self.send_format {
                    SendFormat::Hex => bytes_to_hex_string(&data),
                    SendFormat::Ascii => String::from_utf8_lossy(&data)
                        .replace('\r', "\\r")
                        .replace('\n', "\\n"),
                };

                self.logs.push(format!("{} RX <- {}", ts, display));
            }
        }
        self.logs.len() != before
    }

    pub fn connect(&mut self) {
        if self.port.is_some() {
            return;
        }
        let Some(port_name) = &self.selected_port else {
            self.status = "No port selected".into();
            return;
        };

        match serialport::new(port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(self.flow_control)
            .timeout(Duration::from_millis(100))
            .open()
        {
            Ok(port) => {
                let port = Arc::new(Mutex::new(port));

                let (tx, rx) = mpsc::channel();

                self.read_running = Arc::new(AtomicBool::new(false));
                Self::start_read_thread(port.clone(), tx, self.read_running.clone());

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

    pub fn start_read_thread(
        port: Arc<Mutex<Box<dyn SerialPort>>>,
        tx: Sender<Vec<u8>>,
        running: Arc<AtomicBool>,
    ) {
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

impl Drop for SerialTool {
    fn drop(&mut self) {
        self.read_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn exchanges_hex_and_ascii_over_a_virtual_serial_port() {
        let (mut device, endpoint) =
            serialport::TTYPort::pair().expect("create virtual serial pair");
        device.set_timeout(Duration::from_secs(2)).unwrap();

        let mut tool = SerialTool::new();
        tool.selected_port = endpoint.name();
        drop(endpoint);
        // macOS PTYs do not support IOSSIOSPEED. Zero skips that ioctl.
        #[cfg(target_os = "macos")]
        {
            tool.baud_rate = 0;
        }
        tool.connect();
        assert!(tool.port.is_some(), "{}", tool.status);

        for (format, input, sent, reply, logged) in [
            (
                SendFormat::Hex,
                "00 A5 FF",
                &[0, 0xA5, 0xFF][..],
                &[0xDE, 0xAD][..],
                "DE AD",
            ),
            (
                SendFormat::Ascii,
                "PING",
                &b"PING"[..],
                &b"OK\r\n"[..],
                r"OK\r\n",
            ),
        ] {
            tool.send_format = format;
            tool.input_text = input.into();
            tool.send();

            let mut actual = vec![0; sent.len()];
            device.read_exact(&mut actual).unwrap();
            assert_eq!(actual, sent);
            assert!(tool
                .logs
                .last()
                .unwrap()
                .ends_with(&format!("TX -> {input}")));

            device.write_all(reply).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !tool.receive() {
                assert!(Instant::now() < deadline, "no reply received for {input}");
                thread::sleep(Duration::from_millis(10));
            }
            assert!(tool
                .logs
                .last()
                .unwrap()
                .ends_with(&format!("RX <- {logged}")));
        }

        tool.disconnect();
        assert!(tool.port.is_none());
        assert_eq!(tool.status, "Disconnected");
    }
}
