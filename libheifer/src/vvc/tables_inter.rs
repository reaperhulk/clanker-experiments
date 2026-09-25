// SPDX-License-Identifier: LGPL-3.0-or-later
//! Inter prediction tables from vvdec's `InterpolationFilter.cpp` and
//! `Rom.cpp`.

pub const LUMA_FILTER: [[i32; 8]; 16] = [
    [0, 0, 0, 64, 0, 0, 0, 0],
    [0, 1, -3, 63, 4, -2, 1, 0],
    [-1, 2, -5, 62, 8, -3, 1, 0],
    [-1, 3, -8, 60, 13, -4, 1, 0],
    [-1, 4, -10, 58, 17, -5, 1, 0],
    [-1, 4, -11, 52, 26, -8, 3, -1],
    [-1, 3, -9, 47, 31, -10, 4, -1],
    [-1, 4, -11, 45, 34, -10, 4, -1],
    [-1, 4, -11, 40, 40, -11, 4, -1],
    [-1, 4, -10, 34, 45, -11, 4, -1],
    [-1, 4, -10, 31, 47, -9, 3, -1],
    [-1, 3, -8, 26, 52, -11, 4, -1],
    [0, 1, -5, 17, 58, -10, 4, -1],
    [0, 1, -4, 13, 60, -8, 3, -1],
    [0, 1, -3, 8, 62, -5, 2, -1],
    [0, 1, -2, 4, 63, -3, 1, 0],
];

/// 6-tap filter for 4x4 affine luma blocks (`m_lumaFilter4x4`).
pub const LUMA_FILTER_4X4: [[i32; 8]; 16] = [
    [0, 0, 0, 64, 0, 0, 0, 0],
    [0, 1, -3, 63, 4, -2, 1, 0],
    [0, 1, -5, 62, 8, -3, 1, 0],
    [0, 2, -8, 60, 13, -4, 1, 0],
    [0, 3, -10, 58, 17, -5, 1, 0],
    [0, 3, -11, 52, 26, -8, 2, 0],
    [0, 2, -9, 47, 31, -10, 3, 0],
    [0, 3, -11, 45, 34, -10, 3, 0],
    [0, 3, -11, 40, 40, -11, 3, 0],
    [0, 3, -10, 34, 45, -11, 3, 0],
    [0, 3, -10, 31, 47, -9, 2, 0],
    [0, 2, -8, 26, 52, -11, 3, 0],
    [0, 1, -5, 17, 58, -10, 3, 0],
    [0, 1, -4, 13, 60, -8, 2, 0],
    [0, 1, -3, 8, 62, -5, 1, 0],
    [0, 1, -2, 4, 63, -3, 1, 0],
];

pub const LUMA_ALT_HPEL: [i32; 8] = [0, 3, 9, 20, 20, 9, 3, 0];

pub const CHROMA_FILTER: [[i32; 4]; 32] = [
    [0, 64, 0, 0],
    [-1, 63, 2, 0],
    [-2, 62, 4, 0],
    [-2, 60, 7, -1],
    [-2, 58, 10, -2],
    [-3, 57, 12, -2],
    [-4, 56, 14, -2],
    [-4, 55, 15, -2],
    [-4, 54, 16, -2],
    [-5, 53, 18, -2],
    [-6, 52, 20, -2],
    [-6, 49, 24, -3],
    [-6, 46, 28, -4],
    [-5, 44, 29, -4],
    [-4, 42, 30, -4],
    [-4, 39, 33, -4],
    [-4, 36, 36, -4],
    [-4, 33, 39, -4],
    [-4, 30, 42, -4],
    [-4, 29, 44, -5],
    [-4, 28, 46, -6],
    [-3, 24, 49, -6],
    [-2, 20, 52, -6],
    [-2, 18, 53, -5],
    [-2, 16, 54, -4],
    [-2, 15, 55, -4],
    [-2, 14, 56, -4],
    [-2, 12, 57, -3],
    [-2, 10, 58, -2],
    [-1, 7, 60, -2],
    [0, 4, 62, -2],
    [0, 2, 63, -1],
];

/// Bilinear DMVR filter (`m_bilinearFilterPrec4`).
pub const BILINEAR_PREC4: [[i32; 2]; 16] = [
    [16, 0],
    [15, 1],
    [14, 2],
    [13, 3],
    [12, 4],
    [11, 5],
    [10, 6],
    [9, 7],
    [8, 8],
    [7, 9],
    [6, 10],
    [5, 11],
    [4, 12],
    [3, 13],
    [2, 14],
    [1, 15],
];

