// SPDX-License-Identifier: LGPL-3.0-or-later
//! The syntax layer of libheif's AVC decoder, OpenH264 2.6.0, ahead of
//! reconstruction.
//!
//! OpenH264 splits and unescapes Annex B input its own way, reads syntax with a
//! 32-bit cache that tolerates a bounded over-read into zero padding, and accepts
//! or rejects parameter sets and slice headers with its own limits. This module
//! models those decisions so the Rust decoder only sees NAL units OpenH264 would
//! decode, and every stream OpenH264 rejects fails the same way. The model covers
//! single-layer AVC; SVC extension units are accepted or rejected as OpenH264's
//! header checks decide, never decoded.

/// Any condition that makes OpenH264's `DecodeFrameNoDelay` report an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejected;

type Result<T> = std::result::Result<T, Rejected>;

/// A failed OpenH264 bit read, carrying its `ERR_INFO_*` code
/// (`ERR_INFO_READ_OVERFLOW` 11, `ERR_INFO_READ_LEADING_ZERO` 12).
#[derive(Debug, Clone, Copy)]
struct ReadError(u32);

impl From<ReadError> for Rejected {
    fn from(_: ReadError) -> Self {
        Rejected
    }
}

type Read<T> = std::result::Result<T, ReadError>;

const MAX_SPS_COUNT: usize = 32;
const MAX_PPS_COUNT: usize = 256;
const MAX_MB_SIZE: u32 = 36864;
const MAX_REF_PIC_COUNT: u32 = 16;

/// OpenH264's `SBitStringAux` reader over a NAL payload followed by zero bytes.
struct Bits<'a> {
    data: &'a [u8],
    cur: usize,
    cur_bits: u32,
    left_bits: i32,
    allowed: usize,
    bits: i64,
}

impl<'a> Bits<'a> {
    /// `DecInitBits` + `InitReadBits`.
    fn new(data: &'a [u8], bit_size: i64) -> Result<Self> {
        let allowed = ((bit_size + 7) >> 3).max(0) as usize;
        if allowed == 0 {
            return Err(Rejected);
        }
        let mut bits = Bits {
            data,
            cur: 0,
            cur_bits: 0,
            left_bits: -16,
            allowed,
            bits: bit_size,
        };
        bits.cur_bits =
            u32::from_be_bytes([bits.byte(0), bits.byte(1), bits.byte(2), bits.byte(3)]);
        bits.cur = 4;
        Ok(bits)
    }
    fn byte(&self, at: usize) -> u8 {
        self.data.get(at).copied().unwrap_or(0)
    }
    /// `DUMP_BITS`/`NEED_BITS`/`GET_WORD`.
    fn dump(&mut self, n: u32) -> Read<()> {
        self.cur_bits = self.cur_bits.checked_shl(n).unwrap_or(0);
        self.left_bits += n as i32;
        if self.left_bits > 0 {
            if self.cur > self.allowed + 1 {
                return Err(ReadError(11));
            }
            let word = (u32::from(self.byte(self.cur)) << 8) | u32::from(self.byte(self.cur + 1));
            self.cur_bits |= word.checked_shl(self.left_bits as u32).unwrap_or(0);
            self.left_bits -= 16;
            self.cur += 2;
        }
        Ok(())
    }
    fn ubits(&self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.cur_bits >> (32 - n) }
    }
    fn u(&mut self, n: u32) -> Read<u32> {
        let value = self.ubits(n);
        self.dump(n)?;
        Ok(value)
    }
    fn flag(&mut self) -> Read<bool> {
        Ok(self.u(1)? != 0)
    }
    /// `BsGetUe`.
    fn ue(&mut self) -> Read<u32> {
        if self.cur_bits == 0 {
            return Err(ReadError(12));
        }
        let zeros = self.cur_bits.leading_zeros();
        if zeros > 16 {
            self.dump(16)?;
            self.dump(zeros + 1 - 16)?;
        } else {
            self.dump(zeros + 1)?;
        }
        let mut value = 0;
        if zeros != 0 {
            value = self.ubits(zeros);
            self.dump(zeros)?;
        }
        Ok((1u32 << zeros).wrapping_sub(1).wrapping_add(value))
    }
    fn se(&mut self) -> Read<i32> {
        let code = self.ue()?;
        Ok(if code & 1 != 0 {
            code.div_ceil(2) as i32
        } else {
            -((code >> 1) as i32)
        })
    }
    /// `CheckMoreRBSPData`.
    fn more_rbsp_data(&self) -> bool {
        self.bits - ((self.cur as i64 - 2) << 3) - i64::from(self.left_bits) > 1
    }
}

