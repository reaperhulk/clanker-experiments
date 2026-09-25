// SPDX-License-Identifier: LGPL-3.0-or-later
//! Pure Rust VVC (H.266) decoder, written to reproduce the output of vvdec
//! 3.2.0 as used by libheif's vvdec plugin. Reference picture resampling
//! reports [`Error::Unsupported`]; palette mode is rejected as vvdec does.
//!
//! The decoding processes and tables are translated from vvdec 3.2.0,
//! Copyright (c) 2018-2026 Fraunhofer-Gesellschaft zur Förderung der
//! angewandten Forschung e.V. & The VVdeC Authors, under the Clear BSD
//! License retained in licenses/vvdec.txt.
// The codec modules port vvdec's routines and keep its index loops and
// parameter lists so they stay easy to compare.
#![allow(
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::type_complexity
)]
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

mod alf;
mod bits;
mod cabac;
mod ctu;
mod ctx;
mod deblock;
mod dpb;
pub mod encoder;
mod filter;
pub mod heif;
mod mc;
mod mv;
mod mvpred;
mod pic;
pub mod ps;
mod recon;
mod sao;
mod tables;
mod tables_inter;

use bits::BitReader;
use dpb::{RefPic, SliceRefs};
use mv::MotionInfo;
use pic::Picture;
use ps::{AlfParam, Aps, ApsData, PicHeader, Pps, SliceHeader, Sps};
use std::sync::Arc;

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
    /// The time stamp given with the picture's last slice (vvdec's `cts`).
    pub user_data: u64,
}

const NAL_IDR_W_RADL: u32 = 7;
const NAL_GDR: u32 = 10;
const NAL_VPS: u32 = 14;
const NAL_SPS: u32 = 15;
const NAL_PPS: u32 = 16;
const NAL_PREFIX_APS: u32 = 17;
const NAL_SUFFIX_APS: u32 = 18;
const NAL_PH: u32 = 19;

const NAL_RASL: u32 = 3;
const NAL_RADL: u32 = 2;
const NAL_IDR_N_LP: u32 = 8;
const NAL_CRA: u32 = 9;
const NAL_EOS: u32 = 21;
const NAL_INVALID: u32 = 99;

/// Reference marking (vvdec's `Picture::dpbReferenceMark`).
const UNREFERENCED: u8 = 0;
const SHORT_TERM: u8 = 1;
const LONG_TERM: u8 = 2;

/// A picture being decoded.
struct CurPic {
    pic: Picture,
    sps: Sps,
    pps: Pps,
    ph: PicHeader,
    lmcs: Option<recon::Lmcs>,
    scaling: Option<recon::ScalingMatrices>,
    alf: [Option<AlfParam>; 8],
    slices: Vec<SliceHeader>,
    nal_type: u32,
    tlayer: u32,
    id: u64,
    needed_for_output: bool,
    cts: u64,
}

/// A decoded picture held for reference or output.
struct DpbEntry {
    pic: Arc<RefPic>,
    mark: u8,
    needed_for_output: bool,
    idr: bool,
    tlayer: u32,
    non_ref: bool,
    /// Conformance window in luma samples: left, right, top, bottom.
    crop: [u32; 4],
    cts: u64,
}

