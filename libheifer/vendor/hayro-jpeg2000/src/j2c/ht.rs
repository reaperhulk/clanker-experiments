//! HTJ2K (ITU-T T.814) code-block decoding, ported from OpenJPEG's
//! `ht_dec.c` (BSD-2-Clause, see LICENSE-OPENJPEG) so that decoded
//! coefficients, including those of malformed code-blocks, match it.
//!
//! OpenJPEG's readers align their first reads to the data address; `align`
//! models that address modulo 4. It changes which MEL bytes the initial
//! unstuffing check sees.

use super::ht_luts::{VLC_TBL0, VLC_TBL1};
use alloc::vec::Vec;

/// A code-block's coded data and the parameters OpenJPEG decodes it from.
pub(crate) struct HtCodeBlock<'a> {
    /// The concatenated chunks of the code-block.
    pub(crate) data: &'a [u8],
    /// Address of `data[0]` modulo 4 in OpenJPEG's buffers.
    pub(crate) align: usize,
    /// Number of chunks (segments read, including empty ones).
    pub(crate) num_chunks: usize,
    /// Passes and lengths of the first and second segments.
    pub(crate) segments: [(u32, u32); 2],
    pub(crate) num_segments: usize,
    /// Maximum number of bit-planes of the sub-band (Mb).
    pub(crate) mb: u32,
    /// Zero bit-planes signalled in the packet header.
    pub(crate) zero_bitplanes: u32,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) stripe_causal: bool,
    pub(crate) roi_shift: u8,
}

fn byte(data: &[u8], idx: isize) -> u32 {
    if idx < 0 {
        return 0;
    }
    data.get(idx as usize).copied().unwrap_or(0) as u32
}

fn read_le_u32(data: &[u8], idx: isize) -> u32 {
    byte(data, idx)
        | byte(data, idx + 1) << 8
        | byte(data, idx + 2) << 16
        | byte(data, idx + 3) << 24
}

struct Mel<'a> {
    data: &'a [u8],
    pos: isize,
    tmp: u64,
    bits: i32,
    size: i32,
    unstuff: bool,
    k: i32,
    num_runs: i32,
    runs: u64,
}

impl<'a> Mel<'a> {
    fn new(data: &'a [u8], align: usize, lcup: i32, scup: i32) -> Option<Self> {
        let mut mel = Mel {
            data,
            pos: (lcup - scup) as isize,
            tmp: 0,
            bits: 0,
            size: scup - 1,
            unstuff: false,
            k: 0,
            num_runs: 0,
            runs: 0,
        };
        let num = 4 - ((align as isize + mel.pos) & 3);
        for _ in 0..num {
            if mel.unstuff && byte(data, mel.pos) > 0x8F {
                return None;
            }
            let mut d = if mel.size > 0 { byte(data, mel.pos) as u64 } else { 0xFF };
            if mel.size == 1 {
                d |= 0xF;
            }
            if mel.size > 0 {
                mel.pos += 1;
            }
            mel.size -= 1;
            let d_bits = 8 - mel.unstuff as i32;
            mel.tmp = (mel.tmp << d_bits) | d;
            mel.bits += d_bits;
            mel.unstuff = (d & 0xFF) == 0xFF;
        }
        mel.tmp <<= 64 - mel.bits;
        Some(mel)
    }

    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }
        let mut val: u32 = 0xFFFF_FFFF;
        if self.size > 4 {
            val = read_le_u32(self.data, self.pos);
            self.pos += 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut i = 0;
            while self.size > 1 {
                let v = byte(self.data, self.pos);
                self.pos += 1;
                let m = !(0xFFu32 << i);
                val = (val & m) | (v << i);
                self.size -= 1;
                i += 8;
            }
            let v = byte(self.data, self.pos) | 0xF;
            self.pos += 1;
            let m = !(0xFFu32 << i);
            val = (val & m) | (v << i);
            self.size -= 1;
        }

        let mut bits = 32 - self.unstuff as i32;
        let mut t = val & 0xFF;
        let mut unstuff = (val & 0xFF) == 0xFF;
        bits -= unstuff as i32;
        t <<= 8 - unstuff as u32;

        t |= (val >> 8) & 0xFF;
        unstuff = ((val >> 8) & 0xFF) == 0xFF;
        bits -= unstuff as i32;
        t <<= 8 - unstuff as u32;

        t |= (val >> 16) & 0xFF;
        unstuff = ((val >> 16) & 0xFF) == 0xFF;
        bits -= unstuff as i32;
        t <<= 8 - unstuff as u32;

        t |= (val >> 24) & 0xFF;
        self.unstuff = ((val >> 24) & 0xFF) == 0xFF;

        self.tmp |= (t as u64) << (64 - bits - self.bits);
        self.bits += bits;
    }

    fn decode(&mut self) {
        const MEL_EXP: [i32; 13] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 5];
        if self.bits < 6 {
            self.read();
        }
        while self.bits >= 6 && self.num_runs < 8 {
            let eval = MEL_EXP[self.k as usize];
            let run;
            if self.tmp & (1u64 << 63) != 0 {
                let r = (1 << eval) - 1;
                self.k = if self.k + 1 < 12 { self.k + 1 } else { 12 };
                self.tmp <<= 1;
                self.bits -= 1;
                run = r << 1;
            } else {
                let r = ((self.tmp >> (63 - eval)) as i32) & ((1 << eval) - 1);
                self.k = if self.k - 1 > 0 { self.k - 1 } else { 0 };
                self.tmp <<= eval + 1;
                self.bits -= eval + 1;
                run = (r << 1) + 1;
            }
            let shift = self.num_runs * 7;
            self.runs &= !(0x3Fu64 << shift);
            self.runs |= (run as u64) << shift;
            self.num_runs += 1;
        }
    }

    fn get_run(&mut self) -> i32 {
        if self.num_runs == 0 {
            self.decode();
        }
        let t = (self.runs & 0x7F) as i32;
        self.runs >>= 7;
        self.num_runs -= 1;
        t
    }
}