/// `BsGetTrailingBits`.
fn trailing_bits(last: u8) -> i64 {
    if last == 0 {
        0
    } else {
        i64::from(last.trailing_zeros())
    }
}

fn bit_size(payload: &[u8]) -> i64 {
    match payload.last() {
        Some(&last) => (payload.len() as i64) * 8 - trailing_bits(last),
        None => 0,
    }
}

#[derive(Clone)]
struct Sps {
    chroma_format: u32,
    scaling_4x4: [[u8; 16]; 6],
    scaling_8x8: [[u8; 64]; 2],
    seq_scaling: bool,
    log2_max_frame_num: u32,
    poc_type: u32,
    log2_max_poc_lsb: u32,
    delta_pic_order_always_zero: bool,
    num_ref_frames: u32,
    mb_width: u32,
    mb_height: u32,
    frame_mbs_only: bool,
}

impl Default for Sps {
    fn default() -> Self {
        Sps {
            chroma_format: 1,
            scaling_4x4: [[16; 16]; 6],
            scaling_8x8: [[16; 64]; 2],
            seq_scaling: false,
            log2_max_frame_num: 0,
            poc_type: 0,
            log2_max_poc_lsb: 0,
            delta_pic_order_always_zero: false,
            num_ref_frames: 0,
            mb_width: 0,
            mb_height: 0,
            frame_mbs_only: false,
        }
    }
}

#[derive(Clone, Default)]
struct Pps {
    sps_id: usize,
    cabac: bool,
    pic_order_present: bool,
    slice_groups: u32,
    ref_idx: [u32; 2],
    weighted_pred: bool,
    weighted_bipred: u32,
    init_qp: i32,
    deblocking_control: bool,
    redundant_pic_cnt: bool,
}

const DEFAULT_4X4: [[u8; 16]; 2] = [
    [
        6, 13, 20, 28, 13, 20, 28, 32, 20, 28, 32, 37, 28, 32, 37, 42,
    ],
    [
        10, 14, 20, 24, 14, 20, 24, 27, 20, 24, 27, 30, 24, 27, 30, 34,
    ],
];
const DEFAULT_8X8: [[u8; 64]; 2] = [
    [
        6, 10, 13, 16, 18, 23, 25, 27, 10, 11, 16, 18, 23, 25, 27, 29, 13, 16, 18, 23, 25, 27, 29,
        31, 16, 18, 23, 25, 27, 29, 31, 33, 18, 23, 25, 27, 29, 31, 33, 36, 23, 25, 27, 29, 31, 33,
        36, 38, 25, 27, 29, 31, 33, 36, 38, 40, 27, 29, 31, 33, 36, 38, 40, 42,
    ],
    [
        9, 13, 15, 17, 19, 21, 22, 24, 13, 13, 17, 19, 21, 22, 24, 25, 15, 17, 19, 21, 22, 24, 25,
        27, 17, 19, 21, 22, 24, 25, 27, 28, 19, 21, 22, 24, 25, 27, 28, 30, 21, 22, 24, 25, 27, 28,
        30, 32, 22, 24, 25, 27, 28, 30, 32, 33, 24, 25, 27, 28, 30, 32, 33, 35,
    ],
];

/// `SetScalingListValue`: only the delta range check and read pattern matter here.
fn scaling_list(r: &mut Bits, size: usize, out: &mut [u8]) -> Result<bool> {
    let (mut last, mut next) = (8i32, 8i32);
    for (j, slot) in out.iter_mut().enumerate().take(size) {
        if next != 0 {
            let delta = r.se()?;
            if !(-128..=127).contains(&delta) {
                return Err(Rejected);
            }
            next = (last + delta + 256) % 256;
            if j == 0 && next == 0 {
                return Ok(true);
            }
        }
        *slot = if next == 0 { last as u8 } else { next as u8 };
        last = i32::from(*slot);
    }
    Ok(false)
}

