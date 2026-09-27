// SPDX-License-Identifier: LGPL-3.0-or-later
//! Intra prediction (ITU-T H.264 8.3.1.2, 8.3.2.2, 8.3.3, 8.3.4).

/// Neighbouring samples of an n x n block: `top[0..2n]` are p[x, -1]
/// (x = n..2n-1 already substituted when the top-right is missing),
/// `left[0..n]` are p[-1, y], `corner` is p[-1, -1].
pub struct Edge {
    pub top: [i32; 32],
    pub left: [i32; 16],
    pub corner: i32,
    pub has_top: bool,
    pub has_left: bool,
    pub has_corner: bool,
}

impl Edge {
    #[inline]
    fn p(&self, x: i32, y: i32) -> i32 {
        if y < 0 {
            if x < 0 {
                self.corner
            } else {
                self.top[x as usize]
            }
        } else {
            self.left[y as usize]
        }
    }
}

/// Whether an Intra4x4/Intra8x8 prediction mode can be used.
pub fn nxn_mode_available(mode: u8, e: &Edge) -> bool {
    match mode {
        0 | 3 | 7 => e.has_top,
        1 | 8 => e.has_left,
        2 => true,
        _ => e.has_top && e.has_left && e.has_corner,
    }
}

/// Intra4x4 (n = 4) or Intra8x8 (n = 8, `e` already filtered) prediction,
/// raster order.
pub fn predict_nxn(mode: u8, n: usize, e: &Edge, bit_depth: u8, out: &mut [i32]) {
    let ni = n as i32;
    let last = 2 * ni - 1;
    for y in 0..ni {
        for x in 0..ni {
            let p = |x: i32, y: i32| e.p(x, y);
            let v = match mode {
                0 => p(x, -1),
                1 => p(-1, y),
                2 => {
                    let st: i32 = (0..ni).map(|i| p(i, -1)).sum();
                    let sl: i32 = (0..ni).map(|i| p(-1, i)).sum();
                    let shift = if n == 4 { 2 } else { 3 };
                    match (e.has_top, e.has_left) {
                        (true, true) => (st + sl + ni) >> (shift + 1),
                        (false, true) => (sl + (ni >> 1)) >> shift,
                        (true, false) => (st + (ni >> 1)) >> shift,
                        _ => 1 << (bit_depth - 1),
                    }
                }
                3 => {
                    if x == ni - 1 && y == ni - 1 {
                        (p(last - 1, -1) + 3 * p(last, -1) + 2) >> 2
                    } else {
                        (p(x + y, -1) + 2 * p(x + y + 1, -1) + p(x + y + 2, -1) + 2) >> 2
                    }
                }
                4 => {
                    if x > y {
                        (p(x - y - 2, -1) + 2 * p(x - y - 1, -1) + p(x - y, -1) + 2) >> 2
                    } else if x < y {
                        (p(-1, y - x - 2) + 2 * p(-1, y - x - 1) + p(-1, y - x) + 2) >> 2
                    } else {
                        (p(0, -1) + 2 * p(-1, -1) + p(-1, 0) + 2) >> 2
                    }
                }
                5 => {
                    let z = 2 * x - y;
                    if z >= 0 && z % 2 == 0 {
                        (p(x - (y >> 1) - 1, -1) + p(x - (y >> 1), -1) + 1) >> 1
                    } else if z >= 0 {
                        (p(x - (y >> 1) - 2, -1)
                            + 2 * p(x - (y >> 1) - 1, -1)
                            + p(x - (y >> 1), -1)
                            + 2)
                            >> 2
                    } else if z == -1 {
                        (p(-1, 0) + 2 * p(-1, -1) + p(0, -1) + 2) >> 2
                    } else {
                        (p(-1, y - 2 * x - 1) + 2 * p(-1, y - 2 * x - 2) + p(-1, y - 2 * x - 3) + 2)
                            >> 2
                    }
                }
                6 => {
                    let z = 2 * y - x;
                    if z >= 0 && z % 2 == 0 {
                        (p(-1, y - (x >> 1) - 1) + p(-1, y - (x >> 1)) + 1) >> 1
                    } else if z >= 0 {
                        (p(-1, y - (x >> 1) - 2)
                            + 2 * p(-1, y - (x >> 1) - 1)
                            + p(-1, y - (x >> 1))
                            + 2)
                            >> 2
                    } else if z == -1 {
                        (p(-1, 0) + 2 * p(-1, -1) + p(0, -1) + 2) >> 2
                    } else {
                        (p(x - 2 * y - 1, -1) + 2 * p(x - 2 * y - 2, -1) + p(x - 2 * y - 3, -1) + 2)
                            >> 2
                    }
                }
                7 => {
                    let b = x + (y >> 1);
                    if y % 2 == 0 {
                        (p(b, -1) + p(b + 1, -1) + 1) >> 1
                    } else {
                        (p(b, -1) + 2 * p(b + 1, -1) + p(b + 2, -1) + 2) >> 2
                    }
                }
                _ => {
                    let z = x + 2 * y;
                    let b = y + (x >> 1);
                    let limit = 2 * ni - 3;
                    if z < limit && z % 2 == 0 {
                        (p(-1, b) + p(-1, b + 1) + 1) >> 1
                    } else if z < limit {
                        (p(-1, b) + 2 * p(-1, b + 1) + p(-1, b + 2) + 2) >> 2
                    } else if z == limit {
                        (p(-1, ni - 2) + 3 * p(-1, ni - 1) + 2) >> 2
                    } else {
                        p(-1, ni - 1)
                    }
                }
            };
            out[(y * ni + x) as usize] = v;
        }
    }
}

