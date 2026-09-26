// SPDX-License-Identifier: LGPL-3.0-or-later
//! Transforms, scaling and quantization (ITU-T H.264 8.5.6-8.5.13, flat
//! scaling lists). Reconstruction follows the decoder exactly; the encoder's
//! quantizer derives its step sizes from the same arithmetic in floating
//! point, so encoder and decoder cannot disagree about scale.

/// 8.5.6 normAdjust4x4 rows v[m] (positions: both even, both odd, other).
const V4: [[i32; 3]; 6] = [
    [10, 16, 13],
    [11, 18, 14],
    [13, 20, 16],
    [14, 23, 18],
    [16, 25, 20],
    [18, 29, 23],
];

/// 8.5.9 normAdjust8x8 rows v[m].
const V8: [[i32; 6]; 6] = [
    [20, 18, 32, 19, 25, 24],
    [22, 19, 35, 21, 28, 26],
    [26, 23, 42, 24, 33, 31],
    [28, 25, 45, 26, 35, 33],
    [32, 28, 51, 30, 40, 38],
    [36, 32, 58, 34, 46, 43],
];

/// LevelScale4x4(m, i, j) with flat weights (16).
pub fn level_scale4(m: usize, i: usize, j: usize) -> i32 {
    let class = if i.is_multiple_of(2) && j.is_multiple_of(2) {
        0
    } else if i % 2 == 1 && j % 2 == 1 {
        1
    } else {
        2
    };
    16 * V4[m][class]
}

/// LevelScale8x8(m, i, j) with flat weights (16).
pub fn level_scale8(m: usize, i: usize, j: usize) -> i32 {
    let class = if i.is_multiple_of(4) && j.is_multiple_of(4) {
        0
    } else if i % 2 == 1 && j % 2 == 1 {
        1
    } else if i % 4 == 2 && j % 4 == 2 {
        2
    } else if (i.is_multiple_of(4) && j % 2 == 1) || (i % 2 == 1 && j.is_multiple_of(4)) {
        3
    } else if (i.is_multiple_of(4) && j % 4 == 2) || (i % 4 == 2 && j.is_multiple_of(4)) {
        4
    } else {
        5
    };
    16 * V8[m][class]
}

/// Frame zig-zag scans: scan position -> raster index (row * n + column).
pub const ZIGZAG4: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
pub const ZIGZAG8: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// 8.5.12.1 scaling of a 4x4 block (raster `c`); `dc` replaces c[0] for
/// Intra16x16 and chroma blocks.
pub fn scale4(c: &[i32; 16], qp: i32, dc: Option<i32>) -> [i32; 16] {
    let (m, per) = ((qp % 6) as usize, qp / 6);
    let mut d = [0; 16];
    for (k, v) in d.iter_mut().enumerate() {
        let (i, j) = (k / 4, k % 4);
        let x = c[k] * level_scale4(m, i, j);
        *v = if per >= 4 {
            x << (per - 4)
        } else {
            (x + (1 << (3 - per))) >> (4 - per)
        };
    }
    if let Some(dc) = dc {
        d[0] = dc;
    }
    d
}

/// 8.5.13.1 scaling of an 8x8 block.
pub fn scale8(c: &[i32; 64], qp: i32) -> [i32; 64] {
    let (m, per) = ((qp % 6) as usize, qp / 6);
    let mut d = [0; 64];
    for (k, v) in d.iter_mut().enumerate() {
        let x = c[k] * level_scale8(m, k / 8, k % 8);
        *v = if per >= 6 {
            x << (per - 6)
        } else {
            (x + (1 << (5 - per))) >> (6 - per)
        };
    }
    d
}

