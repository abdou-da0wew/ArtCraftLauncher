//! ArtCraft Studio — a launcher, updater and manager for the seven ArtCraft
//! apps.
//!
//! With no arguments it opens the GUI. With a command it does exactly that
//! thing and prints the result, so everything is scriptable and works over
//! SSH or on a headless machine.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--` separates the launcher's own flags from an app's.
    if !args.is_empty() {
        match args[0].as_str() {
            "-h" | "--help" | "help" | "--version" | "-V" | "list" | "check" | "install"
            | "update" | "launch" | "open" | "rollback" | "versions" | "backup" | "restore"
            | "backups" | "data" | "data-open" | "reveal" | "wipe" | "register" | "register-all"
            | "unregister" | "export" | "import" | "telemetry" | "doctor" | "info" | "self-update" => {
                std::process::exit(artcraft_launcher::cli::main());
            }
            _ => {}
        }
    }

    // Everything else opens the window.
    if let Err(e) = artcraft_launcher::window::run() {
        eprintln!("artcraft-launcher: {e}");
        eprintln!();
        eprintln!("The CLI works without a GPU:");
        eprintln!("  artcraft-launcher list");
        eprintln!("  artcraft-launcher check");
        eprintln!("  artcraft-launcher install pdfcraft");
        std::process::exit(1);
    }
}
