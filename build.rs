fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Embed the window/taskbar icon into the Windows executables.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("embed the Windows icon");
    }
}
