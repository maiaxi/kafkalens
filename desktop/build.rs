fn main() {
    // Fluent is the Windows 11 look; it follows the system light/dark setting.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("compile ui/app.slint");
}
