// SPDX-License-Identifier: LGPL-3.0-or-later
//! Print coded-image parameter sets for compatibility investigations.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(std::env::args_os().nth(1).ok_or("input path required")?)?;
    let container = libheifer::container::Container::parse(&data)?;
    for item in container.items.values().filter(|i| i.kind == *b"hvc1") {
        for nal in container.hevc_nals(item.id)? {
            let kind = (nal[0] >> 1) & 63;
            let rbsp = rusty_h265::nal::unescape(&nal[2..]);
            if kind == 33 {
                println!(
                    "item {} SPS {:?}",
                    item.id,
                    rusty_h265::ps::parse_sps(&rbsp.data)?
                );
            }
            if kind == 34 {
                println!(
                    "item {} PPS {:?}",
                    item.id,
                    rusty_h265::ps::parse_pps(&rbsp.data)?
                );
            }
        }
    }
    Ok(())
}
