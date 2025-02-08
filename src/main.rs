mod serial;

use serial::app::SerialTool;

fn main() {
    let app = SerialTool::new();
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "IoT Debugging Tool",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    );
}