/// 8.5.12.2 inverse 4x4 transform with the final (x + 32) >> 6.
pub fn inverse4(d: &[i32; 16]) -> [i32; 16] {
    let mut f = [0i32; 16];
    for i in 0..4 {
        let r = &d[i * 4..i * 4 + 4];
        let e = r[0] + r[2];
        let fo = r[0] - r[2];
        let g = (r[1] >> 1) - r[3];
        let h = r[1] + (r[3] >> 1);
        f[i * 4] = e + h;
        f[i * 4 + 1] = fo + g;
        f[i * 4 + 2] = fo - g;
        f[i * 4 + 3] = e - h;
    }
    let mut r = [0i32; 16];
    for j in 0..4 {
        let c = |i: usize| f[i * 4 + j];
        let e = c(0) + c(2);
        let fo = c(0) - c(2);
        let g = (c(1) >> 1) - c(3);
        let h = c(1) + (c(3) >> 1);
        r[j] = (e + h + 32) >> 6;
        r[4 + j] = (fo + g + 32) >> 6;
        r[8 + j] = (fo - g + 32) >> 6;
        r[12 + j] = (e - h + 32) >> 6;
    }
    r
}

fn inverse8_1d(s: [i32; 8]) -> [i32; 8] {
    let a0 = s[0] + s[4];
    let a4 = s[0] - s[4];
    let a2 = (s[2] >> 1) - s[6];
    let a6 = s[2] + (s[6] >> 1);
    let b0 = a0 + a6;
    let b2 = a4 + a2;
    let b4 = a4 - a2;
    let b6 = a0 - a6;
    let a1 = -s[3] + s[5] - s[7] - (s[7] >> 1);
    let a3 = s[1] + s[7] - s[3] - (s[3] >> 1);
    let a5 = -s[1] + s[7] + s[5] + (s[5] >> 1);
    let a7 = s[3] + s[5] + s[1] + (s[1] >> 1);
    let b1 = a1 + (a7 >> 2);
    let b7 = a7 - (a1 >> 2);
    let b3 = a3 + (a5 >> 2);
    let b5 = (a3 >> 2) - a5;
    [
        b0 + b7,
        b2 + b5,
        b4 + b3,
        b6 + b1,
        b6 - b1,
        b4 - b3,
        b2 - b5,
        b0 - b7,
    ]
}

/// 8.5.13.2 inverse 8x8 transform with the final (x + 32) >> 6.
pub fn inverse8(d: &[i32; 64]) -> [i32; 64] {
    let mut g = [0i32; 64];
    for i in 0..8 {
        let row = inverse8_1d(d[i * 8..i * 8 + 8].try_into().unwrap());
        g[i * 8..i * 8 + 8].copy_from_slice(&row);
    }
    let mut r = [0i32; 64];
    for j in 0..8 {
        let col = inverse8_1d(std::array::from_fn(|i| g[i * 8 + j]));
        for i in 0..8 {
            r[i * 8 + j] = (col[i] + 32) >> 6;
        }
    }
    r
}

/// 8.5.10: Intra16x16 luma DC (raster 4x4 of levels) to dcY.
pub fn luma_dc(c: &[i32; 16], qp: i32) -> [i32; 16] {
    let f = hadamard4(c);
    let (m, per) = ((qp % 6) as usize, qp / 6);
    let scale = level_scale4(m, 0, 0);
    f.map(|x| {
        if per >= 6 {
            (x * scale) << (per - 6)
        } else {
            (x * scale + (1 << (5 - per))) >> (6 - per)
        }
    })
}

/// 4x4 Hadamard A · c · A (A symmetric, rows [1,1,1,1], [1,1,-1,-1],
/// [1,-1,-1,1], [1,-1,1,-1]).
pub fn hadamard4(c: &[i32; 16]) -> [i32; 16] {
    const A: [[i32; 4]; 4] = [[1, 1, 1, 1], [1, 1, -1, -1], [1, -1, -1, 1], [1, -1, 1, -1]];
    let mut t = [0i32; 16];
    for i in 0..4 {
        for j in 0..4 {
            t[i * 4 + j] = (0..4).map(|k| A[i][k] * c[k * 4 + j]).sum();
        }
    }
    let mut f = [0i32; 16];
    for i in 0..4 {
        for j in 0..4 {
            f[i * 4 + j] = (0..4).map(|k| t[i * 4 + k] * A[k][j]).sum();
        }
    }
    f
}

