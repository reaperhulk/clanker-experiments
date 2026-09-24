// SPDX-License-Identifier: LGPL-3.0-or-later
//! Picture-level decoding state: sample planes, coding units, transform
//! units and the position maps vvdec's `CodingStructure` keeps.
use super::ps::{Pps, Sps};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn valid(&self) -> bool {
        self.w != 0 && self.h != 0
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn area(&self) -> i32 {
        self.w * self.h
    }
}

/// Component blocks of a unit, in each component's own sample coordinates.
#[derive(Clone, Copy, Default, Debug)]
pub struct UnitArea {
    pub blk: [Area; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub chroma: u32,
    pub sx: u32,
    pub sy: u32,
}

impl Format {
    pub fn new(chroma: u32) -> Self {
        let (sx, sy) = match chroma {
            1 => (1, 1),
            2 => (1, 0),
            _ => (0, 0),
        };
        Self { chroma, sx, sy }
    }
    pub fn num_comp(&self) -> usize {
        if self.chroma == 0 { 1 } else { 3 }
    }
    pub fn scale(&self, comp: usize) -> (u32, u32) {
        if comp == 0 { (0, 0) } else { (self.sx, self.sy) }
    }
    pub fn unit(&self, x: i32, y: i32, w: i32, h: i32) -> UnitArea {
        let mut u = UnitArea::default();
        u.blk[0] = Area::new(x, y, w, h);
        if self.chroma != 0 {
            let c = Area::new(x >> self.sx, y >> self.sy, w >> self.sx, h >> self.sy);
            u.blk[1] = c;
            u.blk[2] = c;
        }
        u
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tree {
    #[default]
    D,
    L,
    C,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ModeType {
    #[default]
    All,
    Inter,
    Intra,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pred {
    #[default]
    Inter,
    Intra,
    Ibc,
}

pub const NOT_ISP: u8 = 0;
pub const HOR_ISP: u8 = 1;
pub const VER_ISP: u8 = 2;

pub const MTS_DCT2: u8 = 0;
pub const MTS_SKIP: u8 = 1;
pub const MTS_DST7_DST7: u8 = 2;

pub const PLANAR: u8 = 0;
pub const DC: u8 = 1;
pub const HOR: u8 = 18;
pub const DIA: u8 = 34;
pub const VER: u8 = 50;
pub const VDIA: u8 = 66;
pub const LM_CHROMA: u8 = 67;
pub const MDLM_L: u8 = 68;
pub const MDLM_T: u8 = 69;
pub const DM_CHROMA: u8 = 70;

#[derive(Clone, Debug, Default)]
pub struct Cu {
    pub blk: [Area; 3],
    pub ch_type: usize,
    pub tree: Tree,
    pub mode_type: ModeType,
    pub pred: Pred,
    pub skip: bool,
    pub qp: i32,
    pub chroma_qp_adj: u8,
    pub qt_depth: u32,
    pub depth: u32,
    pub split_series: u32,
    pub intra_dir: [u8; 2],
    pub mip: bool,
    pub mip_transposed: bool,
    pub mrl: u8,
    pub isp: u8,
    pub bdpcm: [u8; 2],
    pub act: bool,
    pub lfnst: u8,
    pub slice: u32,
    pub tile: u32,
    pub ctu: u32,
    pub idx: u32,
    pub left: Option<u32>,
    pub above: Option<u32>,
    pub first_tu: u32,
    pub num_tu: u32,
    pub plane_cbf: [bool; 3],
    pub root_cbf: bool,
    /// IBC block vector in 1/16 luma samples.
    pub bv: (i32, i32),
}

impl Cu {
    pub fn lw(&self) -> i32 {
        self.blk[0].w
    }
    pub fn lh(&self) -> i32 {
        self.blk[0].h
    }
    pub fn lx(&self) -> i32 {
        self.blk[0].x
    }
    pub fn ly(&self) -> i32 {
        self.blk[0].y
    }
    pub fn is_sep_tree(&self, dual: bool) -> bool {
        self.tree != Tree::D || dual
    }
}

#[derive(Clone, Debug, Default)]
pub struct Tu {
    pub blk: [Area; 3],
    pub ch_type: usize,
    pub cbf: u8,
    pub mts: [u8; 3],
    pub joint: u8,
    pub max_scan: [(i32, i32); 3],
    pub idx: u32,
    pub cu: u32,
    pub coeff: [Vec<i32>; 3],
}

impl Tu {
    pub fn cbf(&self, comp: usize) -> bool {
        (self.cbf >> comp) & 1 != 0
    }
    pub fn set_cbf(&mut self, comp: usize, v: bool) {
        self.cbf &= !(1 << comp);
        self.cbf |= u8::from(v) << comp;
    }
}

#[derive(Clone)]
pub struct Plane {
    pub data: Vec<i16>,
    pub stride: usize,
    pub width: usize,
    pub height: usize,
}

impl Plane {
    pub fn new(width: usize, height: usize) -> Self {
        Self { data: vec![0; width * height], stride: width, width, height }
    }
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> i16 {
        self.data[y as usize * self.stride + x as usize]
    }
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, v: i16) {
        self.data[y as usize * self.stride + x as usize] = v;
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct SaoParam {
    /// 0 = off, 1 = new, 2 = merge
    pub mode: u8,
    /// Band offset: 0, edge offset: 1..=4 (class + 1); for merge: 0 left, 1 above.
    pub type_idc: u8,
    pub band_pos: u8,
    pub offset: [i32; 5],
}

#[derive(Clone, Copy, Default, Debug)]
pub struct AlfCtu {
    pub enable: [bool; 3],
    pub filter_idx: u16,
    pub alt: [u8; 2],
    pub cc: [u8; 2],
}

#[derive(Clone)]
pub struct CtuData {
    pub slice: Option<u32>,
    pub sao: [SaoParam; 3],
    pub alf: AlfCtu,
    pub num_cus: u32,
    pub num_tus: u32,
}

pub struct Picture {
    pub fmt: Format,
    pub width: i32,
    pub height: i32,
    pub bit_depth: u32,
    pub ctu_log2: u32,
    pub width_ctus: u32,
    pub height_ctus: u32,
    pub planes: Vec<Plane>,
    pub cus: Vec<Cu>,
    pub tus: Vec<Tu>,
    /// CU index per 4x4 luma unit for the luma (0) and chroma (1) trees.
    pub cu_map: [Vec<u32>; 2],
    pub map_w: usize,
    pub map_h: usize,
    pub ctus: Vec<CtuData>,
}

pub const NONE: u32 = u32::MAX;

impl Picture {
    pub fn new(sps: &Sps, pps: &Pps) -> Self {
        let fmt = Format::new(sps.chroma_format_idc);
        let width = pps.width as i32;
        let height = pps.height as i32;
        let mut planes = vec![Plane::new(width as usize, height as usize)];
        if fmt.chroma != 0 {
            let cw = (width >> fmt.sx) as usize;
            let ch = (height >> fmt.sy) as usize;
            planes.push(Plane::new(cw, ch));
            planes.push(Plane::new(cw, ch));
        }
        let map_w = (width as usize).div_ceil(4);
        let map_h = (height as usize).div_ceil(4);
        let n = (pps.width_ctus * pps.height_ctus) as usize;
        Self {
            fmt,
            width,
            height,
            bit_depth: sps.bit_depth,
            ctu_log2: sps.log2_ctu_size,
            width_ctus: pps.width_ctus,
            height_ctus: pps.height_ctus,
            planes,
            cus: Vec::new(),
            tus: Vec::new(),
            cu_map: [vec![NONE; map_w * map_h], vec![NONE; map_w * map_h]],
            map_w,
            map_h,
            ctus: vec![
                CtuData { slice: None, sao: [SaoParam::default(); 3], alf: AlfCtu::default(), num_cus: 0, num_tus: 0 };
                n
            ],
        }
    }

    /// Picture area of a channel in its own coordinates.
    pub fn chan_area(&self, ch: usize) -> Area {
        if ch == 0 {
            Area::new(0, 0, self.width, self.height)
        } else {
            Area::new(0, 0, self.width >> self.fmt.sx, self.height >> self.fmt.sy)
        }
    }

    /// vvdec's `getCU( pos, chType )`.
    pub fn get_cu(&self, x: i32, y: i32, ch: usize) -> Option<u32> {
        if !self.chan_area(ch).contains(x, y) {
            return None;
        }
        let (lx, ly) = if ch == 0 { (x, y) } else { (x << self.fmt.sx, y << self.fmt.sy) };
        let idx = self.cu_map[ch][(ly >> 2) as usize * self.map_w + (lx >> 2) as usize];
        if idx == NONE { None } else { Some(idx) }
    }

    pub fn ctu_addr_of(&self, x: i32, y: i32, ch: usize) -> u32 {
        let (lx, ly) = if ch == 0 { (x, y) } else { (x << self.fmt.sx, y << self.fmt.sy) };
        (ly >> self.ctu_log2) as u32 * self.width_ctus + (lx >> self.ctu_log2) as u32
    }

    /// Position-based `getCURestricted` used for neighbour derivation.
    pub fn get_cu_restricted_pos(
        &self,
        x: i32,
        y: i32,
        cur_x: i32,
        cur_y: i32,
        slice: u32,
        tile: u32,
        ch: usize,
        wpp: bool,
    ) -> Option<u32> {
        let (sx, sy) = self.fmt.scale(ch);
        let ys = self.ctu_log2 as i32 - sy as i32;
        let xs = self.ctu_log2 as i32 - sx as i32;
        let ydiff = (y >> ys) - (cur_y >> ys);
        let xdiff = (x >> xs) - (cur_x >> xs);
        if ydiff == 0 && xdiff == 0 {
            return self.get_cu(x, y, ch);
        }
        if ydiff > 0 || xdiff > 1 - i32::from(wpp) {
            return None;
        }
        let cu = self.get_cu(x, y, ch)?;
        let c = &self.cus[cu as usize];
        if c.slice == slice && c.tile == tile { Some(cu) } else { None }
    }

    /// CU-based `getCURestricted` with vvdec's `guess` shortcut.
    pub fn get_cu_restricted(&self, x: i32, y: i32, cur: u32, ch: usize, guess: Option<u32>, wpp: bool) -> Option<u32> {
        if let Some(g) = guess
            && self.cus[g as usize].blk[ch].contains(x, y)
        {
            return Some(g);
        }
        let c = &self.cus[cur as usize];
        let (sx, sy) = self.fmt.scale(ch);
        let ys = self.ctu_log2 as i32 - sy as i32;
        let xs = self.ctu_log2 as i32 - sx as i32;
        let ydiff = (y >> ys) - (c.blk[ch].y >> ys);
        let xdiff = (x >> xs) - (c.blk[ch].x >> xs);
        let same_ctu = ydiff == 0 && xdiff == 0;
        let found;
        if same_ctu {
            found = self.get_cu(x, y, ch);
        } else if ydiff > 0 || xdiff > 1 - i32::from(wpp) || (ydiff == 0 && xdiff > 0) {
            return None;
        } else {
            found = self.get_cu(x, y, ch);
        }
        let f = found?;
        let fc = &self.cus[f as usize];
        if same_ctu {
            // Same CTU: the map only holds CUs of this CTU; later CUs are excluded.
            if fc.ctu == c.ctu && fc.idx > c.idx {
                return None;
            }
            return Some(f);
        }
        if fc.slice == c.slice && fc.tile == c.tile { Some(f) } else { None }
    }

    /// The TU of `cu` covering a position (vvdec's `getTU`).
    pub fn get_tu(&self, cu: u32, x: i32, y: i32, ch: usize) -> u32 {
        let c = &self.cus[cu as usize];
        let mut t = c.first_tu;
        if c.num_tu <= 1 {
            return t;
        }
        let end = c.first_tu + c.num_tu;
        while t + 1 < end {
            let b = self.tus[t as usize].blk[ch];
            if b.x + b.w > x && b.y + b.h > y {
                break;
            }
            t += 1;
        }
        t
    }

    pub fn fill_map(&mut self, cu: u32) {
        let c = self.cus[cu as usize].clone();
        for ch in 0..if self.fmt.chroma == 0 { 1 } else { 2 } {
            let b = c.blk[if ch == 0 { 0 } else { 1 }];
            if !b.valid() {
                continue;
            }
            let (sx, sy) = self.fmt.scale(ch);
            let x0 = (b.x << sx) >> 2;
            let y0 = (b.y << sy) >> 2;
            let x1 = ((b.x + b.w) << sx) >> 2;
            let y1 = ((b.y + b.h) << sy) >> 2;
            for y in y0.max(0)..y1.min(self.map_h as i32) {
                for x in x0.max(0)..x1.min(self.map_w as i32) {
                    self.cu_map[ch][y as usize * self.map_w + x as usize] = cu;
                }
            }
        }
    }
}
