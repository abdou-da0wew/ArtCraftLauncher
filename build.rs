use std::path::Path;

fn main() {
    // Re-run when the artwork changes.
    println!("cargo:rerun-if-changed=assets/launcher.ico");
    println!("cargo:rerun-if-changed=assets/launcher.icns");
    println!("cargo:rerun-if-changed=assets/shaders/hero.wgsl");
    println!("cargo:rerun-if-changed=assets/fonts");
    println!("cargo:rerun-if-changed=build.rs");

    // Reserve the launcher's own icon inside the .exe on Windows, so the
    // taskbar, Alt-Tab and Explorer all show the ArtCraft mark rather than
    // the default Rust icon. Guarded on the TARGET, not the host.
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let ico = Path::new("assets/launcher.ico");
        if ico.exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon(ico.to_str().unwrap());
            if let Err(e) = res.compile() {
                println!("cargo:warning=failed to embed launcher.ico: {e}");
            }
        }
    }
}
