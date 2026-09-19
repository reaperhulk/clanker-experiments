// SPDX-License-Identifier: LGPL-3.0-or-later
// Conversion semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Color conversion with the reference's ordered minimum-cost operation graph.
use crate::{
    color::{ColorConversionOptions, Nclx},
    error::Error,
    image::{Image, Plane},
};

const UNDEFINED: Nclx = Nclx {
    primaries: 2,
    transfer: 2,
    matrix: 2,
    full_range: true,
};
const UNSUPPORTED: Error = Error::new(
    4,
    3003,
    c"Unsupported feature: Unsupported color conversion",
);
#[derive(Clone, Copy, Debug)]
struct State {
    cs: i32,
    ch: i32,
    depth: u8,
    alpha: u8,
    nclx: Nclx,
}
impl PartialEq for State {
    fn eq(&self, b: &Self) -> bool {
        (self.cs, self.ch, self.depth, self.alpha) == (b.cs, b.ch, b.depth, b.alpha)
            && (self.cs != 0
                || (self.nclx.primaries, self.nclx.matrix, self.nclx.full_range)
                    == (b.nclx.primaries, b.nclx.matrix, b.nclx.full_range))
    }
}
#[derive(Clone, Copy, Debug)]
enum Op {
    Pack,
    Pack16,
    Unpack16,
    Swap16,
    YuvPacked16,
    Unpack,
    YuvRgb,
    YuvPacked,
    MonoYuv,
    MonoPacked,
    RgbYuv,
    DropAlpha,
    Depth,
    Upsample,
    Downsample,
}
#[derive(Clone)]
struct Node {
    state: State,
    cost: u32,
    prev: usize,
    op: Option<Op>,
}
fn normalized(mut p: Nclx) -> Nclx {
    if p.primaries == 2 {
        p.primaries = 1;
    }
    if p.transfer == 2 {
        p.transfer = 13;
    }
    if p.matrix == 2 {
        p.matrix = 6;
    }
    p
}
fn next(s: State, t: State, o: ColorConversionOptions) -> Vec<(State, Op, u32)> {
    let mut result = Vec::new();
    let mut add = |state, op, cost| result.push((state, op, cost));
    let small = s.depth <= 16 && s.alpha <= 16;
    let alpha_matches = s.alpha == 0 || s.alpha == s.depth;
    // Ordering is part of compatibility: equal-cost paths preserve first discovery.
    if s.cs == 1 && s.ch == 3 && s.depth == 8 && alpha_matches {
        add(
            State {
                ch: 11,
                alpha: 8,
                nclx: UNDEFINED,
                ..s
            },
            Op::Pack,
            11,
        );
        add(
            State {
                ch: 10,
                alpha: 0,
                nclx: UNDEFINED,
                ..s
            },
            Op::Pack,
            11,
        );
    }
    if s.cs == 1 && matches!(s.ch, 10 | 11) && s.depth == 8 {
        add(
            State {
                ch: 3,
                alpha: if t.alpha > 0 { 8 } else { 0 },
                nclx: UNDEFINED,
                ..s
            },
            Op::Unpack,
            11,
        );
    }
    let nearest = s.ch == 3
        || o.preferred_chroma_upsampling_algorithm == 1
        || o.only_use_preferred_chroma_algorithm == 0;
    if s.cs == 0
        && (1..=3).contains(&s.ch)
        && small
        && s.depth >= 8
        && nearest
        && !matches!(s.nclx.matrix, 11 | 14 | 17)
        && (s.nclx.matrix != 16 || s.depth <= 14)
    {
        add(
            State {
                cs: 1,
                ch: 3,
                nclx: UNDEFINED,
                ..s
            },
            Op::YuvRgb,
            11,
        );
    }
    if s.cs == 0
        && s.ch == 1
        && s.depth == 8
        && nearest
        && s.nclx.full_range
        && !matches!(s.nclx.matrix, 0 | 8 | 11 | 14)
        && alpha_matches
    {
        if s.alpha == 0 {
            add(
                State {
                    cs: 1,
                    ch: 10,
                    nclx: UNDEFINED,
                    ..s
                },
                Op::YuvPacked,
                11,
            );
        }
        add(
            State {
                cs: 1,
                ch: 11,
                alpha: 8,
                nclx: UNDEFINED,
                ..s
            },
            Op::YuvPacked,
            11,
        );
    }
    if s.cs == 0
        && s.ch == 1
        && s.depth > 8
        && small
        && nearest
        && alpha_matches
        && !matches!(s.nclx.matrix, 0 | 8 | 11 | 14)
    {
        for ch in [
            if s.alpha > 0 { 15 } else { 14 },
            if s.alpha > 0 { 13 } else { 12 },
        ] {
            add(
                State {
                    cs: 1,
                    ch,
                    nclx: UNDEFINED,
                    ..s
                },
                Op::YuvPacked16,
                11,
            );
        }
    }
    if s.cs == 1 && s.ch == 3 && s.depth > 8 && small && alpha_matches {
        if s.alpha == 0 {
            add(
                State {
                    ch: 12,
                    nclx: UNDEFINED,
                    ..s
                },
                Op::Pack16,
                11,
            );
        }
        add(
            State {
                ch: 13,
                alpha: s.depth,
                nclx: UNDEFINED,
                ..s
            },
            Op::Pack16,
            11,
        );
    }
    if s.cs == 2 && s.ch == 0 && small {
        add(
            State {
                cs: 0,
                ch: 1,
                nclx: UNDEFINED,
                ..s
            },
            Op::MonoYuv,
            6,
        );
        if s.depth == 8 && alpha_matches {
            if s.alpha == 0 {
                add(
                    State {
                        cs: 1,
                        ch: 10,
                        nclx: UNDEFINED,
                        ..s
                    },
                    Op::MonoPacked,
                    11,
                );
            }
            add(
                State {
                    cs: 1,
                    ch: 11,
                    alpha: 8,
                    nclx: UNDEFINED,
                    ..s
                },
                Op::MonoPacked,
                11,
            );
        }
    }
    if s.cs == 1 && (12..=15).contains(&s.ch) && small {
        add(
            State {
                ch: if s.ch < 14 { s.ch + 2 } else { s.ch - 2 },
                nclx: UNDEFINED,
                ..s
            },
            Op::Swap16,
            11,
        );
        if s.ch < 14 && s.depth > 8 {
            add(
                State {
                    ch: 3,
                    alpha: if t.alpha > 0 { s.depth } else { 0 },
                    nclx: UNDEFINED,
                    ..s
                },
                Op::Unpack16,
                11,
            );
        }
    }
    if s.cs == 1
        && s.ch == 3
        && small
        && s.depth >= 8
        && alpha_matches
        && !matches!(t.nclx.matrix, 11 | 14)
    {
        let ch = if t.ch != 3
            && (o.preferred_chroma_downsampling_algorithm == 1
                || o.only_use_preferred_chroma_algorithm == 0)
        {
            t.ch
        } else {
            3
        };
        // Preserve even dead-end states: swap-removal makes them affect equal-cost path selection.
        {
            add(
                State {
                    cs: 0,
                    ch,
                    nclx: t.nclx,
                    ..s
                },
                Op::RgbYuv,
                11,
            );
        }
    }
    if s.ch <= 3 && s.alpha != 0 && t.alpha == 0 {
        add(State { alpha: 0, ..s }, Op::DropAlpha, 1);
    }
    if s.ch <= 3 && s.depth == 8 && t.depth > 8 && t.depth <= 16 {
        add(
            State {
                depth: t.depth,
                alpha: if s.alpha > 0 { t.depth } else { 0 },
                ..s
            },
            Op::Depth,
            11,
        );
    }
    if s.ch <= 3 && s.depth != 8 && small && t.depth == 8 {
        add(
            State {
                depth: 8,
                alpha: if s.alpha > 0 { 8 } else { 0 },
                ..s
            },
            Op::Depth,
            11,
        );
    }
    if s.cs == 0
        && matches!(s.ch, 1 | 2)
        && small
        && s.nclx.matrix != 0
        && o.preferred_chroma_upsampling_algorithm == 2
    {
        add(State { ch: 3, ..s }, Op::Upsample, 11);
    }
    if s.cs == 0
        && s.ch == 3
        && small
        && s.nclx.matrix != 0
        && o.preferred_chroma_downsampling_algorithm == 2
        && matches!(t.ch, 1 | 2)
    {
        add(State { ch: t.ch, ..s }, Op::Downsample, 11);
    }
    result
}

