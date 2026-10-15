fn main() {
    // Fluent is the Windows 11 look; it follows the system light/dark setting.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("compile ui/app.slint");

    // Icon and version info shown by Explorer and the taskbar.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/kafkalens.ico")
            .set("ProductName", "Kafkalens")
            .set("FileDescription", "Kafkalens")
            .set("LegalCopyright", "MIT OR Apache-2.0");
        resource.compile().expect("compile Windows resources");
    }
}
