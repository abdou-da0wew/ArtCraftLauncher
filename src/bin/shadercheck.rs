//! Validates the shader with naga — the same front-end and validator wgpu
//! uses. Catches syntax and type errors at build time instead of at runtime
//! on a machine that may have no other diagnostics.

fn main() {
    let path = std::path::Path::new("assets/shaders/hero.wgsl");
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let module = match naga::front::wgsl::parse_str(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("WGSL PARSE ERROR in {path:?}:\n{e:#?}");
            std::process::exit(1);
        }
    };
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    match validator.validate(&module) {
        Ok(_) => {
            println!("hero.wgsl: parse + validate OK ({} entry points)", {
                let n = module.entry_points.len();
                n
            });
            for ep in &module.entry_points {
                println!(
                    "  {:?} {:?} stage={:?} workgroup_size={:?}",
                    ep.name,
                    ep.stage,
                    ep.stage,
                    ep.workgroup_size
                );
            }
        }
        Err(e) => {
            eprintln!("WGSL VALIDATION ERROR in {path:?}:\n{e:#?}");
            std::process::exit(1);
        }
    }
}
