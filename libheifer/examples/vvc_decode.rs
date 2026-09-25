//! Decodes an Annex B VVC stream and writes its first picture's planes
//! (8-bit samples as bytes, higher depths as little-endian u16), or every
//! picture in output order with `--all`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read input");
    let all = args.iter().any(|a| a == "--all");
    let nals = libheifer::vvc::split_annexb(&data);
    match libheifer::vvc::decode_all(nals) {
        Ok(frames) if !frames.is_empty() => {
            let mut out = Vec::new();
            for frame in frames.iter().take(if all { frames.len() } else { 1 }) {
                for (plane, _, _) in &frame.planes {
                    for &v in plane {
                        if frame.bit_depth > 8 {
                            out.extend_from_slice(&v.to_le_bytes());
                        } else {
                            out.push(v as u8);
                        }
                    }
                }
            }
            let frame = &frames[0];
            eprintln!(
                "frame {}x{} cf={} depth={} pictures={}",
                frame.width,
                frame.height,
                frame.chroma_format,
                frame.bit_depth,
                frames.len()
            );
            if let Some(path) = args.get(2).filter(|a| *a != "--all") {
                std::fs::write(path, out).expect("write output");
            }
        }
        Ok(_) => {
            eprintln!("error: NoPicture");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}