/// 8.3.2.2.1 reference sample filtering for Intra8x8.
pub fn filter8(e: &Edge) -> Edge {
    let mut f = Edge {
        top: e.top,
        left: e.left,
        corner: e.corner,
        has_top: e.has_top,
        has_left: e.has_left,
        has_corner: e.has_corner,
    };
    if e.has_top {
        f.top[0] = if e.has_corner {
            (e.corner + 2 * e.top[0] + e.top[1] + 2) >> 2
        } else {
            (3 * e.top[0] + e.top[1] + 2) >> 2
        };
        for x in 1..15 {
            f.top[x] = (e.top[x - 1] + 2 * e.top[x] + e.top[x + 1] + 2) >> 2;
        }
        f.top[15] = (e.top[14] + 3 * e.top[15] + 2) >> 2;
    }
    if e.has_corner {
        f.corner = match (e.has_top, e.has_left) {
            (true, true) => (e.top[0] + 2 * e.corner + e.left[0] + 2) >> 2,
            (true, false) => (3 * e.corner + e.top[0] + 2) >> 2,
            (false, true) => (3 * e.corner + e.left[0] + 2) >> 2,
            _ => e.corner,
        };
    }
    if e.has_left {
        f.left[0] = if e.has_corner {
            (e.corner + 2 * e.left[0] + e.left[1] + 2) >> 2
        } else {
            (3 * e.left[0] + e.left[1] + 2) >> 2
        };
        for y in 1..7 {
            f.left[y] = (e.left[y - 1] + 2 * e.left[y] + e.left[y + 1] + 2) >> 2;
        }
        f.left[7] = (e.left[6] + 3 * e.left[7] + 2) >> 2;
    }
    f
}

/// Intra16x16 modes 0-3 (vertical, horizontal, DC, plane).
pub fn i16_mode_available(mode: u8, e: &Edge) -> bool {
    match mode {
        0 => e.has_top,
        1 => e.has_left,
        2 => true,
        _ => e.has_top && e.has_left && e.has_corner,
    }
}

