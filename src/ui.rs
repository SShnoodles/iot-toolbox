use crate::i18n::Language;
use crate::modbus::{
    app::{ModbusFunction, ModbusTool},
    display::DisplayFormat,
};
use crate::serial::app::{SendFormat, SerialTool};
use crate::update::{self, UpdateState};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputState, Textarea, TextareaState},
        menu::{DropdownMenu, PopupMenuItem},
        ActiveTheme, Disableable, IconName, Selectable, Sizable, Theme, ThemeMode,
    },
    prelude::FluentBuilder,
    *,
};
use serialport::{DataBits, FlowControl, Parity, StopBits};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Serial,
    Modbus,
}

pub struct Toolbox {
    tab: Tab,
    serial: SerialTool,
    modbus: ModbusTool,
    baud: Entity<InputState>,
    message: Entity<TextareaState>,
    ip: Entity<InputState>,
    port: Entity<InputState>,
    slave: Entity<InputState>,
    address: Entity<InputState>,
    quantity: Entity<InputState>,
    write: Entity<InputState>,
    serial_log: ListState,
    _device_events: Task<()>,
    update: UpdateState,
    show_update_status: bool,
}

impl Toolbox {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // One pending wake is enough: each wake drains both device queues.
        let (wake, events) = async_channel::bounded(1);
        let serial = SerialTool::new(wake.clone());
        let modbus = ModbusTool::new(wake);
        let mut input =
            |value: String| cx.new(|cx| InputState::new(window, cx).default_value(value));
        let baud = input(serial.baud_rate.to_string());
        let ip = input(modbus.tcp_ip.to_string());
        let port = input(modbus.tcp_port.to_string());
        let slave = input(modbus.slave_id.to_string());
        let address = input(modbus.address.to_string());
        let quantity = input(modbus.quantity.to_string());
        let write = input(modbus.write_values.clone());
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(2)
                .placeholder("48 65 6C 6C 6F / Hello")
        });

        let device_events = cx.spawn(async move |view, cx| {
            while events.recv().await.is_ok() {
                if view
                    .update(cx, |this, cx| {
                        let previous_log_count = this.serial.log_count;
                        let serial_changed = this.serial.receive();
                        this.sync_serial_log(previous_log_count);
                        let modbus_changed = this.modbus.receive();
                        if (this.tab == Tab::Serial && serial_changed)
                            || (this.tab == Tab::Modbus && modbus_changed)
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        let serial_log = ListState::new(0, ListAlignment::Top, px(64.));
        serial_log.set_follow_mode(FollowMode::Tail);
        Self::spawn_update_check(cx);
        Self {
            update: UpdateState::Checking,
            show_update_status: false,
            tab: Tab::Serial,
            serial,
            modbus,
            baud,
            message,
            ip,
            port,
            slave,
            address,
            quantity,
            write,
            serial_log,
            _device_events: device_events,
        }
    }

    fn spawn_update_check(cx: &Context<Self>) {
        let request = cx.background_spawn(async { update::check() });
        cx.spawn(async move |view, cx| {
            let update = request.await.unwrap_or(UpdateState::Failed);
            let _ = view.update(cx, |this, cx| {
                this.update = update;
                cx.notify();
            });
        })
        .detach();
    }

    fn check_updates(&mut self, cx: &mut Context<Self>) {
        self.show_update_status = true;
        if !matches!(self.update, UpdateState::Checking) {
            self.update = UpdateState::Checking;
            Self::spawn_update_check(cx);
        }
    }

    fn update_controls(&self, cx: &Context<Self>) -> Div {
        let language = Language::current(cx);
        let message = match &self.update {
            UpdateState::Checking => "Checking for updates…",
            UpdateState::Current => "You're up to date",
            UpdateState::PackageUnavailable => "No update package available",
            UpdateState::Failed => "Update check failed",
            UpdateState::Available { .. } => "",
        };
        row()
            .when(self.show_update_status && !message.is_empty(), |d| {
                d.child(div().text_sm().child(language.text(message)))
            })
            .when_some(
                match &self.update {
                    UpdateState::Available { version, .. } => Some(version.to_string()),
                    _ => None,
                },
                |d, version| {
                    d.child(
                        Button::new("open-update")
                            .small()
                            .primary()
                            .label(format!("{} {version}", language.text("Get update")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let UpdateState::Available { url, .. } = &this.update {
                                    cx.open_url(url);
                                }
                            })),
                    )
                },
            )
            .child(
                action("Check for updates", cx, Self::check_updates).disabled(
                    self.show_update_status && matches!(self.update, UpdateState::Checking),
                ),
            )
    }

    fn switch(&mut self, tab: Tab) {
        if tab != self.tab {
            match tab {
                Tab::Serial => self.modbus.stop_auto_poll(),
                Tab::Modbus => self.serial.disconnect(),
            }
            self.tab = tab;
        }
    }

    fn sync_serial_log(&self, previous_log_count: usize) {
        let appended = self.serial.log_count - previous_log_count;
        if appended == 0 {
            return;
        }
        let previous_len = self.serial_log.item_count();
        let retained = self.serial.logs.len() - appended;
        let evicted = previous_len - retained;
        if evicted > 0 {
            self.serial_log.splice(0..evicted, 0);
        }
        self.serial_log.splice(retained..retained, appended);
    }

    fn serial(&self, cx: &Context<Self>) -> Div {
        let serial = &self.serial;
        let view = cx.entity().downgrade();
        let ports = serial
            .available_ports
            .iter()
            .map(|p| (Some(p.port_name.clone()), p.port_name.clone()));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                panel("Serial connection", cx)
                    .child(
                        row()
                            .child(field(
                                Language::current(cx).text("Port"),
                                choice(
                                    "serial-port",
                                    serial.selected_port.clone(),
                                    ports,
                                    "Select port",
                                    cx,
                                    |this, port, _| this.serial.selected_port = port,
                                ),
                            ))
                            .child(action("Refresh ports", cx, |this, _| {
                                this.serial.available_ports =
                                    serialport::available_ports().unwrap_or_default();
                                if !this.serial.available_ports.iter().any(|p| {
                                    Some(&p.port_name) == this.serial.selected_port.as_ref()
                                }) {
                                    this.serial.selected_port = None;
                                }
                            }))
                            .child(number("Baud rate", &self.baud, false, cx))
                            .child(field(
                                Language::current(cx).text("Data bits"),
                                choice(
                                    "data-bits",
                                    serial.data_bits,
                                    [
                                        (DataBits::Five, "5"),
                                        (DataBits::Six, "6"),
                                        (DataBits::Seven, "7"),
                                        (DataBits::Eight, "8"),
                                    ],
                                    "",
                                    cx,
                                    |this, v, _| this.serial.data_bits = v,
                                ),
                            ))
                            .child(field(
                                Language::current(cx).text("Parity"),
                                choice(
                                    "parity",
                                    serial.parity,
                                    [
                                        (Parity::None, "None"),
                                        (Parity::Odd, "Odd"),
                                        (Parity::Even, "Even"),
                                    ],
                                    "",
                                    cx,
                                    |this, v, _| this.serial.parity = v,
                                ),
                            ))
                            .child(field(
                                Language::current(cx).text("Stop bits"),
                                choice(
                                    "stop-bits",
                                    serial.stop_bits,
                                    [(StopBits::One, "1"), (StopBits::Two, "2")],
                                    "",
                                    cx,
                                    |this, v, _| this.serial.stop_bits = v,
                                ),
                            ))
                            .child(field(
                                Language::current(cx).text("Flow control"),
                                choice(
                                    "flow-control",
                                    serial.flow_control,
                                    [
                                        (FlowControl::None, "None"),
                                        (FlowControl::Software, "Software"),
                                        (FlowControl::Hardware, "Hardware"),
                                    ],
                                    "",
                                    cx,
                                    |this, v, _| this.serial.flow_control = v,
                                ),
                            )),
                    )
                    .child(
                        row()
                            .child(
                                action("Connect", cx, |this, cx| {
                                    match read_number(&this.baud, 1200, 921600, "Baud rate", cx) {
                                        Ok(baud) => {
                                            this.serial.baud_rate = baud;
                                            this.serial.connect();
                                        }
                                        Err(error) => this.serial.status = error,
                                    }
                                })
                                .primary()
                                .disabled(serial.has_session()),
                            )
                            .child(
                                action("Disconnect", cx, |this, _| this.serial.disconnect())
                                    .disabled(!serial.can_disconnect()),
                            ),
                    ),
            )
            .child(
                panel("Send", cx)
                    .child(Textarea::new(&self.message).small())
                    .child(
                        row()
                            .child(choice(
                                "send-format",
                                serial.send_format,
                                [(SendFormat::Hex, "HEX"), (SendFormat::Ascii, "ASCII")],
                                "",
                                cx,
                                |this, v, _| this.serial.send_format = v,
                            ))
                            .child(
                                action("Send", cx, |this, cx| {
                                    this.serial.input_text =
                                        this.message.read(cx).value().to_string();
                                    let previous_log_count = this.serial.log_count;
                                    this.serial.send();
                                    this.sync_serial_log(previous_log_count);
                                })
                                .primary(),
                            ),
                    ),
            )
            .child(panel("Serial log", cx).child(if serial.logs.is_empty() {
                div()
                    .h(px(190.))
                    .child(Language::current(cx).text("No serial traffic"))
                    .into_any_element()
            } else {
                list(self.serial_log.clone(), move |index, _, cx| {
                    let Some(view) = view.upgrade() else {
                        return div().into_any_element();
                    };
                    let logs = &view.read(cx).serial.logs;
                    let line = Language::current(cx).message(&logs[index]).into_owned();
                    div()
                        .font_family(cx.theme().mono_font_family.clone())
                        .whitespace_normal()
                        .child(line)
                        .into_any_element()
                })
                .w_full()
                .h(px(190.))
                .into_any_element()
            }))
    }

    fn modbus(&self, cx: &Context<Self>) -> Div {
        use ModbusFunction::*;
        let language = Language::current(cx);
        let mb = &self.modbus;
        let polling = mb.is_polling();
        let request_active = polling || mb.is_writing();
        let functions = [
            ReadCoils,
            ReadDiscrete,
            ReadHolding,
            ReadInput,
            WriteSingleCoil,
            WriteSingleRegister,
            WriteMultipleCoils,
            WriteMultipleRegisters,
        ];
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                panel("TCP connection", cx).child(
                    row()
                        .child(field(
                            Language::current(cx).text("IP address"),
                            div()
                                .w(px(200.))
                                .child(Input::new(&self.ip).small().disabled(request_active)),
                        ))
                        .child(number("Port", &self.port, request_active, cx)),
                ),
            )
            .child(
                panel("Request", cx).child(
                    row()
                        .child(number("Slave ID", &self.slave, request_active, cx))
                        .child(field(
                            Language::current(cx).text("Function"),
                            choice(
                                "function",
                                mb.function,
                                functions.map(|v| (v, v.label())),
                                "",
                                cx,
                                |this, v, _| {
                                    if this.modbus.function != v {
                                        this.modbus.stop_auto_poll();
                                        this.modbus.function = v;
                                    }
                                },
                            ),
                        ))
                        .child(number("Address", &self.address, request_active, cx))
                        .when(mb.function.is_read(), |d| {
                            d.child(number("Quantity", &self.quantity, request_active, cx))
                        }),
                ),
            )
            .when(mb.function.is_read(), |d| {
                d.child(
                    panel("Results", cx)
                        .child(
                            row()
                                .child(field(
                                    Language::current(cx).text("Rows"),
                                    choice(
                                        "rows",
                                        mb.view_rows,
                                        [(10, "10"), (20, "20")],
                                        "",
                                        cx,
                                        |this, v, _| this.modbus.view_rows = v,
                                    ),
                                ))
                                .child(field(
                                    Language::current(cx).text("Display"),
                                    choice(
                                        "display",
                                        mb.display_format,
                                        DisplayFormat::ALL.map(|v| (v, v.label())),
                                        "",
                                        cx,
                                        |this, v, _| this.modbus.display_format = v,
                                    ),
                                ))
                                .child(if polling {
                                    action("Stop auto poll", cx, |this, _| {
                                        this.modbus.stop_auto_poll()
                                    })
                                } else {
                                    action("Start auto poll", cx, |this, cx| this.run_modbus(cx))
                                        .primary()
                                        .disabled(mb.is_writing())
                                }),
                        )
                        .child(
                            register_row([
                                language.text("Index").into(),
                                language.text("Address").into(),
                                language.text("Raw (hex)").into(),
                                language.text("Value").into(),
                            ])
                            .font_weight(FontWeight::SEMIBOLD)
                            .bg(cx.theme().muted),
                        )
                        .children(mb.rows().into_iter().map(|r| {
                            let raw = r
                                .raw
                                .iter()
                                .map(|v| format!("{v:04X}"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            register_row([r.index.to_string(), r.address.to_string(), raw, r.value])
                                .border_b_1()
                                .border_color(cx.theme().border)
                        })),
                )
            })
            .when(!mb.function.is_read(), |d| {
                d.child(
                    panel("Write values", cx)
                        .child(Input::new(&self.write).small().disabled(request_active))
                        .child(language.text(match mb.function {
                            WriteSingleCoil => "Enter one coil value (0/1, off/on, or false/true).",
                            WriteSingleRegister => {
                                "Enter one register value (0–65535, or 0x0000–0xFFFF)."
                            }
                            WriteMultipleCoils => {
                                "Enter 1–1968 coil values separated by commas or spaces."
                            }
                            WriteMultipleRegisters => {
                                "Enter 1–123 register values separated by commas or spaces."
                            }
                            _ => "",
                        }))
                        .child(
                            action("Send write", cx, |this, cx| this.run_modbus(cx))
                                .primary()
                                .disabled(mb.is_writing()),
                        ),
                )
            })
    }

    fn run_modbus(&mut self, cx: &App) {
        if self.modbus.is_writing() {
            return;
        }
        if let Err(error) = self.read_modbus_fields(cx) {
            self.modbus.status = error;
            return;
        }
        if self.modbus.function.is_read() {
            self.modbus.start_auto_poll();
        } else {
            self.modbus.send_write();
        }
    }

    fn read_modbus_fields(&mut self, cx: &App) -> Result<(), String> {
        let value = self.ip.read(cx).value();
        let ip = value.trim();
        let ip = ip
            .strip_prefix('[')
            .and_then(|ip| ip.strip_suffix(']'))
            .unwrap_or(ip);
        let ip = ip
            .parse()
            .map_err(|_| "Enter a valid IPv4 or IPv6 address".to_owned())?;
        let mb = &mut self.modbus;
        let port = read_number(&self.port, 1, u16::MAX, "Port", cx)?;
        let slave = read_number(&self.slave, 1, 247, "Slave ID", cx)?;
        let address = read_number(&self.address, 0, u16::MAX, "Address", cx)?;
        if mb.function.is_read() {
            mb.quantity = read_number(
                &self.quantity,
                1,
                mb.function.max_read_quantity(),
                "Quantity",
                cx,
            )?;
        }
        mb.tcp_ip = ip;
        mb.tcp_port = port;
        mb.slave_id = slave;
        mb.address = address;
        mb.write_values = self.write.read(cx).value().to_string();
        Ok(())
    }
}

impl Render for Toolbox {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let language = Language::current(cx);
        let status = match self.tab {
            Tab::Serial => &self.serial.status,
            Tab::Modbus => &self.modbus.status,
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                row()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .mr_2()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("IoT Toolbox"),
                    )
                    .child(
                        action("Serial", cx, |this, _| this.switch(Tab::Serial))
                            .selected(self.tab == Tab::Serial),
                    )
                    .child(
                        action("Modbus", cx, |this, _| this.switch(Tab::Modbus))
                            .selected(self.tab == Tab::Modbus),
                    )
                    .child(div().flex_1())
                    .child(self.update_controls(cx))
                    .child(choice(
                        "theme",
                        theme.mode,
                        [(ThemeMode::Light, "Light"), (ThemeMode::Dark, "Dark")],
                        "",
                        cx,
                        |_, mode, cx| Theme::change(mode, None, cx),
                    ))
                    .child(choice(
                        "language",
                        language,
                        [
                            (Language::English, "English"),
                            (Language::Chinese, "简体中文"),
                        ],
                        "",
                        cx,
                        |_, language, cx| language.set(cx),
                    )),
            )
            .child(
                div()
                    .id("workspace")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .child(match self.tab {
                        Tab::Serial => self.serial(cx),
                        Tab::Modbus => self.modbus(cx),
                    }),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_sm()
                    .child(format!(
                        "{}: {}",
                        language.text("Status"),
                        language.message(status)
                    )),
            )
    }
}