/// `g_BcwWeights`, indexed through `g_BcwInternBcw`.
pub const BCW_WEIGHTS: [i32; 5] = [-2, 3, 4, 5, 10];
pub const BCW_INTERN_BCW: [usize; 5] = [2, 0, 1, 3, 4];

pub const GEO_DIS: [i8; 32] = [
    8, 8, 8, 8, 4, 4, 2, 1, 0, -1, -2, -4, -4, -8, -8, -8, -8, -8, -8, -8, -4, -4, -2, -1, 0, 1, 2,
    4, 4, 8, 8, 8,
];
const GEO_ANGLE2MASK: [i8; 32] = [
    0, -1, 1, 2, 3, 4, -1, -1, 5, -1, -1, 4, 3, 2, 1, -1, 0, -1, 1, 2, 3, 4, -1, -1, 5, -1, -1, 4,
    3, 2, 1, -1,
];
pub const GEO_ANGLE2MIRROR: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 2,
];
pub const GEO_WEIGHT_MASK_SIZE: i32 = 3 * (64 >> 3) * 2 + 64;

pub struct GeoTables {
    pub params: [(i32, i32); 64],
    pub weights: Vec<Vec<i16>>,
    /// [split][hIdx][wIdx] -> (offsetX, offsetY)
    pub offsets: Vec<[[(i32, i32); 4]; 4]>,
}

/// vvdec's `initGeoTemplate`.
pub fn geo() -> &'static GeoTables {
    static T: std::sync::OnceLock<GeoTables> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut params = [(0i32, 0i32); 64];
        let mut mode = 0;
        for angle in 0..32 {
            for dist in 0..4 {
                let m = GEO_ANGLE2MASK[angle as usize];
                if (dist == 0 && angle >= 16)
                    || ((dist == 2 || dist == 0) && (m == 0 || m == 5))
                    || m == -1
                {
                    continue;
                }
                params[mode] = (angle, dist);
                mode += 1;
            }
        }
        let size = GEO_WEIGHT_MASK_SIZE;
        let mut weights = vec![vec![0i16; (size * size) as usize]; 6];
        for angle in 0..(32 >> 2) + 1 {
            let m = GEO_ANGLE2MASK[angle as usize];
            if m == -1 {
                continue;
            }
            let dx = angle;
            let dy = (dx + 8) % 32;
            let rho = i32::from(GEO_DIS[dx as usize]) * (1 << 7)
                + i32::from(GEO_DIS[dy as usize]) * (1 << 7);
            let mask_offset = (2 * 64 - size) >> 1;
            let mut index = 0usize;
            for y in 0..size {
                let look_y = (((y + mask_offset) << 1) + 1) * i32::from(GEO_DIS[dy as usize]);
                for x in 0..size {
                    let sx = ((x + mask_offset) << 1) + 1;
                    // int16_t arithmetic as in vvdec
                    let idx = (sx * i32::from(GEO_DIS[dx as usize]) + look_y - rho) as i16 as i32;
                    weights[m as usize][index] = ((32 + idx + 4) >> 3).clamp(0, 8) as i16;
                    index += 1;
                }
            }
        }
        let mut offsets = vec![[[(0i32, 0i32); 4]; 4]; 64];
        for h_idx in 0..4 {
            let h = 1i32 << (h_idx + 3);
            for w_idx in 0..4 {
                let w = 1i32 << (w_idx + 3);
                for (split, off) in offsets.iter_mut().enumerate() {
                    let (angle, dist) = params[split];
                    let mut ox = (size - w) >> 1;
                    let mut oy = (size - h) >> 1;
                    if dist > 0 {
                        if angle % 16 == 8 || (angle % 16 != 0 && h >= w) {
                            oy += if angle < 16 {
                                (dist * h) >> 3
                            } else {
                                -((dist * h) >> 3)
                            };
                        } else {
                            ox += if angle < 16 {
                                (dist * w) >> 3
                            } else {
                                -((dist * w) >> 3)
                            };
                        }
                    }
                    off[h_idx][w_idx] = (ox, oy);
                }
            }
        }
        GeoTables {
            params,
            weights,
            offsets,
        }
    })
}

pub fn geo_params(split: usize) -> (i32, i32) {
    geo().params[split]
}

pub fn geo_mask(angle: i32) -> usize {
    GEO_ANGLE2MASK[angle as usize] as usize
}