/// `ParseScalingList` for an SPS (`pps == None`) or a PPS.
fn scaling_lists(
    r: &mut Bits,
    sps: &Sps,
    pps_transform_8x8: Option<bool>,
    lists4: &mut [[u8; 16]; 6],
    lists8: &mut [[u8; 64]; 2],
) -> Result<()> {
    let count = match pps_transform_8x8 {
        None => {
            if sps.chroma_format != 3 {
                8
            } else {
                12
            }
        }
        Some(t8) => 6 + usize::from(t8) * if sps.chroma_format != 3 { 2 } else { 6 },
    };
    let init = pps_transform_8x8.is_some() && sps.seq_scaling;
    let fallback4 = [
        if init {
            sps.scaling_4x4[0]
        } else {
            DEFAULT_4X4[0]
        },
        if init {
            sps.scaling_4x4[3]
        } else {
            DEFAULT_4X4[1]
        },
    ];
    let fallback8 = [
        if init {
            sps.scaling_8x8[0]
        } else {
            DEFAULT_8X8[0]
        },
        if init {
            sps.scaling_8x8[1]
        } else {
            DEFAULT_8X8[1]
        },
    ];
    for i in 0..count {
        let present = r.flag()?;
        if present {
            if i < 6 {
                if scaling_list(r, 16, &mut lists4[i])? {
                    lists4[i] = DEFAULT_4X4[i / 3];
                }
            } else if i - 6 < 2 {
                if scaling_list(r, 64, &mut lists8[i - 6])? {
                    lists8[i - 6] = DEFAULT_8X8[(i - 6) & 1];
                }
            } else {
                // 4:4:4 chroma 8x8 lists: parsed, not retained for 4:2:0 decoding.
                let mut scratch = [0u8; 64];
                scaling_list(r, 64, &mut scratch)?;
            }
        } else if i < 6 {
            lists4[i] = if i != 0 && i != 3 {
                lists4[i - 1]
            } else {
                fallback4[i / 3]
            };
        } else if i - 6 < 2 {
            lists8[i - 6] = fallback8[i & 1];
        }
    }
    Ok(())
}

/// `GetLevelLimits`: (MaxFS, MaxDpbMbs) for valid level_idc values.
fn level_limits(level: u32, constraint3: bool) -> Option<(u32, u32)> {
    Some(match level {
        9 => (99, 396),
        10 => (99, 396),
        11 if constraint3 => (99, 396),
        11 => (396, 900),
        12 | 13 | 20 => (396, 2376),
        21 => (792, 4752),
        22 | 30 => (1620, 8100),
        31 => (3600, 18000),
        32 => (5120, 20480),
        40 | 41 => (8192, 32768),
        42 => (8704, 34816),
        50 => (22080, 110400),
        51 | 52 => (36864, 184320),
        _ => return None,
    })
}

/// HRD parameters as OpenH264 2.6.0 reads them (`_PARSE_NALHRD_VCLHRD_PARAMS_`):
/// every read error is ignored, and `cpb_cnt_minus1` receives the return code
/// of its read (0, or the read error code) instead of the value.
fn hrd(r: &mut Bits) {
    let count = match r.ue() {
        Ok(_) => 0,
        Err(ReadError(code)) => code,
    };
    let _ = r.u(4);
    let _ = r.u(4);
    for _ in 0..=count {
        let _ = r.ue();
        let _ = r.ue();
        let _ = r.flag();
    }
    for _ in 0..4 {
        let _ = r.u(5);
    }
}

