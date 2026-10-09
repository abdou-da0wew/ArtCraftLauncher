//! PNG / ICO / ICNS encoding — hand-rolled so the launcher carries no image
//! library. All three formats can legally wrap zlib-compressed pixel data:
//!  * PNG  — IHDR + IDAT + IEND with per-row filter byte 0
//!  * ICO  — ICONDIR + ICONDIRENTRY, each entry a full PNG (Vista and later)
//!  * ICNS — `ic07`/`ic08`/`ic09`/`ic10`/`is32`/`il32` OSType + length + data

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use std::path::Path;

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = 0xffff_ffffu32;
    for b in data {
        c = table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

/// A complete PNG file from straight-alpha RGBA.
pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize / 2 + 128);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&be32(w));
    ihdr.extend_from_slice(&be32(h));
    ihdr.push(8); // bit depth
    ihdr.push(6); // colour type RGBA
    ihdr.push(0); // deflate
    ihdr.push(0); // adaptive filtering
    ihdr.push(0); // no interlace
    chunk(&mut out, b"IHDR", &ihdr);

    // IDAT — one filter byte per row
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for y in 0..h {
        raw.push(0);
        let s = (y * w * 4) as usize;
        raw.extend_from_slice(&rgba[s..s + (w * 4) as usize]);
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
    let _ = enc.write_all(&raw);
    let idat = enc.finish().unwrap_or_default();
    chunk(&mut out, b"IDAT", &idat);

    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&be32(data.len() as u32));
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&be32(crc32(&body)));
}

/// A multi-size `.ico`. `images` are `(size, png_bytes)`.
pub fn ico_bytes(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    // ICONDIR
    out.extend_from_slice(&[0, 0]); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, png) in images {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&le32(png.len() as u32)); // bytes in resource
        out.extend_from_slice(&le32(offset));
        offset += png.len() as u32;
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}

const IC07: &[u8; 4] = b"ic07"; // 128x128 PNG
const IC08: &[u8; 4] = b"ic08"; // 256x256 PNG
const IC09: &[u8; 4] = b"ic09"; // 512x512 PNG
const IC10: &[u8; 4] = b"ic10"; // 1024x1024 PNG (@2x of 512)

/// A multi-size `.icns`.
pub fn icns_bytes(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let total: usize = images.iter().map(|(_, d)| d.len() + 8).sum();
    let mut out = Vec::with_capacity(total + 8);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&be32((total + 8) as u32));
    for (size, data) in images {
        let tag: &[u8; 4] = match size {
            s if *s >= 1024 => IC10,
            s if *s >= 512 => IC09,
            s if *s >= 256 => IC08,
            _ => IC07,
        };
        out.extend_from_slice(tag);
        out.extend_from_slice(&be32((data.len() + 8) as u32));
        out.extend_from_slice(data);
    }
    out
}

/// Write bytes to `path`, creating parents, and mark it executable on unix.
pub fn write_file(path: &Path, data: &[u8], executable: bool) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, data).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}
