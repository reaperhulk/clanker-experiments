//! Encodes a raw 8-bit YUV picture with the built-in VVC encoder:
//! `vvc_encode in.yuv width height chroma_format qp effort out.266`.
use libheifer::vvc::encoder::{Picture8, Settings, encode};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 8 {
        eprintln!("usage: vvc_encode in.yuv width height chroma_format qp effort out.266");
        std::process::exit(2);
    }
    let data = std::fs::read(&a[1]).expect("input");
    let (w, h): (u32, u32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let chroma: u32 = a[4].parse().unwrap();
    let (sx, sy) = match chroma {
        1 => (1, 1),
        2 => (1, 0),
        _ => (0, 0),
    };
    let ls = (w * h) as usize;
    let cs = if chroma == 0 {
        0
    } else {
        ((w >> sx) * (h >> sy)) as usize
    };
    let input = Picture8 {
        width: w,
        height: h,
        planes: [&data[..ls], &data[ls..ls + cs], &data[ls + cs..ls + 2 * cs]],
    };
    let s = Settings {
        qp: a[5].parse().unwrap(),
        chroma,
        sar: None,
        effort: a[6].parse().unwrap(),
        deblocking: true,
    };
    let t = std::time::Instant::now();
    let (nals, _) = encode(&input, &s).expect("encode");
    eprintln!("{:.3}s", t.elapsed().as_secs_f64());
    let mut out = Vec::new();
    for n in nals {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&n);
    }
    std::fs::write(&a[7], out).expect("output");
}
