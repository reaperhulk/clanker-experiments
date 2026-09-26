// SPDX-License-Identifier: LGPL-3.0-or-later
//! Write the first HEVC item's NAL units as an Annex B stream.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let data = std::fs::read(args.next().ok_or("input path required")?)?;
    let container = libheifer::container::Container::parse(&data)?;
    let item = container
        .items
        .values()
        .find(|i| i.kind == *b"hvc1")
        .ok_or("no hvc1 item")?;
    let mut out = Vec::new();
    for nal in container.hevc_nals(item.id)? {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&nal);
    }
    std::fs::write(args.next().ok_or("output path required")?, out)?;
    Ok(())
}