/// A stateful decoder for a sequence of NAL units.
pub struct Decoder {
    sps: Vec<Option<Sps>>,
    pps: Vec<Option<Pps>>,
    /// [type][id]
    aps: Vec<Vec<Option<Aps>>>,
    ph: Option<PicHeader>,
    cur: Option<CurPic>,
    dpb: Vec<DpbEntry>,
    output: std::collections::VecDeque<Frame>,
    next_id: u64,
    prev_tid0_poc: i32,
    poc_cra: i32,
    associated_irap: u32,
    first_slice_in_sequence: bool,
    last_no_output_before_recovery: bool,
    no_output_before_recovery: bool,
    poc_random_access: i32,
    prev_poc: i32,
    no_output_prior_pics: Option<i32>,
    gdr_recovery_poc: Option<i32>,
    gdr_recovered: bool,
    nal_cts: u64,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            sps: vec![None; 16],
            pps: vec![None; 64],
            aps: vec![vec![None; 8], vec![None; 8], vec![None; 8]],
            associated_irap: NAL_INVALID,
            first_slice_in_sequence: true,
            poc_random_access: i32::MAX,
            prev_poc: i32::MAX,
            ph: None,
            cur: None,
            dpb: Vec::new(),
            output: std::collections::VecDeque::new(),
            next_id: 0,
            prev_tid0_poc: 0,
            poc_cra: 0,
            last_no_output_before_recovery: false,
            no_output_before_recovery: false,
            no_output_prior_pics: None,
            gdr_recovery_poc: None,
            gdr_recovered: false,
            nal_cts: 0,
        }
    }

    /// `push_nal` with a time stamp that pictures take from their slices.
    pub fn push_nal_with(&mut self, nal: &[u8], cts: u64) -> Result<(), Error> {
        self.nal_cts = cts;
        self.push_nal(nal)
    }

    /// Decodes one NAL unit (without start code or length prefix). After an
    /// error the partially decoded picture is dropped.
    pub fn push_nal(&mut self, nal: &[u8]) -> Result<(), Error> {
        let result = self.decode_nal(nal);
        if result.is_err() {
            self.cur = None;
        }
        result
    }

    fn decode_nal(&mut self, nal: &[u8]) -> Result<(), Error> {
        if nal.len() < 2 {
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
                self.ph = Some(ps::parse_picture_header(
                    &mut r, &self.sps, &self.pps, true,
                )?);
            }
            NAL_EOS => {
                if self.cur.is_some() {
                    self.finish_picture()?;
                }
                self.associated_irap = NAL_INVALID;
                self.poc_cra = 0;
                self.gdr_recovery_poc = None;
                self.gdr_recovered = false;
                self.poc_random_access = i32::MAX;
                self.prev_poc = i32::MAX;
                self.first_slice_in_sequence = true;
            }
            0..=3 | 7..=10 => self.decode_slice(kind, tid_plus1 - 1, &rbsp, &removed, &mut r)?,
            _ => {}
        }
        Ok(())
    }

    /// vvdec's `xUpdatePreviousTid0POC`.
    fn update_prev_tid0(&mut self, tlayer: u32, nal_type: u32, non_ref: bool, poc: i32) {
        if tlayer == 0 && nal_type != NAL_RASL && nal_type != NAL_RADL && !non_ref {
            self.prev_tid0_poc = poc;
        }
    }

    /// vvdec's `isRandomAccessSkipPicture`.
    fn random_access_skip(&mut self, nal_type: u32, poc: i32) -> bool {
        if nal_type == NAL_IDR_W_RADL || nal_type == NAL_IDR_N_LP {
            self.poc_random_access = -i32::MAX;
        } else if self.poc_random_access == i32::MAX {
            if nal_type == NAL_CRA || nal_type == NAL_GDR {
                self.poc_random_access = poc;
            } else {
                return true;
            }
        } else if poc < self.poc_random_access && nal_type == NAL_RASL {
            return true;
        }
        false
    }

    fn decode_slice(
        &mut self,
        kind: u32,
        tlayer: u32,
        rbsp: &[u8],
        removed: &[usize],
        r: &mut BitReader,
    ) -> Result<(), Error> {
        let ph_in_sh = r.flag()?;
        if ph_in_sh {
            self.ph = Some(ps::parse_picture_header(r, &self.sps, &self.pps, false)?);
        }
        let mut ph = self
            .ph
            .clone()
            .ok_or(Error::Invalid("Picture Header missing"))?;
        let pps = self.pps[ph.pps_id as usize]
            .clone()
            .ok_or(Error::Invalid("Invalid PPS"))?;
        let sps = self.sps[pps.sps_id as usize]
            .clone()
            .ok_or(Error::Invalid("Invalid SPS"))?;
        let aps_ctx: Vec<Vec<Option<Aps>>> = self.aps.clone();
        let ctx = ps::SliceContext {
            sps: &sps,
            pps: &pps,
            ph: &ph,
            aps: &aps_ctx,
        };
        let sh = ps::parse_slice_header(r, kind, &ctx, ph_in_sh, removed, 0)?;
        let first_in_pic = sh.ctus.first() == Some(&0);
        if first_in_pic && self.cur.is_some() {
            self.finish_picture()?;
        }
        // picture order count (vvdec's parseSliceHeader)
        let max_lsb = 1i32 << sps.bits_for_poc;
        let idr = kind == NAL_IDR_W_RADL || kind == NAL_IDR_N_LP;
        let msb = if ph.poc_msb_present {
            (ph.poc_msb_val as i32).wrapping_mul(max_lsb)
        } else if idr {
            0
        } else {
            let lsb = ph.poc_lsb as i32;
            let prev_lsb = self.prev_tid0_poc & (max_lsb - 1);
            let prev_msb = self.prev_tid0_poc - prev_lsb;
            if lsb < prev_lsb && prev_lsb - lsb >= max_lsb / 2 {
                prev_msb + max_lsb
            } else if lsb > prev_lsb && lsb - prev_lsb > max_lsb / 2 {
                prev_msb - max_lsb
            } else {
                prev_msb
            }
        };
        let mut poc = msb.wrapping_add(ph.poc_lsb as i32);
        let irap = (NAL_IDR_W_RADL..=NAL_CRA).contains(&kind);
        let cra_or_gdr = kind == NAL_CRA || kind == NAL_GDR;
        if first_in_pic && !pps.mixed_nalu_types && (irap || kind == NAL_GDR) {
            self.poc_cra = poc;
            self.associated_irap = kind;
        }
        self.update_prev_tid0(tlayer, kind, ph.non_ref, poc);
        if irap || kind == NAL_GDR {
            self.no_output_before_recovery =
                !pps.mixed_nalu_types && (self.first_slice_in_sequence || idr);
            if cra_or_gdr {
                self.last_no_output_before_recovery = self.no_output_before_recovery;
            }
            if sh.no_output_of_prior_pics {
                self.no_output_prior_pics = Some(poc);
            } else {
                self.no_output_prior_pics = None;
            }
        }
        if first_in_pic
            && poc != self.prev_poc
            && (irap || kind == NAL_GDR)
            && self.no_output_before_recovery
            && let Some(limit) = self.no_output_prior_pics.take()
        {
            // checkNoOutputPriorPics
            for e in &mut self.dpb {
                if e.mark != UNREFERENCED && e.pic.poc < limit {
                    e.needed_for_output = false;
                }
            }
        }
        if !pps.mixed_nalu_types && kind == NAL_RASL && self.last_no_output_before_recovery {
            ph.pic_output_flag = false;
        }
        if !pps.mixed_nalu_types && cra_or_gdr && self.last_no_output_before_recovery {
            poc &= max_lsb - 1;
            self.update_prev_tid0(tlayer, kind, ph.non_ref, poc);
            if self.no_output_prior_pics.is_some() {
                self.no_output_prior_pics = Some(poc);
            }
        }
        if self.random_access_skip(kind, poc) {
            return Ok(());
        }
        if !first_in_pic && self.cur.is_none() {
            return Err(Error::Invalid(
                "m_pcParsePic should be initialized, when this is not the first slice in the picture",
            ));
        }
        // lost or unavailable references
        if sh.slice_type != ps::I_SLICE {
            for l in 0..2 {
                while let Some((missing, lt)) = self.missing_reference(&sh, l, poc, &sps) {
                    let unavailable = !pps.mixed_nalu_types
                        && ((idr && (sps.idr_rpl_present || pps.rpl_info_in_ph))
                            || (cra_or_gdr && self.no_output_before_recovery));
                    if !unavailable {
                        return Err(Error::Invalid("missing reference picture"));
                    }
                    self.insert_unavailable(missing, lt, tlayer, &sps, &pps);
                }
            }
        }
        if first_in_pic {
            self.mark_references(&sh, poc, &sps, &pps);
            self.start_picture(&sps, &pps, &ph, kind, tlayer, poc)?;
        }
        let (refs, slice_refs) = self.construct_ref_lists(&sh, poc, tlayer, &sps)?;
        let inter = self.slice_inter(&sh, &sps, &ph, &slice_refs, refs)?;
        let cur = self.cur.as_mut().ok_or(Error::NoPicture)?;
        cur.cts = self.nal_cts;
        let slice_idx = cur.slices.len() as u32;
        cur.pic.slice_refs.push(slice_refs);
        let si = ctu::SliceInfo {
            sps: &cur.sps,
            pps: &cur.pps,
            ph: &cur.ph,
            sh: &sh,
            slice_idx,
            alf_aps: &cur.alf,
            lmcs: cur.lmcs.as_ref(),
            scaling: cur.scaling.as_ref(),
            inter: &inter,
        };
        decode_slice_data(&mut cur.pic, &si, &rbsp[sh.data_offset.min(rbsp.len())..])?;
        cur.slices.push(sh);
        self.prev_poc = poc;
        self.first_slice_in_sequence = false;
        let complete = self
            .cur
            .as_ref()
            .is_some_and(|c| c.pic.ctus.iter().all(|t| t.slice.is_some()));
        if complete {
            self.finish_picture()?;
        }
        Ok(())
    }

    /// vvdec's `checkThatAllRefPicsAreAvailable` for one list: the first
    /// missing POC and whether its entry is long-term.
    fn missing_reference(
        &mut self,
        sh: &SliceHeader,
        l: usize,
        poc: i32,
        sps: &Sps,
    ) -> Option<(i32, bool)> {
        if sh.nal_type == NAL_IDR_W_RADL || sh.nal_type == NAL_IDR_N_LP {
            return None;
        }
        let rpl = &sh.rpl[l];
        let active = (sh.num_ref_idx[l] as usize).min(rpl.entries.len());
        let bits = sps.bits_for_poc;
        let has_lt = rpl.entries.iter().any(|e| e.lt && !e.ilrp);
        for ii in 0..active {
            if !has_lt {
                break;
            }
            let e = rpl.entries[ii];
            if !e.lt {
                continue;
            }
            let ref_poc = rpl.lt_ref_poc(ii, poc, bits);
            if self.dpb.iter().any(|p| {
                p.mark == LONG_TERM && ps::lt_poc_equal(p.pic.poc, ref_poc, bits, e.msb_present)
            }) {
                continue;
            }
            if let Some(p) = self.dpb.iter_mut().find(|p| {
                p.mark == SHORT_TERM && ps::lt_poc_equal(p.pic.poc, ref_poc, bits, e.msb_present)
            }) {
                p.mark = LONG_TERM;
                continue;
            }
            return Some((e.id, true));
        }
        let has_st = rpl.entries.iter().any(|e| !e.lt);
        for ii in 0..active {
            let e = rpl.entries[ii];
            if e.lt {
                continue;
            }
            let check = poc.wrapping_add(e.id);
            if !self
                .dpb
                .iter()
                .any(|p| p.pic.poc == check && p.mark != UNREFERENCED)
                && has_st
            {
                return Some((check, false));
            }
        }
        None
    }

    /// vvdec's `prepareUnavailablePicture` with a grey picture.
    fn insert_unavailable(&mut self, poc: i32, lt: bool, tlayer: u32, sps: &Sps, pps: &Pps) {
        self.next_id += 1;
        let fmt = pic::Format::new(sps.chroma_format_idc);
        let mut pic = RefPic::grey(
            self.next_id,
            poc,
            fmt,
            pps.width as i32,
            pps.height as i32,
            sps.bit_depth,
            sps.log2_ctu_size,
        );
        pic.scaling_win = pps.scaling_win;
        pic.collocated = (sps.chroma_hor_collocated, sps.chroma_ver_collocated);
        self.dpb.push(DpbEntry {
            pic: Arc::new(pic),
            mark: if lt { LONG_TERM } else { SHORT_TERM },
            needed_for_output: false,
            idr: false,
            tlayer,
            non_ref: false,
            crop: [0; 4],
            cts: 0,
        });
        if tlayer == 0 {
            self.prev_tid0_poc = poc;
        }
        if self.poc_random_access == i32::MAX {
            self.poc_random_access = poc;
        }
    }

    /// vvdec's `applyReferencePictureListBasedMarking`.
    fn mark_references(&mut self, sh: &SliceHeader, poc: i32, sps: &Sps, pps: &Pps) {
        let bits = sps.bits_for_poc;
        for l in 0..2 {
            let rpl = &sh.rpl[l];
            for i in 0..rpl.entries.len() {
                let e = rpl.entries[i];
                if !e.lt || e.ilrp {
                    continue;
                }
                let lt_poc = rpl.lt_ref_poc(i, poc, bits);
                let mut available_st = None;
                for (k, p) in self.dpb.iter().enumerate() {
                    if p.mark == UNREFERENCED {
                        continue;
                    }
                    let equal = ps::lt_poc_equal(p.pic.poc, lt_poc, bits, e.msb_present);
                    if p.mark == LONG_TERM && equal {
                        break;
                    }
                    if p.mark == SHORT_TERM && equal {
                        available_st = Some(k);
                    }
                }
                if let Some(k) = available_st {
                    self.dpb[k].mark = LONG_TERM;
                }
            }
        }
        let idr = sh.nal_type == NAL_IDR_W_RADL || sh.nal_type == NAL_IDR_N_LP;
        if idr && !pps.mixed_nalu_types {
            for p in &mut self.dpb {
                p.mark = UNREFERENCED;
            }
        } else {
            for k in 0..self.dpb.len() {
                let p = &self.dpb[k];
                if p.mark == UNREFERENCED {
                    continue;
                }
                let mut is_ref = false;
                for l in 0..2 {
                    let rpl = &sh.rpl[l];
                    for i in 0..rpl.entries.len() {
                        let e = rpl.entries[i];
                        if e.ilrp {
                            continue;
                        }
                        if e.lt {
                            let lt_poc = rpl.lt_ref_poc(i, poc, bits);
                            if p.mark == LONG_TERM
                                && ps::lt_poc_equal(p.pic.poc, lt_poc, bits, e.msb_present)
                            {
                                is_ref = true;
                            }
                        } else if p.pic.poc == poc.wrapping_add(e.id) {
                            is_ref = true;
                        }
                    }
                }
                if !is_ref && p.pic.poc != poc {
                    self.dpb[k].mark = UNREFERENCED;
                }
            }
        }
        self.dpb
            .retain(|p| p.mark != UNREFERENCED || p.needed_for_output);
    }

    fn start_picture(
        &mut self,
        sps: &Sps,
        pps: &Pps,
        ph: &PicHeader,
        kind: u32,
        tlayer: u32,
        poc: i32,
    ) -> Result<(), Error> {
        // ALF APS as available for the picture.
        let mut alf: [Option<AlfParam>; 8] = Default::default();
        for (i, a) in self.aps[0].iter().enumerate() {
            if let Some(Aps {
                data: ApsData::Alf(p),
                ..
            }) = a
            {
                alf[i] = Some(p.clone());
            }
        }
        let mut lmcs = None;
        if ph.lmcs_enabled {
            match &self.aps[1][ph.lmcs_aps_id as usize] {
                Some(Aps {
                    data: ApsData::Lmcs(p),
                    ..
                }) => {
                    lmcs = Some(recon::Lmcs::new(p, sps.bit_depth)?);
                }
                _ => return Err(Error::Invalid("LMCS APS activation failed!")),
            }
        }
        let mut scaling = None;
        if ph.explicit_scaling_list {
            match &self.aps[2][ph.scaling_list_aps_id as usize] {
                Some(Aps {
                    data: ApsData::Scaling(l),
                    ..
                }) => scaling = Some(recon::ScalingMatrices::new(l)),
                _ => return Err(Error::Invalid("scaling list APS not found")),
            }
        }
        let mut needed = ph.pic_output_flag;
        // GDR and RASL output rules of xDecodeSliceHead
        if !self.gdr_recovered && kind == NAL_GDR && self.gdr_recovery_poc.is_none() {
            self.gdr_recovery_poc = Some(poc + ph.recovery_poc_cnt);
        }
        if !self.gdr_recovered && (self.gdr_recovery_poc == Some(poc) || ph.recovery_poc_cnt == 0) {
            self.gdr_recovered = true;
            self.gdr_recovery_poc = None;
        }
        let recovering =
            self.associated_irap == NAL_GDR && self.gdr_recovery_poc.is_some_and(|p| poc < p);
        if kind == NAL_GDR && self.gdr_recovered {
            needed = true;
        } else if (kind == NAL_RASL && self.last_no_output_before_recovery)
            || (kind == NAL_GDR && self.no_output_before_recovery)
            || (recovering && (!self.gdr_recovered || self.last_no_output_before_recovery))
        {
            needed = false;
        }
        self.next_id += 1;
        let mut pic = Picture::new(sps, pps);
        pic.poc = poc;
        self.cur = Some(CurPic {
            pic,
            sps: sps.clone(),
            pps: pps.clone(),
            ph: ph.clone(),
            lmcs,
            scaling,
            alf,
            slices: Vec::new(),
            nal_type: kind,
            tlayer,
            id: self.next_id,
            needed_for_output: needed,
            cts: 0,
        });
        Ok(())
    }

    /// vvdec's `Slice::constructRefPicLists`.
    fn construct_ref_lists(
        &mut self,
        sh: &SliceHeader,
        poc: i32,
        tlayer: u32,
        sps: &Sps,
    ) -> Result<([Vec<Arc<RefPic>>; 2], SliceRefs), Error> {
        let mut refs: [Vec<Arc<RefPic>>; 2] = Default::default();
        let mut info = SliceRefs {
            slice_type: sh.slice_type,
            poc,
            ..Default::default()
        };
        if sh.slice_type == ps::I_SLICE {
            return Ok((refs, info));
        }
        let bits = sps.bits_for_poc;
        for l in 0..2 {
            let rpl = &sh.rpl[l];
            let active = sh.num_ref_idx[l] as usize;
            if rpl.entries.len() < active {
                return Err(Error::Invalid(
                    "For each i equal to 0 or 1, num_ref_entries[ i ][ RplsIdx[ i ] ] shall not be less than NumRefIdxActive[ i ].",
                ));
            }
            for ii in 0..rpl.entries.len() {
                let e = rpl.entries[ii];
                let (k, ref_poc) = if !e.lt {
                    let ref_poc = poc.wrapping_add(e.id);
                    if ref_poc == poc {
                        return Err(Error::Invalid(
                            "An STRP entry must not refer to a picture with the current POC",
                        ));
                    }
                    let k = self
                        .dpb
                        .iter()
                        .position(|p| p.pic.poc == ref_poc && p.mark != UNREFERENCED)
                        .ok_or(Error::Invalid("Picture pointer missing from ref pic list"))?;
                    self.dpb[k].mark = SHORT_TERM;
                    (k, ref_poc)
                } else {
                    let mut ref_poc = rpl.lt_ref_poc(ii, poc, bits);
                    let k = self
                        .dpb
                        .iter()
                        .position(|p| {
                            p.pic.poc != poc
                                && p.mark != UNREFERENCED
                                && ps::lt_poc_equal(ref_poc, p.pic.poc, bits, e.msb_present)
                        })
                        .ok_or(Error::Invalid("Picture pointer missing from ref pic list"))?;
                    if !e.msb_present {
                        ref_poc = self.dpb[k].pic.poc;
                    }
                    if poc.wrapping_sub(ref_poc) >= 1 << 24 {
                        return Err(Error::Invalid("LTRP POC difference"));
                    }
                    self.dpb[k].mark = LONG_TERM;
                    (k, ref_poc)
                };
                let p = &self.dpb[k];
                if p.pic.poc != ref_poc {
                    return Err(Error::Invalid("reference picture as wrong POC"));
                }
                if p.pic.fmt.chroma != sps.chroma_format_idc {
                    return Err(Error::Invalid("reference picture has wrong chroma format"));
                }
                if ii < active {
                    if p.tlayer > tlayer {
                        return Err(Error::Invalid("reference picture temporal id"));
                    }
                    if p.non_ref {
                        return Err(Error::Invalid(
                            "reference picture is a non-reference picture",
                        ));
                    }
                    let lt = p.mark == LONG_TERM;
                    for j in 0..refs[l].len() {
                        if refs[l][j].id == p.pic.id && info.ref_lt[l][j] != lt {
                            return Err(Error::Invalid(
                                "STRP and LTRP entries refer to the same picture",
                            ));
                        }
                    }
                    refs[l].push(p.pic.clone());
                    info.ref_poc[l].push(ref_poc);
                    info.ref_lt[l].push(lt);
                    info.ref_id[l].push(p.pic.id);
                }
            }
        }
        Ok((refs, info))
    }

    /// Slice-level inter state: low-delay check and symmetric MVD references.
    fn slice_inter(
        &self,
        sh: &SliceHeader,
        sps: &Sps,
        ph: &PicHeader,
        info: &SliceRefs,
        refs: [Vec<Arc<RefPic>>; 2],
    ) -> Result<ctu::SliceInter, Error> {
        let cur = self.cur.as_ref().ok_or(Error::NoPicture)?;
        let poc = info.poc;
        if sh.slice_type != ps::I_SLICE && sps.wraparound && !cur.pps.wraparound {
            // vvdec would read the never-extended wraparound buffer margins
            return Err(Error::Unsupported("SPS wraparound without PPS wraparound"));
        }
        let mut check_ldc = true;
        if sh.slice_type != ps::I_SLICE {
            let lists = if sh.slice_type == ps::B_SLICE { 2 } else { 1 };
            for l in 0..lists {
                if info.ref_poc[l].iter().any(|&p| p > poc) {
                    check_ldc = false;
                }
            }
        } else {
            check_ldc = false;
        }
        let mut sym = [-1i8; 2];
        let mut bidir = false;
        if sps.smvd && !check_ldc && !ph.mvd_l1_zero && sh.slice_type == ps::B_SLICE {
            let search = |l: usize, forward: bool| -> (i32, i8) {
                let mut best = poc;
                let mut idx = -1i8;
                for (r, &p) in info.ref_poc[l].iter().enumerate() {
                    if info.ref_lt[l][r] {
                        continue;
                    }
                    let ok = if forward {
                        p < poc && (p > best || idx == -1)
                    } else {
                        p > poc && (p < best || idx == -1)
                    };
                    if ok {
                        best = p;
                        idx = r as i8;
                    }
                }
                (best, idx)
            };
            let (mut fwd, mut i0) = search(0, true);
            let (mut bwd, mut i1) = search(1, false);
            if !(fwd < poc && bwd > poc) {
                (bwd, i0) = search(0, false);
                (fwd, i1) = search(1, true);
            }
            if fwd < poc && bwd > poc {
                bidir = true;
                sym = [i0, i1];
            }
        }
        let col = if ph.temporal_mvp && sh.slice_type != ps::I_SLICE {
            let l = if sh.slice_type == ps::B_SLICE {
                usize::from(!sh.col_from_l0)
            } else {
                0
            };
            refs[l].get(sh.col_ref_idx as usize).cloned()
        } else {
            None
        };
        Ok(ctu::SliceInter {
            refs,
            info: info.clone(),
            col,
            check_ldc,
            bidir,
            sym_ref: sym,
        })
    }

    fn finish_picture(&mut self) -> Result<(), Error> {
        let Some(mut cur) = self.cur.take() else {
            return Ok(());
        };
        // Every CTU must have been decoded.
        if cur.pic.ctus.iter().any(|c| c.slice.is_none()) {
            return Err(Error::Invalid("picture incomplete"));
        }
        filter::loop_filter(
            &mut cur.pic,
            &cur.sps,
            &cur.pps,
            &cur.ph,
            &cur.slices,
            cur.lmcs.as_ref(),
            &cur.alf,
        )?;
        let pic = &mut cur.pic;
        // Collocated motion: every second 4x4 unit in both directions, with
        // DMVR refinements (DecCu::TaskFinishMotionInfo); intra slices keep
        // invalid motion.
        let col_w = (pic.width as usize).div_ceil(8);
        let col_h = (pic.height as usize).div_ceil(8);
        let mut col = vec![MotionInfo::default(); col_w * col_h];
        if !cur.ph.non_ref {
            for y in 0..col_h {
                for x in 0..col_w {
                    let (lx, ly) = (x as i32 * 8, y as i32 * 8);
                    let ctu = pic.ctu_addr_of(lx, ly, 0) as usize;
                    let s = pic.ctus[ctu].slice.unwrap_or(0) as usize;
                    if cur.slices[s].slice_type != ps::I_SLICE {
                        col[y * col_w + x] = pic.mi(lx, ly);
                    }
                }
            }
            for d in &pic.dmvr {
                let dy = d.h.min(16);
                let dx = d.w.min(16);
                let mut num = 0;
                let mut y = d.y;
                while y < d.y + d.h {
                    let mut x = d.x;
                    while x < d.x + d.w {
                        let delta = d.deltas[num];
                        let mv0 = d.mv[0].add(delta);
                        let mv1 = d.mv[1].sub(delta);
                        let mut y2 = ((y - 1) & !7) + 8;
                        while y2 < y + dy {
                            let mut x2 = ((x - 1) & !7) + 8;
                            while x2 < x + dx {
                                let mi = &mut col[(y2 >> 3) as usize * col_w + (x2 >> 3) as usize];
                                mi.mv[0] = mv0;
                                mi.mv[1] = mv1;
                                x2 += 8;
                            }
                            y2 += 8;
                        }
                        num += 1;
                        x += dx;
                    }
                    y += dy;
                }
            }
        }
        let win = if cur.pps.conf_win_present {
            &cur.pps.conf_win
        } else {
            &cur.sps.conf_win
        };
        let (ux, uy) = (cur.sps.sub_width_c(), cur.sps.sub_height_c());
        let crop = [win.left * ux, win.right * ux, win.top * uy, win.bottom * uy];
        let ctu_slice = pic.ctus.iter().map(|c| c.slice.unwrap_or(0)).collect();
        let refpic = RefPic {
            id: cur.id,
            poc: pic.poc,
            fmt: pic.fmt,
            width: pic.width,
            height: pic.height,
            bit_depth: pic.bit_depth,
            planes: std::mem::take(&mut pic.planes),
            ctu_log2: pic.ctu_log2,
            width_ctus: pic.width_ctus,
            ctu_slice,
            slices: std::mem::take(&mut pic.slice_refs),
            col,
            col_w,
            wrap: cur.pps.wraparound.then_some(cur.pps.wrap_offset),
            scaling_win: cur.pps.scaling_win,
            collocated: (cur.sps.chroma_hor_collocated, cur.sps.chroma_ver_collocated),
        };
        self.dpb.push(DpbEntry {
            pic: Arc::new(refpic),
            mark: if cur.ph.non_ref {
                UNREFERENCED
            } else {
                SHORT_TERM
            },
            needed_for_output: cur.needed_for_output,
            idr: cur.nal_type == NAL_IDR_W_RADL || cur.nal_type == NAL_IDR_N_LP,
            tlayer: cur.tlayer,
            non_ref: cur.ph.non_ref,
            crop,
            cts: cur.cts,
        });
        let reorder = cur.sps.num_reorder_pics;
        while let Some(f) = self.next_output(false, reorder)? {
            self.output.push_back(f);
        }
        self.dpb
            .retain(|p| p.mark != UNREFERENCED || p.needed_for_output);
        Ok(())
    }

    /// vvdec's `PicListManager::getNextOutputPic` (without the tune-in
    /// delay, which only postpones output).
    fn next_output(&mut self, mut flush: bool, reorder: u32) -> Result<Option<Frame>, Error> {
        let mut start = 0;
        let mut end = self.dpb.len();
        let mut found = false;
        for (i, p) in self.dpb.iter().enumerate() {
            if !p.needed_for_output {
                continue;
            }
            if p.idr {
                if !found {
                    start = i;
                } else {
                    end = i;
                    break;
                }
            }
            found = true;
        }
        if !found {
            return Ok(None);
        }
        if end < self.dpb.len() && self.dpb[end].idr {
            flush = true;
        }
        let range = start..end;
        let pending = self.dpb[range.clone()]
            .iter()
            .filter(|p| p.needed_for_output)
            .count() as u32;
        if pending <= reorder && !flush {
            return Ok(None);
        }
        let Some(k) = range
            .filter(|&k| self.dpb[k].needed_for_output)
            .min_by_key(|&k| self.dpb[k].pic.poc)
        else {
            return Ok(None);
        };
        self.dpb[k].needed_for_output = false;
        output_frame(&self.dpb[k]).map(Some)
    }

    /// Removes and returns the next picture in output order, if any.
    pub fn pop_output(&mut self) -> Option<Frame> {
        self.output.pop_front()
    }

    /// Finishes the current picture and outputs all pending pictures.
    pub fn flush(&mut self) -> Result<(), Error> {
        if self.cur.is_some() {
            self.finish_picture()?;
        }
        while let Some(f) = self.next_output(true, 0)? {
            self.output.push_back(f);
        }
        self.dpb
            .retain(|p| p.mark != UNREFERENCED || p.needed_for_output);
        Ok(())
    }
}

