//! Decodes the first picture of an Annex B VVC stream and writes its planes
//! (8-bit samples as bytes, higher depths as little-endian u16).
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read input");
    let nals = libheifer::vvc::split_annexb(&data);
    match libheifer::vvc::decode_nals(nals) {
        Ok(frame) => {
            let mut out = Vec::new();
            for (plane, _, _) in &frame.planes {
                for &v in plane {
                    if frame.bit_depth > 8 {
                        out.extend_from_slice(&v.to_le_bytes());
                    } else {
                        out.push(v as u8);
                    }
                }
            }
            eprintln!("frame {}x{} cf={} depth={}", frame.width, frame.height, frame.chroma_format, frame.bit_depth);
            if let Some(path) = args.get(2) {
                std::fs::write(path, out).expect("write output");
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}
