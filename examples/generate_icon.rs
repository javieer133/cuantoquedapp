//! Generates the app icon (a usage gauge) as the PNGs of a macOS `.iconset` folder.
//! Usage: `cargo run --release --example generate_icon -- <output folder>`
//! No dependencies: the PNG encoder stores the data uncompressed (the packaging script
//! recompresses it with `sips`).
use std::path::PathBuf;

/// Share of the gauge that is filled, as in "62% used".
const FILLED: f64 = 0.62;

/// Output file name and pixel size for every entry of a macOS iconset.
const ICONSET: [(&str, usize); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

/// Sizes stored in the Windows `.ico` (Explorer, taskbar, alt-tab).
const ICO_SIZES: [usize; 4] = [16, 32, 48, 256];

/// `.ico` file with one PNG-compressed image per size (supported since Windows Vista).
fn encode_ico(sizes: &[usize]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&size| encode_png(size, size, &render(size)))
        .collect();
    let mut ico = vec![0, 0, 1, 0]; // reserved, type 1 = icon
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (&size, image) in sizes.iter().zip(&images) {
        ico.push(if size >= 256 { 0 } else { size as u8 }); // 0 means 256
        ico.push(if size >= 256 { 0 } else { size as u8 });
        ico.extend_from_slice(&[0, 0]); // palette colors, reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // color planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(image.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += image.len();
    }
    for image in images {
        ico.extend_from_slice(&image);
    }
    ico
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(folder) = args.next().map(PathBuf::from) else {
        eprintln!("Usage: generate_icon <iconset folder> [--ico <file.ico>]");
        std::process::exit(2);
    };
    if args.next().is_some_and(|flag| flag == "--ico") {
        let Some(path) = args.next().map(PathBuf::from) else {
            eprintln!("Missing the path after --ico");
            std::process::exit(2);
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(&path, encode_ico(&ICO_SIZES)) {
            eprintln!("Could not write the .ico: {error}");
            std::process::exit(1);
        }
        println!("Windows icon in {}", path.display());
    }
    if let Err(error) = std::fs::create_dir_all(&folder) {
        eprintln!("Could not create the folder: {error}");
        std::process::exit(1);
    }
    let mut rendered: Vec<(usize, Vec<u8>)> = Vec::new();
    for (name, size) in ICONSET {
        if !rendered.iter().any(|(done, _)| *done == size) {
            rendered.push((size, encode_png(size, size, &render(size))));
        }
        let png = &rendered.iter().find(|(done, _)| *done == size).unwrap().1;
        if let Err(error) = std::fs::write(folder.join(name), png) {
            eprintln!("Could not write {name}: {error}");
            std::process::exit(1);
        }
    }
    println!("{} files in {}", ICONSET.len(), folder.display());
}

type Rgb = [f64; 3];

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// RGBA pixels of the icon at `size` x `size`, with 3x3 supersampling for smooth edges.
fn render(size: usize) -> Vec<u8> {
    const SAMPLES: usize = 3;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f64 + (sx as f64 + 0.5) / SAMPLES as f64;
                    let py = y as f64 + (sy as f64 + 0.5) / SAMPLES as f64;
                    if let Some(color) = color_at(px / size as f64, py / size as f64) {
                        r += color[0];
                        g += color[1];
                        b += color[2];
                        a += 1.0;
                    }
                }
            }
            let start = (y * size + x) * 4;
            if a > 0.0 {
                pixels[start] = (r / a * 255.0).round() as u8;
                pixels[start + 1] = (g / a * 255.0).round() as u8;
                pixels[start + 2] = (b / a * 255.0).round() as u8;
                pixels[start + 3] = (a / (SAMPLES * SAMPLES) as f64 * 255.0).round() as u8;
            }
        }
    }
    pixels
}

/// Color of the icon at (`u`, `v`) in 0..1 coordinates; `None` outside the artwork.
fn color_at(u: f64, v: f64) -> Option<Rgb> {
    // macOS icons leave a margin: the artwork is ~80% of the canvas.
    let half = 0.4025;
    let radius = 0.2237 * (half * 2.0);
    let (dx, dy) = (u - 0.5, v - 0.5);
    let qx = dx.abs() - (half - radius);
    let qy = dy.abs() - (half - radius);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
    if outside > 0.0 {
        return None;
    }

    // Dark vertical gradient background.
    let background = mix(
        [0.153, 0.188, 0.247],
        [0.067, 0.094, 0.153],
        (v - 0.1) / 0.8,
    );

    // Gauge: a 270 degree ring, gap at the bottom, filled clockwise from the lower left.
    let distance = (dx * dx + dy * dy).sqrt();
    let (ring, width) = (0.25, 0.075);
    if (distance - ring).abs() <= width / 2.0 {
        let angle = dy.atan2(dx).to_degrees(); // 0 = right, clockwise positive (y down)
        let t = (angle - 135.0).rem_euclid(360.0) / 270.0;
        if t <= 1.0 {
            return Some(if t <= FILLED {
                mix([0.20, 0.83, 0.60], [0.10, 0.65, 0.45], t / FILLED)
            } else {
                [0.294, 0.333, 0.388]
            });
        }
    }
    // Small hub in the center, like a gauge pivot.
    if distance <= 0.045 {
        return Some([0.90, 0.92, 0.94]);
    }
    Some(background)
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc = table[((crc ^ u32::from(*byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Minimal RGBA PNG: zlib stream made of stored (uncompressed) deflate blocks.
fn encode_png(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * 4 + 1) * height);
    for row in rgba.chunks(width * 4) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len())); // BFINAL, BTYPE = stored
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::new();
    header.extend_from_slice(&(width as u32).to_be_bytes());
    header.extend_from_slice(&(height as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}
