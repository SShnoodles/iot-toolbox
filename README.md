# IoT Toolbox

![IoT Toolbox logo](packaging/icons/iot-toolbox-256.png)

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

# 2. Linux only — install native build dependencies (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install libgl1-mesa-dev libudev-dev libwayland-dev \
  libx11-xcb-dev libxkbcommon-dev pkg-config

# 3. Windows only — add the msvc target (only needed once)
rustup target add x86_64-pc-windows-msvc

# 4. Build release version
cargo build --release
cargo build --release --target x86_64-pc-windows-msvc
```

## Release packages

The `Build release packages` GitHub Actions workflow creates these desktop
packages:

| Platform | Packages |
| --- | --- |
| Linux | `.tar.gz`, Ubuntu/Debian `.deb`, and `.AppImage` |
| Windows | Portable `.zip` and Inno Setup installer `.exe` |
| macOS | `.app.zip` and `.dmg`, for Apple Silicon and Intel |

- Run it manually from **Actions → Build release packages → Run workflow** to
  download the packages as a workflow artifact.
- Push a version tag to build the packages and attach them to a GitHub Release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Install the downloaded Debian package with:

```bash
sudo apt install ./iot-toolbox_0.1.0_amd64.deb
```

Or run the AppImage without installing it:

```bash
chmod +x iot-toolbox-0.1.0-linux-x86_64.AppImage
./iot-toolbox-0.1.0-linux-x86_64.AppImage
```

On Windows, extract the portable `.zip` or run the `-setup.exe` installer.
The generated Windows binaries are not code signed.

On macOS, open the `.dmg` and drag **IoT Toolbox** to **Applications**, or
extract the `.app.zip`. The generated macOS apps are ad-hoc signed but not
notarized, so Gatekeeper may ask for confirmation on first launch.

## License
MIT License

---
Made with ❤️ and egui

Thanks to [emilk/egui](https://github.com/emilk/egui) for the excellent immediate-mode GUI library!
