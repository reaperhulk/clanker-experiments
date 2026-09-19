// SPDX-License-Identifier: LGPL-3.0-or-later
// Layout semantics adapted from libheif, Copyright Dirk Farin and contributors.
use super::{Component, Configuration, Result, unspecified, unsupported};
use crate::{container::Container, context::ContextError, image::Image};

fn aligned(n: u64, unit: u64) -> u64 {
    if unit == 0 {
        n
    } else {
        n.div_ceil(unit) * unit
    }
}
fn row_overflow() -> ContextError {
    ContextError::invalid(
        129,
        "Invalid image size: uncompressed tile row size exceeds 32-bit range",
    )
}
#[derive(Clone)]
struct Entry {
    config: Component,
    id: u32,
    channel: i32,
    width: u32,
    height: u32,
}
#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Byte,
    BlockComponent,
    BlockPixel,
    Component,
    Pixel,
    Mixed,
    Row,
    Tile,
}
fn choose(c: &Configuration) -> Result<Layout> {
    if c.interleave == 0
        && c.block_size == 0
        && c.pixel_size == 0
        && c.sampling == 0
        && c.components
            .iter()
            .all(|s| matches!(s.bits, 8 | 16 | 32 | 64) || (s.bits == 128 && s.format == 2))
    {
        return Ok(Layout::Byte);
    }
    let block_bits = u16::from(c.block_size) * 8;
    if c.interleave == 0
        && (1..=8).contains(&c.block_size)
        && c.pixel_size == 0
        && c.sampling == 0
        && c.flags & 0x80 == 0
        && c.components.iter().all(|s| {
            s.bits <= 16 && s.format == 0 && s.bits <= block_bits && s.bits > block_bits / 2
        })
    {
        return Ok(Layout::BlockComponent);
    }
    let common = c
        .components
        .iter()
        .all(|s| s.bits <= 16 && s.format == 0 && s.align <= 2)
        && c.block_size == 0
        && c.flags & 0x70 == 0;
    let endian = c.flags & 0x80 == 0 || c.components.iter().all(|s| s.bits <= 8);
    let tile_align = match c.sampling {
        1 => c.tile_align.is_multiple_of(2),
        2 => c.tile_align.is_multiple_of(4),
        _ => true,
    };
    let layout = if common {
        match c.interleave {
            0 | 4
                if c.sampling <= 2
                    && (c.sampling == 0 || c.row_align.is_multiple_of(2))
                    && tile_align
                    && c.pixel_size == 0
                    && endian =>
            {
                Some(if c.interleave == 0 {
                    Layout::Component
                } else {
                    Layout::Tile
                })
            }
            1 if c.sampling == 0 && c.flags & 0x80 == 0 => Some(Layout::Pixel),
            2 if matches!(c.sampling, 1 | 2) && tile_align && c.pixel_size == 0 && endian => {
                Some(Layout::Mixed)
            }
            3 if c.sampling == 0 && c.pixel_size == 0 && endian => Some(Layout::Row),
            _ => None,
        }
    } else {
        None
    };
    let layout = layout.or_else(|| {
        if c.interleave == 1
            && (1..=8).contains(&c.pixel_size)
            && (c.block_size == 0 || u32::from(c.block_size) == c.pixel_size)
            && c.sampling == 0
            && c.flags & 0x80 == 0
            && c.components.iter().all(|s| s.bits <= 16 && s.format == 0)
            && c.components.iter().map(|s| u64::from(s.bits)).sum::<u64>()
                <= u64::from(c.pixel_size) * 8
        {
            Some(Layout::BlockPixel)
        } else {
            None
        }
    });
    layout.ok_or_else(|| {
        unsupported(format!(
            "No decoder found for uncompressed format (interleave_type of {})",
            c.interleave
        ))
    })
}
fn hard_limits(c: &Configuration) -> Result<()> {
    for s in &c.components {
        let message = match s.format {
            0 | 3 if s.bits > 64 => Some("Maximum supported integer bit-depth is 64 bits."),
            1 if !matches!(s.bits, 32 | 64) => Some("Only 32 bit and 64 bit floats are supported."),
            2 if !matches!(s.bits, 64 | 128) => {
                Some("Only 2x32 bit and 2x64 bit complex values are supported.")
            }
            _ => None,
        };
        if let Some(m) = message {
            return Err(ContextError::new(
                4,
                0,
                format!("Unsupported feature: Unspecified: {m}"),
            ));
        }
    }
    if !matches!(c.interleave, 1 | 5) && c.pixel_size != 0 {
        return Err(unspecified(
            "uncC pixel_size must be 0 for interleave_types other than 1 or 5.",
        ));
    }
    if c.interleave == 1
        && c.pixel_size != 0
        && c.components
            .iter()
            .map(|s| u64::from(s.bits))
            .sum::<u64>()
            .div_ceil(8)
            > u64::from(c.pixel_size)
    {
        return Err(unspecified(
            "uncC pixel_size smaller than sum of component sizes.",
        ));
    }
    Ok(())
}
fn sizes(c: &Configuration, entries: &[Entry], layout: Layout, w: u32, h: u32) -> Result<Vec<u64>> {
    if matches!(layout, Layout::BlockComponent | Layout::BlockPixel) {
        let bytes = if layout == Layout::BlockComponent {
            u64::from(c.block_size)
        } else {
            u64::from(c.pixel_size)
        };
        let row = bytes * u64::from(w);
        if row > u64::from(u32::MAX) {
            return Err(row_overflow());
        }
        let n = if layout == Layout::BlockComponent {
            entries.len() as u64
        } else {
            1
        };
        return Ok(vec![aligned(
            aligned(row, u64::from(c.row_align)) * u64::from(h) * n,
            u64::from(c.tile_align),
        )]);
    }
    let mut rows = Vec::new();
    for e in entries {
        let mut bits = u64::from(e.config.bits);
        if e.config.align != 0 && !(layout == Layout::Mixed && matches!(e.channel, 1 | 2)) {
            bits = aligned(bits.div_ceil(8), u64::from(e.config.align)) * 8;
        }
        let width = if layout == Layout::Tile { w } else { e.width };
        let row = bits * u64::from(width);
        if row > u64::from(u32::MAX) {
            return Err(row_overflow());
        }
        rows.push(aligned(
            row.div_ceil(8),
            if layout == Layout::Mixed {
                0
            } else {
                u64::from(c.row_align)
            },
        ));
    }
    let size = match layout {
        Layout::BlockComponent | Layout::BlockPixel => unreachable!(),
        Layout::Tile => {
            return Ok(rows
                .iter()
                .map(|row| aligned(row * u64::from(h), u64::from(c.tile_align)))
                .collect());
        }
        Layout::Byte | Layout::Component | Layout::Mixed => rows
            .iter()
            .zip(entries)
            .map(|(r, e)| r * u64::from(e.height))
            .sum(),
        Layout::Row => aligned(rows.iter().sum(), u64::from(c.row_align)) * u64::from(h),
        Layout::Pixel => {
            let mut bits = 0;
            for _ in 0..w {
                let mut pixel = 0;
                for e in entries {
                    if e.config.align != 0 {
                        bits = aligned(bits, 8);
                        pixel += aligned(
                            u64::from(e.config.bits).div_ceil(8),
                            u64::from(e.config.align),
                        ) * 8;
                    } else {
                        pixel += u64::from(e.config.bits);
                    }
                }
                if c.pixel_size != 0 {
                    pixel = aligned(pixel.div_ceil(8), u64::from(c.pixel_size)) * 8;
                }
                bits += pixel;
                if bits > u64::from(u32::MAX) {
                    return Err(row_overflow());
                }
            }
            aligned(bits.div_ceil(8), u64::from(c.row_align)) * u64::from(h)
        }
    };
    Ok(vec![aligned(size, u64::from(c.tile_align))])
}
struct Bits<'a> {
    data: &'a [u8],
    pos: u64,
}
impl Bits<'_> {
    fn read(&mut self, n: u16) -> u128 {
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1)
                | u128::from(
                    self.data
                        .get((self.pos / 8) as usize)
                        .map_or(0, |b| (b >> (7 - self.pos % 8)) & 1),
                );
            self.pos += 1;
        }
        value
    }
    fn align(&mut self, unit: u64, start: u64) {
        self.pos = aligned(self.pos, 8);
        self.pos = (start + aligned(self.pos / 8 - start, unit)) * 8;
    }
}
fn sample(
    bits: &mut Bits<'_>,
    e: &Entry,
    image: &mut Image,
    x: u32,
    y: u32,
    byte: bool,
    little: bool,
) -> Result<()> {
    let c = &e.config;
    let mut value;
    if byte {
        let n = u64::from(c.bits / 8);
        let size = aligned(n, u64::from(c.align));
        let begin = bits.pos / 8;
        if begin > bits.data.len() as u64 || size > bits.data.len() as u64 - begin {
            return Err(unspecified(
                "Bytealign-component interleave: insufficient data",
            ));
        }
        value = bits.read(c.bits.min(64));
        if little {
            value = match c.bits.min(64) {
                16 => (value as u16).swap_bytes() as u128,
                32 => (value as u32).swap_bytes() as u128,
                64 => (value as u64).swap_bytes() as u128,
                _ => value,
            };
        }
        if c.bits == 128 {
            let mut second = bits.read(64) as u64;
            if little {
                second = second.swap_bytes();
            }
            let plane = image.component_plane_mut(e.id).unwrap();
            let offset = y as usize * plane.stride + x as usize * 16;
            plane.data_mut()[offset..offset + 8].copy_from_slice(&(value as u64).to_ne_bytes());
            plane.data_mut()[offset + 8..offset + 16].copy_from_slice(&second.to_ne_bytes());
            bits.pos = (begin + size) * 8;
            return Ok(());
        }
        bits.pos = (begin + size) * 8;
    } else {
        if c.align != 0 {
            bits.pos = aligned(bits.pos, 8) + u64::from(c.align) * 8 - u64::from(c.bits);
        }
        value = bits.read(c.bits);
    }
    let plane = image.component_plane_mut(e.id).unwrap();
    // The legacy reader uses ceil(bit depth / 8) for destination addressing.
    let n = usize::from(c.bits.div_ceil(8));
    let offset = y as usize * plane.stride + x as usize * n;
    let bytes = value.to_ne_bytes();
    let source = if cfg!(target_endian = "little") {
        &bytes[..n]
    } else {
        &bytes[16 - n..]
    };
    plane.data_mut()[offset..offset + n].copy_from_slice(source);
    Ok(())
}
fn row(
    bits: &mut Bits<'_>,
    e: &Entry,
    image: &mut Image,
    (tx, ty, y): (u32, u32, u32),
    (byte, little): (bool, bool),
    align: u32,
) -> Result<()> {
    let start = bits.pos / 8;
    for x in 0..e.width {
        sample(
            bits,
            e,
            image,
            tx * e.width + x,
            ty * e.height + y,
            byte,
            little,
        )?;
    }
    bits.align(u64::from(align), start);
    Ok(())
}
fn tile(
    data: &[u8],
    c: &Configuration,
    entries: &[Entry],
    layout: Layout,
    image: &mut Image,
    (tx, ty): (u32, u32),
    (w, h): (u32, u32),
) -> Result<()> {
    let mut bits = Bits { data, pos: 0 };
    let little = c.flags & 0x80 != 0;
    match layout {
        Layout::BlockComponent | Layout::BlockPixel => {
            let block = if layout == Layout::BlockComponent {
                u64::from(c.block_size)
            } else {
                u64::from(c.pixel_size)
            };
            let groups = if layout == Layout::BlockComponent {
                entries.len()
            } else {
                1
            };
            for group in 0..groups {
                for y in 0..h {
                    let start = bits.pos / 8;
                    for x in 0..w {
                        let at = bits.pos / 8;
                        if at > data.len() as u64 || block > data.len() as u64 - at {
                            return Err(unspecified(if layout == Layout::BlockComponent {
                                "Block-component interleave: insufficient data"
                            } else {
                                "Block-pixel interleave: insufficient data"
                            }));
                        }
                        let mut value = bits.read((block * 8) as u16) as u64;
                        if c.flags & 0x20 != 0 {
                            value = value.swap_bytes() >> (64 - block * 8);
                        }
                        let selected = if layout == Layout::BlockComponent {
                            &entries[group..group + 1]
                        } else {
                            entries
                        };
                        let mut offset = if c.flags & 0x40 != 0 { block * 8 } else { 0 };
                        for i in 0..selected.len() {
                            let e = &selected[if c.flags & 0x10 != 0 {
                                i
                            } else {
                                selected.len() - 1 - i
                            }];
                            let n = u64::from(e.config.bits);
                            let shift = if c.flags & 0x40 != 0 {
                                offset -= n;
                                offset
                            } else {
                                let shift = offset;
                                offset += n;
                                shift
                            };
                            let v = (value >> shift) & ((1u64 << n) - 1);
                            let p = image.component_plane_mut(e.id).unwrap();
                            let bytes = usize::from(e.config.bits.div_ceil(8));
                            let dest =
                                (ty * h + y) as usize * p.stride + (tx * w + x) as usize * bytes;
                            let b = (v as u16).to_ne_bytes();
                            if bytes == 1 {
                                p.data_mut()[dest] = v as u8;
                            } else {
                                p.data_mut()[dest..dest + 2].copy_from_slice(&b);
                            }
                        }
                    }
                    bits.align(u64::from(c.row_align), start);
                }
            }
        }
        Layout::Byte | Layout::Component | Layout::Tile => {
            for e in entries {
                let start = bits.pos / 8;
                for y in 0..e.height {
                    row(
                        &mut bits,
                        e,
                        image,
                        (tx, ty, y),
                        (layout == Layout::Byte, little),
                        c.row_align,
                    )?;
                }
                if layout == Layout::Tile {
                    bits.align(u64::from(c.tile_align), start);
                }
            }
        }
        Layout::Row => {
            for y in 0..h {
                for e in entries {
                    row(
                        &mut bits,
                        e,
                        image,
                        (tx, ty, y),
                        (false, false),
                        c.row_align,
                    )?;
                }
            }
        }
        Layout::Pixel => {
            for y in 0..h {
                let start = bits.pos / 8;
                for x in 0..w {
                    let pixel = bits.pos.div_ceil(8);
                    for e in entries {
                        sample(&mut bits, e, image, tx * w + x, ty * h + y, false, false)?;
                    }
                    if c.pixel_size != 0 {
                        let count = bits.pos.div_ceil(8) - pixel;
                        if count > u64::from(c.pixel_size) {
                            return Err(unspecified("Uncompressed image: invalid 'pixel_size'"));
                        }
                        bits.pos += (u64::from(c.pixel_size) - count) * 8;
                    }
                }
                bits.align(u64::from(c.row_align), start);
            }
        }
        Layout::Mixed => {
            let mut chroma = false;
            for e in entries {
                if matches!(e.channel, 1 | 2) {
                    if chroma {
                        continue;
                    }
                    let other = entries
                        .iter()
                        .find(|p| p.channel == 3 - e.channel)
                        .ok_or_else(|| unspecified("Missing chroma component"))?;
                    let mut first = e.clone();
                    first.config.bits = first.config.bits.div_ceil(8) * 8;
                    first.config.align = 0;
                    let mut second = other.clone();
                    second.config.bits = second.config.bits.next_power_of_two().max(8);
                    second.config.align = 0;
                    for y in 0..e.height {
                        for x in 0..e.width {
                            sample(
                                &mut bits,
                                &first,
                                image,
                                tx * e.width + x,
                                ty * e.height + y,
                                false,
                                false,
                            )?;
                            sample(
                                &mut bits,
                                &second,
                                image,
                                tx * e.width + x,
                                ty * e.height + y,
                                false,
                                false,
                            )?;
                        }
                    }
                    chroma = true;
                } else {
                    for y in 0..e.height {
                        row(&mut bits, e, image, (tx, ty, y), (false, false), 0)?;
                    }
                }
            }
        }
    }
    Ok(())
}
pub fn decode(
    container: &Container<'_>,
    id: u32,
    budget: Option<std::sync::Arc<crate::security::Budget>>,
) -> Result<Image> {
    let (c, defs) = Configuration::load(container, id)?;
    let (w, h) = container.dimensions(id)?;
    if w == 0 || h == 0 {
        container.limits.check_image_size(w, h)?;
    }
    c.header(defs.as_deref(), Some((w, h)))?;
    if c.pixel_size > 0
        && u64::from(c.pixel_size) * u64::from(w) * u64::from(h) > u64::from(u32::MAX)
    {
        return Err(unspecified(
            "Aligned total image size exceeds maximum integer range",
        ));
    }
    if c.row_align > u32::MAX / 8 {
        return Err(unspecified(
            "Aligned row size larger than supported maximum",
        ));
    }
    hard_limits(&c)?;
    let (cs, ch, _) = c.color(defs.as_deref())?;
    let defs = defs.unwrap();
    let mut image = Image::new(w, h, cs, if cs == 0 && ch == 99 { 3 } else { ch })?;
    image.chroma = ch;
    image.budget = budget;
    let (tw, th) = (w / c.columns, h / c.rows);
    let mut entries = Vec::new();
    for comp in &c.components {
        let kind = defs[comp.index as usize].kind;
        let channel = crate::components::channel_for_type(kind);
        let subx = matches!(channel, 1 | 2) && matches!(ch, 1 | 2);
        let suby = matches!(channel, 1 | 2) && ch == 1;
        let pw = if subx { w.div_ceil(2) } else { w };
        let ph = if suby { h.div_ceil(2) } else { h };
        let id = image.add_component(pw, ph, kind, i32::from(comp.format), i32::from(comp.bits))?;
        entries.push(Entry {
            config: comp.clone(),
            id,
            channel,
            width: if subx { tw / 2 } else { tw },
            height: if suby { th / 2 } else { th },
        });
    }
    let layout = choose(&c)?;
    let sizes = sizes(&c, &entries, layout, tw, th)?;
    let source = super::compression::Source::new(container, id, image.budget.clone())?;
    let count = u64::from(c.columns) * u64::from(c.rows);
    for ty in 0..c.rows {
        for tx in 0..c.columns {
            let index = u64::from(ty) * u64::from(c.columns) + u64::from(tx);
            let mut tile_data = Vec::new();
            let mut base = 0u64;
            for size in &sizes {
                let start = size
                    .checked_mul(index)
                    .and_then(|v| base.checked_add(v))
                    .ok_or_else(row_overflow)?;
                let bytes = source.range(start, *size, index)?;
                tile_data
                    .try_reserve(bytes.len())
                    .map_err(|_| crate::error::Error::ALLOCATION)?;
                tile_data.extend_from_slice(&bytes);
                base = size
                    .checked_mul(count)
                    .and_then(|v| base.checked_add(v))
                    .ok_or_else(row_overflow)?;
            }
            tile(
                &tile_data,
                &c,
                &entries,
                layout,
                &mut image,
                (tx, ty),
                (tw, th),
            )?;
        }
    }
    if c.components.is_empty() {
        return Err(ContextError::invalid(
            129,
            "Invalid image size: Decoded image does not have the size signaled in the file.",
        ));
    }
    Ok(image)
}