/// `ParseVui`; the values are unused, only its reads and failures matter.
fn vui(r: &mut Bits) -> Result<()> {
    if r.flag()? {
        let idc = r.u(8)?;
        if idc == 255 {
            r.u(16)?;
            r.u(16)?;
        }
    }
    if r.flag()? {
        r.flag()?;
    }
    if r.flag()? {
        r.u(3)?;
        r.flag()?;
        if r.flag()? {
            r.u(8)?;
            r.u(8)?;
            r.u(8)?;
        }
    }
    if r.flag()? {
        r.ue()?;
        r.ue()?;
    }
    if r.flag()? {
        r.u(16)?;
        r.u(16)?;
        r.u(16)?;
        r.u(16)?;
        r.flag()?;
    }
    let nal_hrd = r.flag()?;
    if nal_hrd {
        hrd(r);
    }
    let vcl_hrd = r.flag()?;
    if vcl_hrd {
        hrd(r);
    }
    if nal_hrd || vcl_hrd {
        let _ = r.flag(); // low_delay_hrd_flag
    }
    r.flag()?;
    if r.flag()? {
        r.flag()?;
        for _ in 0..6 {
            r.ue()?;
        }
    }
    Ok(())
}

enum SpsResult {
    Stored(usize, Box<Sps>),
    /// Unsupported profile: success without storing or marking an SPS.
    Ignored,
}

/// `ParseSps` for an AVC SPS.
fn parse_sps(r: &mut Bits) -> Result<SpsResult> {
    let profile = r.u(8)?;
    if !matches!(profile, 66 | 77 | 83 | 86 | 88 | 100) {
        return Ok(SpsResult::Ignored);
    }
    let mut constraint = [false; 6];
    for flag in &mut constraint {
        *flag = r.flag()?;
    }
    r.u(2)?;
    let level = r.u(8)?;
    let id = r.ue()? as usize;
    if id >= MAX_SPS_COUNT {
        return Err(Rejected);
    }
    let (max_fs, max_dpb) = level_limits(level, constraint[3]).ok_or(Rejected)?;
    let (max_fs_52, _) = level_limits(52, false).unwrap();
    let mut sps = Sps::default();
    if matches!(profile, 83 | 86 | 100 | 110 | 122 | 244 | 44) {
        sps.chroma_format = r.ue()?;
        if sps.chroma_format > 1 {
            return Err(Rejected);
        }
        if r.ue()? != 0 || r.ue()? != 0 {
            return Err(Rejected);
        }
        r.flag()?; // qpprime_y_zero_transform_bypass_flag (ignored by OpenH264)
        sps.seq_scaling = r.flag()?;
        if sps.seq_scaling {
            let mut lists4 = [[16u8; 16]; 6];
            let mut lists8 = [[16u8; 64]; 2];
            let copy = sps.clone();
            scaling_lists(r, &copy, None, &mut lists4, &mut lists8)?;
            sps.scaling_4x4 = lists4;
            sps.scaling_8x8 = lists8;
        }
    }
    let log2_frame = r.ue()?;
    if log2_frame > 12 {
        return Err(Rejected);
    }
    sps.log2_max_frame_num = 4 + log2_frame;
    sps.poc_type = r.ue()?;
    if sps.poc_type == 0 {
        let log2_poc = r.ue()?;
        if log2_poc > 12 {
            return Err(Rejected);
        }
        sps.log2_max_poc_lsb = 4 + log2_poc;
    } else if sps.poc_type == 1 {
        sps.delta_pic_order_always_zero = r.flag()?;
        r.se()?;
        r.se()?;
        let cycle = r.ue()?;
        if cycle > 255 {
            return Err(Rejected);
        }
        for _ in 0..cycle {
            r.se()?;
        }
    }
    if sps.poc_type > 2 {
        return Err(Rejected);
    }
    sps.num_ref_frames = r.ue()?;
    r.flag()?;
    let check = |mbs: u32| {
        let square = u64::from(mbs) * u64::from(mbs);
        mbs == 0
            || mbs > MAX_MB_SIZE
            || (square > 8 * u64::from(max_fs) && square > 8 * u64::from(max_fs_52))
    };
    sps.mb_width = r.ue()?.wrapping_add(1);
    if check(sps.mb_width) {
        return Err(Rejected);
    }
    sps.mb_height = r.ue()?.wrapping_add(1);
    if check(sps.mb_height) {
        return Err(Rejected);
    }
    let total = u64::from(sps.mb_width) * u64::from(sps.mb_height);
    if total > u64::from(max_fs) && total > u64::from(max_fs_52) {
        return Err(Rejected);
    }
    if sps.num_ref_frames > 16 {
        return Err(Rejected);
    }
    let _ = max_dpb;
    sps.frame_mbs_only = r.flag()?;
    if !sps.frame_mbs_only {
        return Err(Rejected);
    }
    r.flag()?;
    if r.flag()? {
        let (left, right) = (r.ue()?, r.ue()?);
        if i64::from(left) + i64::from(right) > i64::from(sps.mb_width) * 16 / 2 {
            return Err(Rejected);
        }
        let (top, bottom) = (r.ue()?, r.ue()?);
        if i64::from(top) + i64::from(bottom) > i64::from(sps.mb_height) * 16 / 2 {
            return Err(Rejected);
        }
    }
    if r.flag()? {
        vui(r)?;
    }
    Ok(SpsResult::Stored(id, Box::new(sps)))
}