/// 8.5.11.1 chroma DC for 4:2:0 (2x2, raster) and 4:2:2 (4 rows x 2
/// columns, raster): the DC values of the chroma 4x4 blocks in raster order.
pub fn chroma_dc(c: &[i32], qp: i32, rows: usize) -> Vec<i32> {
    if rows == 2 {
        let f = [
            c[0] + c[1] + c[2] + c[3],
            c[0] - c[1] + c[2] - c[3],
            c[0] + c[1] - c[2] - c[3],
            c[0] - c[1] - c[2] + c[3],
        ];
        let scale = level_scale4((qp % 6) as usize, 0, 0);
        f.iter().map(|&x| ((x * scale) << (qp / 6)) >> 5).collect()
    } else {
        const A: [[i32; 4]; 4] = [[1, 1, 1, 1], [1, 1, -1, -1], [1, -1, -1, 1], [1, -1, 1, -1]];
        let mut f = [0i32; 8];
        for i in 0..4 {
            for j in 0..2 {
                // A4 · c · A2
                let mut s = 0;
                for k in 0..4 {
                    let t = c[k * 2] + if j == 0 { c[k * 2 + 1] } else { -c[k * 2 + 1] };
                    s += A[i][k] * t;
                }
                f[i * 2 + j] = s;
            }
        }
        let qp_dc = qp + 3;
        let scale = level_scale4((qp_dc % 6) as usize, 0, 0);
        let per = qp_dc / 6;
        f.iter()
            .map(|&x| {
                if per >= 6 {
                    (x * scale) << (per - 6)
                } else {
                    (x * scale + (1 << (5 - per))) >> (6 - per)
                }
            })
            .collect()
    }
}

/// Forward 4x4 core transform Cf · X · Cfᵀ.
pub fn forward4(x: &[i32; 16]) -> [i32; 16] {
    let mut t = [0i32; 16];
    for i in 0..4 {
        let r = &x[i * 4..i * 4 + 4];
        let (s03, d03, s12, d12) = (r[0] + r[3], r[0] - r[3], r[1] + r[2], r[1] - r[2]);
        t[i * 4] = s03 + s12;
        t[i * 4 + 1] = 2 * d03 + d12;
        t[i * 4 + 2] = s03 - s12;
        t[i * 4 + 3] = d03 - 2 * d12;
    }
    let mut w = [0i32; 16];
    for j in 0..4 {
        let c = |i: usize| t[i * 4 + j];
        let (s03, d03, s12, d12) = (c(0) + c(3), c(0) - c(3), c(1) + c(2), c(1) - c(2));
        w[j] = s03 + s12;
        w[4 + j] = 2 * d03 + d12;
        w[8 + j] = s03 - s12;
        w[12 + j] = d03 - 2 * d12;
    }
    w
}

/// The 8x8 forward integer transform matrix (x8).
const T8: [[i32; 8]; 8] = [
    [8, 8, 8, 8, 8, 8, 8, 8],
    [12, 10, 6, 3, -3, -6, -10, -12],
    [8, 4, -4, -8, -8, -4, 4, 8],
    [10, -3, -12, -6, 6, 12, 3, -10],
    [8, -8, -8, 8, 8, -8, -8, 8],
    [6, -12, 3, 10, -10, -3, 12, -6],
    [4, -8, 8, -4, -4, 8, -8, 4],
    [3, -6, 10, -12, 12, -10, 6, -3],
];

/// Forward 8x8 transform T · X · Tᵀ (scaled by 64 against the inverse).
pub fn forward8(x: &[i32; 64]) -> [i32; 64] {
    let mut t = [0i32; 64];
    for i in 0..8 {
        for k in 0..8 {
            t[i * 8 + k] = (0..8).map(|j| x[i * 8 + j] * T8[k][j]).sum();
        }
    }
    let mut w = [0i32; 64];
    for k in 0..8 {
        for l in 0..8 {
            w[k * 8 + l] = (0..8).map(|i| T8[k][i] * t[i * 8 + l]).sum();
        }
    }
    w
}