fn output_frame(e: &DpbEntry) -> Result<Frame, Error> {
    let pic = &e.pic;
    let [left, right, top, bottom] = e.crop;
    let w = (pic.width as u32)
        .checked_sub(left + right)
        .ok_or(Error::Invalid("conformance window"))?;
    let h = (pic.height as u32)
        .checked_sub(top + bottom)
        .ok_or(Error::Invalid("conformance window"))?;
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
    Ok(Frame {
        width: w,
        height: h,
        chroma_format: pic.fmt.chroma,
        bit_depth: pic.bit_depth,
        planes,
        user_data: e.cts,
    })
}

fn decode_slice_data<'d>(
    pic: &mut Picture,
    si: &ctu::SliceInfo,
    data: &'d [u8],
) -> Result<(), Error> {
    let sps = si.sps;
    let pps = si.pps;
    let sh = si.sh;
    // substreams
    let mut subs: Vec<&'d [u8]> = Vec::new();
    let mut pos = 0usize;
    for &size in &sh.entry_points {
        let end = pos
            .checked_add(size as usize)
            .filter(|&e| e <= data.len())
            .ok_or(Error::Invalid("Exceeded FIFO size"))?;
        subs.push(&data[pos..end]);
        pos = end;
    }
    subs.push(&data[pos..]);
    // cabac_init_flag swaps the P and B tables (CABACReader::initCtxModels).
    let swap = pps.cabac_init_present && sh.cabac_init;
    let init_type = match sh.slice_type {
        ps::I_SLICE => 2,
        ps::P_SLICE => usize::from(!swap),
        _ => usize::from(swap),
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
        if cx > 0
            && dec.pic.ctus[addr as usize - 1].slice.is_none()
            && sh.ctus.first() != Some(&addr)
            && !pps.rect_slice
        {
            // vvdec requires the left CTU to be parsed; with ordered slices
            // this always holds.
        }
        let restart =
            |dec: &mut ctu::CtuDecoder<'_, '_, 'd>, sub_id: usize, cur_sub: &mut usize| {
                dec.cabac.ctx = cabac::Contexts::new(init_type, sh.qp);
                if sub_id != *cur_sub {
                    dec.cabac.restart(subs[sub_id]);
                    *cur_sub = sub_id;
                } else {
                    dec.cabac.restart_here();
                }
            };
        if cx == tx {
            dec.pic.ibc_hist.clear();
            dec.pic.hmvp.clear();
        }
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
            if dec
                .pic
                .get_cu_restricted_pos(px, py - 1, px, py, si.slice_idx, tile_idx, 0, wpp)
                .is_some()
                && let Some(c) = &wpp_ctx
            {
                dec.cabac.ctx = c.clone();
            }
            prev_qp = [sh.qp, sh.qp];
        }
        dec.pic.ctus[addr as usize].slice = Some(si.slice_idx);
        dec.pic.ctus[addr as usize].tile = tile_idx;
        dec.ctu_addr = addr;
        dec.tile = tile_idx;
        let area = dec.pic.fmt.unit(
            cx as i32 * ctu_size,
            cy as i32 * ctu_size,
            ctu_size,
            ctu_size,
        );
        dec.coding_tree_unit(area, &mut prev_qp)?;
        if cx == tx && wpp {
            wpp_ctx = Some(dec.cabac.ctx.clone());
        }
        if i == sh.ctus.len() - 1 {
            if dec.cabac.decode_terminate() == 0 {
                return Err(Error::Invalid("Expecting a terminating bit"));
            }
            if !dec.cabac.finish_ok() {
                return Err(Error::Invalid(
                    "No proper stop/alignment pattern at end of CABAC stream.",
                ));
            }
        } else if cx + 1 == tx + tw && (cy + 1 == ty + th || wpp) {
            if dec.cabac.decode_terminate() == 0 {
                return Err(Error::Invalid("Expecting a terminating bit"));
            }
            if !dec.cabac.finish_ok() {
                return Err(Error::Invalid(
                    "No proper stop/alignment pattern at end of CABAC stream.",
                ));
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
    decode_all(nals)?.into_iter().next().ok_or(Error::NoPicture)
}

/// Decodes all pictures of a sequence of NAL units, in output order.
pub fn decode_all<'a>(nals: impl IntoIterator<Item = &'a [u8]>) -> Result<Vec<Frame>, Error> {
    let mut d = Decoder::new();
    for nal in nals {
        if let Err(e) = d.push_nal(nal) {
            if std::env::var_os("VVC_DEBUG").is_some() {
                eprintln!(
                    "NAL type {} failed: {e:?}",
                    nal.get(1).map_or(0, |b| b >> 3)
                );
            }
            return Err(e);
        }
    }
    d.flush()?;
    Ok(d.output.into_iter().collect())
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