/// `ParsePps`.
fn parse_pps(r: &mut Bits, sps_list: &[Option<Box<Sps>>]) -> Result<(usize, Pps)> {
    let id = r.ue()? as usize;
    if id >= MAX_PPS_COUNT {
        return Err(Rejected);
    }
    let mut pps = Pps {
        sps_id: r.ue()? as usize,
        ..Default::default()
    };
    if pps.sps_id >= MAX_SPS_COUNT {
        return Err(Rejected);
    }
    pps.cabac = r.flag()?;
    pps.pic_order_present = r.flag()?;
    pps.slice_groups = r.ue()?.wrapping_add(1);
    if pps.slice_groups > 8 {
        return Err(Rejected);
    }
    if pps.slice_groups > 1 {
        let map_type = r.ue()?;
        if map_type > 1 {
            return Err(Rejected);
        }
        if map_type == 0 {
            for _ in 0..pps.slice_groups {
                r.ue()?;
            }
        }
    }
    pps.ref_idx = [r.ue()?.wrapping_add(1), r.ue()?.wrapping_add(1)];
    if pps.ref_idx.iter().any(|&n| n > MAX_REF_PIC_COUNT) {
        return Err(Rejected);
    }
    pps.weighted_pred = r.flag()?;
    pps.weighted_bipred = r.u(2)?;
    pps.init_qp = 26 + r.se()?;
    if !(0..=51).contains(&pps.init_qp) {
        return Err(Rejected);
    }
    let init_qs = 26 + r.se()?;
    if !(0..=51).contains(&init_qs) {
        return Err(Rejected);
    }
    if !(-12..=12).contains(&r.se()?) {
        return Err(Rejected);
    }
    pps.deblocking_control = r.flag()?;
    r.flag()?;
    pps.redundant_pic_cnt = r.flag()?;
    if r.more_rbsp_data() {
        let transform_8x8 = r.flag()?;
        if r.flag()? {
            let sps = sps_list[pps.sps_id].as_deref().ok_or(Rejected)?;
            let mut lists4 = [[16u8; 16]; 6];
            let mut lists8 = [[16u8; 64]; 2];
            scaling_lists(r, sps, Some(transform_8x8), &mut lists4, &mut lists8)?;
        }
        if !(-12..=12).contains(&r.se()?) {
            return Err(Rejected);
        }
    }
    Ok((id, pps))
}