fn row() -> Div {
    div().flex().flex_wrap().items_end().gap_2()
}

fn panel(title: &'static str, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(Language::current(cx).text(title)),
        )
}

fn field(label: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_sm()
        .child(label)
        .child(control)
}

fn number(label: &'static str, state: &Entity<InputState>, disabled: bool, cx: &App) -> Div {
    field(
        Language::current(cx).text(label),
        div()
            .w(px(82.))
            .child(Input::new(state).small().disabled(disabled)),
    )
}

fn read_number<T: std::str::FromStr + PartialOrd + std::fmt::Display>(
    state: &Entity<InputState>,
    min: T,
    max: T,
    label: &str,
    cx: &App,
) -> Result<T, String> {
    state
        .read(cx)
        .value()
        .trim()
        .parse()
        .ok()
        .filter(|v| v >= &min && v <= &max)
        .ok_or_else(|| match Language::current(cx) {
            Language::English => format!("{label} must be an integer from {min} to {max}"),
            Language::Chinese => format!(
                "{}必须是 {min} 到 {max} 之间的整数",
                Language::Chinese.text(label)
            ),
        })
}

fn register_row(values: [String; 4]) -> Div {
    div().flex().gap_2().px_3().py_1().text_sm().children(
        values
            .into_iter()
            .zip([70., 85., 180., 220.])
            .map(|(value, width)| div().w(px(width)).child(value)),
    )
}

