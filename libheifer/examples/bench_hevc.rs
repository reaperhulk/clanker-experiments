// SPDX-License-Identifier: LGPL-3.0-or-later
use libheifer::{container::Container, hevc};
use std::{fs, hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: bench_hevc image.heic iterations".into());
    }
    let data = fs::read(&args[1])?;
    let iterations: u32 = args[2].parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let mut checksum = 0u64;
    let mut elapsed = 0u128;
    for iteration in 0..=iterations {
        let start = Instant::now();
        let container = Container::parse(black_box(&data))?;
        for item in container.top_level_images() {
            let image = hevc::decode_item(&container, item.id)?;
            checksum += u64::from(image.plane(0).ok_or("missing Y")?.data()[0]);
            black_box(image);
        }
        drop(container);
        if iteration != 0 {
            elapsed += start.elapsed().as_nanos();
        }
    }
    println!("{{\"ns\":{elapsed},\"iterations\":{iterations},\"checksum\":{checksum}}}");
    Ok(())
}
