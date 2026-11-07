use super::utils::{bytes_to_hex_string, now_timestamp, parse_hex_string};
use serialport::{SerialPort, SerialPortInfo};
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const LOG_LIMIT: usize = 1_000;
const FRAME_LIMIT: usize = 4_096;
const READ_TIMEOUT: Duration = Duration::from_millis(30);

#[derive(Clone, Copy, PartialEq)]
pub enum SendFormat {
    Hex,
    Ascii,
}

struct WriteRequest {
    bytes: Vec<u8>,
    format: SendFormat,
}

enum Event {
    Connected(String),
    Received(Vec<u8>),
    Sent(WriteRequest),
    WriteFailed(String),
    Closed(String),
}

struct Session {
    writes: SyncSender<WriteRequest>,
    events: Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub struct SerialTool {
    pub available_ports: Vec<SerialPortInfo>,
    pub selected_port: Option<String>,
    pub baud_rate: u32,
    pub data_bits: serialport::DataBits,
    pub parity: serialport::Parity,
    pub stop_bits: serialport::StopBits,
    pub flow_control: serialport::FlowControl,
    pub logs: VecDeque<String>,
    pub log_count: usize,
    pub input_text: String,
    pub status: String,
    pub send_format: SendFormat,
    session: Option<Session>,
    connected: bool,
    wake: async_channel::Sender<()>,
}

impl SerialTool {
    pub fn new(wake: async_channel::Sender<()>) -> Self {
        Self {
            available_ports: serialport::available_ports().unwrap_or_default(),
            selected_port: None,
            baud_rate: 9600,
            data_bits: serialport::DataBits::Eight,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,
            flow_control: serialport::FlowControl::None,
            logs: VecDeque::new(),
            log_count: 0,
            input_text: String::new(),
            status: "Disconnected".into(),
            send_format: SendFormat::Hex,
            session: None,
            connected: false,
            wake,
        }
    }

    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    pub fn can_disconnect(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| !session.stop.load(Ordering::Relaxed))
    }

    pub fn receive(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..128 {
            let Some(session) = &self.session else { break };
            let event = match session.events.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => Event::Closed("Disconnected".into()),
            };
            let stopping = session.stop.load(Ordering::Relaxed);
            match event {
                Event::Closed(status) => {
                    self.session = None;
                    self.connected = false;
                    self.status = if stopping {
                        "Disconnected".into()
                    } else {
                        status
                    };
                }
                Event::Connected(_) if stopping => continue,
                Event::Connected(name) => {
                    self.connected = true;
                    self.status = format!("Connected: {name}");
                    self.log("Connected".into());
                }
                Event::Received(bytes) => {
                    let display = display_bytes(&bytes, self.send_format)
                        .replace('\r', "\\r")
                        .replace('\n', "\\n");
                    self.log(format!("{} RX <- {display}", now_timestamp()));
                }
                Event::Sent(request) => {
                    let display = display_bytes(&request.bytes, request.format)
                        .replace('\r', "\\r")
                        .replace('\n', "\\n");
                    self.log(format!("{} TX -> {display}", now_timestamp()));
                }
                Event::WriteFailed(error) => self.log(format!("TX -- Send failed: {error}")),
            }
            changed = true;
        }
        changed
    }

    fn log(&mut self, mut line: String) {
        const LINE_LIMIT: usize = 16_384;
        if line.len() > LINE_LIMIT {
            line.truncate(line.floor_char_boundary(LINE_LIMIT));
            line.push('…');
        }
        if self.logs.len() == LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
        self.log_count += 1;
    }

    pub fn connect(&mut self) {
        if self.has_session() {
            return;
        }
        let Some(name) = self.selected_port.clone() else {
            self.status = "No port selected".into();
            return;
        };
        let builder = serialport::new(&name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(self.flow_control)
            .timeout(READ_TIMEOUT);
        let (writes, requests) = mpsc::sync_channel(32);
        let (event_tx, events) = mpsc::sync_channel(128);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        self.session = Some(Session {
            writes,
            events,
            stop,
        });
        self.status = "Connecting…".into();

        let wake = self.wake.clone();
        thread::spawn(move || {
            let publish = |event| {
                if event_tx.send(event).is_err() {
                    return false;
                }
                let _ = wake.try_send(());
                true
            };
            let status = match builder.open() {
                Ok(mut port) => {
                    if !publish(Event::Connected(name)) {
                        return;
                    }
                    run_port(port.as_mut(), &requests, &publish, &worker_stop)
                }
                Err(error) => format!("Connect failed: {error}"),
            };
            publish(Event::Closed(status));
        });
    }

    pub fn disconnect(&mut self) {
        if let Some(session) = &self.session {
            session.stop.store(true, Ordering::Relaxed);
            self.connected = false;
            self.status = "Disconnecting…".into();
        }
    }

    pub fn send(&mut self) {
        let Some(session) = self.session.as_ref().filter(|_| self.connected) else {
            self.log("TX -- Not connected".into());
            return;
        };
        let bytes = match self.send_format {
            SendFormat::Hex => match parse_hex_string(&self.input_text) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.log(format!("TX -- HEX error: {error}"));
                    return;
                }
            },
            SendFormat::Ascii => self.input_text.as_bytes().to_vec(),
        };
        match session.writes.try_send(WriteRequest {
            bytes,
            format: self.send_format,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.log("TX -- Send queue full".into()),
            Err(TrySendError::Disconnected(_)) => self.log("TX -- Not connected".into()),
        }
    }
}

