# IoT Toolbox

**Cross-platform, lightweight IoT debugging tool**  
Built with **Rust + egui** — runs natively on Windows, macOS and Linux  

👉 **[Download latest release](https://github.com/SShnoodles/iot-toolbox/releases)** (when available)  
👉 Or build it yourself in ~2 minutes

---

## Features
- [x] Serial
- [x] Modbus Read
- [ ] Modbus

## In action

![iot toolbox1](https://s1.imagehub.cc/images/2026/01/15/a68f865bb87bc5a7e6f83bf99d4348e2.png)
## Build from source

```bash
# 1. Make sure you have recent Rust (1.89+ recommended)
rustup update

# 2. Windows only — add the msvc target (only needed once)
rustup target add x86_64-pc-windows-msvc

# 3. Build release version
cargo build --release
cargo build --release --target x86_64-pc-windows-msvc
```

## License
MIT License

---
Made with ❤️ and egui

Thanks to [emilk/egui](https://github.com/emilk/egui) for the excellent immediate-mode GUI library!