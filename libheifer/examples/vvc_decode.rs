//! Decodes an Annex B VVC stream and writes its first picture's planes
//! (8-bit samples as bytes, higher depths as little-endian u16), or every
//! picture in output order with `--all`.
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read input");
    let all = args.iter().any(|a| a == "--all");
    let mut out = args
        .get(2)
        .filter(|a| *a != "--all")
        .map(|p| std::io::BufWriter::new(std::fs::File::create(p).expect("create output")));
    let mut d = libheifer::vvc::Decoder::new();
    let mut count = 0usize;
    let mut first = None;
    let mut write = |frame: libheifer::vvc::Frame, count: &mut usize| {
        if *count == 0 {
            first = Some((
                frame.width,
                frame.height,
                frame.chroma_format,
                frame.bit_depth,
            ));
        }
        if (all || *count == 0)
            && let Some(o) = out.as_mut()
        {
            let mut buf = Vec::new();
            for (plane, _, _) in &frame.planes {
                for &v in plane {
                    if frame.bit_depth > 8 {
                        buf.extend_from_slice(&v.to_le_bytes());
                    } else {
                        buf.push(v as u8);
                    }
                }
            }
            o.write_all(&buf).expect("write output");
        }
        *count += 1;
    };
    let mut result = Ok(());
    for nal in libheifer::vvc::split_annexb(&data) {
        result = d.push_nal(nal);
        if result.is_err() {
            break;
        }
        while let Some(f) = d.pop_output() {
            write(f, &mut count);
        }
    }
    if result.is_ok() {
        result = d.flush();
        while let Some(f) = d.pop_output() {
            write(f, &mut count);
        }
    }
    if let Some(o) = out.as_mut() {
        o.flush().expect("write output");
    }
    if let Err(e) = result {
        eprintln!("error: {e} (after {count} output pictures)");
        std::process::exit(2);
    }
    let Some((w, h, cf, depth)) = first else {
        eprintln!("error: NoPicture");
        std::process::exit(2);
    };
    eprintln!("frame {w}x{h} cf={cf} depth={depth} pictures={count}");
}