fn display_bytes(bytes: &[u8], format: SendFormat) -> String {
    match format {
        SendFormat::Hex => bytes_to_hex_string(bytes),
        SendFormat::Ascii => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn run_port(
    port: &mut dyn SerialPort,
    requests: &Receiver<WriteRequest>,
    publish: &impl Fn(Event) -> bool,
    stop: &AtomicBool,
) -> String {
    let mut buffer = [0; 256];
    let mut frame = Vec::new();
    let mut last_receive = Instant::now();
    let status = loop {
        if stop.load(Ordering::Relaxed) {
            break "Disconnected".into();
        }
        match requests.try_recv() {
            Ok(request) => {
                let event = match port.write_all(&request.bytes) {
                    Ok(()) => Event::Sent(request),
                    Err(error) => Event::WriteFailed(error.to_string()),
                };
                if !publish(event) {
                    return "Disconnected".into();
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break "Disconnected".into(),
        }

        match port.read(&mut buffer) {
            Ok(0) => thread::sleep(Duration::from_millis(2)),
            Ok(count) => {
                frame.extend_from_slice(&buffer[..count]);
                last_receive = Instant::now();
            }
            Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(error) => break format!("Serial read failed: {error}"),
        }
        if !frame.is_empty()
            && (frame.len() >= FRAME_LIMIT || last_receive.elapsed() >= READ_TIMEOUT)
        {
            if !publish(Event::Received(std::mem::take(&mut frame))) {
                return "Disconnected".into();
            }
        }
    };
    if !frame.is_empty() {
        publish(Event::Received(frame));
    }
    status
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

        let (wake, events) = async_channel::bounded(1);
        let mut tool = SerialTool::new(wake);
        tool.selected_port = endpoint.name();
        drop(endpoint);
        // macOS PTYs do not support IOSSIOSPEED. Zero skips that ioctl.
        #[cfg(target_os = "macos")]
        {
            tool.baud_rate = 0;
        }
        tool.connect();
        wait_for(&mut tool, &events, |tool| {
            tool.connected || !tool.has_session()
        });
        assert!(tool.connected, "{}", tool.status);

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
            wait_for(&mut tool, &events, |tool| {
                tool.logs
                    .back()
                    .unwrap()
                    .ends_with(&format!("TX -> {input}"))
            });

            device.write_all(reply).unwrap();
            wait_for(&mut tool, &events, |tool| {
                tool.logs
                    .back()
                    .unwrap()
                    .ends_with(&format!("RX <- {logged}"))
            });
        }

        tool.disconnect();
        wait_for(&mut tool, &events, |tool| !tool.has_session());
        assert_eq!(tool.status, "Disconnected");
    }

    fn wait_for(
        tool: &mut SerialTool,
        events: &async_channel::Receiver<()>,
        ready: impl Fn(&SerialTool) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !ready(tool) {
            assert!(
                Instant::now() < deadline,
                "serial operation timed out: {}",
                tool.status
            );
            thread::sleep(Duration::from_millis(5));
            if events.try_recv().is_ok() {
                tool.receive();
            }
        }
    }
}
