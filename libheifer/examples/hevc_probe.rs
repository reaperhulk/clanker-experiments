// SPDX-License-Identifier: LGPL-3.0-or-later
use libheifer::{container::Container, hevc};
use std::{fs, io::Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 && args.len() != 4 {
        return Err("usage: hevc_probe input.heic output.bin [default]".into());
    }
    let data = fs::read(&args[1])?;
    let container = Container::parse(&data)?;
    let mut context = libheifer::context::Context::default();
    if args.len() == 4 {
        context.read(std::sync::Arc::new(data.clone()))?;
    }
    let mut out = fs::File::create(&args[2])?;
    let ids: Vec<_> = container.top_level_images().map(|i| i.id).collect();
    out.write_all(&(ids.len() as u32).to_le_bytes())?;
    for id in ids {
        let image = if let Some(document) = &context.document {
            libheifer::decoding::decode(
                document,
                id,
                99,
                99,
                libheifer::decoding::DecodeOptions {
                    ignore_transformations: true,
                    ..Default::default()
                },
            )?
        } else {
            hevc::decode_item(&container, id)?
        };
        for n in [
            id,
            image.width,
            image.height,
            image.colorspace as u32,
            image.chroma as u32,
        ] {
            out.write_all(&n.to_le_bytes())?;
        }
        for channel in [0, 1, 2] {
            if let Some(p) = image.plane(channel) {
                for n in [
                    p.width,
                    p.height,
                    u32::from(p.bit_depth),
                    p.storage_bits() as u32,
                    p.stride as u32,
                ] {
                    out.write_all(&n.to_le_bytes())?;
                }
                let row = p.width as usize * p.bytes_per_pixel;
                for y in 0..p.height as usize {
                    out.write_all(&p.data()[y * p.stride..y * p.stride + row])?;
                }
            } else {
                out.write_all(&[0; 20])?;
            }
        }
    }
    Ok(())
}
