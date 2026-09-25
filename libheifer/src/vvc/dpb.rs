// SPDX-License-Identifier: LGPL-3.0-or-later
//! Decoded reference pictures: samples plus the collocated motion field and
//! the per-slice reference lists that temporal motion vector prediction and
//! deblocking consult, following vvdec's `Picture`/`CodingStructure`.
use super::mv::{MotionInfo, Mv};
use super::pic::{Format, Plane};

/// A slice's resolved reference lists.
#[derive(Clone, Debug, Default)]
pub struct SliceRefs {
    pub slice_type: u32,
    pub poc: i32,
    pub ref_poc: [Vec<i32>; 2],
    pub ref_lt: [Vec<bool>; 2],
    /// Unique picture ids (vvdec compares `Picture*`).
    pub ref_id: [Vec<u64>; 2],
}

impl SliceRefs {
    pub fn is_b(&self) -> bool {
        self.slice_type == super::ps::B_SLICE
    }
}

/// An immutable decoded picture as used for inter prediction.
pub struct RefPic {
    pub id: u64,
    pub poc: i32,
    pub fmt: Format,
    pub width: i32,
    pub height: i32,
    pub bit_depth: u32,
    pub planes: Vec<Plane>,
    pub ctu_log2: u32,
    pub width_ctus: u32,
    /// Slice index of each CTU (`CtuData::slice`).
    pub ctu_slice: Vec<u32>,
    pub slices: Vec<SliceRefs>,
    /// Collocated motion on the 8x8 grid (`ColocatedMotionInfo`).
    pub col: Vec<MotionInfo>,
    pub col_w: usize,
    /// Wraparound offset of the picture's PPS when its border is extended
    /// by wrapping (vvdec's `PIC_RECON_WRAP` buffer).
    pub wrap: Option<i32>,
}

impl RefPic {
    /// `CodingStructure::getColInfo`: the motion and slice at a position.
    pub fn col_info(&self, x: i32, y: i32) -> (MotionInfo, Option<&SliceRefs>) {
        let mi = self.col[(y >> 3) as usize * self.col_w + (x >> 3) as usize];
        let ctu = (y >> self.ctu_log2) as usize * self.width_ctus as usize
            + (x >> self.ctu_log2) as usize;
        let slice = self
            .ctu_slice
            .get(ctu)
            .and_then(|&s| self.slices.get(s as usize));
        (mi, slice)
    }

    /// A grey picture standing in for an unavailable reference
    /// (`prepareUnavailablePicture` with `fillGrey`).
    pub fn grey(
        id: u64,
        poc: i32,
        fmt: Format,
        width: i32,
        height: i32,
        bit_depth: u32,
        ctu_log2: u32,
    ) -> Self {
        let mut planes = vec![Plane::new(width as usize, height as usize)];
        if fmt.chroma != 0 {
            let (cw, ch) = ((width >> fmt.sx) as usize, (height >> fmt.sy) as usize);
            planes.push(Plane::new(cw, ch));
            planes.push(Plane::new(cw, ch));
        }
        for p in &mut planes {
            p.data.fill(1 << (bit_depth - 1));
        }
        let col_w = (width as usize).div_ceil(8);
        let col_h = (height as usize).div_ceil(8);
        Self {
            id,
            poc,
            fmt,
            width,
            height,
            bit_depth,
            planes,
            ctu_log2,
            width_ctus: (width as u32).div_ceil(1 << ctu_log2),
            ctu_slice: Vec::new(),
            slices: Vec::new(),
            col: vec![MotionInfo::default(); col_w * col_h],
            col_w,
            wrap: None,
        }
    }
}

/// DMVR refinements of one coding unit, applied to the collocated field
/// (`DecCu::TaskFinishMotionInfo`).
#[derive(Clone, Debug, Default)]
pub struct DmvrRefinement {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub mv: [Mv; 2],
    pub deltas: Vec<Mv>,
}
