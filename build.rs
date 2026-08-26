fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/iot-toolbox.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("packaging/windows/iot-toolbox.ico")
            .set("ProductName", "IoT Toolbox")
            .set("FileDescription", "Serial and Modbus debugging tool")
            .set("OriginalFilename", "iot-toolbox.exe")
            .compile()
            .expect("failed to compile Windows application resources");
    }
}