/// Backward-growing VLC and MRP segments.
struct Rev<'a> {
    data: &'a [u8],
    pos: isize,
    tmp: u64,
    bits: u32,
    size: i32,
    unstuff: bool,
}

impl<'a> Rev<'a> {
    fn new_vlc(data: &'a [u8], align: usize, lcup: i32, scup: i32) -> Self {
        let mut pos = (lcup - 2) as isize;
        let d = byte(data, pos);
        pos -= 1;
        let tmp = (d >> 4) as u64;
        let mut vlc = Rev {
            data,
            pos,
            tmp,
            bits: 4 - ((tmp & 7) == 7) as u32,
            size: scup - 2,
            unstuff: (d | 0xF) > 0x8F,
        };
        let num = 1 + ((align as isize + vlc.pos) & 3) as i32;
        let tnum = if num < vlc.size { num } else { vlc.size };
        for _ in 0..tnum {
            let d = byte(data, vlc.pos) as u64;
            vlc.pos -= 1;
            let d_bits = 8 - (vlc.unstuff && (d & 0x7F) == 0x7F) as u32;
            vlc.tmp |= d << vlc.bits;
            vlc.bits += d_bits;
            vlc.unstuff = d > 0x8F;
        }
        vlc.size -= tnum;
        vlc.read(false);
        vlc
    }

    fn new_mrp(data: &'a [u8], align: usize, lcup: i32, len2: i32) -> Self {
        let mut mrp = Rev {
            data,
            pos: (lcup + len2 - 1) as isize,
            tmp: 0,
            bits: 0,
            size: len2,
            unstuff: true,
        };
        let num = 1 + ((align as isize + mrp.pos) & 3);
        for _ in 0..num {
            let d = if mrp.size > 0 {
                let d = byte(data, mrp.pos) as u64;
                mrp.pos -= 1;
                d
            } else {
                0
            };
            mrp.size -= 1;
            let d_bits = 8 - (mrp.unstuff && (d & 0x7F) == 0x7F) as u32;
            mrp.tmp |= d << mrp.bits;
            mrp.bits += d_bits;
            mrp.unstuff = d > 0x8F;
        }
        mrp.read(true);
        mrp
    }

    /// `rev_read` and `rev_read_mrp`, which only differ in their comments.
    fn read(&mut self, _mrp: bool) {
        if self.bits > 32 {
            return;
        }
        let mut val: u32 = 0;
        if self.size > 3 {
            val = read_le_u32(self.data, self.pos - 3);
            self.pos -= 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut i = 24;
            while self.size > 0 {
                let v = byte(self.data, self.pos);
                self.pos -= 1;
                val |= v << i;
                self.size -= 1;
                i -= 8;
            }
        }

        let mut tmp = val >> 24;
        let mut bits = 8 - (self.unstuff && ((val >> 24) & 0x7F) == 0x7F) as u32;
        let mut unstuff = (val >> 24) > 0x8F;

        tmp |= ((val >> 16) & 0xFF) << bits;
        bits += 8 - (unstuff && ((val >> 16) & 0x7F) == 0x7F) as u32;
        unstuff = ((val >> 16) & 0xFF) > 0x8F;

        tmp |= ((val >> 8) & 0xFF) << bits;
        bits += 8 - (unstuff && ((val >> 8) & 0x7F) == 0x7F) as u32;
        unstuff = ((val >> 8) & 0xFF) > 0x8F;

        tmp |= (val & 0xFF) << bits;
        bits += 8 - (unstuff && (val & 0x7F) == 0x7F) as u32;
        unstuff = (val & 0xFF) > 0x8F;

        self.tmp |= (tmp as u64) << self.bits;
        self.bits += bits;
        self.unstuff = unstuff;
    }

    fn fetch(&mut self, mrp: bool) -> u32 {
        if self.bits < 32 {
            self.read(mrp);
            if self.bits < 32 {
                self.read(mrp);
            }
        }
        self.tmp as u32
    }

    fn advance(&mut self, num_bits: u32) -> u32 {
        // OpenJPEG only asserts this in debug builds.
        self.tmp = self.tmp.checked_shr(num_bits).unwrap_or(0);
        self.bits = self.bits.wrapping_sub(num_bits);
        self.tmp as u32
    }
}

