// SPDX-License-Identifier: LGPL-3.0-or-later
// AV1 configuration semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Codec-independent configuration extraction for caller-supplied AV1 packets.
use crate::image::Image;

pub struct Configuration {
    profile: u8,
    level: u8,
    tier: u8,
    high: u8,
    twelve: u8,
    mono: u8,
    sx: u8,
    sy: u8,
    position: u8,
}
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn get(&mut self, n: usize) -> u32 {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1)
                | u32::from(
                    self.data.get(self.at / 8).copied().unwrap_or(0) >> (7 - self.at % 8) & 1,
                );
            self.at = self.at.saturating_add(1);
        }
        v
    }
    fn skip(&mut self, n: usize) {
        self.at = self.at.saturating_add(n);
    }
    fn flag(&mut self) -> bool {
        self.get(1) != 0
    }
}
impl Configuration {
    pub fn from_image(image: &Image) -> Self {
        let depth = image.plane(0).map_or(-1, |p| i32::from(p.bit_depth));
        let profile = if depth <= 10 && matches!(image.chroma, 0 | 1) {
            0
        } else if depth <= 10 && image.chroma == 3 {
            1
        } else {
            2
        };
        let (w, h) = image.plane(0).map_or((-1i64, -1i64), |p| {
            (i64::from(p.width), i64::from(p.height))
        });
        let level = if w <= 8192 && h <= 4352 && w * h <= 8912896 {
            13
        } else if w <= 16384 && h <= 8704 && w * h <= 35651584 {
            17
        } else {
            31
        };
        Self {
            profile,
            level,
            tier: 0,
            high: u8::from(depth > 8),
            twelve: u8::from(depth >= 12),
            mono: u8::from(image.chroma == 0),
            sx: u8::from(matches!(image.chroma, 1 | 2)),
            sy: u8::from(image.chroma == 1),
            position: if image.chroma == 1 { 0 } else { 2 },
        }
    }
    pub fn bytes(&self) -> Vec<u8> {
        vec![
            0x81,
            self.profile << 5 | self.level & 31,
            self.tier << 7
                | self.high << 6
                | self.twelve << 5
                | self.mono << 4
                | self.sx << 3
                | self.sy << 2
                | self.position & 3,
            0,
        ]
    }
    pub fn update(&mut self, data: &[u8]) {
        let mut bits = Bits { data, at: 0 };
        loop {
            if bits.at >= data.len().saturating_mul(8) {
                return;
            }
            bits.skip(1);
            let kind = bits.get(4);
            let extension = bits.flag();
            let has_size = bits.flag();
            bits.skip(1);
            if extension {
                bits.skip(8);
            }
            let mut size = 0u64;
            if has_size {
                for i in 0..8 {
                    let b = bits.get(8);
                    size |= u64::from(b & 127) << (i * 7);
                    if b & 128 == 0 {
                        break;
                    }
                }
            }
            if kind == 1 {
                break;
            }
            if !has_size || size > i32::MAX as u64 {
                return;
            }
            bits.skip(size as usize * 8);
        }
        self.profile = bits.get(3) as u8;
        bits.skip(1); // still_picture
        let reduced = bits.flag();
        if reduced {
            self.level = bits.get(5) as u8;
            self.tier = 0;
        } else {
            let mut model = false;
            let mut delay_bits = 0;
            if bits.flag() {
                bits.skip(64);
                if bits.flag() {
                    let mut zeros = 0;
                    while zeros < 32 && !bits.flag() {
                        zeros += 1;
                    }
                    if zeros < 32 {
                        bits.skip(zeros);
                    }
                }
                model = bits.flag();
                if model {
                    delay_bits = bits.get(5) as usize + 1;
                    bits.skip(42);
                }
            }
            let display_delay = bits.flag();
            let points = bits.get(5) + 1;
            for i in 0..points {
                bits.skip(12);
                let level = bits.get(5) as u8;
                if i == 0 {
                    self.level = level;
                }
                if level > 7 {
                    let tier = bits.get(1) as u8;
                    if i == 0 {
                        self.tier = tier;
                    }
                }
                if model && bits.flag() {
                    bits.skip(delay_bits * 2 + 1);
                }
                if display_delay && bits.flag() {
                    bits.skip(4);
                }
            }
        }
        let width_bits = bits.get(4) as usize + 1;
        let height_bits = bits.get(4) as usize + 1;
        bits.skip(width_bits + height_bits);
        if !reduced && bits.flag() {
            bits.skip(7);
        }
        bits.skip(3);
        if !reduced {
            bits.skip(4);
            let order = bits.flag();
            if order {
                bits.skip(2);
            }
            let screen = if bits.flag() { 2 } else { bits.get(1) };
            if screen > 0 && !bits.flag() {
                bits.skip(1);
            }
            if order {
                bits.skip(3);
            }
        }
        bits.skip(3);
        self.high = bits.get(1) as u8;
        self.twelve = if self.profile == 2 && self.high != 0 {
            bits.get(1) as u8
        } else {
            0
        };
        self.mono = if self.profile == 1 {
            0
        } else {
            bits.get(1) as u8
        };
        let color = if bits.flag() {
            (bits.get(8), bits.get(8), bits.get(8))
        } else {
            (2, 2, 2)
        };
        if self.mono != 0 {
            self.sx = 1;
            self.sy = 1;
            self.position = 0;
        } else if color == (1, 13, 0) {
            self.sx = 0;
            self.sy = 0;
        } else {
            bits.skip(1);
            (self.sx, self.sy) = match self.profile {
                0 => (1, 1),
                1 => (0, 0),
                _ if self.twelve != 0 => {
                    let x = bits.get(1) as u8;
                    (x, if x != 0 { bits.get(1) as u8 } else { 0 })
                }
                _ => (1, 0),
            };
            if self.sx != 0 && self.sy != 0 {
                self.position = bits.get(2) as u8;
            }
        }
    }
}