/// Forward DC transforms applied to the forward-transform DC coefficients:
/// 4x4 Hadamard (luma, 4:4:4 chroma), 2x2 (4:2:0), 4x2 (4:2:2).
pub fn forward_dc(w: &[i32], rows: usize, cols: usize) -> Vec<i32> {
    let h = |n: usize| -> Vec<Vec<i32>> {
        match n {
            2 => vec![vec![1, 1], vec![1, -1]],
            _ => vec![
                vec![1, 1, 1, 1],
                vec![1, 1, -1, -1],
                vec![1, -1, -1, 1],
                vec![1, -1, 1, -1],
            ],
        }
    };
    let (hr, hc) = (h(rows), h(cols));
    let mut t = vec![0; rows * cols];
    for i in 0..rows {
        for j in 0..cols {
            t[i * cols + j] = (0..rows).map(|k| hr[i][k] * w[k * cols + j]).sum();
        }
    }
    let mut f = vec![0; rows * cols];
    for i in 0..rows {
        for j in 0..cols {
            f[i * cols + j] = (0..cols).map(|k| t[i * cols + k] * hc[k][j]).sum();
        }
    }
    f
}

/// Quantizer step tables: the forward coefficient one unit level produces
/// through the decoder's scaling and inverse transform, per position.
pub struct Steps {
    pub ac4: [f64; 16],
    pub ac8: [f64; 64],
    /// Intra16x16 DC (after the forward Hadamard, no normalization).
    pub dc_luma: f64,
    /// Chroma DC (4:2:0 or 4:2:2 forward DC transform).
    pub dc_chroma: f64,
}

impl Steps {
    /// Steps for scaling QP `qp` (QP'Y or QP'C); `chroma_rows` 2 or 4.
    pub fn new(qp: i32, chroma_rows: usize) -> Steps {
        let (m, per) = ((qp % 6) as usize, qp / 6);
        let scale4 = |i, j| f64::from(level_scale4(m, i, j)) * 2f64.powi(per) / 16.0;
        let scale8 = |i, j| f64::from(level_scale8(m, i, j)) * 2f64.powi(per) / 64.0;
        let mut ac4 = [0.0; 16];
        for (k, s) in ac4.iter_mut().enumerate() {
            let mut d = [0.0; 16];
            d[k] = scale4(k / 4, k % 4);
            let r = inverse4_f(&d);
            *s = forward4_f(&r)[k];
        }
        let mut ac8 = [0.0; 64];
        for (k, s) in ac8.iter_mut().enumerate() {
            let mut d = [0.0; 64];
            d[k] = scale8(k / 8, k % 8);
            let r = inverse8_f(&d);
            *s = forward8_f(&r)[k];
        }
        // DC: a unit level at DC position 0 of the Hadamard; the forward
        // DC coefficient it produces at position 0.
        let dc = |levels: usize, dequant: f64| {
            // Every transform-domain DC gets +-dequant; a block DC d00 of
            // value v gives a flat residual v/64, whose forward W00 is
            // 16 v/64; the forward Hadamard of equal-signed magnitudes
            // returns levels * that.
            levels as f64 * 16.0 * dequant / 64.0
        };
        let dc_luma = dc(16, f64::from(level_scale4(m, 0, 0)) * 2f64.powi(per) / 64.0);
        let dc_chroma = if chroma_rows == 2 {
            dc(4, f64::from(level_scale4(m, 0, 0)) * 2f64.powi(per) / 32.0)
        } else {
            let q = qp + 3;
            dc(
                8,
                f64::from(level_scale4((q % 6) as usize, 0, 0)) * 2f64.powi(q / 6) / 64.0,
            )
        };
        Steps {
            ac4,
            ac8,
            dc_luma,
            dc_chroma,
        }
    }
}

fn inverse4_f(d: &[f64; 16]) -> [f64; 16] {
    let ci = [
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 0.5, -0.5, -1.0],
        [1.0, -1.0, -1.0, 1.0],
        [0.5, -1.0, 1.0, -0.5],
    ];
    // r = Ciᵀ d Ci / 64
    let mut t = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            t[i * 4 + j] = (0..4).map(|k| d[i * 4 + k] * ci[k][j]).sum();
        }
    }
    let mut r = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| ci[k][i] * t[k * 4 + j]).sum::<f64>() / 64.0;
        }
    }
    r
}