/// `ParseSliceHeaderSyntaxs` for a non-extension slice.
fn parse_slice_header(
    r: &mut Bits,
    idr: bool,
    nal_ref_idc: u8,
    sps_list: &[Option<Box<Sps>>],
    pps_list: &[Option<Pps>],
) -> Result<()> {
    let first_mb = r.ue()?;
    if first_mb > 36863 {
        return Err(Rejected);
    }
    let mut slice_type = r.ue()?;
    if slice_type > 9 {
        return Err(Rejected);
    }
    if slice_type > 4 {
        slice_type -= 5;
    }
    // 0 P, 1 B, 2 I, 3 SP, 4 SI.
    if idr && slice_type != 2 {
        return Err(Rejected);
    }
    let pps_id = r.ue()? as usize;
    if pps_id > MAX_PPS_COUNT - 1 {
        return Err(Rejected);
    }
    let pps = pps_list[pps_id].as_ref().ok_or(Rejected)?;
    if pps.slice_groups == 0 {
        return Err(Rejected);
    }
    let sps = sps_list[pps.sps_id].as_deref().ok_or(Rejected)?;
    if sps.num_ref_frames == 0 && slice_type != 2 && slice_type != 4 {
        return Err(Rejected);
    }
    if sps.log2_max_frame_num == 0 {
        return Err(Rejected);
    }
    if first_mb > sps.mb_width * sps.mb_height - 1 {
        return Err(Rejected);
    }
    let frame_num = r.u(sps.log2_max_frame_num)?;
    if !sps.frame_mbs_only {
        return Err(Rejected);
    }
    if idr {
        if frame_num != 0 {
            return Err(Rejected);
        }
        if r.ue()? > 65535 {
            return Err(Rejected);
        }
    }
    if sps.poc_type == 0 {
        r.u(sps.log2_max_poc_lsb)?;
        if pps.pic_order_present {
            r.se()?;
        }
    } else if sps.poc_type == 1 && !sps.delta_pic_order_always_zero {
        r.se()?;
        if pps.pic_order_present {
            r.se()?;
        }
    }
    if pps.redundant_pic_cnt {
        let count = r.ue()?;
        if count > 127 || count > 0 {
            return Err(Rejected);
        }
    }
    if slice_type == 1 {
        r.flag()?;
    }
    let mut refs = pps.ref_idx;
    if (slice_type == 0 || slice_type == 1) && r.flag()? {
        let l0 = r.ue()?;
        if l0 > 15 {
            return Err(Rejected);
        }
        refs[0] = l0 + 1;
        if slice_type == 1 {
            let l1 = r.ue()?;
            if l1 > 15 {
                return Err(Rejected);
            }
            refs[1] = l1 + 1;
        }
    }
    if refs.iter().any(|&n| n > MAX_REF_PIC_COUNT) {
        return Err(Rejected);
    }
    // ParseRefPicListReordering.
    if slice_type != 2 && slice_type != 4 {
        for &count in &refs[..if slice_type == 1 { 2 } else { 1 }] {
            if r.flag()? {
                let mut index = 0u32;
                loop {
                    let idc = r.ue()?;
                    if (index >= MAX_REF_PIC_COUNT && idc != 3) || idc > 3 {
                        return Err(Rejected);
                    }
                    if idc == 3 {
                        break;
                    }
                    if index >= count || index >= MAX_REF_PIC_COUNT {
                        return Err(Rejected);
                    }
                    if idc == 0 || idc == 1 {
                        if r.ue()? > 1u32 << sps.log2_max_frame_num {
                            return Err(Rejected);
                        }
                    } else if idc == 2 {
                        r.ue()?;
                    }
                    index += 1;
                }
            }
        }
    }
    if (pps.weighted_pred && slice_type == 0) || (pps.weighted_bipred == 1 && slice_type == 1) {
        pred_weight_table(r, sps, slice_type, refs)?;
    }
    if nal_ref_idc != 0 {
        dec_ref_pic_marking(r, sps, idr)?;
    }
    if pps.cabac && slice_type != 2 && slice_type != 4 && r.ue()? > 2 {
        return Err(Rejected);
    }
    let qp = pps.init_qp + r.se()?;
    if !(0..=51).contains(&qp) {
        return Err(Rejected);
    }
    if slice_type == 3 || slice_type == 4 {
        return Err(Rejected);
    }
    if pps.deblocking_control {
        let idc = r.ue()?;
        if idc > 6 {
            return Err(Rejected);
        }
        if idc != 1 {
            for _ in 0..2 {
                let offset = r.se()?.wrapping_mul(2);
                if !(-12..=12).contains(&offset) {
                    return Err(Rejected);
                }
            }
        }
    }
    Ok(())
}

/// `ParsePredWeightedTable`.
fn pred_weight_table(r: &mut Bits, sps: &Sps, slice_type: u32, refs: [u32; 2]) -> Result<()> {
    if r.ue()? > 7 {
        return Err(Rejected);
    }
    if sps.chroma_format != 0 && r.ue()? > 7 {
        return Err(Rejected);
    }
    for &count in &refs[..if slice_type == 1 { 2 } else { 1 }] {
        for _ in 0..count {
            if r.flag()? {
                for _ in 0..2 {
                    if !(-128..=127).contains(&r.se()?) {
                        return Err(Rejected);
                    }
                }
            }
            if sps.chroma_format != 0 && r.flag()? {
                for _ in 0..4 {
                    if !(-128..=127).contains(&r.se()?) {
                        return Err(Rejected);
                    }
                }
            }
        }
    }
    Ok(())
}

