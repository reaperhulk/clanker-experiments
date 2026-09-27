// SPDX-License-Identifier: LGPL-3.0-or-later
use libheifer::{container::Container, hevc};
use std::{fs, hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: profile_hevc image.heic")?;
    let data = fs::read(path)?;
    rusty_h265::prof::enable();
    let start = Instant::now();
    let container = Container::parse(&data)?;
    for item in container.top_level_images() {
        black_box(hevc::decode_item(&container, item.id)?);
    }
    drop(container);
    println!(
        "{}",
        rusty_h265::prof::report(start.elapsed().as_nanos() as u64)
    );
    Ok(())
}