/// Forward-growing MagSgn and SPP segments.
struct Frwd<'a> {
    data: &'a [u8],
    pos: isize,
    tmp: u64,
    bits: u32,
    unstuff: bool,
    size: i32,
    x: u32,
}

impl<'a> Frwd<'a> {
    fn new(data: &'a [u8], align: usize, start: usize, size: i32, x: u32) -> Self {
        let mut msp = Frwd {
            data,
            pos: start as isize,
            tmp: 0,
            bits: 0,
            unstuff: false,
            size,
            x,
        };
        let num = 4 - ((align as isize + msp.pos) & 3);
        for _ in 0..num {
            let d = if msp.size > 0 {
                let d = byte(data, msp.pos) as u64;
                msp.pos += 1;
                d
            } else {
                x as u64
            };
            msp.size -= 1;
            msp.tmp |= d.checked_shl(msp.bits).unwrap_or(0);
            msp.bits += 8 - msp.unstuff as u32;
            msp.unstuff = (d & 0xFF) == 0xFF;
        }
        msp.read();
        msp
    }

    fn read(&mut self) {
        let fill = if self.x != 0 { 0xFFFF_FFFFu32 } else { 0 };
        let mut val;
        if self.size > 3 {
            val = read_le_u32(self.data, self.pos);
            self.pos += 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut i = 0;
            val = fill;
            while self.size > 0 {
                let v = byte(self.data, self.pos);
                self.pos += 1;
                let m = !(0xFFu32 << i);
                val = (val & m) | (v << i);
                self.size -= 1;
                i += 8;
            }
        } else {
            val = fill;
        }

        let mut bits = 8 - self.unstuff as u32;
        let mut t = val & 0xFF;
        let mut unstuff = (val & 0xFF) == 0xFF;

        t |= ((val >> 8) & 0xFF) << bits;
        bits += 8 - unstuff as u32;
        unstuff = ((val >> 8) & 0xFF) == 0xFF;

        t |= ((val >> 16) & 0xFF) << bits;
        bits += 8 - unstuff as u32;
        unstuff = ((val >> 16) & 0xFF) == 0xFF;

        t |= ((val >> 24) & 0xFF) << bits;
        bits += 8 - unstuff as u32;
        self.unstuff = ((val >> 24) & 0xFF) == 0xFF;

        self.tmp |= (t as u64).checked_shl(self.bits).unwrap_or(0);
        self.bits += bits;
    }

    fn advance(&mut self, num_bits: u32) {
        self.tmp = self.tmp.checked_shr(num_bits).unwrap_or(0);
        self.bits = self.bits.wrapping_sub(num_bits);
    }

    fn fetch(&mut self) -> u32 {
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        self.tmp as u32
    }
}

const UVLC_DEC: [u8; 8] = [
    3 | (5 << 2) | (5 << 5),
    1 | (1 << 5),
    2 | (2 << 5),
    1 | (1 << 5),
    3 | (1 << 2) | (3 << 5),
    1 | (1 << 5),
    2 | (2 << 5),
    1 | (1 << 5),
];

fn suffix(vlc: u32, len: u32) -> u32 {
    vlc & ((1u32 << len) - 1)
}

fn decode_init_uvlc(mut vlc: u32, mode: u32, u: &mut [u32; 2]) -> u32 {
    let mut consumed = 0;
    if mode == 0 {
        u[0] = 1;
        u[1] = 1;
    } else if mode <= 2 {
        let d = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d & 3;
        consumed += d & 3;
        let suffix_len = (d >> 2) & 7;
        consumed += suffix_len;
        let d = (d >> 5) + suffix(vlc, suffix_len);
        u[0] = if mode == 1 { d + 1 } else { 1 };
        u[1] = if mode == 1 { 1 } else { d + 1 };
    } else if mode == 3 {
        let d1 = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d1 & 3;
        consumed += d1 & 3;
        if (d1 & 3) > 2 {
            u[1] = (vlc & 1) + 1 + 1;
            consumed += 1;
            vlc >>= 1;
            let suffix_len = (d1 >> 2) & 7;
            consumed += suffix_len;
            u[0] = (d1 >> 5) + suffix(vlc, suffix_len) + 1;
        } else {
            let d2 = UVLC_DEC[(vlc & 7) as usize] as u32;
            vlc >>= d2 & 3;
            consumed += d2 & 3;
            let suffix_len = (d1 >> 2) & 7;
            consumed += suffix_len;
            u[0] = (d1 >> 5) + suffix(vlc, suffix_len) + 1;
            vlc >>= suffix_len;
            let suffix_len = (d2 >> 2) & 7;
            consumed += suffix_len;
            u[1] = (d2 >> 5) + suffix(vlc, suffix_len) + 1;
        }
    } else if mode == 4 {
        let d1 = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d1 & 3;
        consumed += d1 & 3;
        let d2 = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d2 & 3;
        consumed += d2 & 3;
        let suffix_len = (d1 >> 2) & 7;
        consumed += suffix_len;
        u[0] = (d1 >> 5) + suffix(vlc, suffix_len) + 3;
        vlc >>= suffix_len;
        let suffix_len = (d2 >> 2) & 7;
        consumed += suffix_len;
        u[1] = (d2 >> 5) + suffix(vlc, suffix_len) + 3;
    }
    consumed
}

