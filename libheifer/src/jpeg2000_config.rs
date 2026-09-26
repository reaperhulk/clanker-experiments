// SPDX-License-Identifier: LGPL-3.0-or-later
//! JPEG 2000 SIZ geometry and native handle precision descriptions.
#[derive(Clone)]
pub struct Component {
    pub depth: u8,
    pub signed: bool,
    pub dx: u8,
    pub dy: u8,
}
pub struct Header {
    pub width: u32,
    pub height: u32,
    pub x0: u32,
    pub y0: u32,
    pub components: Vec<Component>,
}
pub fn header(data: &[u8]) -> Option<Header> {
    if data.get(..4)? != [255, 79, 255, 81] {
        return None;
    }
    let u16_at = |at: usize| -> Option<u16> {
        Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
    };
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
    };
    let size = u16_at(4)?;
    if !(41..=49190).contains(&size) {
        return None;
    }
    let x0 = u32_at(16)?;
    let y0 = u32_at(20)?;
    let width = u32_at(8)?.wrapping_sub(x0);
    let height = u32_at(12)?.wrapping_sub(y0);
    let count = usize::from(u16_at(40)?);
    if !(1..=16384).contains(&count) {
        return None;
    }
    let mut components = Vec::new();
    for c in data.get(42..42 + 3 * count)?.chunks_exact(3) {
        components.push(Component {
            depth: (c[0] & 127) + 1,
            signed: c[0] & 128 != 0,
            dx: c[1],
            dy: c[2],
        });
    }
    Some(Header {
        width,
        height,
        x0,
        y0,
        components,
    })
}
pub fn description(data: &[u8]) -> Option<(i32, i32)> {
    let h = header(data)?;
    let at = 42 + 3 * h.components.len();
    if at + 2 >= data.len() {
        return None;
    }
    if data.get(at..at + 2) == Some(&[255, 80]) {
        let size = usize::from(u16::from_be_bytes(
            data.get(at + 2..at + 4)?.try_into().ok()?,
        ));
        if !(8..=70).contains(&size) || at + 2 + size > data.len() {
            return None;
        }
        let mask = u32::from_be_bytes(data.get(at + 4..at + 8)?.try_into().ok()?);
        if (mask & 0x7fffffff).count_ones() as usize * 2 > size - 6 {
            return None;
        }
    }
    Some((
        i32::from(h.components[0].depth),
        h.components.get(1).map_or(-1, |c| i32::from(c.depth)),
    ))
}