pub fn predict16(mode: u8, e: &Edge, bit_depth: u8, out: &mut [i32; 256]) {
    let max = (1 << bit_depth) - 1;
    match mode {
        0 => {
            for y in 0..16 {
                out[y * 16..y * 16 + 16].copy_from_slice(&e.top[..16]);
            }
        }
        1 => {
            for y in 0..16 {
                out[y * 16..y * 16 + 16].fill(e.left[y]);
            }
        }
        2 => {
            let st: i32 = e.top[..16].iter().sum();
            let sl: i32 = e.left[..16].iter().sum();
            let v = match (e.has_top, e.has_left) {
                (true, true) => (st + sl + 16) >> 5,
                (false, true) => (sl + 8) >> 4,
                (true, false) => (st + 8) >> 4,
                _ => 1 << (bit_depth - 1),
            };
            out.fill(v);
        }
        _ => {
            let h: i32 = (0..8)
                .map(|i| (i + 1) * (e.p(8 + i, -1) - e.p(6 - i, -1)))
                .sum();
            let v: i32 = (0..8)
                .map(|i| (i + 1) * (e.p(-1, 8 + i) - e.p(-1, 6 - i)))
                .sum();
            let a = 16 * (e.left[15] + e.top[15]);
            let b = (5 * h + 32) >> 6;
            let c = (5 * v + 32) >> 6;
            for y in 0..16i32 {
                for x in 0..16i32 {
                    out[(y * 16 + x) as usize] =
                        ((a + b * (x - 7) + c * (y - 7) + 16) >> 5).clamp(0, max);
                }
            }
        }
    }
}

/// Chroma modes: 0 DC, 1 horizontal, 2 vertical, 3 plane.
pub fn chroma_mode_available(mode: u8, e: &Edge) -> bool {
    match mode {
        0 => true,
        1 => e.has_left,
        2 => e.has_top,
        _ => e.has_top && e.has_left && e.has_corner,
    }
}

/// Chroma prediction for a `w` x `h` block (8x8 for 4:2:0, 8x16 for 4:2:2);
/// raster order.
pub fn predict_chroma(mode: u8, w: usize, h: usize, e: &Edge, bit_depth: u8, out: &mut [i32]) {
    let max = (1 << bit_depth) - 1;
    match mode {
        0 => {
            for yo in (0..h).step_by(4) {
                for xo in (0..w).step_by(4) {
                    let st: i32 = e.top[xo..xo + 4].iter().sum();
                    let sl: i32 = e.left[yo..yo + 4].iter().sum();
                    let (t, l) = (e.has_top, e.has_left);
                    let both = (st + sl + 4) >> 3;
                    let top = (st + 2) >> 2;
                    let left = (sl + 2) >> 2;
                    let none = 1 << (bit_depth - 1);
                    let v = if (xo == 0 && yo == 0) || (xo > 0 && yo > 0) {
                        match (t, l) {
                            (true, true) => both,
                            (false, true) => left,
                            (true, false) => top,
                            _ => none,
                        }
                    } else if xo > 0 {
                        if t {
                            top
                        } else if l {
                            left
                        } else {
                            none
                        }
                    } else if l {
                        left
                    } else if t {
                        top
                    } else {
                        none
                    };
                    for y in yo..yo + 4 {
                        out[y * w + xo..y * w + xo + 4].fill(v);
                    }
                }
            }
        }
        1 => {
            for y in 0..h {
                out[y * w..y * w + w].fill(e.left[y]);
            }
        }
        2 => {
            for y in 0..h {
                out[y * w..y * w + w].copy_from_slice(&e.top[..w]);
            }
        }
        _ => {
            let (xcf, ycf) = (if w == 16 { 4 } else { 0 }, if h == 16 { 4 } else { 0 });
            let hs: i32 = (0..4 + xcf)
                .map(|i| (i + 1) * (e.p(4 + xcf + i, -1) - e.p(2 + xcf - i, -1)))
                .sum();
            let vs: i32 = (0..4 + ycf)
                .map(|i| (i + 1) * (e.p(-1, 4 + ycf + i) - e.p(-1, 2 + ycf - i)))
                .sum();
            let a = 16 * (e.left[h - 1] + e.top[w - 1]);
            let b = ((34 - 29 * i32::from(w == 16)) * hs + 32) >> 6;
            let c = ((34 - 29 * i32::from(h == 16)) * vs + 32) >> 6;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    out[(y * w as i32 + x) as usize] =
                        ((a + b * (x - 3 - xcf) + c * (y - 3 - ycf) + 16) >> 5).clamp(0, max);
                }
            }
        }
    }
}