fn decode_noninit_uvlc(mut vlc: u32, mode: u32, u: &mut [u32; 2]) -> u32 {
    let mut consumed = 0;
    if mode == 0 {
        u[0] = 1;
        u[1] = 1;
    } else if mode <= 2 {
        let d = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d & 3;
        consumed += d & 3;
        let suffix_len = (d >> 2) & 7;
        consumed += suffix_len;
        let d = (d >> 5) + suffix(vlc, suffix_len);
        u[0] = if mode == 1 { d + 1 } else { 1 };
        u[1] = if mode == 1 { 1 } else { d + 1 };
    } else if mode == 3 {
        let d1 = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d1 & 3;
        consumed += d1 & 3;
        let d2 = UVLC_DEC[(vlc & 7) as usize] as u32;
        vlc >>= d2 & 3;
        consumed += d2 & 3;
        let suffix_len = (d1 >> 2) & 7;
        consumed += suffix_len;
        u[0] = (d1 >> 5) + suffix(vlc, suffix_len) + 1;
        vlc >>= suffix_len;
        let suffix_len = (d2 >> 2) & 7;
        consumed += suffix_len;
        u[1] = (d2 >> 5) + suffix(vlc, suffix_len) + 1;
    }
    consumed
}

fn count_bits(v: u32) -> u32 {
    32 - v.leading_zeros()
}

/// Decodes one MagSgn sample: `e_k` and `e_1` are the EMB bits of the sample.
#[inline(always)]
fn mag_sgn(magsgn: &mut Frwd<'_>, u_q: u32, e_k: u32, e_1: u32, p: u32) -> (u32, u32) {
    let ms_val = magsgn.fetch();
    let m_n = u_q.wrapping_sub(e_k);
    magsgn.advance(m_n);
    let val = ms_val << 31;
    let mut v_n = ms_val & 1u32.checked_shl(m_n).unwrap_or(0).wrapping_sub(1);
    v_n |= e_1.checked_shl(m_n).unwrap_or(0);
    v_n |= 1;
    (val | (v_n.wrapping_add(2)).wrapping_shl(p.wrapping_sub(1)), v_n)
}

/// Decodes the quad pair samples of two rows at `sp` (row `sp`, row
/// `sp + stride`), shared by the initial and non-initial line loops.
#[inline(always)]
fn decode_quad_pair(
    decoded: &mut [u32],
    mut sp: usize,
    stride: usize,
    lsp: &mut [u8],
    mut li: usize,
    qinf: [u32; 2],
    u_q: [u32; 2],
    locs: u32,
    magsgn: &mut Frwd<'_>,
    p: u32,
) {
    for q in 0..2 {
        let qi = qinf[q];
        let u = u_q[q];
        if qi & 0x10 != 0 {
            decoded[sp] = mag_sgn(magsgn, u, (qi >> 12) & 1, (qi & 0x100) >> 8, p).0;
        } else if locs & (0x1 << (4 * q)) != 0 {
            decoded[sp] = 0;
        }
        if qi & 0x20 != 0 {
            let (value, v_n) = mag_sgn(magsgn, u, (qi >> 13) & 1, (qi & 0x200) >> 9, p);
            decoded[sp + stride] = value;
            let t = (lsp[li] & 0x7F) as u32;
            let v_n = count_bits(v_n);
            lsp[li] = (0x80 | if t > v_n { t } else { v_n }) as u8;
        } else if locs & (0x2 << (4 * q)) != 0 {
            decoded[sp + stride] = 0;
        }
        li += 1;
        sp += 1;
        if qi & 0x40 != 0 {
            decoded[sp] = mag_sgn(magsgn, u, (qi >> 14) & 1, (qi & 0x400) >> 10, p).0;
        } else if locs & (0x4 << (4 * q)) != 0 {
            decoded[sp] = 0;
        }
        lsp[li] = 0;
        if qi & 0x80 != 0 {
            let (value, v_n) = mag_sgn(magsgn, u, (qi >> 15) & 1, (qi & 0x800) >> 11, p);
            decoded[sp + stride] = value;
            lsp[li] = (0x80 | count_bits(v_n)) as u8;
        } else if locs & (0x8 << (4 * q)) != 0 {
            decoded[sp + stride] = 0;
        }
        sp += 1;
    }
}

