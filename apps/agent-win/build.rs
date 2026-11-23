fn main() {
    #[cfg(windows)]
    {
        let mut r = winresource::WindowsResource::new();
        r.set_icon_with_id("../desktop/assets/icon.ico", "1").set("FileDescription", "EQaura tray helper");
        r.compile().expect("the Windows resources should compile");
    }
}
