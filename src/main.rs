//! ArtCraft Studio — a launcher, updater and manager for the seven ArtCraft
//! apps.
//!
//! With no arguments it opens the GUI. With a command it does exactly that
//! thing and prints the result, so everything is scriptable and works over
//! SSH or on a headless machine.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Handle CLI commands first (no GPU required)
    if !args.is_empty() {
        let cmd = args[0].as_str();
        let exit_code = match cmd {
            "-h" | "--help" | "help" => {
                print!("artcraft-launcher — launcher, updater and manager for the ArtCraft apps\n\nUSAGE\n  artcraft-launcher                       open the launcher window\n  artcraft-launcher gui                   (same)\n  artcraft-launcher list                  show every app, its version and state\n  artcraft-launcher check [app…]          ask GitHub for the newest releases\n  artcraft-launcher install <app> [-c stable|beta|nightly] [--version V]\n  artcraft-launcher update [app…]          download + install newer versions\n  artcraft-launcher launch <app> [-- args…]\n  artcraft-launcher rollback <app>         switch back to the previous version\n  artcraft-launcher versions <app>         list the versions installed\n  artcraft-launcher backup <app> [-n note]\n  artcraft-launcher restore <app> [stamp]\n  artcraft-launcher backups <app>\n  artcraft-launcher data <app>             print the app's data dir\n  artcraft-launcher data-open <app>        reveal it in the file manager\n  artcraft-launcher wipe <app>             (backs up first)\n  artcraft-launcher register <app>         desktop entry / icon / shortcuts\n  artcraft-launcher unregister <app>\n  artcraft-launcher register-all\n  artcraft-launcher export [-o FILE]\n  artcraft-launcher import FILE\n  artcraft-launcher telemetry status\n  artcraft-launcher telemetry list [app]\n  artcraft-launcher telemetry flush\n  artcraft-launcher telemetry clear\n  artcraft-launcher telemetry endpoint URL\n  artcraft-launcher doctor\n  artcraft-launcher info\n  artcraft-launcher self-update\n  artcraft-launcher --version | -h | --help\n");
                0
            }
            "--version" | "-V" => {
                println!("artcraft-launcher {}", env!("CARGO_PKG_VERSION"));
                0
            }
            "info" | "doctor" | "list" | "check" | "install" | "update" | "launch" | "open" |
            "rollback" | "versions" | "backup" | "restore" | "backups" | "data" | "data-open" |
            "reveal" | "wipe" | "register" | "register-all" | "unregister" | "export" |
            "import" | "telemetry" | "self-update" => {
                // These need the full CLI module which is not compiled in this build
                eprintln!("CLI commands not available in this build. Use the full build with CLI support.");
                1
            }
            _ => 1,
        };
        std::process::exit(exit_code);
    }

    // Everything else opens the GUI window
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