pub fn convert(
    mut image: Image,
    cs: i32,
    ch: i32,
    profile: Nclx,
    output_depth: u8,
    options: ColorConversionOptions,
) -> Result<Image, Error> {
    let first = image
        .channels()
        .min()
        .and_then(|c| image.plane(c))
        .ok_or(UNSUPPORTED)?;
    let depth = first.bit_depth;
    let alpha = image.plane(6).map_or(
        if matches!(image.chroma, 11 | 13 | 15) {
            depth
        } else {
            0
        },
        |p| p.bit_depth,
    );
    let input = State {
        cs: image.colorspace,
        ch: image.chroma,
        depth,
        alpha,
        nclx: normalized(image.color.nclx.unwrap_or(UNDEFINED)),
    };
    let mut target_profile = profile;
    if target_profile.primaries == 2 {
        target_profile.primaries = input.nclx.primaries;
    }
    if target_profile.transfer == 2 {
        target_profile.transfer = input.nclx.transfer;
    }
    if target_profile.matrix == 2 {
        target_profile.matrix = input.nclx.matrix;
    }
    let depth = if matches!(ch, 10 | 11) {
        8
    } else if (12..=15).contains(&ch) && depth <= 8 {
        10
    } else if output_depth != 0 {
        output_depth
    } else {
        depth
    };
    let target = State {
        cs,
        ch,
        depth,
        alpha: if ch >= 10 {
            if matches!(ch, 11 | 13 | 15) { depth } else { 0 }
        } else if alpha > 0 {
            depth
        } else {
            0
        },
        nclx: target_profile,
    };
    if input == target {
        return Ok(image);
    }
    let mut border = vec![Node {
        state: input,
        cost: 0,
        prev: 0,
        op: None,
    }];
    let mut processed: Vec<Node> = Vec::new();
    let end = loop {
        let index = border
            .iter()
            .enumerate()
            .min_by_key(|(_, n)| n.cost)
            .map(|(i, _)| i)
            .ok_or(UNSUPPORTED)?;
        let current = border.swap_remove(index);
        let id = processed.len();
        processed.push(current.clone());
        if current.state == target {
            break id;
        }
        for (state, op, cost) in next(current.state, target, options) {
            if processed.iter().any(|n| n.state == state) {
                continue;
            }
            let node = Node {
                state,
                cost: current.cost + cost,
                prev: id,
                op: Some(op),
            };
            if let Some(old) = border.iter_mut().find(|n| n.state == state) {
                if old.cost > node.cost {
                    *old = node;
                }
            } else {
                border.push(node);
            }
        }
    };
    let mut path = Vec::new();
    let mut at = end;
    while at != 0 {
        path.push(at);
        at = processed[at].prev;
    }
    for at in path.into_iter().rev() {
        let node = &processed[at];
        let mut out = apply(
            &image,
            processed[node.prev].state,
            node.state,
            node.op.unwrap(),
        )?;
        out.warnings = image.warnings.clone();
        out.color = image.color.try_clone()?;
        out.sensor = image.sensor.clone();
        out.sample = image.sample.clone();
        out.projection = image.projection;
        out.tai_timestamp = image.tai_timestamp;
        out.color.nclx = Some(node.state.nclx);
        out.pixel_aspect_ratio = image.pixel_aspect_ratio;
        out.premultiplied_alpha = image.premultiplied_alpha;
        image = out;
    }
    Ok(image)
}
fn sample(p: &Plane, x: u32, y: u32) -> i32 {
    let at = y as usize * p.stride + x as usize * if p.bit_depth <= 8 { 1 } else { 2 };
    if p.bit_depth <= 8 {
        i32::from(p.data()[at])
    } else {
        i32::from(u16::from_ne_bytes([p.data()[at], p.data()[at + 1]]))
    }
}
fn put(p: &mut Plane, x: u32, y: u32, v: i32) {
    let at = y as usize * p.stride + x as usize * if p.bit_depth <= 8 { 1 } else { 2 };
    if p.bit_depth <= 8 {
        p.data_mut()[at] = v as u8;
    } else {
        p.data_mut()[at..at + 2].copy_from_slice(&(v as u16).to_ne_bytes());
    }
}
fn copy_plane(src: &Image, out: &mut Image, ch: i32) -> Result<(), Error> {
    if let Some(p) = src.plane(ch) {
        out.add_plane(ch, p.width, p.height, p.bit_depth.into())?;
        let dest = out.plane_mut(ch).unwrap();
        for y in 0..p.height as usize {
            let to = y * dest.stride;
            let from = y * p.stride;
            let n = p.width as usize * p.bytes_per_pixel;
            dest.data_mut()[to..to + n].copy_from_slice(&p.data()[from..from + n]);
        }
    }
    Ok(())
}
fn kr_kb(p: Nclx) -> Option<(f32, f32)> {
    Some(match p.matrix {
        1 => (0.2126, 0.0722),
        4 => (0.30, 0.11),
        5 | 6 => (0.299, 0.114),
        7 => (0.212, 0.087),
        9 | 10 => (0.2627, 0.0593),
        12 | 13 => {
            let p = p.decode().ok()?;
            let (rx, ry, gx, gy, bx, by, wx, wy) = (
                p.color_primary_red_x,
                p.color_primary_red_y,
                p.color_primary_green_x,
                p.color_primary_green_y,
                p.color_primary_blue_x,
                p.color_primary_blue_y,
                p.color_primary_white_x,
                p.color_primary_white_y,
            );
            let (zr, zg, zb, zw) = (
                1.0 - (rx + ry),
                1.0 - (gx + gy),
                1.0 - (bx + by),
                1.0 - (wx + wy),
            );
            let denom = wy
                * (rx * (gy * zb - by * zg) + gx * (by * zr - ry * zb) + bx * (ry * zg - gy * zr));
            if denom == 0.0 {
                return None;
            }
            (
                (ry * (wx * (gy * zb - by * zg)
                    + wy * (bx * zg - gx * zb)
                    + zw * (gx * by - bx * gy)))
                    / denom,
                (by * (wx * (ry * zg - gy * zr)
                    + wy * (gx * zr - rx * zg)
                    + zw * (rx * gy - gx * ry)))
                    / denom,
            )
        }
        _ => return None,
    })
}
fn coefficients(p: Nclx) -> [f32; 4] {
    if let Some((r, b)) = kr_kb(p) {
        [
            2.0 * (-r + 1.0),
            2.0 * b * (-b + 1.0) / (b + r - 1.0),
            2.0 * r * (-r + 1.0) / (b + r - 1.0),
            2.0 * (-b + 1.0),
        ]
    } else {
        [1.402, -0.344136, -0.714136, 1.772]
    }
}
fn clip(f: f32, max: i32) -> i32 {
    ((f + 0.5) as i32).clamp(0, max)
}
fn apply(image: &Image, s: State, t: State, op: Op) -> Result<Image, Error> {
    let (w, h) = (image.width, image.height);
    let mut out = Image::new(w, h, t.cs, t.ch)?.with_budget(image.budget.clone());
    match op {
        Op::Pack16 | Op::YuvPacked16 => {
            out.add_plane(10, w, h, t.depth.into())?;
            let n = if matches!(t.ch, 13 | 15) { 4 } else { 3 };
            // The direct upstream HDR decoder conversion also creates an empty alpha plane.
            if matches!(op, Op::YuvPacked16) && s.alpha > 0 {
                out.add_plane(6, w, h, s.depth.into())?;
            }
            let q = out.plane_mut(10).unwrap();
            let max = (1 << s.depth) - 1;
            let p = image.color.nclx.unwrap_or(UNDEFINED);
            let cf = coefficients(p);
            for y in 0..h {
                for x in 0..w {
                    let rgb = if matches!(op, Op::Pack16) {
                        [3, 4, 5].map(|c| sample(image.plane(c).unwrap(), x, y))
                    } else {
                        let mut yy = sample(image.plane(0).unwrap(), x, y) as f32;
                        let mut cb = (sample(image.plane(1).unwrap(), x / 2, y / 2)
                            - (1 << (s.depth - 1))) as f32;
                        let mut cr = (sample(image.plane(2).unwrap(), x / 2, y / 2)
                            - (1 << (s.depth - 1))) as f32;
                        if !p.full_range {
                            yy = (yy - (16 << (s.depth - 8)) as f32) * 1.1689;
                            cb *= 1.1429;
                            cr *= 1.1429;
                        }
                        [
                            clip(yy + cf[0] * cr, max),
                            clip(yy + cf[1] * cb + cf[2] * cr, max),
                            clip(yy + cf[3] * cb, max),
                        ]
                    };
                    let alpha = image.plane(6).map_or(max, |a| sample(a, x, y));
                    for (c, v) in [rgb[0], rgb[1], rgb[2], alpha]
                        .into_iter()
                        .enumerate()
                        .take(n)
                    {
                        let bytes = if t.ch >= 14 {
                            (v as u16).to_le_bytes()
                        } else {
                            (v as u16).to_be_bytes()
                        };
                        let at = y as usize * q.stride + (x as usize * n + c) * 2;
                        q.data_mut()[at..at + 2].copy_from_slice(&bytes);
                    }
                }
            }
        }
        Op::Swap16 => {
            let p = image.plane(10).unwrap();
            out.add_plane(10, w, h, t.depth.into())?;
            let q = out.plane_mut(10).unwrap();
            for y in 0..h as usize {
                for x in (0..w as usize * p.bytes_per_pixel).step_by(2) {
                    let from = y * p.stride + x;
                    let to = y * q.stride + x;
                    q.data_mut()[to] = p.data()[from + 1];
                    q.data_mut()[to + 1] = p.data()[from];
                }
            }
        }
        Op::Unpack16 => {
            let p = image.plane(10).unwrap();
            let n = if s.ch == 13 { 4 } else { 3 };
            for c in 0..if t.alpha > 0 { 4 } else { 3 } {
                let channel = if c == 3 { 6 } else { 3 + c as i32 };
                out.add_plane(channel, w, h, t.depth.into())?;
                let q = out.plane_mut(channel).unwrap();
                for y in 0..h {
                    for x in 0..w {
                        let v = if c >= n {
                            (1 << t.depth) - 1
                        } else {
                            let at = y as usize * p.stride + (x as usize * n + c) * 2;
                            i32::from(u16::from_be_bytes([p.data()[at], p.data()[at + 1]]))
                        };
                        put(q, x, y, v);
                    }
                }
            }
        }
        Op::DropAlpha => {
            for ch in image.channels().filter(|c| *c != 6) {
                copy_plane(image, &mut out, ch)?;
            }
        }
        Op::Depth => {
            for ch in image.channels() {
                let p = image.plane(ch).unwrap();
                out.add_plane(ch, p.width, p.height, t.depth.into())?;
                let q = out.plane_mut(ch).unwrap();
                for y in 0..p.height {
                    for x in 0..p.width {
                        let v = sample(p, x, y);
                        let v = if t.depth < p.bit_depth {
                            v >> (p.bit_depth - t.depth)
                        } else if t.depth == 8 && p.bit_depth < 8 {
                            let mut bit = 1u32 << (16 - p.bit_depth);
                            let mut factor = 0;
                            while bit != 0 {
                                factor |= bit;
                                bit >>= p.bit_depth;
                            }
                            ((v as u32 * factor) >> 8) as i32
                        } else {
                            (v << (t.depth - p.bit_depth)) | (v >> (2 * p.bit_depth - t.depth))
                        };
                        put(q, x, y, v);
                    }
                }
            }
        }
        Op::Pack | Op::MonoPacked | Op::YuvPacked => {
            out.add_plane(10, w, h, 8)?;
            let q = out.plane_mut(10).unwrap();
            let n = if t.ch == 11 { 4 } else { 3 };
            let coeff = coefficients(image.color.nclx.unwrap_or(UNDEFINED))
                .map(|v| (v * 256.0).round() as i32);
            for y in 0..h {
                for x in 0..w {
                    let rgb = match op {
                        Op::Pack => [3, 4, 5].map(|c| sample(image.plane(c).unwrap(), x, y)),
                        Op::MonoPacked => [sample(image.plane(0).unwrap(), x, y); 3],
                        _ => {
                            let yy = sample(image.plane(0).unwrap(), x, y);
                            let cb = sample(image.plane(1).unwrap(), x / 2, y / 2) - 128;
                            let cr = sample(image.plane(2).unwrap(), x / 2, y / 2) - 128;
                            [
                                yy + ((coeff[0] * cr + 128) >> 8),
                                yy + ((coeff[1] * cb + coeff[2] * cr + 128) >> 8),
                                yy + ((coeff[3] * cb + 128) >> 8),
                            ]
                        }
                    };
                    let at = y as usize * q.stride + x as usize * n;
                    for (c, v) in rgb.into_iter().enumerate() {
                        q.data_mut()[at + c] = v.clamp(0, 255) as u8;
                    }
                    if n == 4 {
                        q.data_mut()[at + 3] =
                            image.plane(6).map_or(255, |p| sample(p, x, y) as u8);
                    }
                }
            }
        }
        Op::Unpack => {
            let p = image.plane(10).unwrap();
            let n = if s.ch == 11 { 4 } else { 3 };
            for c in 0..if t.alpha > 0 { 4 } else { 3 } {
                let ch = if c == 3 { 6 } else { 3 + c as i32 };
                out.add_plane(ch, w, h, 8)?;
                let q = out.plane_mut(ch).unwrap();
                for y in 0..h {
                    for x in 0..w {
                        put(
                            q,
                            x,
                            y,
                            if c == 3 && n == 3 {
                                255
                            } else {
                                p.data()[y as usize * p.stride + x as usize * n + c] as i32
                            },
                        );
                    }
                }
            }
        }
        Op::YuvRgb => {
            for c in 3..=5 {
                out.add_plane(c, w, h, t.depth.into())?;
            }
            let max = (1 << s.depth) - 1;
            let half = 1 << (s.depth - 1);
            let offset = 16 << (s.depth - 8);
            let p = image.color.nclx.unwrap_or(UNDEFINED);
            let cf = coefficients(p);
            let (sx, sy) = subsample(s.ch);
            for y in 0..h {
                for x in 0..w {
                    let yy = sample(image.plane(0).ok_or(UNSUPPORTED)?, x, y);
                    let cb = sample(image.plane(1).ok_or(UNSUPPORTED)?, x / sx, y / sy) - half;
                    let cr = sample(image.plane(2).ok_or(UNSUPPORTED)?, x / sx, y / sy) - half;
                    let rgb = match p.matrix {
                        0 => {
                            if p.full_range {
                                [cr + half, yy, cb + half]
                            } else {
                                [
                                    clip((cr + half - offset) as f32 * 1.1429, max),
                                    clip((yy - offset) as f32 * 1.1689, max),
                                    clip((cb + half - offset) as f32 * 1.1429, max),
                                ]
                            }
                        }
                        8 => [yy - cb + cr, yy + cb, yy - cb - cr],
                        16 => {
                            let tmp = yy - (cb >> 1);
                            let g = tmp + cb;
                            let b = tmp - (cr >> 1);
                            let r = b + cr;
                            [r * 4, g * 4, b * 4]
                        }
                        _ => {
                            let (yy, cb, cr) = if p.full_range {
                                (yy as f32, cb as f32, cr as f32)
                            } else {
                                (
                                    (yy - offset) as f32 * 1.1689,
                                    cb as f32 * 1.1429,
                                    cr as f32 * 1.1429,
                                )
                            };
                            [
                                clip(yy + cf[0] * cr, max),
                                clip(yy + cf[1] * cb + cf[2] * cr, max),
                                clip(yy + cf[3] * cb, max),
                            ]
                        }
                    };
                    for (i, v) in rgb.into_iter().enumerate() {
                        put(out.plane_mut(3 + i as i32).unwrap(), x, y, v.clamp(0, max));
                    }
                }
            }
            copy_plane(image, &mut out, 6)?;
        }
        Op::MonoYuv => {
            copy_plane(image, &mut out, 0)?;
            copy_plane(image, &mut out, 6)?;
            for c in [1, 2] {
                out.add_plane(c, w.div_ceil(2), h.div_ceil(2), s.depth.into())?;
                let q = out.plane_mut(c).unwrap();
                for y in 0..q.height {
                    for x in 0..q.width {
                        put(q, x, y, 1 << (s.depth - 1));
                    }
                }
            }
        }
        Op::Upsample | Op::Downsample => {
            copy_plane(image, &mut out, 0)?;
            copy_plane(image, &mut out, 6)?;
            let (sx, sy) = subsample(t.ch);
            for c in [1, 2] {
                let p = image.plane(c).ok_or(UNSUPPORTED)?;
                out.add_plane(c, w.div_ceil(sx), h.div_ceil(sy), s.depth.into())?;
                let q = out.plane_mut(c).unwrap();
                match op {
                    Op::Upsample => upsample(p, q, w, h, s.ch),
                    _ => {
                        for y in 0..q.height {
                            for x in 0..q.width {
                                // The pinned 4:2:2 operator leaves its odd final corner zero.
                                if t.ch == 2 && w % 2 == 1 && x == q.width - 1 && y == q.height - 1
                                {
                                    continue;
                                }
                                let mut sum = 0;
                                let mut count = 0;
                                for dy in 0..sy {
                                    for dx in 0..sx {
                                        let (ix, iy) = (x * sx + dx, y * sy + dy);
                                        if ix < w && iy < h {
                                            sum += sample(p, ix, iy);
                                            count += 1;
                                        }
                                    }
                                }
                                put(q, x, y, (sum + count / 2) / count);
                            }
                        }
                    }
                }
            }
        }
        Op::RgbYuv => rgb_to_yuv(image, &mut out, t)?,
    }
    Ok(out)
}
fn subsample(ch: i32) -> (u32, u32) {
    match ch {
        1 => (2, 2),
        2 => (2, 1),
        _ => (1, 1),
    }
}
fn upsample(p: &Plane, q: &mut Plane, w: u32, h: u32, ch: i32) {
    if ch == 2 {
        for y in 0..h {
            put(q, 0, y, sample(p, 0, y));
            if w.is_multiple_of(2) {
                put(q, w - 1, y, sample(p, w / 2 - 1, y));
            }
            for x in (1..w.saturating_sub(1)).step_by(2) {
                let a = sample(p, x / 2, y);
                let b = sample(p, x / 2 + 1, y);
                put(q, x, y, (3 * a + b + 2) / 4);
                put(q, x + 1, y, (a + 3 * b + 2) / 4);
            }
        }
        return;
    }
    put(q, 0, 0, sample(p, 0, 0));
    for cx in 0..(w - 1) / 2 {
        let a = sample(p, cx / 2, 0);
        let b = sample(p, cx / 2 + 1, 0);
        put(q, 2 * cx + 1, 0, (3 * a + b + 2) / 4);
        put(q, 2 * cx + 2, 0, (a + 3 * b + 2) / 4);
    }
    if w.is_multiple_of(2) {
        put(q, w - 1, 0, sample(p, w / 2 - 1, 0));
    }
    for cy in 0..(h - 1) / 2 {
        let a = sample(p, 0, cy / 2);
        let b = sample(p, 0, cy / 2 + 1);
        put(q, 0, 2 * cy + 1, (3 * a + b + 2) / 4);
        put(q, 0, 2 * cy + 2, (a + 3 * b + 2) / 4);
    }
    if h.is_multiple_of(2) {
        put(q, 0, h - 1, sample(p, 0, h / 2 - 1));
    }
    if w.is_multiple_of(2) {
        for cy in 0..(h - 1) / 2 {
            let a = sample(p, w / 2 - 1, cy / 2);
            let b = sample(p, w / 2 - 1, cy / 2 + 1);
            put(q, w - 1, 2 * cy + 1, (3 * a + b + 2) / 4);
            put(q, w - 1, 2 * cy + 2, (a + 3 * b + 2) / 4);
        }
    }
    if h.is_multiple_of(2) {
        for cx in 0..(w - 1) / 2 {
            let a = sample(p, cx / 2, h / 2 - 1);
            let b = sample(p, cx / 2 + 1, h / 2 - 1);
            put(q, 2 * cx + 1, h - 1, (3 * a + b + 2) / 4);
            put(q, 2 * cx + 2, h - 1, (a + 3 * b + 2) / 4);
        }
    }
    if w.is_multiple_of(2) && h.is_multiple_of(2) {
        put(q, w - 1, h - 1, sample(p, w / 2 - 1, h / 2 - 1));
    }
    for y in (1..h.saturating_sub(1)).step_by(2) {
        for x in (1..w.saturating_sub(1)).step_by(2) {
            let (cx, cy) = (x / 2, y / 2);
            let a = sample(p, cx, cy);
            let b = sample(p, cx + 1, cy);
            let c = sample(p, cx, cy + 1);
            let d = sample(p, cx + 1, cy + 1);
            put(q, x, y, (9 * a + 3 * b + 3 * c + d + 8) / 16);
            put(q, x + 1, y, (3 * a + 9 * b + c + 3 * d + 8) / 16);
            put(q, x, y + 1, (3 * a + b + 9 * c + 3 * d + 8) / 16);
            put(q, x + 1, y + 1, (a + 3 * b + 3 * c + 9 * d + 8) / 16);
        }
    }
}
fn rgb_to_yuv(image: &Image, out: &mut Image, t: State) -> Result<(), Error> {
    let (w, h) = (image.width, image.height);
    let (sx, sy) = subsample(t.ch);
    for c in 0..=2 {
        out.add_plane(
            c,
            if c == 0 { w } else { w.div_ceil(sx) },
            if c == 0 { h } else { h.div_ceil(sy) },
            t.depth.into(),
        )?;
    }
    let p = t.nclx;
    let coeff = if let Some((r, b)) = kr_kb(p) {
        [
            [r, 1.0 - r - b, b],
            [-r / (1.0 - b) / 2.0, -(1.0 - r - b) / (1.0 - b) / 2.0, 0.5],
            [0.5, -(1.0 - r - b) / (1.0 - r) / 2.0, -b / (1.0 - r) / 2.0],
        ]
    } else {
        [
            [0.299, 0.587, 0.114],
            [-0.168735, -0.331264, 0.5],
            [0.5, -0.418688, -0.081312],
        ]
    };
    let (max, half, offset) = ((1 << t.depth) - 1, 1 << (t.depth - 1), 16 << (t.depth - 8));
    let rgb = |x, y| [3, 4, 5].map(|c| sample(image.plane(c).unwrap(), x, y));
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = rgb(x, y);
            let v = match p.matrix {
                0 => {
                    if p.full_range {
                        g
                    } else {
                        clip(g as f32 * 219.0 / 256.0 + offset as f32, max)
                    }
                }
                8 => g / 2 + (r + b) / 4,
                _ => {
                    let v =
                        r as f32 * coeff[0][0] + g as f32 * coeff[0][1] + b as f32 * coeff[0][2];
                    clip(
                        if p.full_range {
                            v
                        } else {
                            v * 219.0 / 256.0 + offset as f32
                        },
                        max,
                    )
                }
            };
            put(out.plane_mut(0).unwrap(), x, y, v);
        }
    }
    for y in (0..h).step_by(sy as usize) {
        for x in (0..w).step_by(sx as usize) {
            let [r, g, b] = rgb(x, y);
            let (cb, cr) = match p.matrix {
                0 => {
                    if p.full_range {
                        (b, r)
                    } else {
                        (
                            clip(b as f32 * 224.0 / 256.0 + offset as f32, max),
                            clip(r as f32 * 224.0 / 256.0 + offset as f32, max),
                        )
                    }
                }
                8 => (
                    (g / 2 - (r + b) / 4 + half).clamp(0, max),
                    ((r - b) / 2 + half).clamp(0, max),
                ),
                _ => {
                    let mut vals = [r as f32, g as f32, b as f32];
                    if sx > 1 || sy > 1 {
                        let x2 = if sx == 2 && sy == 2 {
                            (x + 1).min(w - 1)
                        } else {
                            x
                        };
                        let y2 = if sy == 2 { (y + 1).min(h - 1) } else { y };
                        for pix in [rgb(x2, y), rgb(x, y2), rgb(x2, y2)] {
                            for i in 0..3 {
                                vals[i] += pix[i] as f32;
                            }
                        }
                        for v in &mut vals {
                            *v *= 0.25;
                        }
                    }
                    let val = |c: usize| {
                        let v =
                            vals[0] * coeff[c][0] + vals[1] * coeff[c][1] + vals[2] * coeff[c][2];
                        clip(
                            (if p.full_range { v } else { v * 224.0 / 256.0 }) + half as f32,
                            max,
                        )
                    };
                    (val(1), val(2))
                }
            };
            put(out.plane_mut(1).unwrap(), x / sx, y / sy, cb);
            put(out.plane_mut(2).unwrap(), x / sx, y / sy, cr);
        }
    }
    copy_plane(image, out, 6)
}