/// Magnitude refinement of the stripe whose significance is `sig_arr`.
fn magref_stripe(
    decoded: &mut [u32],
    dpp: usize,
    stride: usize,
    width: usize,
    sig_arr: &[u32],
    magref: &mut Rev<'_>,
    p: u32,
) {
    let half = 1u32.wrapping_shl(p.wrapping_sub(2));
    let mut entry = 0;
    let mut i = 0;
    while i < width {
        let mut cwd = magref.fetch(true);
        let sig = sig_arr[entry];
        entry += 1;
        let mut col_mask = 0xFu32;
        let mut dp = dpp + i;
        if sig != 0 {
            for _ in 0..8 {
                if sig & col_mask != 0 {
                    let mut sample_mask = 0x1111_1111u32 & col_mask;
                    for row in 0..4 {
                        if sig & sample_mask != 0 {
                            let sym = cwd & 1;
                            let idx = dp + row * stride;
                            decoded[idx] ^= (1 - sym).wrapping_shl(p.wrapping_sub(1));
                            decoded[idx] |= half;
                            cwd >>= 1;
                        }
                        sample_mask = sample_mask.wrapping_add(sample_mask);
                    }
                }
                col_mask <<= 4;
                dp += 1;
            }
        }
        magref.advance(sig.count_ones());
        i += 8;
    }
}

/// Builds the membership of a stripe from its significance.
fn stripe_mbr(sig: &[u32], mbr: &mut [u32], width: usize) {
    let mut prev = 0u32;
    let mut k = 0;
    let mut i = 0;
    while i < width {
        let mut m = sig[k];
        m |= prev >> 28;
        m |= sig[k] << 4;
        m |= sig[k] >> 4;
        m |= sig[k + 1] << 28;
        prev = sig[k];
        let t = m;
        let mut z = m;
        z |= (t & 0x7777_7777) << 1;
        z |= (t & 0xEEEE_EEEE) >> 1;
        mbr[k] = z & !sig[k];
        i += 8;
        k += 1;
    }
}

/// Adds membership from the next stripe (`nxt_sig`) to `cur_mbr`.
fn add_next_membership(
    cur_sig: &[u32],
    cur_mbr: &mut [u32],
    nxt_sig: &[u32],
    width: usize,
    stripe_causal: bool,
) {
    let mut prev = 0u32;
    let mut k = 0;
    let mut i = 0;
    while i < width {
        let mut t = nxt_sig[k];
        t |= prev >> 28;
        t |= nxt_sig[k] << 4;
        t |= nxt_sig[k] >> 4;
        t |= nxt_sig[k + 1] << 28;
        prev = nxt_sig[k];
        if !stripe_causal {
            cur_mbr[k] |= (t & 0x1111_1111) << 3;
        }
        cur_mbr[k] &= !cur_sig[k];
        i += 8;
        k += 1;
    }
}

/// Significance propagation of the stripe starting at row `row`.
#[allow(clippy::too_many_arguments)]
fn sigprop_stripe(
    decoded: &mut [u32],
    row: usize,
    stride: usize,
    width: usize,
    cur_sig: &[u32],
    cur_mbr: &mut [u32],
    nxt_sig: &[u32],
    nxt_mbr: &mut [u32],
    pattern: u32,
    sigprop: &mut Frwd<'_>,
    p: u32,
) {
    let val = 3u32.wrapping_shl(p.wrapping_sub(2));
    let mut k = 0;
    let mut i = 0;
    while i < width {
        let mut mbr = cur_mbr[k] & pattern;
        let mut new_sig = 0u32;
        if mbr != 0 {
            let mut n = 0;
            while n < 8 {
                let mut cwd = sigprop.fetch();
                let mut cnt = 0u32;
                let mut dp = row * stride + i + n;
                let mut col_mask = 0xFu32 << (4 * n);
                let inv_sig = !cur_sig[k] & pattern;
                let end = if n + 4 + i < width { n + 4 } else { width - i };
                let mut j = n;
                while j < end {
                    if col_mask & mbr != 0 {
                        let mut sample_mask = 0x1111_1111u32 & col_mask;
                        for pattern_bits in [0x32u32, 0x74, 0xE8, 0xC0] {
                            if mbr & sample_mask != 0 {
                                if cwd & 1 != 0 {
                                    new_sig |= sample_mask;
                                    let t = pattern_bits.wrapping_shl((j * 4) as u32);
                                    mbr |= t & inv_sig;
                                }
                                cwd >>= 1;
                                cnt += 1;
                            }
                            sample_mask = sample_mask.wrapping_add(sample_mask);
                        }
                    }
                    j += 1;
                    dp += 1;
                    col_mask = col_mask.wrapping_shl(4);
                }
                let _ = dp;

                if new_sig & (0xFFFFu32 << (4 * n)) != 0 {
                    let mut dp = row * stride + i + n;
                    let mut col_mask = 0xFu32 << (4 * n);
                    let mut j = n;
                    while j < end {
                        if col_mask & new_sig != 0 {
                            let mut sample_mask = 0x1111_1111u32 & col_mask;
                            for r in 0..4 {
                                if new_sig & sample_mask != 0 {
                                    decoded[dp + r * stride] |= ((cwd & 1) << 31) | val;
                                    cwd >>= 1;
                                    cnt += 1;
                                }
                                sample_mask = sample_mask.wrapping_add(sample_mask);
                            }
                        }
                        j += 1;
                        dp += 1;
                        col_mask = col_mask.wrapping_shl(4);
                    }
                }
                sigprop.advance(cnt);

                if n == 4 {
                    let mut t = new_sig >> 28;
                    t |= ((t & 0xE) >> 1) | ((t & 7) << 1);
                    cur_mbr[k + 1] |= t & !cur_sig[k + 1];
                }
                n += 4;
            }
        }
        new_sig |= cur_sig[k];
        let ux = (new_sig & 0x8888_8888) >> 3;
        let tx = ux | (ux << 4) | (ux >> 4);
        if i > 0 {
            nxt_mbr[k - 1] |= (ux << 28) & !nxt_sig[k - 1];
        }
        nxt_mbr[k] |= tx & !nxt_sig[k];
        nxt_mbr[k + 1] |= (ux >> 28) & !nxt_sig[k + 1];
        i += 8;
        k += 1;
    }
}

