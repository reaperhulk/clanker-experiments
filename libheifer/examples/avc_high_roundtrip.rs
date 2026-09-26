// SPDX-License-Identifier: LGPL-3.0-or-later
//! Writes libheifer's High-profile AVC encoder output for synthetic pictures
//! in every supported format: `<name>.264` (Annex B) and `<name>.rec`, the
//! encoder's reconstruction without deblocking (or the input, lossless), as
//! planar samples (8-bit, or 16-bit little-endian above 8 bits). Streams
//! with the deblocking filter on have no exact reconstruction to compare
//! with; the decoders are compared with each other.
//! tools/test_avc_high_roundtrip.py decodes the streams with test-only native
//! decoders and compares.
//!
//! usage: avc_high_roundtrip OUTPUT_DIR
use libheifer::avc_encoder::VuiSignal;
use libheifer::avc_high::{Picture, Settings, encode_with_reconstruction};
use std::io::Write;

fn picture(w: usize, h: usize, chroma: u8, depth: u8, pattern: u32) -> [Vec<u16>; 3] {
    let (sx, sy) = match chroma {
        1 => (2, 2),
        2 => (2, 1),
        _ => (1, 1),
    };
    let mut state = 0x1234567u32.wrapping_add(pattern * 7919 + (w * 31 + h) as u32);
    let mut noise = || {
        state = state.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fff_ffff;
        state >> 16
    };
    let top = (1u32 << depth) - 1;
    let dims = [
        (w, h),
        (w.div_ceil(sx), h.div_ceil(sy)),
        (w.div_ceil(sx), h.div_ceil(sy)),
    ];
    std::array::from_fn(|c| {
        if c > 0 && chroma == 0 {
            return vec![];
        }
        let (pw, ph) = dims[c];
        let mut out = Vec::with_capacity(pw * ph);
        for y in 0..ph {
            for x in 0..pw {
                let (xi, yi, ci) = (x as i64, y as i64, c as i64);
                let v: i64 = match pattern {
                    0 => (xi * 255 / (pw as i64 - 1).max(1) + yi * 3 + ci * 40) & 255,
                    1 => i64::from(noise() & 255),
                    2 => {
                        if ((xi >> 2) + (yi >> 3) + ci) & 1 != 0 {
                            230
                        } else {
                            20
                        }
                    }
                    3 => (xi * xi + yi * 7 * (ci + 1) + i64::from(noise() & 15)) & 255,
                    _ => {
                        128 + (((xi - pw as i64 / 2) * (yi - ph as i64 / 2) * (ci + 1)) >> 3)
                            + i64::from(noise() & 31)
                            - 16
                    }
                };
                let v = v.clamp(0, 255) as u32;
                let v = if depth > 8 {
                    (v * top / 255 + (noise() & ((1 << (depth - 8)) - 1))).min(top)
                } else {
                    v
                };
                out.push(v as u16);
            }
        }
        out
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    std::fs::create_dir_all(&dir)?;
    let mut manifest = std::fs::File::create(dir.join("manifest.txt"))?;
    let sizes = [
        (16, 16),
        (64, 48),
        (50, 38),
        (130, 66),
        (8, 8),
        (2, 2),
        (200, 136),
    ];
    for chroma in 0..4u8 {
        for depth in [8u8, 10] {
            for &(w, h) in &sizes {
                for pattern in 0..5 {
                    for (label, qp, lossless, t8) in [
                        ("q12", 12, false, true),
                        ("q24-deblock", 24, false, true),
                        ("q40-deblock", 40, false, true),
                        ("q30", 30, false, true),
                        ("q30n8", 30, false, false),
                        ("q45", 45, false, true),
                        ("qmin", -12, false, true),
                        ("lossless", 0, true, true),
                        ("lossless-n8", 0, true, false),
                    ] {
                        // A few formats x sizes for everything; all patterns only at one size.
                        if pattern != 4 && (w, h) != (64, 48) {
                            continue;
                        }
                        let planes = picture(w, h, chroma, depth, pattern);
                        let pic = Picture {
                            width: w as u32,
                            height: h as u32,
                            chroma,
                            bit_depth: depth,
                            planes: [&planes[0], &planes[1], &planes[2]],
                        };
                        let settings = Settings {
                            qp,
                            lossless,
                            transform_8x8: t8,
                            deblocking: label.ends_with("deblock"),
                            vui: VuiSignal::default(),
                        };
                        let name = format!("c{chroma}-d{depth}-{w}x{h}-p{pattern}-{label}");
                        let (units, rec) = encode_with_reconstruction(&pic, &settings)?;
                        let mut stream = Vec::new();
                        for u in units {
                            stream.extend_from_slice(&[0, 0, 0, 1]);
                            stream.extend_from_slice(&u);
                        }
                        std::fs::write(dir.join(format!("{name}.264")), &stream)?;
                        let mut raw = Vec::new();
                        for plane in &rec {
                            for &v in plane {
                                if depth > 8 {
                                    raw.extend_from_slice(&v.to_le_bytes());
                                } else {
                                    raw.push(v as u8);
                                }
                            }
                        }
                        if lossless {
                            let input: Vec<u16> = planes.iter().flatten().copied().collect();
                            let got: Vec<u16> = rec.iter().flatten().copied().collect();
                            assert_eq!(
                                input, got,
                                "{name}: lossless reconstruction differs from the input"
                            );
                        }
                        std::fs::write(dir.join(format!("{name}.rec")), &raw)?;
                        writeln!(
                            manifest,
                            "{name} {chroma} {depth} {w} {h} {}",
                            u8::from(settings.deblocking)
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
