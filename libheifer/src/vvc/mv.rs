// SPDX-License-Identifier: LGPL-3.0-or-later
//! Motion vectors and motion information, following vvdec's `Mv.h` and
//! `MotionInfo.h` (vectors in 1/16 luma samples).

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mv {
    pub x: i32,
    pub y: i32,
}

pub const MV_PRECISION_4PEL: i32 = 0;
pub const MV_PRECISION_INT: i32 = 2;
pub const MV_PRECISION_HALF: i32 = 3;
pub const MV_PRECISION_QUARTER: i32 = 4;
pub const MV_PRECISION_INTERNAL: i32 = 6;

/// `Mv::m_amvrPrecision`, indexed by AMVR index (0 quarter, 1 integer,
/// 2 four-sample, 3 half).
const AMVR_PRECISION: [i32; 4] = [
    MV_PRECISION_QUARTER,
    MV_PRECISION_INT,
    MV_PRECISION_4PEL,
    MV_PRECISION_HALF,
];

pub const IMV_HPEL: u8 = 3;

impl Mv {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn add(self, o: Mv) -> Mv {
        Mv::new(self.x.wrapping_add(o.x), self.y.wrapping_add(o.y))
    }

    pub fn sub(self, o: Mv) -> Mv {
        Mv::new(self.x.wrapping_sub(o.x), self.y.wrapping_sub(o.y))
    }

    /// `operator<<=`.
    pub fn shl(self, i: i32) -> Mv {
        Mv::new(self.x.wrapping_mul(1 << i), self.y.wrapping_mul(1 << i))
    }

    pub fn change_precision(self, src: i32, dst: i32) -> Mv {
        let shift = dst - src;
        if shift >= 0 {
            self.shl(shift)
        } else {
            let r = -shift;
            let off = 1 << (r - 1);
            let f = |v: i32| {
                if v >= 0 {
                    (v + off - 1) >> r
                } else {
                    (v + off) >> r
                }
            };
            Mv::new(f(self.x), f(self.y))
        }
    }

    pub fn change_precision_amvr(self, amvr: u8, dst: i32) -> Mv {
        self.change_precision(AMVR_PRECISION[amvr as usize], dst)
    }

    pub fn round_to_precision(self, src: i32, dst: i32) -> Mv {
        self.change_precision(src, dst).change_precision(dst, src)
    }

    pub fn round_to_amvr_signal_precision(self, src: i32, amvr: u8) -> Mv {
        self.round_to_precision(src, AMVR_PRECISION[amvr as usize])
    }

    /// `clipToStorageBitDepth`: clamp to 18 bits.
    pub fn clip_storage(self) -> Mv {
        let c = |v: i32| v.clamp(-(1 << 17), (1 << 17) - 1);
        Mv::new(c(self.x), c(self.y))
    }

    /// `mvCliptoStorageBitDepth`: periodic wrap to 18 bits.
    pub fn wrap_storage(self) -> Mv {
        let w = |v: i32| {
            let v = v.wrapping_add(1 << 18) & ((1 << 18) - 1);
            if v >= 1 << 17 { v - (1 << 18) } else { v }
        };
        Mv::new(w(self.x), w(self.y))
    }

    /// `scaleMv`.
    pub fn scale(self, scale: i32) -> Mv {
        let f = |v: i32| {
            let p = scale.wrapping_mul(v);
            ((p + 128 - i32::from(p >= 0)) >> 8).clamp(-131072, 131071)
        };
        Mv::new(f(self.x), f(self.y))
    }
}

/// `roundAffineMv`.
pub fn round_affine(x: i32, y: i32, shift: i32) -> (i32, i32) {
    let off = 1 << (shift - 1);
    (
        (x + off - i32::from(x >= 0)) >> shift,
        (y + off - i32::from(y >= 0)) >> shift,
    )
}

/// Motion of one 4x4 luma unit; a negative reference index is invalid.
#[derive(Clone, Copy, Debug)]
pub struct MotionInfo {
    pub mv: [Mv; 2],
    pub ref_idx: [i8; 2],
}

impl Default for MotionInfo {
    fn default() -> Self {
        Self {
            mv: [Mv::default(); 2],
            ref_idx: [-1, -1],
        }
    }
}

impl PartialEq for MotionInfo {
    /// vvdec's `MotionInfo::operator==`: vectors only compared for valid lists.
    fn eq(&self, o: &Self) -> bool {
        for l in 0..2 {
            if self.ref_idx[l] != o.ref_idx[l] {
                return false;
            }
            if self.ref_idx[l] >= 0 && self.mv[l] != o.mv[l] {
                return false;
            }
        }
        true
    }
}

impl MotionInfo {
    pub fn inter_dir(&self) -> u8 {
        u8::from(self.ref_idx[0] >= 0) + 2 * u8::from(self.ref_idx[1] >= 0)
    }
    pub fn is_inter(&self) -> bool {
        self.inter_dir() != 0
    }
}

/// History-based candidate (vvdec's `HPMVInfo`).
#[derive(Clone, Copy, Debug, Default)]
pub struct HpMvInfo {
    pub mi: MotionInfo,
    pub bcw: u8,
    pub alt_hpel: bool,
}

impl HpMvInfo {
    /// `MotionHist::addMiToLut`: prune the first equal entry or the oldest.
    pub fn add_to_lut(lut: &mut Vec<HpMvInfo>, mi: HpMvInfo) {
        let pos = lut.iter().position(|e| e.mi == mi.mi);
        if let Some(i) = pos {
            lut.remove(i);
        } else if lut.len() == 5 {
            lut.remove(0);
        }
        lut.push(mi);
    }
}