fn action(
    label: &'static str,
    cx: &Context<Toolbox>,
    handler: impl Fn(&mut Toolbox, &mut Context<Toolbox>) + 'static,
) -> Button {
    Button::new(label)
        .small()
        .label(Language::current(cx).text(label))
        .on_click(cx.listener(move |this, _, _, cx| {
            handler(this, cx);
            cx.notify();
        }))
}

fn choice<T: Clone + PartialEq + 'static, L: AsRef<str>>(
    id: &'static str,
    selected: T,
    options: impl IntoIterator<Item = (T, L)>,
    placeholder: &'static str,
    cx: &Context<Toolbox>,
    set: fn(&mut Toolbox, T, &mut Context<Toolbox>),
) -> impl IntoElement {
    let language = Language::current(cx);
    let options: Vec<_> = options
        .into_iter()
        .map(|(value, label)| {
            (
                value,
                SharedString::from(language.text(label.as_ref()).to_owned()),
            )
        })
        .collect();
    let label = options
        .iter()
        .find(|(v, _)| *v == selected)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| language.text(placeholder).into());
    let view = cx.entity().downgrade();
    Button::new(id)
        .small()
        .label(label)
        .icon(IconName::ChevronDown)
        .disabled(options.is_empty())
        .dropdown_menu(move |menu, _, _| {
            options.iter().fold(menu, |menu, (value, label)| {
                let view = view.clone();
                let checked = *value == selected;
                let value = value.clone();
                menu.item(PopupMenuItem::new(label.clone()).checked(checked).on_click(
                    move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            set(this, value.clone(), cx);
                            cx.notify();
                        });
                    },
                ))
            })
        })
}
