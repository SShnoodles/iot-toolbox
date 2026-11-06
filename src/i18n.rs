use gpui_kit::{App, Global};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Language {
    #[default]
    English,
    Chinese,
}

impl Global for Language {}

impl Language {
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    pub fn set(self, cx: &mut App) {
        cx.set_global(self);
        gpui_kit::component::set_locale(match self {
            Self::English => "en",
            Self::Chinese => "zh-CN",
        });
    }

    pub fn text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::English {
            return english;
        }

        match english {
            "Serial" => "串口",
            "Serial connection" => "串口连接",
            "Select port" => "选择端口",
            "Refresh ports" => "刷新端口",
            "Port" => "端口",
            "Baud rate" => "波特率",
            "Data bits" => "数据位",
            "Parity" => "校验",
            "Stop bits" => "停止位",
            "Flow control" => "流控",
            "None" => "无",
            "Odd" => "奇校验",
            "Even" => "偶校验",
            "Software" => "软件",
            "Hardware" => "硬件",
            "Connect" => "连接",
            "Disconnect" => "断开",
            "Send" => "发送",
            "Serial log" => "串口日志",
            "No serial traffic" => "暂无串口数据",
            "TCP connection" => "TCP 连接",
            "IP address" => "IP 地址",
            "Request" => "请求",
            "Slave ID" => "从站地址",
            "Function" => "功能码",
            "Address" => "地址",
            "Quantity" => "数量",
            "Results" => "结果",
            "Rows" => "行数",
            "Display" => "显示格式",
            "Start auto poll" => "开始轮询",
            "Stop auto poll" => "停止轮询",
            "Index" => "序号",
            "Raw (hex)" => "原始值（十六进制）",
            "Value" => "数值",
            "Write values" => "写入值",
            "Send write" => "发送写入请求",
            "Enter one coil value (0/1, off/on, or false/true)." => {
                "输入一个线圈值（0/1、off/on 或 false/true）。"
            }
            "Enter one register value (0–65535, or 0x0000–0xFFFF)." => {
                "输入一个寄存器值（0–65535 或 0x0000–0xFFFF）。"
            }
            "Enter 1–1968 coil values separated by commas or spaces." => {
                "输入 1–1968 个线圈值，以逗号或空格分隔。"
            }
            "Enter 1–123 register values separated by commas or spaces." => {
                "输入 1–123 个寄存器值，以逗号或空格分隔。"
            }
            "01 Read Coils (0x)" => "01 读线圈（0x）",
            "02 Read Discrete Inputs (1x)" => "02 读离散输入（1x）",
            "03 Read Holding Registers (4x)" => "03 读保持寄存器（4x）",
            "04 Read Input Registers (3x)" => "04 读输入寄存器（3x）",
            "05 Write Single Coil (0x)" => "05 写单个线圈（0x）",
            "06 Write Single Register (4x)" => "06 写单个寄存器（4x）",
            "15 Write Multiple Coils (0x)" => "15 写多个线圈（0x）",
            "16 Write Multiple Registers (4x)" => "16 写多个寄存器（4x）",
            "Signed" => "有符号整数",
            "Unsigned" => "无符号整数",
            "Hex" => "十六进制",
            "Binary" => "二进制",
            "Long" => "32 位整数",
            "Long Inverse" => "32 位整数（字序交换）",
            "Float" => "单精度浮点",
            "Float Inverse" => "单精度浮点（字序交换）",
            "Double" => "双精度浮点",
            "Double Inverse" => "双精度浮点（字序交换）",
            "Light" => "浅色",
            "Dark" => "深色",
            "Check for updates" => "检查更新",
            "Checking for updates…" => "正在检查更新…",
            "You're up to date" => "已是最新版本",
            "No update package available" => "暂无适用的更新包",
            "Update check failed" => "检查更新失败",
            "Get update" => "获取更新",
            "Status" => "状态",
            "Disconnected" => "已断开",
            "Connected" => "已连接",
            "No port selected" => "未选择端口",
            "Auto Poll started..." => "正在启动轮询…",
            "Auto Poll running" => "正在轮询",
            "Auto Poll stopped" => "轮询已停止",
            "TX -- Not connected" => "TX -- 串口未连接",
            _ => english,
        }
    }

    pub fn message<'a>(self, message: &'a str) -> std::borrow::Cow<'a, str> {
        if self == Self::Chinese {
            for (prefix, translation) in [
                ("Connected: ", "已连接："),
                ("Connect failed: ", "连接失败："),
                ("Read error: ", "读取失败："),
                ("Write error: ", "写入失败："),
                ("Invalid write value: ", "写入值无效："),
                ("TX -- HEX error: ", "TX -- 十六进制数据无效："),
                ("TX -- Send failed: ", "TX -- 发送失败："),
            ] {
                if let Some(detail) = message.strip_prefix(prefix) {
                    return format!("{translation}{detail}").into();
                }
            }
            if let Some(function) = message
                .strip_prefix("Sending ")
                .and_then(|text| text.strip_suffix("..."))
            {
                return format!("正在发送 {}…", self.text(function)).into();
            }
            if let Some((count, address)) = message
                .strip_prefix("Write successful: ")
                .and_then(|text| text.split_once(" value(s) at address "))
            {
                return format!("写入成功，起始地址 {address}，共 {count} 个值").into();
            }
        }
        self.text(message).into()
    }
}
