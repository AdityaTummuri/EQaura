fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("the Slint UI should compile");
    #[cfg(windows)]
    windows_resources();
}

/// Gives the Windows program its icon and version details.
#[cfg(windows)]
fn windows_resources() {
    let mut r = winresource::WindowsResource::new();
    r.set_icon("assets/icon.ico").set("ProductName", "EQaura").set("FileDescription", "EQaura equalizer");
    r.compile().expect("the Windows resources should compile");
}