const SIG_LEN: usize = 132;

/// Decodes the code-block into `out` (row-major, `width * height`) as
/// OpenJPEG's T1 data: signed values with one extra fractional bit.
/// `None` corresponds to OpenJPEG failing the decode.
pub(crate) fn decode(cb: &HtCodeBlock<'_>, out: &mut Vec<i32>) -> Option<()> {
    let width = cb.width;
    let height = cb.height;
    out.clear();
    out.resize(width * height, 0);

    if cb.roi_shift != 0 {
        return None;
    }
    if cb.mb == 0 || cb.num_chunks == 0 {
        return Some(());
    }

    let zero_bplanes = cb.zero_bitplanes;
    let cblk_len = cb.data.len();
    let data = cb.data;

    let mut num_passes = if cb.num_segments > 0 { cb.segments[0].0 } else { 0 };
    num_passes = num_passes.wrapping_add(if cb.num_segments > 1 { cb.segments[1].0 } else { 0 });
    let lengths1 = if num_passes > 0 { cb.segments[0].1 } else { 0 };
    let lengths2 = if num_passes > 1 { cb.segments[1].1 } else { 0 };

    if num_passes > 1 && lengths2 == 0 {
        num_passes = 1;
    }
    if num_passes > 3 {
        return None;
    }
    if cb.mb > 30 {
        return None;
    }
    if zero_bplanes > cb.mb {
        return None;
    } else if zero_bplanes == cb.mb && num_passes > 1 {
        num_passes = 1;
    }

    let p = cb.mb + 1 - zero_bplanes;
    let zero_bplanes_p1 = zero_bplanes + 1;

    if lengths1 < 2
        || lengths1 as usize > cblk_len
        || (lengths1 as u64 + lengths2 as u64) as usize > cblk_len
    {
        return None;
    }
    let lcup = lengths1 as i32;
    let scup = ((data[lcup as usize - 1] as i32) << 4) + (data[lcup as usize - 2] as i32 & 0xF);
    if scup < 2 || scup > lcup || scup > 4079 {
        return None;
    }

    let mut mel = Mel::new(data, cb.align, lcup, scup)?;
    let mut vlc = Rev::new_vlc(data, cb.align, lcup, scup);
    let mut magsgn = Frwd::new(data, cb.align, 0, lcup - scup, 0xFF);
    let mut sigprop = if num_passes > 1 {
        Some(Frwd::new(data, cb.align, lengths1 as usize, lengths2 as i32, 0))
    } else {
        None
    };
    let mut magref = if num_passes > 2 {
        Some(Rev::new_mrp(data, cb.align, lcup, lengths2 as i32))
    } else {
        None
    };

    let stride = width;
    // Samples are written as unsigned words; two spare rows cover the second
    // row of the final quad row.
    let mut decoded: Vec<u32> = alloc::vec![0; width * (height + 2) + 8];
    let mut sigma1 = [0u32; SIG_LEN];
    let mut sigma2 = [0u32; SIG_LEN];
    let mut mbr1 = [0u32; SIG_LEN];
    let mut mbr2 = [0u32; SIG_LEN];
    let mut line_state = [0u8; 528];

    let w = width as i32;
    let h = height as i32;

    // Initial two lines.
    {
        let mut si = 0usize;
        let mut sip_shift = 0u32;
        line_state[0] = 0;
        let mut run = mel.get_run();
        let mut qinf = [0u32; 2];
        let mut c_q = 0u32;
        let mut sp = 0usize;
        let mut li = 0usize;
        let mut x = 0i32;
        while x < w {
            let mut u_q = [0u32; 2];
            let mut vlc_val = vlc.fetch(false);
            qinf[0] = VLC_TBL0[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;
            if c_q == 0 {
                run -= 2;
                qinf[0] = if run == -1 { qinf[0] } else { 0 };
                if run < 0 {
                    run = mel.get_run();
                }
            }
            c_q = ((qinf[0] & 0x10) >> 4) | ((qinf[0] & 0xE0) >> 5);
            vlc_val = vlc.advance(qinf[0] & 0x7);
            sigma1[si] |= (((qinf[0] & 0x30) >> 4) | ((qinf[0] & 0xC0) >> 2)) << sip_shift;

            qinf[1] = 0;
            if x + 2 < w {
                qinf[1] = VLC_TBL0[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;
                if c_q == 0 {
                    run -= 2;
                    qinf[1] = if run == -1 { qinf[1] } else { 0 };
                    if run < 0 {
                        run = mel.get_run();
                    }
                }
                c_q = ((qinf[1] & 0x10) >> 4) | ((qinf[1] & 0xE0) >> 5);
                vlc_val = vlc.advance(qinf[1] & 0x7);
            }
            sigma1[si] |= ((qinf[1] & 0x30) | ((qinf[1] & 0xC0) << 2)) << (4 + sip_shift);
            si += if x & 0x7 != 0 { 1 } else { 0 };
            sip_shift ^= 0x10;

            let mut uvlc_mode = ((qinf[0] & 0x8) >> 3) | ((qinf[1] & 0x8) >> 2);
            if uvlc_mode == 3 {
                run -= 2;
                uvlc_mode += if run == -1 { 1 } else { 0 };
                if run < 0 {
                    run = mel.get_run();
                }
            }
            let consumed = decode_init_uvlc(vlc_val, uvlc_mode, &mut u_q);
            if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
                return None;
            }
            vlc.advance(consumed);

            let mut locs = 0xFFu32;
            if x + 4 > w {
                locs >>= ((x + 4 - w) << 1) as u32;
            }
            locs = if h > 1 { locs } else { locs & 0x55 };
            if (((qinf[0] & 0xF0) >> 4) | (qinf[1] & 0xF0)) & !locs != 0 {
                return None;
            }

            decode_quad_pair(
                &mut decoded,
                sp,
                stride,
                &mut line_state,
                li,
                qinf,
                u_q,
                locs,
                &mut magsgn,
                p,
            );
            sp += 4;
            li += 2;
            x += 4;
        }
        // The last line-state write of the loop sits at `li`.
        let _ = li;

        let mut sip_shift_state = sip_shift;
        // Non-initial lines.
        let mut y = 2i32;
        while y < h {
            sip_shift_state ^= 0x2;
            sip_shift_state &= 0xFFFF_FFEF;
            let use_sigma2 = y & 0x4 != 0;

            let mut li = 0usize;
            let mut ls0 = line_state[0];
            line_state[0] = 0;
            let mut sp = (y as usize) * stride;
            c_q = 0;
            let mut si = 0usize;
            let mut x = 0i32;
            while x < w {
                let mut u_q = [0u32; 2];
                c_q |= (ls0 >> 7) as u32;
                c_q |= ((line_state[li + 1] >> 5) & 0x4) as u32;
                let mut vlc_val = vlc.fetch(false);
                qinf[0] = VLC_TBL1[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;
                if c_q == 0 {
                    run -= 2;
                    qinf[0] = if run == -1 { qinf[0] } else { 0 };
                    if run < 0 {
                        run = mel.get_run();
                    }
                }
                c_q = ((qinf[0] & 0x40) >> 5) | ((qinf[0] & 0x80) >> 6);
                vlc_val = vlc.advance(qinf[0] & 0x7);
                {
                    let sig = if use_sigma2 { &mut sigma2 } else { &mut sigma1 };
                    sig[si] |= (((qinf[0] & 0x30) >> 4) | ((qinf[0] & 0xC0) >> 2))
                        << sip_shift_state;
                }

                qinf[1] = 0;
                if x + 2 < w {
                    c_q |= (line_state[li + 1] >> 7) as u32;
                    c_q |= ((line_state[li + 2] >> 5) & 0x4) as u32;
                    qinf[1] = VLC_TBL1[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;
                    if c_q == 0 {
                        run -= 2;
                        qinf[1] = if run == -1 { qinf[1] } else { 0 };
                        if run < 0 {
                            run = mel.get_run();
                        }
                    }
                    c_q = ((qinf[1] & 0x40) >> 5) | ((qinf[1] & 0x80) >> 6);
                    vlc_val = vlc.advance(qinf[1] & 0x7);
                }
                {
                    let sig = if use_sigma2 { &mut sigma2 } else { &mut sigma1 };
                    sig[si] |=
                        ((qinf[1] & 0x30) | ((qinf[1] & 0xC0) << 2)) << (4 + sip_shift_state);
                }
                si += if x & 0x7 != 0 { 1 } else { 0 };
                sip_shift_state ^= 0x10;

                let uvlc_mode = ((qinf[0] & 0x8) >> 3) | ((qinf[1] & 0x8) >> 2);
                let consumed = decode_noninit_uvlc(vlc_val, uvlc_mode, &mut u_q);
                vlc.advance(consumed);

                if (qinf[0] & 0xF0) & (qinf[0] & 0xF0).wrapping_sub(1) != 0 {
                    let mut e = (ls0 & 0x7F) as u32;
                    let ne = (line_state[li + 1] & 0x7F) as u32;
                    e = if e > ne { e } else { ne };
                    u_q[0] += if e > 2 { e - 2 } else { 0 };
                }
                if (qinf[1] & 0xF0) & (qinf[1] & 0xF0).wrapping_sub(1) != 0 {
                    let mut e = (line_state[li + 1] & 0x7F) as u32;
                    let nf = (line_state[li + 2] & 0x7F) as u32;
                    e = if e > nf { e } else { nf };
                    u_q[1] += if e > 2 { e - 2 } else { 0 };
                }
                if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
                    return None;
                }

                ls0 = line_state[li + 2];
                line_state[li + 1] = 0;
                line_state[li + 2] = 0;

                let mut locs = 0xFFu32;
                if x + 4 > w {
                    locs >>= ((x + 4 - w) << 1) as u32;
                }
                locs = if y + 2 <= h { locs } else { locs & 0x55 };
                if (((qinf[0] & 0xF0) >> 4) | (qinf[1] & 0xF0)) & !locs != 0 {
                    return None;
                }

                decode_quad_pair(
                    &mut decoded,
                    sp,
                    stride,
                    &mut line_state,
                    li,
                    qinf,
                    u_q,
                    locs,
                    &mut magsgn,
                    p,
                );
                sp += 4;
                li += 2;
                x += 4;
            }

            y += 2;
            if num_passes > 1 && (y & 3) == 0 {
                let yu = y as usize;
                if num_passes > 2 {
                    let cur_sig = if y & 0x4 != 0 { &sigma1 } else { &sigma2 };
                    magref_stripe(
                        &mut decoded,
                        (yu - 4) * stride,
                        stride,
                        width,
                        cur_sig,
                        magref.as_mut()?,
                        p,
                    );
                }

                if y >= 4 {
                    if y & 0x4 != 0 {
                        stripe_mbr(&sigma1, &mut mbr1, width);
                    } else {
                        stripe_mbr(&sigma2, &mut mbr2, width);
                    }
                }

                if y >= 8 {
                    let sp_ = sigprop.as_mut()?;
                    if y & 0x4 != 0 {
                        add_next_membership(&sigma2, &mut mbr2, &sigma1, width, cb.stripe_causal);
                        sigprop_stripe(
                            &mut decoded,
                            yu - 8,
                            stride,
                            width,
                            &sigma2,
                            &mut mbr2,
                            &sigma1,
                            &mut mbr1,
                            0xFFFF_FFFF,
                            sp_,
                            p,
                        );
                        let n = ((width + 7) >> 3) + 1;
                        sigma2[..n].fill(0);
                    } else {
                        add_next_membership(&sigma1, &mut mbr1, &sigma2, width, cb.stripe_causal);
                        sigprop_stripe(
                            &mut decoded,
                            yu - 8,
                            stride,
                            width,
                            &sigma1,
                            &mut mbr1,
                            &sigma2,
                            &mut mbr2,
                            0xFFFF_FFFF,
                            sp_,
                            p,
                        );
                        let n = ((width + 7) >> 3) + 1;
                        sigma1[..n].fill(0);
                    }
                }
            }
        }
    }

    // Terminating.
    if num_passes > 1 {
        if num_passes > 2 && ((h & 3) == 1 || (h & 3) == 2) {
            let cur_sig = if h & 0x4 != 0 { &sigma2 } else { &sigma1 };
            magref_stripe(
                &mut decoded,
                ((h & 0xFF_FFFC) as usize) * stride,
                stride,
                width,
                cur_sig,
                magref.as_mut()?,
                p,
            );
        }

        if (h & 3) == 1 || (h & 3) == 2 {
            if h & 0x4 != 0 {
                stripe_mbr(&sigma2, &mut mbr2, width);
            } else {
                stripe_mbr(&sigma1, &mut mbr1, width);
            }
        }

        let mut st = h;
        st -= if h > 6 { ((h + 1) & 3) + 3 } else { h };
        let mut y = st;
        let sp_ = sigprop.as_mut()?;
        while y < h {
            let pattern = match h - y {
                3 => 0x7777_7777u32,
                2 => 0x3333_3333,
                1 => 0x1111_1111,
                _ => 0xFFFF_FFFF,
            };
            if y & 0x4 != 0 {
                if h - y > 4 {
                    add_next_membership(&sigma2, &mut mbr2, &sigma1, width, cb.stripe_causal);
                }
                sigprop_stripe(
                    &mut decoded,
                    y as usize,
                    stride,
                    width,
                    &sigma2,
                    &mut mbr2,
                    &sigma1,
                    &mut mbr1,
                    pattern,
                    sp_,
                    p,
                );
            } else {
                if h - y > 4 {
                    add_next_membership(&sigma1, &mut mbr1, &sigma2, width, cb.stripe_causal);
                }
                sigprop_stripe(
                    &mut decoded,
                    y as usize,
                    stride,
                    width,
                    &sigma1,
                    &mut mbr1,
                    &sigma2,
                    &mut mbr2,
                    pattern,
                    sp_,
                    p,
                );
            }
            y += 4;
        }
    }

    for (o, &v) in out.iter_mut().zip(&decoded[..width * height]) {
        let val = (v & 0x7FFF_FFFF) as i32;
        *o = if v & 0x8000_0000 != 0 { -val } else { val };
    }
    Some(())
}
