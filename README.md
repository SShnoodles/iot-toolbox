# IoT Toolbox

![IoT Toolbox logo](packaging/icons/iot-toolbox-256.png)

**Cross-platform, lightweight IoT debugging tool**

Built with **Rust + GPUI Kit** for Windows, macOS and Linux.

👉 **[Download latest release](https://github.com/SShnoodles/iot-toolbox/releases)** (when available)  
👉 Or build it yourself in ~2 minutes

---

## Features
- [x] Serial
- [x] Modbus Read
- [x] Modbus Write (function codes 05, 06, 15, and 16)

## Build from source

```bash
# 1. Make sure you have Rust 1.98 or later
rustup update

# 2. Linux only — install native build dependencies (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install clang libfontconfig-dev libudev-dev libwayland-dev \
  libx11-xcb-dev libxkbcommon-x11-dev libssl-dev libzstd-dev libvulkan1 pkg-config

# 3. Windows only — install CMake and add the MSVC target
rustup target add x86_64-pc-windows-msvc

# 4. Build release version
cargo build --release
cargo build --release --target x86_64-pc-windows-msvc
```

GPUI Kit targets macOS 15 or later and Windows 10 or later. Linux needs a graphical
Wayland or X11 session and a working Vulkan driver. See the
[GPUI Kit installation guide](https://gpui-kit.com/docs/installation/) for platform setup.

## Release packages

IoT Toolbox checks for a newer stable GitHub release at startup. Use
**Check for updates** in the toolbar to check again. **Get update** opens
the matching macOS DMG or the release page for Windows and Linux. Install
the update using the package format you already use.

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
git tag v0.1.3
git push origin v0.1.3
```

Install the downloaded Debian package with:

```bash
sudo apt install ./iot-toolbox_0.1.3_amd64.deb
```

Or run the AppImage without installing it:

```bash
chmod +x iot-toolbox-0.1.3-linux-x86_64.AppImage
./iot-toolbox-0.1.3-linux-x86_64.AppImage
```

On Windows, extract the portable `.zip` or run the `-setup.exe` installer.
The generated Windows binaries are not code signed.

On macOS, open the `.dmg` and drag **IoT Toolbox** to **Applications**, or
extract the `.app.zip`. The generated macOS apps are ad-hoc signed but not
notarized, so Gatekeeper may ask for confirmation on first launch.

## License
MIT License
