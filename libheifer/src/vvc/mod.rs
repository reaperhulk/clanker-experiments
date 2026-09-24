// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust VVC (H.266) still-picture decoder, written to reproduce the
//! output of vvdec 3.2.0 as used by libheif's vvdec plugin. Intra pictures
//! only; inter prediction reports [`Error::Unsupported`].
/// Emits vvdec-style `D_SYNTAX` lines on stderr when `VVC_TRACE` is set;
/// a development aid for diffing against vvdec's tracing build.
macro_rules! vtrace {
    ($($a:tt)*) => {
        if $crate::vvc::trace_enabled() {
            eprintln!($($a)*);
        }
    };
}

pub(crate) fn trace_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VVC_TRACE").is_some())
}

mod bits;
mod cabac;
mod ctu;
mod ctx;
mod deblock;
mod filter;
mod pic;
pub mod ps;
mod recon;
mod tables;

use bits::BitReader;
use pic::Picture;
use ps::{AlfParam, Aps, ApsData, PicHeader, Pps, SliceHeader, Sps};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A conformance or range check failed (vvdec throws and the plugin
    /// reports a decoding error).
    Invalid(&'static str),
    /// A feature vvdec supports that this decoder does not implement yet.
    Unsupported(&'static str),
    /// No picture was decoded.
    NoPicture,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// A decoded, conformance-window-cropped picture.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub chroma_format: u32,
    pub bit_depth: u32,
    /// Planes as (samples, width, height).
    pub planes: Vec<(Vec<u16>, usize, usize)>,
}

const NAL_IDR_W_RADL: u32 = 7;
const NAL_GDR: u32 = 10;
const NAL_VPS: u32 = 14;
const NAL_SPS: u32 = 15;
const NAL_PPS: u32 = 16;
const NAL_PREFIX_APS: u32 = 17;
const NAL_SUFFIX_APS: u32 = 18;
const NAL_PH: u32 = 19;

#[derive(Default)]
struct Decoder {
    sps: Vec<Option<Sps>>,
    pps: Vec<Option<Pps>>,
    /// [type][id]
    aps: Vec<Vec<Option<Aps>>>,
    ph: Option<PicHeader>,
    ph_pending: bool,
    pic: Option<Picture>,
    pic_sps: Option<Sps>,
    pic_pps: Option<Pps>,
    pic_ph: Option<PicHeader>,
    lmcs: Option<recon::Lmcs>,
    alf_for_pic: Option<[Option<AlfParam>; 8]>,
    slices: Vec<SliceHeader>,
    done: bool,
}

impl Decoder {
    fn new() -> Self {
        Self {
            sps: vec![None; 16],
            pps: vec![None; 64],
            aps: vec![vec![None; 8], vec![None; 8], vec![None; 8]],
            ..Default::default()
        }
    }

    fn push_nal(&mut self, nal: &[u8]) -> Result<(), Error> {
        if nal.len() < 2 || self.done {
            return Ok(());
        }
        if nal[0] & 0x80 != 0 {
            return Err(Error::Invalid("forbidden_zero_bit"));
        }
        let layer = u32::from(nal[0] & 0x3f);
        let kind = u32::from(nal[1] >> 3);
        let tid_plus1 = u32::from(nal[1] & 7);
        if tid_plus1 == 0 {
            return Err(Error::Invalid("nuh_temporal_id_plus1"));
        }
        if layer != 0 {
            // Only the base layer is decoded.
            return Ok(());
        }
        let (rbsp, removed) = bits::unescape(&nal[2..]);
        let mut r = BitReader::new(&rbsp);
        match kind {
            NAL_VPS => {}
            NAL_SPS => {
                let sps = ps::parse_sps(&mut r)?;
                let id = sps.id as usize;
                self.sps[id] = Some(sps);
            }
            NAL_PPS => {
                let pps = ps::parse_pps(&mut r, &self.sps)?;
                let id = pps.id as usize;
                self.pps[id] = Some(pps);
            }
            NAL_PREFIX_APS | NAL_SUFFIX_APS => {
                if let Some(aps) = ps::parse_aps(&mut r)? {
                    let (k, id) = (aps.kind as usize, aps.id as usize);
                    self.aps[k][id] = Some(aps);
                }
            }
            NAL_PH => {
                if !self.slices.is_empty() {
                    self.finish_picture()?;
                    self.done = true;
                    return Ok(());
                }
                self.ph = Some(ps::parse_picture_header(&mut r, &self.sps, &self.pps, true)?);
                self.ph_pending = true;
            }
            0..=3 | 7..=10 => self.decode_slice(kind, &rbsp, &removed, &mut r)?,
            _ => {}
        }
        Ok(())
    }

    fn decode_slice(&mut self, kind: u32, rbsp: &[u8], removed: &[usize], r: &mut BitReader) -> Result<(), Error> {
        let ph_in_sh = r.flag()?;
        if ph_in_sh {
            if !self.slices.is_empty() {
                self.finish_picture()?;
                self.done = true;
                return Ok(());
            }
            self.ph = Some(ps::parse_picture_header(r, &self.sps, &self.pps, false)?);
        } else if self.ph_pending && !self.slices.is_empty() {
            // unreachable: a PH NAL finishes the picture above
        }
        let ph = self.ph.clone().ok_or(Error::Invalid("Picture Header missing"))?;
        self.ph_pending = false;
        let pps = self.pps[ph.pps_id as usize].clone().ok_or(Error::Invalid("Invalid PPS"))?;
        let sps = self.sps[pps.sps_id as usize].clone().ok_or(Error::Invalid("Invalid SPS"))?;
        if self.pic.is_none() {
            // ALF APS as available for the picture.
            let mut alf: [Option<AlfParam>; 8] = Default::default();
            for (i, a) in self.aps[0].iter().enumerate() {
                if let Some(Aps { data: ApsData::Alf(p), .. }) = a {
                    alf[i] = Some(p.clone());
                }
            }
            self.alf_for_pic = Some(alf);
            self.lmcs = None;
            if ph.lmcs_enabled {
                match &self.aps[1][ph.lmcs_aps_id as usize] {
                    Some(Aps { data: ApsData::Lmcs(p), .. }) => {
                        self.lmcs = Some(recon::Lmcs::new(p, sps.bit_depth, ph.chroma_residual_scale)?);
                    }
                    _ => return Err(Error::Invalid("LMCS APS activation failed!")),
                }
            }
            if ph.explicit_scaling_list {
                return Err(Error::Unsupported("explicit scaling lists"));
            }
            self.pic = Some(Picture::new(&sps, &pps));
            self.pic_sps = Some(sps.clone());
            self.pic_pps = Some(pps.clone());
            self.pic_ph = Some(ph.clone());
        }
        let slice_idx = self.slices.len() as u32;
        let aps_ctx: Vec<Vec<Option<Aps>>> = self.aps.clone();
        let ctx = ps::SliceContext { sps: &sps, pps: &pps, ph: &ph, aps: &aps_ctx };
        let sh = ps::parse_slice_header(r, kind, &ctx, ph_in_sh, removed, 0)?;
        if sh.slice_type != ps::I_SLICE {
            return Err(Error::Unsupported("inter slices"));
        }
        let _ = (NAL_IDR_W_RADL, NAL_GDR);
        let alf = self.alf_for_pic.clone().unwrap_or_default();
        let si = ctu::SliceInfo {
            sps: &sps,
            pps: &pps,
            ph: &ph,
            sh: &sh,
            slice_idx,
            alf_aps: &alf,
            lmcs: self.lmcs.as_ref(),
            scaling: None,
        };
        let pic = self.pic.as_mut().ok_or(Error::NoPicture)?;
        decode_slice_data(pic, &si, &rbsp[sh.data_offset.min(rbsp.len())..])?;
        self.slices.push(sh);
        Ok(())
    }

    fn finish_picture(&mut self) -> Result<(), Error> {
        let pic = self.pic.as_mut().ok_or(Error::NoPicture)?;
        let sps = self.pic_sps.as_ref().unwrap();
        let pps = self.pic_pps.as_ref().unwrap();
        let ph = self.pic_ph.as_ref().unwrap();
        // Every CTU must have been decoded.
        if pic.ctus.iter().any(|c| c.slice.is_none()) {
            return Err(Error::Invalid("picture incomplete"));
        }
        let alf = self.alf_for_pic.clone().unwrap_or_default();
        filter::loop_filter(pic, sps, pps, ph, &self.slices, self.lmcs.as_ref(), &alf)?;
        Ok(())
    }

    fn output(&self) -> Result<Frame, Error> {
        let pic = self.pic.as_ref().ok_or(Error::NoPicture)?;
        let sps = self.pic_sps.as_ref().unwrap();
        let pps = self.pic_pps.as_ref().unwrap();
        let win = if pps.conf_win_present { &pps.conf_win } else { &sps.conf_win };
        let (ux, uy) = (sps.sub_width_c(), sps.sub_height_c());
        let left = win.left * ux;
        let right = win.right * ux;
        let top = win.top * uy;
        let bottom = win.bottom * uy;
        let w = (pic.width as u32).checked_sub(left + right).ok_or(Error::Invalid("conformance window"))?;
        let h = (pic.height as u32).checked_sub(top + bottom).ok_or(Error::Invalid("conformance window"))?;
        if w == 0 || h == 0 {
            return Err(Error::Invalid("empty output picture"));
        }
        let mut planes = Vec::new();
        for (c, plane) in pic.planes.iter().enumerate() {
            let (sx, sy) = pic.fmt.scale(c);
            let pw = plane.width - ((left + right) >> sx) as usize;
            let phh = plane.height - ((top + bottom) >> sy) as usize;
            let (x0, y0) = ((left >> sx) as usize, (top >> sy) as usize);
            let mut data = Vec::with_capacity(pw * phh);
            for y in 0..phh {
                let row = &plane.data[(y0 + y) * plane.stride + x0..(y0 + y) * plane.stride + x0 + pw];
                data.extend(row.iter().map(|&v| v as u16));
            }
            planes.push((data, pw, phh));
        }
        Ok(Frame { width: w, height: h, chroma_format: sps.chroma_format_idc, bit_depth: sps.bit_depth, planes })
    }
}

fn decode_slice_data<'d>(pic: &mut Picture, si: &ctu::SliceInfo, data: &'d [u8]) -> Result<(), Error> {
    let sps = si.sps;
    let pps = si.pps;
    let sh = si.sh;
    // substreams
    let mut subs: Vec<&'d [u8]> = Vec::new();
    let mut pos = 0usize;
    for &size in &sh.entry_points {
        let end = pos.checked_add(size as usize).filter(|&e| e <= data.len()).ok_or(Error::Invalid("Exceeded FIFO size"))?;
        subs.push(&data[pos..end]);
        pos = end;
    }
    subs.push(&data[pos..]);
    let init_type = match sh.slice_type {
        ps::I_SLICE => 2,
        ps::P_SLICE => 1,
        _ => 0,
    };
    let ctx0 = cabac::Contexts::new(init_type, sh.qp);
    let mut dec = ctu::CtuDecoder {
        cabac: cabac::Cabac::new(subs[0], ctx0.clone()),
        si,
        pic,
        chroma_qp_adj: 0,
        ctu_addr: 0,
        tile: 0,
        wpp: sps.entropy_coding_sync,
    };
    let mut prev_qp = [sh.qp, sh.qp];
    let mut sub_id = 0usize;
    let mut cur_sub = 0usize;
    let mut wpp_ctx: Option<cabac::Contexts> = None;
    let wpp = sps.entropy_coding_sync;
    let ctu_size = sps.ctu_size as i32;
    for (i, &addr) in sh.ctus.iter().enumerate() {
        let (cx, cy) = (addr % pps.width_ctus, addr / pps.width_ctus);
        let tcol = pps.ctu_to_tile_col[cx as usize] as usize;
        let trow = pps.ctu_to_tile_row[cy as usize] as usize;
        let (tx, ty) = (pps.tile_col_bd[tcol], pps.tile_row_bd[trow]);
        let tw = pps.tile_col_bd[tcol + 1] - tx;
        let th = pps.tile_row_bd[trow + 1] - ty;
        let tile_idx = trow as u32 * pps.num_tile_cols() + tcol as u32;
        if cx > 0 && dec.pic.ctus[addr as usize - 1].slice.is_none() && sh.ctus.first() != Some(&addr) && !pps.rect_slice {
            // vvdec requires the left CTU to be parsed; with ordered slices
            // this always holds.
        }
        let restart = |dec: &mut ctu::CtuDecoder<'_, '_, 'd>, sub_id: usize, cur_sub: &mut usize| {
            dec.cabac.ctx = cabac::Contexts::new(init_type, sh.qp);
            if sub_id != *cur_sub {
                dec.cabac.restart(subs[sub_id]);
                *cur_sub = sub_id;
            } else {
                dec.cabac.restart_here();
            }
        };
        if cx == tx && cy == ty {
            if i != 0 {
                restart(&mut dec, sub_id, &mut cur_sub);
            }
            prev_qp = [sh.qp, sh.qp];
        } else if cx == tx && wpp {
            if i != 0 {
                restart(&mut dec, sub_id, &mut cur_sub);
            }
            let (px, py) = (cx as i32 * ctu_size, cy as i32 * ctu_size);
            if dec.pic.get_cu_restricted_pos(px, py - 1, px, py, si.slice_idx, tile_idx, 0, wpp).is_some()
                && let Some(c) = &wpp_ctx
            {
                dec.cabac.ctx = c.clone();
            }
            prev_qp = [sh.qp, sh.qp];
        }
        dec.pic.ctus[addr as usize].slice = Some(si.slice_idx);
        dec.ctu_addr = addr;
        dec.tile = tile_idx;
        let area = dec.pic.fmt.unit(cx as i32 * ctu_size, cy as i32 * ctu_size, ctu_size, ctu_size);
        dec.coding_tree_unit(area, &mut prev_qp)?;
        if cx == tx && wpp {
            wpp_ctx = Some(dec.cabac.ctx.clone());
        }
        if i == sh.ctus.len() - 1 {
            if dec.cabac.decode_terminate() == 0 {
                return Err(Error::Invalid("Expecting a terminating bit"));
            }
            if !dec.cabac.finish_ok() {
                return Err(Error::Invalid("No proper stop/alignment pattern at end of CABAC stream."));
            }
        } else if cx + 1 == tx + tw && (cy + 1 == ty + th || wpp) {
            if dec.cabac.decode_terminate() == 0 {
                return Err(Error::Invalid("Expecting a terminating bit"));
            }
            if !dec.cabac.finish_ok() {
                return Err(Error::Invalid("No proper stop/alignment pattern at end of CABAC stream."));
            }
            if sps.entry_points_present {
                sub_id += 1;
                if sub_id >= subs.len() {
                    return Err(Error::Invalid("missing substream"));
                }
            }
        }
    }
    Ok(())
}

/// Decodes the first picture of a sequence of NAL units (each without start
/// code or length prefix).
pub fn decode_nals<'a>(nals: impl IntoIterator<Item = &'a [u8]>) -> Result<Frame, Error> {
    let mut d = Decoder::new();
    for nal in nals {
        if let Err(e) = d.push_nal(nal) {
            if std::env::var_os("VVC_DEBUG").is_some() {
                eprintln!("NAL type {} failed: {e:?}", nal.get(1).map_or(0, |b| b >> 3));
            }
            return Err(e);
        }
    }
    if !d.done {
        if d.slices.is_empty() {
            return Err(Error::NoPicture);
        }
        d.finish_picture()?;
    }
    d.output()
}

/// Splits an Annex B byte stream into NAL units.
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0;
    let n = data.len();
    let mut start = None;
    while i + 3 <= n {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if let Some(s) = start {
                let mut e = i;
                while e > s && data[e - 1] == 0 {
                    e -= 1;
                }
                out.push(&data[s..e]);
            }
            i += 3;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        out.push(&data[s..]);
    }
    out
}
