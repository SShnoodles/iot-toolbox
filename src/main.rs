mod i18n;
mod modbus;
mod serial;
mod ui;
mod update;

use gpui_kit::*;

const APP_FULL: &str = concat!("IoT Toolbox v", env!("CARGO_PKG_VERSION"));

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            cx.set_app_identity("iot-toolbox", "IoT Toolbox");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(720.)),
                    cx,
                ))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                window.set_window_title(APP_FULL);
                window.set_app_id("iot-toolbox");
                cx.new(|cx| ui::Toolbox::new(window, cx))
            })
            .expect("failed to open IoT Toolbox window");
            cx.activate(true);
        });
}
