//! Embeds the app icon and version info in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/AppIcon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/AppIcon.ico")
            .set("ProductName", "Lazy Arranger Midi DAW")
            .set("FileDescription", "Lazy Arranger Midi DAW");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=could not embed the Windows icon: {error}");
        }
    }
}
