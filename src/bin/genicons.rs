//! Regenerates the launcher's own icon assets (`assets/launcher.ico`,
//! `assets/launcher.icns`) and the per-app reserved icons, from the embedded
//! artwork. Run with `cargo run --bin genicons` after changing the artwork.

fn main() {
    let root = std::path::Path::new("assets");
    let _ = std::fs::create_dir_all(root);

    // The ArtCraft mark at 1024, downsampled to each size.
    let (rgba, w, h) = artcraft_launcher::osint::launcher_icon_source().owned();
    let ico_sizes = [256u32, 64, 48, 32, 24, 16];
    let icns_sizes = [512u32, 256, 128, 32, 16];
    let src = artcraft_launcher::osint::IconSource::from_rgba(rgba.clone(), w, h);
    let ico: Vec<(u32, Vec<u8>)> = ico_sizes
        .iter()
        .map(|s| (*s, artcraft_launcher::osint::icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
        .collect();
    let icns: Vec<(u32, Vec<u8>)> = icns_sizes
        .iter()
        .map(|s| (*s, artcraft_launcher::osint::icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
        .collect();
    std::fs::write(root.join("launcher.ico"), artcraft_launcher::osint::ico_bytes(&ico)).unwrap();
    std::fs::write(root.join("launcher.icns"), artcraft_launcher::osint::icns_bytes(&icns)).unwrap();

    for a in artcraft_launcher::apps::APPS {
        let bytes: &[u8] = match a.slug {
            "photocraft" => include_bytes!("../../assets/images/photocraft-icon.png"),
            "vectorcraft" => include_bytes!("../../assets/images/vectorcraft-icon.png"),
            "filmcraft" => include_bytes!("../../assets/images/filmcraft-icon.png"),
            "lightcraft" => include_bytes!("../../assets/images/lightcraft-icon.png"),
            "pdfcraft" => include_bytes!("../../assets/images/pdfcraft-icon.png"),
            "effectcraft" => include_bytes!("../../assets/images/effectcraft-icon.png"),
            "designcraft" => include_bytes!("../../assets/images/designcraft-icon.png"),
            _ => include_bytes!("../../assets/images/pdfcraft-icon.png"),
        };
        let mut d = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
        let mut buf = vec![0u8; d.output_buffer_size()];
        let info = d.next_frame(&mut buf).unwrap();
        let src = artcraft_launcher::osint::IconSource::from_rgba(
            buf[..info.buffer_size()].to_vec(),
            info.width,
            info.height,
        );
        let app_ico: Vec<(u32, Vec<u8>)> = ico_sizes
            .iter()
            .map(|s| (*s, artcraft_launcher::osint::icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
            .collect();
        let app_icns: Vec<(u32, Vec<u8>)> = icns_sizes
            .iter()
            .map(|s| (*s, artcraft_launcher::osint::icons::encode_png(&src.resized(*s, *s).rgba, *s, *s)))
            .collect();
        std::fs::write(
            root.join(format!("{}.ico", a.slug)),
            artcraft_launcher::osint::ico_bytes(&app_ico),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("{}.icns", a.slug)),
            artcraft_launcher::osint::icns_bytes(&app_icns),
        )
        .unwrap();
    }
    println!("wrote assets/launcher.ico, assets/launcher.icns and 7 per-app pairs");
}
