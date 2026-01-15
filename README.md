# IoT Toolbox

**Cross-platform, lightweight IoT debugging tool**  
Built with **Rust + egui** — runs natively on Windows, macOS and Linux  
( Web / WASM support planned for the future )

👉 **[Download latest release](https://github.com/SShnoodles/iot-toolbox/releases)** (when available)  
👉 Or build it yourself in ~2 minutes

---

## Features
- [x] Serial
- [ ] Modbus

## In action

( Screenshots coming soon)

## Build from source

```bash
# 1. Make sure you have recent Rust (1.75+ recommended)
rustup update

# 2. Windows only — add the msvc target (only needed once)
rustup target add x86_64-pc-windows-msvc

# 3. Build release version
cargo build --release
```

## License
MIT License

---
Made with ❤️ and egui

Thanks to [emilk/egui](https://github.com/emilk/egui) for the excellent immediate-mode GUI library!