/// `ParseDecRefPicMarking`.
fn dec_ref_pic_marking(r: &mut Bits, sps: &Sps, idr: bool) -> Result<()> {
    if idr {
        r.flag()?;
        r.flag()?;
        return Ok(());
    }
    if !r.flag()? {
        return Ok(());
    }
    let (mut allow5, mut has4, mut has5, mut has6) = (true, false, false, false);
    for _ in 0..66 {
        let op = r.ue()?;
        match op {
            0 => break,
            1 | 3 => {
                allow5 = false;
                r.ue()?;
            }
            2 => {
                allow5 = false;
                r.ue()?;
            }
            _ => {}
        }
        if op == 3 || op == 6 {
            if op == 6 {
                if has6 {
                    return Err(Rejected);
                }
                has6 = true;
            }
            r.ue()?;
        } else if op == 4 {
            if has4 {
                return Err(Rejected);
            }
            has4 = true;
            let max = i64::from(r.ue()?) - 1;
            if max > i64::from(sps.num_ref_frames) {
                return Err(Rejected);
            }
        } else if op == 5 {
            if !allow5 || has5 {
                return Err(Rejected);
            }
            has5 = true;
        }
    }
    Ok(())
}

/// One NAL unit after OpenH264's byte-level processing: header byte plus RBSP,
/// with its trailing zero bytes still present.
fn split(stream: &[u8]) -> Result<Vec<Vec<u8>>> {
    // DetectStartCodePrefix.
    let start = stream
        .windows(3)
        .position(|w| w == [0, 0, 1])
        .ok_or(Rejected)?
        + 3;
    let src = &stream[start..];
    let mut units = Vec::new();
    let mut current = Vec::new();
    let mut at = 0usize;
    let mut nal_start_bytes = false;
    while at < src.len() {
        if at + 2 < src.len() && src[at] == 0 && src[at + 1] == 0 && src[at + 2] <= 3 {
            let third = src[at + 2];
            if nal_start_bytes && third != 0 && third != 1 {
                return Err(Rejected);
            }
            match third {
                2 => return Err(Rejected),
                0 => {
                    current.push(0);
                    at += 1;
                    nal_start_bytes = true;
                }
                3 => {
                    if !(at + 3 < src.len() && src[at + 3] > 3) {
                        current.extend_from_slice(&[0, 0]);
                    }
                    at += 3;
                }
                _ => {
                    nal_start_bytes = false;
                    units.push(std::mem::take(&mut current));
                    at += 3;
                }
            }
            continue;
        }
        // Only a zero byte can start the special cases above: copy the run up
        // to the next zero (or this non-special zero) in one step.
        let run = src[at + 1..]
            .iter()
            .position(|&b| b == 0)
            .map_or(src.len(), |n| at + 1 + n);
        current.extend_from_slice(&src[at..run]);
        at = run;
    }
    units.push(current);
    Ok(units)
}

/// NAL units OpenH264 decodes, and when it decodes their access unit.
pub struct Accepted {
    /// Header byte plus trailing-zero-stripped RBSP, in stream order.
    pub units: Vec<Vec<u8>>,
    /// Whether an SEI or access unit delimiter after the slices completes the
    /// access unit during the data call. An incomplete picture is then an
    /// error; otherwise it is decoded by the flush call, where an incomplete
    /// picture yields no image and no error.
    pub constructed_early: bool,
}