fn forward4_f(x: &[f64; 16]) -> [f64; 16] {
    let cf = [
        [1.0, 1.0, 1.0, 1.0],
        [2.0, 1.0, -1.0, -2.0],
        [1.0, -1.0, -1.0, 1.0],
        [1.0, -2.0, 2.0, -1.0],
    ];
    let mut t = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            t[i * 4 + j] = (0..4).map(|k| x[i * 4 + k] * cf[j][k]).sum();
        }
    }
    let mut w = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            w[i * 4 + j] = (0..4).map(|k| cf[i][k] * t[k * 4 + j]).sum();
        }
    }
    w
}

fn inverse8_f(d: &[f64; 64]) -> [f64; 64] {
    // The spec's butterflies are exact multiples of these basis values.
    let b = |k: usize, n: usize| f64::from(T8[k][n]) / 8.0;
    let mut t = [0.0; 64];
    for i in 0..8 {
        for n in 0..8 {
            t[i * 8 + n] = (0..8).map(|k| d[i * 8 + k] * b(k, n)).sum();
        }
    }
    let mut r = [0.0; 64];
    for m in 0..8 {
        for n in 0..8 {
            r[m * 8 + n] = (0..8).map(|k| b(k, m) * t[k * 8 + n]).sum::<f64>() / 64.0;
        }
    }
    r
}

fn forward8_f(x: &[f64; 64]) -> [f64; 64] {
    let mut t = [0.0; 64];
    for i in 0..8 {
        for k in 0..8 {
            t[i * 8 + k] = (0..8).map(|j| x[i * 8 + j] * f64::from(T8[k][j])).sum();
        }
    }
    let mut w = [0.0; 64];
    for k in 0..8 {
        for l in 0..8 {
            w[k * 8 + l] = (0..8).map(|i| f64::from(T8[k][i]) * t[i * 8 + l]).sum();
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u32) -> i32 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((*seed >> 16) & 0xff) as i32 - 128
    }

    #[test]
    fn inverse8_matches_float_basis() {
        let mut seed = 7;
        for _ in 0..50 {
            let d: [i32; 64] = std::array::from_fn(|_| lcg(&mut seed) * 16);
            let r = inverse8(&d);
            let rf = inverse8_f(&d.map(f64::from));
            for k in 0..64 {
                assert!(
                    (f64::from(r[k]) - rf[k]).abs() < 2.0,
                    "{k} {} {}",
                    r[k],
                    rf[k]
                );
            }
        }
    }

    #[test]
    fn quantization_round_trips() {
        let mut seed = 3;
        for qp in [0, 12, 24, 30, 40, 51, 63] {
            let steps = Steps::new(qp, 2);
            let tol = 1.0 + steps.ac4[0].max(steps.ac8[0]) / 30.0;
            for _ in 0..20 {
                let x: [i32; 16] = std::array::from_fn(|_| lcg(&mut seed));
                let w = forward4(&x);
                let c: [i32; 16] =
                    std::array::from_fn(|k| (f64::from(w[k]) / steps.ac4[k]).round() as i32);
                let r = inverse4(&scale4(&c, qp, None));
                for k in 0..16 {
                    assert!(
                        f64::from((r[k] - x[k]).abs()) <= tol * 2.0,
                        "qp {qp} 4x4 {k}: {} {}",
                        r[k],
                        x[k]
                    );
                }
                let x8: [i32; 64] = std::array::from_fn(|_| lcg(&mut seed));
                let w = forward8(&x8);
                let c: [i32; 64] =
                    std::array::from_fn(|k| (f64::from(w[k]) / steps.ac8[k]).round() as i32);
                let r = inverse8(&scale8(&c, qp));
                for k in 0..64 {
                    assert!(
                        f64::from((r[k] - x8[k]).abs()) <= tol * 2.0,
                        "qp {qp} 8x8 {k}: {} {}",
                        r[k],
                        x8[k]
                    );
                }
            }
        }
    }
}