/// Decide what OpenH264 does with an Annex B stream: reject it, or accept NAL
/// units for reconstruction.
pub fn accept(stream: &[u8]) -> Result<Accepted> {
    let mut sps: Vec<Option<Box<Sps>>> = vec![None; MAX_SPS_COUNT];
    let mut pps: Vec<Option<Pps>> = vec![None; MAX_PPS_COUNT];
    let (mut sps_exist, mut subsps_exist, mut pps_exist) = (false, false, false);
    let mut accepted = Vec::new();
    let mut pending_slices = false;
    let mut constructed_early = false;
    for mut unit in split(stream)? {
        // ParseNalHeader: trailing zero bytes are not part of the unit.
        while unit.last() == Some(&0) {
            unit.pop();
        }
        // An empty unit reads its header from OpenH264's zeroed reserve bytes.
        let header = unit.first().copied().unwrap_or(0);
        if header & 0x80 != 0 {
            return Err(Rejected);
        }
        let nal_ref_idc = (header >> 5) & 3;
        let kind = header & 31;
        if !matches!(kind, 6 | 7 | 9) && !sps_exist {
            return Err(Rejected);
        }
        if !matches!(kind, 6 | 7 | 8 | 9 | 15) && !pps_exist {
            return Err(Rejected);
        }
        let payload = unit.get(1..).unwrap_or(&[]);
        if (matches!(kind, 1 | 5) && !(sps_exist || pps_exist))
            || (matches!(kind, 14 | 20) && !(sps_exist || subsps_exist || pps_exist))
        {
            return Err(Rejected);
        }
        match kind {
            1 | 5 => {
                let mut r = Bits::new(payload, bit_size(payload))?;
                parse_slice_header(&mut r, kind == 5, nal_ref_idc, &sps, &pps)?;
                pending_slices = true;
                accepted.push(unit);
            }
            14 | 20 => {
                // Prefix NAL / coded slice extension header (SVC).
                if payload.len() < 3 {
                    return Err(Rejected);
                }
                // DecodeNalHeaderExt.
                let quality_id = payload[1] & 0x0F;
                let use_ref_base = payload[2] & 0x10 != 0;
                if quality_id != 0 || use_ref_base {
                    return Err(Rejected);
                }
                if kind == 14 && nal_ref_idc != 0 {
                    // The prefix unit's own syntax is parsed with its result ignored,
                    // but its bitstream must initialize.
                    let rest = &payload[3..];
                    Bits::new(rest, bit_size(rest))?;
                }
                if kind == 20 {
                    // Extension slices need a stored subset SPS, which this
                    // single-layer model never has: the slice header fails.
                    return Err(Rejected);
                }
            }
            7 | 15 if !payload.is_empty() => {
                let mut r = Bits::new(payload, bit_size(payload))?;
                match parse_sps(&mut r)? {
                    SpsResult::Stored(id, parsed) => {
                        sps[id] = Some(parsed);
                        if kind == 7 {
                            sps_exist = true;
                        } else {
                            subsps_exist = true;
                        }
                        if kind == 7 {
                            accepted.push(unit);
                        }
                    }
                    SpsResult::Ignored => {}
                }
            }
            8 if !payload.is_empty() => {
                let mut r = Bits::new(payload, bit_size(payload))?;
                let (id, parsed) = parse_pps(&mut r, &sps)?;
                pps[id] = Some(parsed);
                pps_exist = true;
                accepted.push(unit);
            }
            6 | 9 if pending_slices => {
                constructed_early = true;
                pending_slices = false;
            }
            _ => {}
        }
    }
    Ok(Accepted {
        units: accepted,
        constructed_early,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_follows_openh264_unescaping() {
        let units = split(&[0, 0, 1, 0x67, 0, 0, 3, 1, 0, 0, 1, 0x68, 0, 0, 3, 9]).unwrap();
        assert_eq!(units, vec![vec![0x67, 0, 0, 1], vec![0x68, 9]]);
        assert!(split(&[0, 0, 1, 0x67, 0, 0, 2]).is_err());
        assert!(split(&[1, 2, 3]).is_err());
    }

    #[test]
    fn reader_tolerates_bounded_over_read() {
        // Four bytes load at initialization; refills may pass the end by at
        // most one byte, so a one-byte unit fails its first refill.
        let mut r = Bits::new(&[0x80], 1).unwrap();
        assert_eq!(r.u(16).unwrap(), 0x8000);
        assert!(r.u(16).is_err());
        let mut r = Bits::new(&[0x80, 0, 0, 0, 0], 33).unwrap();
        assert_eq!(r.u(16).unwrap(), 0x8000);
        assert_eq!(r.u(16).unwrap(), 0);
        assert_eq!(r.u(16).unwrap(), 0);
        assert!(r.u(16).is_err());
    }
}
