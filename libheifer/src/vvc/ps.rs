// SPDX-License-Identifier: LGPL-3.0-or-later
//! VVC parameter sets, picture and slice headers (H.266 clause 7.3), parsed
//! in the order and with the checks of vvdec's `HLSyntaxReader`.
use super::Error;
use super::bits::{BitReader, ceil_log2};

pub const MAX_QP: i32 = 63;

fn check(condition: bool, what: &'static str) -> Result<(), Error> {
    if condition {
        Err(Error::Invalid(what))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Window {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Ptl {
    pub profile_idc: u32,
    pub level_idc: u32,
    pub intra_only: bool,
    pub one_au_only: bool,
}

/// Partition constraints indexed [intra luma, inter, intra chroma].
pub type Constraints = [u32; 3];

/// One `ref_pic_list_struct` entry; inter-layer entries count as long-term,
/// as in vvdec.
#[derive(Clone, Copy, Debug, Default)]
pub struct RplEntry {
    /// Short-term: POC delta; long-term: POC LSBs.
    pub id: i32,
    pub lt: bool,
    pub ilrp: bool,
    pub msb_present: bool,
    pub msb_cycle: i32,
}

#[derive(Clone, Debug, Default)]
pub struct RefPicList {
    pub num_entries: u32,
    pub ltrp_in_header: bool,
    pub entries: Vec<RplEntry>,
}

impl RefPicList {
    /// vvdec's `ReferencePictureList::calcLTRefPOC`.
    pub fn lt_ref_poc(&self, i: usize, cur_poc: i32, bits_for_poc: u32) -> i32 {
        let e = &self.entries[i];
        let cycle = 1i32 << bits_for_poc;
        let mut poc = e.id & (cycle - 1);
        if e.msb_present {
            poc += cur_poc - e.msb_cycle * cycle - (cur_poc & (cycle - 1));
        }
        poc
    }
}

/// vvdec's `isLTPocEqual`.
pub fn lt_poc_equal(a: i32, b: i32, bits_for_poc: u32, msb_present: bool) -> bool {
    if msb_present {
        a == b
    } else {
        let mask = (1i32 << bits_for_poc) - 1;
        (a & mask) == (b & mask)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Sps {
    pub id: u32,
    pub vps_id: u32,
    pub max_sublayers: u32,
    pub chroma_format_idc: u32,
    pub log2_ctu_size: u32,
    pub ctu_size: u32,
    pub ptl: Ptl,
    pub gdr_enabled: bool,
    pub rpr_enabled: bool,
    pub res_change_in_clvs: bool,
    pub max_width: u32,
    pub max_height: u32,
    pub conf_win: Window,
    pub subpic_info_present: bool,
    pub num_subpics: u32,
    pub independent_subpics: bool,
    pub subpic_x: Vec<u32>,
    pub subpic_y: Vec<u32>,
    pub subpic_w: Vec<u32>,
    pub subpic_h: Vec<u32>,
    pub subpic_treated_as_pic: Vec<bool>,
    pub loop_filter_across_subpic: Vec<bool>,
    pub subpic_id_len: u32,
    pub subpic_id_mapping_explicit: bool,
    pub subpic_id_mapping_present: bool,
    pub subpic_id: Vec<u32>,
    pub bit_depth: u32,
    pub qp_bd_offset: i32,
    pub entropy_coding_sync: bool,
    pub entry_points_present: bool,
    pub bits_for_poc: u32,
    pub poc_msb_flag: bool,
    pub poc_msb_len: u32,
    pub extra_ph_bits: Vec<bool>,
    pub extra_sh_bits: Vec<bool>,
    pub log2_min_cb_size: u32,
    pub split_cons_override: bool,
    pub min_qt: Constraints,
    pub max_mtt_depth: Constraints,
    pub max_bt: Constraints,
    pub max_tt: Constraints,
    pub dual_tree: bool,
    pub log2_max_tb_size: u32,
    pub transform_skip: bool,
    pub log2_max_ts_size: u32,
    pub bdpcm: bool,
    pub mts: bool,
    pub explicit_mts_intra: bool,
    pub explicit_mts_inter: bool,
    pub lfnst: bool,
    pub joint_cbcr: bool,
    /// Chroma QP mapping tables [cb, cr, joint], indexed by qp + qp_bd_offset.
    pub chroma_qp_table: [Vec<i32>; 3],
    pub sao: bool,
    pub alf: bool,
    pub ccalf: bool,
    pub lmcs: bool,
    pub weighted_pred: bool,
    pub weighted_bipred: bool,
    pub long_term_refs: bool,
    pub inter_layer_pred: bool,
    pub idr_rpl_present: bool,
    pub rpl1_same_as_rpl0: bool,
    pub rpl_lists: [Vec<RefPicList>; 2],
    pub wraparound: bool,
    pub temporal_mvp: bool,
    pub sbtmvp: bool,
    pub amvr: bool,
    pub bdof: bool,
    pub bdof_control_in_ph: bool,
    pub smvd: bool,
    pub dmvr: bool,
    pub dmvr_control_in_ph: bool,
    pub mmvd: bool,
    pub mmvd_fullpel_only: bool,
    pub max_num_merge_cand: u32,
    pub sbt: bool,
    pub affine: bool,
    pub max_num_affine_merge_cand: u32,
    pub affine_type: bool,
    pub affine_amvr: bool,
    pub prof: bool,
    pub prof_control_in_ph: bool,
    pub bcw: bool,
    pub ciip: bool,
    pub gpm: bool,
    pub max_num_gpm_cand: u32,
    pub log2_parallel_merge_level: u32,
    pub isp: bool,
    pub mrl: bool,
    pub mip: bool,
    pub cclm: bool,
    pub chroma_hor_collocated: bool,
    pub chroma_ver_collocated: bool,
    pub act: bool,
    pub internal_minus_input_bit_depth: u32,
    pub ibc: bool,
    pub max_num_ibc_merge_cand: u32,
    pub ladf: bool,
    pub ladf_qp_offset: Vec<i32>,
    pub ladf_lower_bound: Vec<i32>,
    pub scaling_list: bool,
    pub scaling_matrix_for_lfnst_disabled: bool,
    pub scaling_matrix_for_alt_colour_space_disabled: bool,
    pub scaling_matrix_designated_colour_space: bool,
    pub dep_quant: bool,
    pub sign_data_hiding: bool,
    pub virtual_boundaries_enabled: bool,
    pub virtual_boundaries_present: bool,
    pub vb_pos_x: Vec<u32>,
    pub vb_pos_y: Vec<u32>,
    pub field_seq: bool,
    /// dpb_max_num_reorder_pics and dpb_max_dec_pic_buffering_minus1 + 1 of
    /// the highest sublayer.
    pub num_reorder_pics: u32,
    pub max_dec_pic_buffering: u32,
}

impl Sps {
    pub fn sub_width_c(&self) -> u32 {
        if self.chroma_format_idc == 1 || self.chroma_format_idc == 2 {
            2
        } else {
            1
        }
    }
    pub fn sub_height_c(&self) -> u32 {
        if self.chroma_format_idc == 1 { 2 } else { 1 }
    }
    /// vvdec's syntax checks derive SubWidthC/SubHeightC from its chroma
    /// channel scale, which treats 4:0:0 horizontally like 4:2:0.
    pub fn check_sub_width_c(&self) -> u32 {
        if self.chroma_format_idc == 3 { 1 } else { 2 }
    }
    pub fn check_sub_height_c(&self) -> u32 {
        if self.chroma_format_idc == 1 { 2 } else { 1 }
    }
}

fn parse_constraint_info(r: &mut BitReader, ptl: &mut Ptl) -> Result<(), Error> {
    if r.flag()? {
        ptl.intra_only = r.flag()?;
        r.flag()?; // all layers independent
        ptl.one_au_only = r.flag()?;
        let sixteen_minus = r.read(4)?;
        check(
            sixteen_minus > 8,
            "gci_sixteen_minus_max_bitdepth_constraint_idc shall be in the range 0 to 8, inclusive",
        )?;
        r.read(2)?;
        // 10 NAL type flags, 6 partitioning flags
        r.read(10)?;
        r.read(6)?;
        r.read(2)?; // max log2 ctu size
        r.read(3)?; // override, mtt, dual tree
        r.read(6)?; // intra tools
        r.read(17)?; // inter tools
        r.read(13)?; // transform/quant
        r.read(6)?; // loop filters
        let reserved = r.read(8)?;
        for _ in 0..reserved {
            check(r.flag()?, "gci_reserved_zero_bit not equal to zero")?;
        }
    }
    while !r.is_byte_aligned() {
        check(r.flag()?, "gci_alignment_zero_bit not equal to zero")?;
    }
    Ok(())
}

pub fn parse_ptl(
    r: &mut BitReader,
    present: bool,
    max_sublayers_minus1: u32,
) -> Result<Ptl, Error> {
    let mut ptl = Ptl::default();
    if present {
        ptl.profile_idc = r.read(7)?;
        r.flag()?; // tier
    }
    ptl.level_idc = r.read(8)?;
    r.flag()?; // frame only
    let multilayer = r.flag()?;
    // Main 10 (1), Main 10 4:4:4 (33), their still-picture variants (65, 97).
    check(
        matches!(ptl.profile_idc, 1 | 33 | 65 | 97) && multilayer,
        "ptl_multilayer_enabled_flag shall be equal to 0 for non-multilayer profiles",
    )?;
    // Multilayer profiles (17, 49, 81, 113).
    if matches!(ptl.profile_idc, 17 | 49 | 81 | 113) {
        return Err(Error::Unsupported("Multilayer profiles not yet supported"));
    }
    if present {
        parse_constraint_info(r, &mut ptl)?;
    }
    let mut level_present = vec![false; max_sublayers_minus1 as usize];
    for i in (0..max_sublayers_minus1 as usize).rev() {
        level_present[i] = r.flag()?;
    }
    while !r.is_byte_aligned() {
        r.flag()?;
    }
    for i in (0..max_sublayers_minus1 as usize).rev() {
        if level_present[i] {
            r.read(8)?;
        }
    }
    if present {
        let num_sub_profiles = r.read(8)?;
        for _ in 0..num_sub_profiles {
            r.read(32)?;
        }
    }
    Ok(ptl)
}

fn parse_ref_pic_list(
    r: &mut BitReader,
    sps: &Sps,
    rpls_idx_is_header: bool,
) -> Result<RefPicList, Error> {
    let mut rpl = RefPicList::default();
    let num = r.uvlc_range(0, 29, "num_ref_entries")?;
    rpl.num_entries = num;
    if sps.long_term_refs && num > 0 && !rpls_idx_is_header {
        rpl.ltrp_in_header = r.flag()?;
    } else if sps.long_term_refs {
        rpl.ltrp_in_header = true;
    }
    let mut prev = 0i32;
    for i in 0..num {
        if sps.inter_layer_pred && r.flag()? {
            r.uvlc_range(0, 64, "ilrp_idx")?;
            rpl.entries.push(RplEntry {
                lt: true,
                ilrp: true,
                ..Default::default()
            });
            continue;
        }
        let long_term = if sps.long_term_refs {
            !r.flag()?
        } else {
            false
        };
        if !long_term {
            let abs = r.uvlc_range(0, (1 << 15) - 1, "abs_delta_poc_st")? as i32;
            let mut delta = abs;
            if (!sps.weighted_pred && !sps.weighted_bipred) || i == 0 {
                delta += 1;
            }
            if delta > 0 && r.flag()? {
                delta = -delta;
            }
            delta += prev;
            prev = delta;
            rpl.entries.push(RplEntry {
                id: delta,
                ..Default::default()
            });
        } else {
            let lsb = if !rpl.ltrp_in_header {
                r.read(sps.bits_for_poc)? as i32
            } else {
                0
            };
            rpl.entries.push(RplEntry {
                id: lsb,
                lt: true,
                ..Default::default()
            });
        }
    }
    Ok(rpl)
}

fn derive_chroma_qp_table(
    qp_bd_offset: i32,
    start: i32,
    delta_in: &[i32],
    delta_out: &[i32],
) -> Result<Vec<i32>, Error> {
    let n = delta_in.len();
    let mut qp_in = vec![0i32; n + 1];
    let mut qp_out = vec![0i32; n + 1];
    qp_in[0] = start + 26;
    qp_out[0] = qp_in[0];
    for j in 0..n {
        qp_in[j + 1] = qp_in[j] + delta_in[j] + 1;
        qp_out[j + 1] = qp_out[j] + delta_out[j];
    }
    for j in 0..=n {
        check(
            qp_in[j] < -qp_bd_offset || qp_in[j] > MAX_QP,
            "qpInVal out of range",
        )?;
        check(
            qp_out[j] < -qp_bd_offset || qp_out[j] > MAX_QP,
            "qpOutVal out of range",
        )?;
    }
    let size = (MAX_QP + qp_bd_offset + 1) as usize;
    let mut table = vec![0i32; size];
    let idx = |k: i32| (k + qp_bd_offset) as usize;
    table[idx(qp_in[0])] = qp_out[0];
    let mut k = qp_in[0] - 1;
    while k >= -qp_bd_offset {
        table[idx(k)] = (table[idx(k + 1)] - 1).clamp(-qp_bd_offset, MAX_QP);
        k -= 1;
    }
    for j in 0..n {
        let sh = (delta_in[j] + 1) >> 1;
        let mut m = 1;
        for k in qp_in[j] + 1..=qp_in[j + 1] {
            let v =
                table[idx(qp_in[j])] + ((qp_out[j + 1] - qp_out[j]) * m + sh) / (delta_in[j] + 1);
            // vvdec writes through a std::vector; indices beyond MAX_QP are
            // out of bounds there and rejected here.
            let slot = table
                .get_mut(idx(k))
                .ok_or(Error::Invalid("chroma QP table index"))?;
            *slot = v;
            m += 1;
        }
    }
    for k in qp_in[n] + 1..=MAX_QP {
        table[idx(k)] = (table[idx(k - 1)] + 1).clamp(-qp_bd_offset, MAX_QP);
    }
    Ok(table)
}

fn skip_general_hrd(r: &mut BitReader) -> Result<(bool, bool, bool, u32), Error> {
    let units = r.read(32)?;
    check(units == 0, "num_units_in_tick shall be greater than 0")?;
    let scale = r.read(32)?;
    check(
        scale == 0,
        "The value of time_scale shall be greater than 0.",
    )?;
    let nal = r.flag()?;
    let vcl = r.flag()?;
    let mut du = false;
    let mut cpb_cnt_minus1 = 0;
    if nal || vcl {
        r.flag()?;
        du = r.flag()?;
        if du {
            r.read(8)?;
        }
        r.read(8)?;
        if du {
            r.read(4)?;
        }
        cpb_cnt_minus1 = r.uvlc_range(0, 31, "hrd_cpb_cnt_minus1")?;
    }
    Ok((nal, vcl, du, cpb_cnt_minus1))
}

fn skip_ols_hrd(
    r: &mut BitReader,
    general: (bool, bool, bool, u32),
    first: u32,
    max_minus1: u32,
) -> Result<(), Error> {
    let (nal, vcl, du, cpb) = general;
    for _ in first..=max_minus1 {
        let fixed_general = r.flag()?;
        let fixed_within = if fixed_general { true } else { r.flag()? };
        if fixed_within {
            r.uvlc_range(0, 2047, "elemental_duration_in_tc_minus1")?;
        } else if (nal || vcl) && cpb == 0 {
            r.flag()?;
        }
        for present in [nal, vcl] {
            if !present {
                continue;
            }
            let mut prev_rate = 0u32;
            let mut prev_size = 0u32;
            let mut prev_du_size = 0u32;
            let mut prev_du_rate = 0u32;
            for j in 0..=cpb {
                let rate = r.uvlc()?;
                check(j > 0 && rate <= prev_rate, "bit_rate_value_minus1 order")?;
                let size = r.uvlc_range(0, u32::MAX - 1, "cpb_size_value_minus1")?;
                check(j > 0 && size > prev_size, "cpb_size_value_minus1 order")?;
                if du {
                    let du_size = r.uvlc_range(0, u32::MAX - 1, "cpb_size_du_value_minus1")?;
                    check(
                        j > 0 && du_size > prev_du_size,
                        "cpb_size_du_value_minus1 order",
                    )?;
                    let du_rate = r.uvlc_range(0, u32::MAX - 1, "bit_rate_du_value_minus1")?;
                    check(
                        j > 0 && du_rate <= prev_du_rate,
                        "bit_rate_du_value_minus1 order",
                    )?;
                    prev_du_size = du_size;
                    prev_du_rate = du_rate;
                }
                r.flag()?;
                prev_rate = rate;
                prev_size = size;
            }
        }
    }
    Ok(())
}

pub fn parse_sps(r: &mut BitReader) -> Result<Sps, Error> {
    let mut s = Sps {
        id: r.read(4)?,
        vps_id: r.read(4)?,
        ..Default::default()
    };
    let max_sublayers_minus1 = r.code_range(3, 0, 6, "sps_max_sublayers_minus1")?;
    s.max_sublayers = max_sublayers_minus1 + 1;
    s.chroma_format_idc = r.read(2)?;
    let log2_ctu_minus5 = r.code_range(2, 0, 2, "sps_log2_ctu_size_minus5")?;
    s.log2_ctu_size = log2_ctu_minus5 + 5;
    s.ctu_size = 1 << s.log2_ctu_size;
    let ctb_log2 = s.log2_ctu_size;
    let ctb = s.ctu_size;
    let ptl_present = r.flag()?;
    check(
        s.vps_id == 0 && !ptl_present,
        "When sps_video_parameter_set_id is equal to 0, the value of sps_ptl_dpb_hrd_params_present_flag shall be equal to 1",
    )?;
    if ptl_present {
        s.ptl = parse_ptl(r, true, max_sublayers_minus1)?;
    }
    s.gdr_enabled = r.flag()?;
    s.rpr_enabled = r.flag()?;
    if s.rpr_enabled {
        s.res_change_in_clvs = r.flag()?;
    }
    s.max_width = r.uvlc()?;
    s.max_height = r.uvlc()?;
    let sub_w = s.check_sub_width_c();
    let sub_h = s.check_sub_height_c();
    if r.flag()? {
        let left = r.uvlc()?;
        let right = r.uvlc()?;
        let top = r.uvlc()?;
        let bottom = r.uvlc()?;
        check(
            u64::from(sub_w) * (u64::from(left) + u64::from(right)) > u64::from(s.max_width),
            "conformance window width",
        )?;
        check(
            u64::from(sub_h) * (u64::from(top) + u64::from(bottom)) > u64::from(s.max_height),
            "conformance window height",
        )?;
        s.conf_win = Window {
            left,
            right,
            top,
            bottom,
        };
    }
    s.subpic_info_present = r.flag()?;
    check(
        s.res_change_in_clvs && s.subpic_info_present,
        "When sps_res_change_in_clvs_allowed_flag is equal to 1, the value of sps_subpic_info_present_flag shall be equal to 0.",
    )?;
    let width_ctus = s.max_width.div_ceil(ctb);
    let height_ctus = s.max_height.div_ceil(ctb);
    if s.subpic_info_present {
        let num_minus1 = r.uvlc()?;
        check(
            u64::from(num_minus1) + 1 > u64::from(width_ctus) * u64::from(height_ctus),
            "Invalid sps_num_subpics_minus1 value",
        )?;
        s.num_subpics = num_minus1 + 1;
        let n = s.num_subpics as usize;
        s.subpic_x = vec![0; n];
        s.subpic_y = vec![0; n];
        s.subpic_w = vec![0; n];
        s.subpic_h = vec![0; n];
        s.subpic_treated_as_pic = vec![false; n];
        s.loop_filter_across_subpic = vec![false; n];
        if num_minus1 == 0 {
            s.subpic_w[0] = width_ctus;
            s.subpic_h[0] = height_ctus;
            s.independent_subpics = true;
            s.subpic_treated_as_pic[0] = true;
        } else {
            s.independent_subpics = r.flag()?;
            let same_size = r.flag()?;
            let bits_w = ceil_log2(width_ctus);
            let bits_h = ceil_log2(height_ctus);
            for i in 0..n {
                if !same_size || i == 0 {
                    s.subpic_x[i] = if i > 0 && s.max_width > ctb {
                        r.read(bits_w)?
                    } else {
                        0
                    };
                    s.subpic_y[i] = if i > 0 && s.max_height > ctb {
                        r.read(bits_h)?
                    } else {
                        0
                    };
                    s.subpic_w[i] = if i < num_minus1 as usize && s.max_width > ctb {
                        r.read(bits_w)? + 1
                    } else {
                        width_ctus.wrapping_sub(s.subpic_x[i])
                    };
                    s.subpic_h[i] = if i < num_minus1 as usize && s.max_height > ctb {
                        r.read(bits_h)? + 1
                    } else {
                        height_ctus.wrapping_sub(s.subpic_y[i])
                    };
                } else {
                    check(s.subpic_w[0] == 0 || s.subpic_h[0] == 0, "subpicture size")?;
                    let cols = width_ctus / s.subpic_w[0];
                    check(
                        (cols * height_ctus / s.subpic_h[0]).wrapping_sub(1) != num_minus1,
                        "numSubpicCols * tmpHeightVal / ( sps_subpic_height_minus1[ 0 ] + 1 ) - 1",
                    )?;
                    check(
                        !width_ctus.is_multiple_of(s.subpic_w[0]),
                        "tmpWidthVal % ( sps_subpic_width_minus1[ 0 ] + 1 )",
                    )?;
                    check(
                        !height_ctus.is_multiple_of(s.subpic_h[0]),
                        "tmpHeightVal % ( sps_subpic_height_minus1[ 0 ] + 1 )",
                    )?;
                    s.subpic_x[i] = (i as u32 % cols) * s.subpic_w[0];
                    s.subpic_y[i] = (i as u32 / cols) * s.subpic_h[0];
                    s.subpic_w[i] = s.subpic_w[0];
                    s.subpic_h[i] = s.subpic_h[0];
                }
                let conf = &s.conf_win;
                check(
                    u64::from(s.subpic_x[i]) * u64::from(ctb)
                        >= i64::from(s.max_width).wrapping_sub(i64::from(conf.right * sub_w))
                            as u64,
                    "sps_subpic_ctu_top_left_x",
                )?;
                check(
                    u64::from(s.subpic_x[i].wrapping_add(s.subpic_w[i])) * u64::from(ctb)
                        <= u64::from(conf.left * sub_w),
                    "sps_subpic_width_minus1",
                )?;
                check(
                    u64::from(s.subpic_y[i]) * u64::from(ctb)
                        >= i64::from(s.max_height).wrapping_sub(i64::from(conf.bottom * sub_h))
                            as u64,
                    "sps_subpic_ctu_top_left_y",
                )?;
                check(
                    u64::from(s.subpic_y[i].wrapping_add(s.subpic_h[i])) * u64::from(ctb)
                        <= u64::from(conf.top * sub_h),
                    "sps_subpic_height_minus1",
                )?;
                if !s.independent_subpics {
                    s.subpic_treated_as_pic[i] = r.flag()?;
                    s.loop_filter_across_subpic[i] = r.flag()?;
                } else {
                    s.subpic_treated_as_pic[i] = true;
                }
            }
        }
        s.subpic_id_len = r.uvlc_range(0, 15, "sps_subpic_id_len_minus1")? + 1;
        check(
            (1u64 << s.subpic_id_len) < u64::from(s.num_subpics),
            "sps_subpic_id_len_minus1",
        )?;
        s.subpic_id_mapping_explicit = r.flag()?;
        if s.subpic_id_mapping_explicit {
            s.subpic_id_mapping_present = r.flag()?;
            if s.subpic_id_mapping_present {
                s.subpic_id = (0..n)
                    .map(|_| r.read(s.subpic_id_len))
                    .collect::<Result<_, _>>()?;
            }
        }
    } else {
        s.num_subpics = 1;
        s.subpic_x = vec![0];
        s.subpic_y = vec![0];
        s.subpic_w = vec![width_ctus];
        s.subpic_h = vec![height_ctus];
        s.subpic_treated_as_pic = vec![true];
        s.loop_filter_across_subpic = vec![false];
    }
    if !s.subpic_id_mapping_explicit || !s.subpic_id_mapping_present {
        s.subpic_id = (0..s.num_subpics).collect();
    }
    let bitdepth_minus8 = r.uvlc_range(0, 8, "sps_bitdepth_minus8")?;
    // Profile limits: Main 10 and its still/4:4:4 variants cap at 10 bits;
    // format range extensions (Main 12/16) allow more.
    let max_bit_depth = match s.ptl.profile_idc {
        1 | 17 | 33 | 49 | 65 | 81 | 97 | 113 => 10,
        2 | 10 | 34 | 42 | 66 | 98 => 12,
        35 | 43 | 99 => 16,
        _ => 16,
    };
    if s.ptl.profile_idc != 0 {
        check(
            bitdepth_minus8 + 8 > max_bit_depth,
            "sps_bitdepth_minus8 exceeds range supported by signalled profile",
        )?;
    }
    s.bit_depth = bitdepth_minus8 + 8;
    s.qp_bd_offset = 6 * bitdepth_minus8 as i32;
    s.entropy_coding_sync = r.flag()?;
    s.entry_points_present = r.flag()?;
    let log2_poc_minus4 = r.code_range(4, 0, 12, "sps_log2_max_pic_order_cnt_lsb_minus4")?;
    s.bits_for_poc = log2_poc_minus4 + 4;
    s.poc_msb_flag = r.flag()?;
    if s.poc_msb_flag {
        s.poc_msb_len =
            r.uvlc_range(0, 32 - log2_poc_minus4 - 5, "sps_poc_msb_cycle_len_minus1")? + 1;
    }
    let extra_ph = r.code_range(2, 0, 2, "sps_num_extra_ph_bytes")?;
    s.extra_ph_bits = (0..extra_ph * 8)
        .map(|_| r.flag())
        .collect::<Result<_, _>>()?;
    let extra_sh = r.code_range(2, 0, 2, "sps_num_extra_sh_bytes")?;
    s.extra_sh_bits = (0..extra_sh * 8)
        .map(|_| r.flag())
        .collect::<Result<_, _>>()?;
    if ptl_present {
        let sublayer_dpb = if max_sublayers_minus1 > 0 {
            r.flag()?
        } else {
            false
        };
        let mut prev_buffering = 0u32;
        let mut prev_reorder = 0u32;
        for i in if sublayer_dpb {
            0
        } else {
            max_sublayers_minus1
        }..=max_sublayers_minus1
        {
            let buffering_minus1 = r.uvlc()?;
            check(
                i > 0 && sublayer_dpb && buffering_minus1.wrapping_add(1) < prev_buffering,
                "dpb_max_dec_pic_buffering_minus1 order",
            )?;
            let reorder = r.uvlc_range(0, buffering_minus1, "dpb_max_num_reorder_pics")?;
            check(
                i > 0 && sublayer_dpb && buffering_minus1 < prev_reorder,
                "dpb_max_num_reorder_pics order",
            )?;
            r.uvlc_range(0, u32::MAX - 1, "dpb_max_latency_increase_plus1")?;
            prev_buffering = buffering_minus1.wrapping_add(1);
            prev_reorder = reorder;
            s.num_reorder_pics = reorder;
            s.max_dec_pic_buffering = buffering_minus1.wrapping_add(1);
        }
    }
    let min_cb_minus2 = r.uvlc_range(
        0,
        4.min(log2_ctu_minus5 + 3),
        "sps_log2_min_luma_coding_block_size_minus2",
    )?;
    s.log2_min_cb_size = min_cb_minus2 + 2;
    let min_cb_log2 = s.log2_min_cb_size;
    let min_cb = 1u32 << min_cb_log2;
    check(
        min_cb > ctb.min(64),
        "The value of MinCbSizeY shall be less than or equal to VSize.",
    )?;
    check(
        min_cb_log2 > ctb_log2,
        "Invalid log2_min_luma_coding_block_size_minus2 signalled",
    )?;
    check(
        s.max_width == 0 || s.max_width & (8.max(min_cb) - 1) != 0,
        "sps_pic_width_max_in_luma_samples",
    )?;
    check(
        s.max_height == 0 || s.max_height & (8.max(min_cb) - 1) != 0,
        "sps_pic_height_max_in_luma_samples",
    )?;
    s.split_cons_override = r.flag()?;
    let min_qt_intra_y = r.uvlc_range(
        0,
        6.min(ctb_log2) - min_cb_log2,
        "sps_log2_diff_min_qt_min_cb_intra_slice_luma",
    )? + min_cb_log2;
    let mtt_intra_y = r.uvlc_range(
        0,
        2 * (ctb_log2 - min_cb_log2),
        "sps_max_mtt_hierarchy_depth_intra_slice_luma",
    )?;
    let mut min_qt = [1u32 << min_qt_intra_y, 0, 0];
    let mut max_depth = [mtt_intra_y, 0, 0];
    let mut max_tt = [1u32 << min_qt_intra_y, 0, 0];
    let mut max_bt = [1u32 << min_qt_intra_y, 0, 0];
    if mtt_intra_y != 0 {
        max_bt[0] <<= r.uvlc_range(
            0,
            ctb_log2 - min_qt_intra_y,
            "sps_log2_diff_max_bt_min_qt_intra_slice_luma",
        )?;
        let tt_max = 6.min(ctb_log2) as i64 - i64::from(min_qt_intra_y);
        let v = r.uvlc()?;
        check(
            i64::from(v) > tt_max,
            "sps_log2_diff_max_tt_min_qt_intra_slice_luma",
        )?;
        max_tt[0] <<= v;
    }
    check(
        max_tt[0] > 64,
        "The value of sps_log2_diff_max_tt_min_qt_intra_slice_luma shall be in the range of 0 to min(6,CtbLog2SizeY) - MinQtLog2SizeIntraY",
    )?;
    if s.chroma_format_idc != 0 {
        s.dual_tree = r.flag()?;
    }
    if s.dual_tree {
        let min_qt_c = r.uvlc_range(
            0,
            6.min(ctb_log2) - min_cb_log2,
            "sps_log2_diff_min_qt_min_cb_intra_slice_chroma",
        )? + min_cb_log2;
        max_depth[2] = r.uvlc_range(
            0,
            2 * (ctb_log2 - min_cb_log2),
            "sps_max_mtt_hierarchy_depth_intra_slice_chroma",
        )?;
        min_qt[2] = 1 << min_qt_c;
        max_tt[2] = min_qt[2];
        max_bt[2] = min_qt[2];
        if max_depth[2] != 0 {
            let lim = 6.min(ctb_log2) as i64 - i64::from(min_qt_c);
            let v = r.uvlc()?;
            check(
                i64::from(v) > lim,
                "sps_log2_diff_max_bt_min_qt_intra_slice_chroma",
            )?;
            max_bt[2] <<= v;
            let v = r.uvlc()?;
            check(
                i64::from(v) > lim,
                "sps_log2_diff_max_tt_min_qt_intra_slice_chroma",
            )?;
            max_tt[2] <<= v;
            check(
                max_tt[2] > 64,
                "sps_log2_diff_max_tt_min_qt_intra_slice_chroma",
            )?;
            check(
                max_bt[2] > 64,
                "sps_log2_diff_max_bt_min_qt_intra_slice_chroma",
            )?;
        }
    }
    let min_qt_inter = r.uvlc_range(
        0,
        6.min(ctb_log2) - min_cb_log2,
        "sps_log2_diff_min_qt_min_cb_inter_slice",
    )? + min_cb_log2;
    max_depth[1] = r.uvlc_range(
        0,
        2 * (ctb_log2 - min_cb_log2),
        "sps_max_mtt_hierarchy_depth_inter_slice",
    )?;
    min_qt[1] = 1 << min_qt_inter;
    max_tt[1] = min_qt[1];
    max_bt[1] = min_qt[1];
    if max_depth[1] != 0 {
        max_bt[1] <<= r.uvlc_range(
            0,
            ctb_log2 - min_qt_inter,
            "sps_log2_diff_max_bt_min_qt_inter_slice",
        )?;
        let lim = 6.min(ctb_log2) as i64 - i64::from(min_qt_inter);
        let v = r.uvlc()?;
        check(
            i64::from(v) > lim,
            "sps_log2_diff_max_tt_min_qt_inter_slice",
        )?;
        max_tt[1] <<= v;
    }
    s.min_qt = min_qt;
    s.max_mtt_depth = max_depth;
    s.max_bt = max_bt;
    s.max_tt = max_tt;
    s.log2_max_tb_size = if ctb > 32 {
        5 + u32::from(r.flag()?)
    } else {
        5
    };
    s.transform_skip = r.flag()?;
    if s.transform_skip {
        s.log2_max_ts_size = r.uvlc_range(0, 3, "sps_log2_transform_skip_max_size_minus2")? + 2;
        s.bdpcm = r.flag()?;
    }
    s.mts = r.flag()?;
    if s.mts {
        s.explicit_mts_intra = r.flag()?;
        s.explicit_mts_inter = r.flag()?;
    }
    s.lfnst = r.flag()?;
    if s.chroma_format_idc != 0 {
        s.joint_cbcr = r.flag()?;
        let same = r.flag()?;
        let num_tables = if same {
            1
        } else if s.joint_cbcr {
            3
        } else {
            2
        };
        let mut tables: Vec<Vec<i32>> = Vec::new();
        for _ in 0..num_tables {
            let start = r.svlc_range(-26 - s.qp_bd_offset, 36, "sps_qp_table_start_minus26")?;
            let points_minus1 =
                r.uvlc_range(0, (36 - start) as u32, "sps_num_points_in_qp_table_minus1")?;
            let mut delta_in = Vec::new();
            let mut delta_out = Vec::new();
            for _ in 0..=points_minus1 {
                let din = r.uvlc()?;
                let diff = r.uvlc()?;
                delta_in.push(din as i32);
                delta_out.push((diff ^ din) as i32);
            }
            tables.push(derive_chroma_qp_table(
                s.qp_bd_offset,
                start,
                &delta_in,
                &delta_out,
            )?);
        }
        s.chroma_qp_table = [
            tables[0].clone(),
            tables.get(1).unwrap_or(&tables[0]).clone(),
            tables.get(2).unwrap_or(&tables[0]).clone(),
        ];
    }
    s.sao = r.flag()?;
    s.alf = r.flag()?;
    s.ccalf = if s.alf && s.chroma_format_idc != 0 {
        r.flag()?
    } else {
        false
    };
    s.lmcs = r.flag()?;
    s.weighted_pred = r.flag()?;
    s.weighted_bipred = r.flag()?;
    s.long_term_refs = r.flag()?;
    if s.vps_id > 0 {
        s.inter_layer_pred = r.flag()?;
    }
    s.idr_rpl_present = r.flag()?;
    s.rpl1_same_as_rpl0 = r.flag()?;
    for i in 0..if s.rpl1_same_as_rpl0 { 1 } else { 2 } {
        let num = r.uvlc_range(0, 64, "sps_num_ref_pic_lists")?;
        let mut lists = Vec::new();
        for _ in 0..num {
            lists.push(parse_ref_pic_list(r, &s, false)?);
        }
        s.rpl_lists[i] = lists;
    }
    if s.rpl1_same_as_rpl0 {
        let mut copy = s.rpl_lists[0].clone();
        if !s.long_term_refs {
            for rpl in &mut copy {
                rpl.entries.retain(|e| !e.lt);
            }
        }
        s.rpl_lists[1] = copy;
    }
    s.wraparound = r.flag()?;
    for i in 0..s.num_subpics as usize {
        check(
            s.subpic_treated_as_pic[i] && s.subpic_w[i] != width_ctus && s.wraparound,
            "sps_ref_wraparound_enabled_flag with subpictures",
        )?;
    }
    s.temporal_mvp = r.flag()?;
    if s.temporal_mvp {
        s.sbtmvp = r.flag()?;
    }
    s.amvr = r.flag()?;
    s.bdof = r.flag()?;
    if s.bdof {
        s.bdof_control_in_ph = r.flag()?;
    }
    s.smvd = r.flag()?;
    s.dmvr = r.flag()?;
    if s.dmvr {
        s.dmvr_control_in_ph = r.flag()?;
    }
    s.mmvd = r.flag()?;
    if s.mmvd {
        s.mmvd_fullpel_only = r.flag()?;
    }
    s.max_num_merge_cand = 6 - r.uvlc_range(0, 5, "sps_six_minus_max_num_merge_cand")?;
    s.sbt = r.flag()?;
    s.affine = r.flag()?;
    if s.affine {
        s.max_num_affine_merge_cand = 5 - r.uvlc_range(
            0,
            5 - u32::from(s.sbtmvp),
            "sps_five_minus_max_num_subblock_merge_cand",
        )?;
        s.affine_type = r.flag()?;
        if s.amvr {
            s.affine_amvr = r.flag()?;
        }
        s.prof = r.flag()?;
        if s.prof {
            s.prof_control_in_ph = r.flag()?;
        }
    }
    s.bcw = r.flag()?;
    s.ciip = r.flag()?;
    if s.max_num_merge_cand >= 2 {
        s.gpm = r.flag()?;
        if s.gpm && s.max_num_merge_cand >= 3 {
            s.max_num_gpm_cand = s.max_num_merge_cand
                - r.uvlc_range(
                    0,
                    s.max_num_merge_cand - 2,
                    "sps_max_num_merge_cand_minus_max_num_gpm_cand",
                )?;
        } else if s.gpm {
            s.max_num_gpm_cand = 2;
        }
    }
    s.log2_parallel_merge_level =
        r.uvlc_range(0, ctb_log2 - 2, "sps_log2_parallel_merge_level_minus2")? + 2;
    s.isp = r.flag()?;
    s.mrl = r.flag()?;
    s.mip = r.flag()?;
    if s.chroma_format_idc != 0 {
        s.cclm = r.flag()?;
    }
    if s.chroma_format_idc == 1 {
        s.chroma_hor_collocated = r.flag()?;
        s.chroma_ver_collocated = r.flag()?;
    } else {
        // Inferred to be 1 when not present.
        s.chroma_hor_collocated = true;
        s.chroma_ver_collocated = true;
    }
    let palette = r.flag()?;
    if palette {
        return Err(Error::Invalid("palette mode is not yet supported"));
    }
    if s.chroma_format_idc == 3 && s.log2_max_tb_size != 6 {
        s.act = r.flag()?;
    }
    if s.transform_skip {
        s.internal_minus_input_bit_depth =
            r.uvlc_range(0, 8, "sps_internal_bit_depth_minus_input_bit_depth")?;
    }
    s.ibc = r.flag()?;
    if s.ibc {
        s.max_num_ibc_merge_cand =
            6 - r.uvlc_range(0, 5, "sps_six_minus_max_num_ibc_merge_cand")?;
    }
    s.ladf = r.flag()?;
    if s.ladf {
        let n = r.code_range(2, 0, 3, "sps_num_ladf_intervals_minus2")? + 2;
        s.ladf_qp_offset = vec![r.svlc_range(-63, 63, "sps_ladf_lowest_interval_qp_offset")?];
        s.ladf_lower_bound = vec![0];
        for i in 0..(n - 1) as usize {
            s.ladf_qp_offset
                .push(r.svlc_range(-63, 63, "sps_ladf_qp_offset")?);
            let d = r.uvlc_range(0, (1 << s.bit_depth) - 3, "sps_ladf_delta_threshold_minus1")?;
            let prev = s.ladf_lower_bound[i];
            s.ladf_lower_bound.push(prev + d as i32 + 1);
        }
    }
    s.scaling_list = r.flag()?;
    if s.lfnst && s.scaling_list {
        s.scaling_matrix_for_lfnst_disabled = r.flag()?;
    }
    if s.act && s.scaling_list {
        s.scaling_matrix_for_alt_colour_space_disabled = r.flag()?;
        if s.scaling_matrix_for_alt_colour_space_disabled {
            s.scaling_matrix_designated_colour_space = r.flag()?;
        }
    }
    s.dep_quant = r.flag()?;
    s.sign_data_hiding = r.flag()?;
    s.virtual_boundaries_enabled = r.flag()?;
    if s.virtual_boundaries_enabled {
        s.virtual_boundaries_present = r.flag()?;
        if s.virtual_boundaries_present {
            let nv = r.uvlc_range(
                0,
                if s.max_width <= 8 { 0 } else { 3 },
                "sps_num_ver_virtual_boundaries",
            )?;
            for _ in 0..nv {
                let max = s.max_width.div_ceil(8).saturating_sub(2);
                s.vb_pos_x
                    .push((r.uvlc_range(0, max, "sps_virtual_boundary_pos_x_minus1")? + 1) << 3);
            }
            let nh = r.uvlc_range(
                0,
                if s.max_height <= 8 { 0 } else { 3 },
                "sps_num_hor_virtual_boundaries",
            )?;
            for _ in 0..nh {
                let max = s.max_height.div_ceil(8).saturating_sub(2);
                s.vb_pos_y
                    .push((r.uvlc_range(0, max, "sps_virtual_boundary_pos_y_minus1")? + 1) << 3);
            }
        }
    }
    if ptl_present && r.flag()? {
        let general = skip_general_hrd(r)?;
        let sublayer_cpb = if max_sublayers_minus1 > 0 {
            r.flag()?
        } else {
            false
        };
        let first = if sublayer_cpb {
            0
        } else {
            max_sublayers_minus1
        };
        skip_ols_hrd(r, general, first, max_sublayers_minus1)?;
    }
    s.field_seq = r.flag()?;
    if r.flag()? {
        let payload = r.uvlc_range(0, 1023, "sps_vui_payload_size_minus1")? + 1;
        while !r.is_byte_aligned() {
            check(r.flag()?, "sps_vui_alignment_zero_bit not equal to 0")?;
        }
        // libheif's vvdec plugin does not use VUI colour information.
        let bits = payload as usize * 8;
        check(bits > r.bits_left(), "VUI payload exceeds SPS")?;
        for _ in 0..bits {
            r.read(1)?;
        }
    }
    if r.flag()? {
        while r.more_rbsp_data()? {
            r.flag()?;
        }
    }
    r.trailing_bits()?;
    Ok(s)
}

#[derive(Clone, Debug, Default)]
pub struct SubPic {
    pub id: u32,
    pub ctu_x: u32,
    pub ctu_y: u32,
    pub width_ctus: u32,
    pub height_ctus: u32,
    pub num_slices: u32,
    pub treated_as_pic: bool,
    pub loop_filter_across: bool,
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Pps {
    pub id: u32,
    pub sps_id: u32,
    pub mixed_nalu_types: bool,
    pub width: u32,
    pub height: u32,
    pub conf_win: Window,
    pub conf_win_present: bool,
    pub output_flag_present: bool,
    pub no_pic_partition: bool,
    pub subpic_id_mapping_present: bool,
    pub num_subpics: u32,
    pub subpic_ids: Vec<u32>,
    pub ctu_size: u32,
    pub log2_ctu_size: u32,
    pub width_ctus: u32,
    pub height_ctus: u32,
    pub tile_col_bd: Vec<u32>,
    pub tile_row_bd: Vec<u32>,
    pub ctu_to_tile_col: Vec<u32>,
    pub ctu_to_tile_row: Vec<u32>,
    pub loop_filter_across_tiles: bool,
    pub rect_slice: bool,
    pub single_slice_per_subpic: bool,
    pub num_slices_in_pic: u32,
    /// CTU raster addresses of each rectangular slice.
    pub slice_map: Vec<Vec<u32>>,
    pub subpics: Vec<SubPic>,
    pub loop_filter_across_slices: bool,
    pub cabac_init_present: bool,
    pub num_ref_idx_default: [u32; 2],
    pub rpl1_idx_present: bool,
    pub weighted_pred: bool,
    pub weighted_bipred: bool,
    pub wraparound: bool,
    pub init_qp_minus26: i32,
    pub cu_qp_delta: bool,
    pub chroma_tool_offsets: bool,
    pub cb_qp_offset: i32,
    pub cr_qp_offset: i32,
    pub joint_cbcr_qp_offset_present: bool,
    pub joint_cbcr_qp_offset: i32,
    pub slice_chroma_qp_offsets: bool,
    pub cu_chroma_qp_offset_list: bool,
    /// (cb, cr, joint) offsets; entry 0 is unused (index is offset idx + 1).
    pub chroma_qp_offset_list: Vec<[i32; 3]>,
    pub deblocking_control_present: bool,
    pub deblocking_override_enabled: bool,
    pub deblocking_disabled: bool,
    pub dbf_info_in_ph: bool,
    pub beta_offset_div2: [i32; 3],
    pub tc_offset_div2: [i32; 3],
    pub rpl_info_in_ph: bool,
    pub sao_info_in_ph: bool,
    pub alf_info_in_ph: bool,
    pub wp_info_in_ph: bool,
    pub qp_delta_info_in_ph: bool,
    pub ph_extension: bool,
    pub sh_extension: bool,
}

impl Pps {
    pub fn num_tile_cols(&self) -> u32 {
        self.tile_col_bd.len() as u32 - 1
    }
    pub fn num_tile_rows(&self) -> u32 {
        self.tile_row_bd.len() as u32 - 1
    }
    pub fn num_tiles(&self) -> u32 {
        self.num_tile_cols() * self.num_tile_rows()
    }
    pub fn subpic_idx_from_id(&self, id: u32) -> usize {
        self.subpics.iter().position(|s| s.id == id).unwrap_or(0)
    }
}

fn add_ctus(map: &mut Vec<u32>, x0: u32, x1: u32, y0: u32, y1: u32, width: u32) {
    for y in y0..y1 {
        for x in x0..x1 {
            map.push(y * width + x);
        }
    }
}

struct RectSlice {
    tile_idx: u32,
    width_tiles: u32,
    height_tiles: u32,
    num_slices_in_tile: u32,
    height_ctus: u32,
}

fn init_tiles(
    p: &mut Pps,
    mut col_widths: Vec<u32>,
    mut row_heights: Vec<u32>,
) -> Result<(), Error> {
    let mut remaining = p.width_ctus;
    for &w in &col_widths {
        check(w > remaining, "Tile column width exceeds picture width")?;
        remaining -= w;
    }
    let mut uniform = *col_widths.last().ok_or(Error::Invalid("tile columns"))?;
    while remaining > 0 {
        check(
            col_widths.len() >= 20,
            "Number of tile columns exceeds valid range",
        )?;
        uniform = uniform.min(remaining);
        col_widths.push(uniform);
        remaining -= uniform;
    }
    let mut remaining = p.height_ctus;
    for &h in &row_heights {
        check(h > remaining, "Tile row height exceeds picture height")?;
        remaining -= h;
    }
    let mut uniform = *row_heights.last().ok_or(Error::Invalid("tile rows"))?;
    while remaining > 0 {
        uniform = uniform.min(remaining);
        row_heights.push(uniform);
        remaining -= uniform;
    }
    p.tile_col_bd = vec![0];
    for w in &col_widths {
        let last = *p.tile_col_bd.last().unwrap();
        p.tile_col_bd.push(last + w);
    }
    p.tile_row_bd = vec![0];
    for h in &row_heights {
        let last = *p.tile_row_bd.last().unwrap();
        p.tile_row_bd.push(last + h);
    }
    p.ctu_to_tile_col.clear();
    let mut col = 0usize;
    for x in 0..=p.width_ctus {
        if col + 1 < p.tile_col_bd.len() && x == p.tile_col_bd[col + 1] {
            col += 1;
        }
        p.ctu_to_tile_col.push(col as u32);
    }
    p.ctu_to_tile_row.clear();
    let mut row = 0usize;
    for y in 0..=p.height_ctus {
        if row + 1 < p.tile_row_bd.len() && y == p.tile_row_bd[row + 1] {
            row += 1;
        }
        p.ctu_to_tile_row.push(row as u32);
    }
    Ok(())
}

fn init_rect_slice_map(p: &mut Pps, sps: &Sps, rect: &mut [RectSlice]) -> Result<(), Error> {
    let w = p.width_ctus;
    if p.single_slice_per_subpic {
        p.num_slices_in_pic = sps.num_subpics;
        p.slice_map = vec![Vec::new(); sps.num_subpics as usize];
        if sps.num_subpics > 1 {
            for i in 0..sps.num_subpics as usize {
                let left = sps.subpic_x[i];
                let right = left + sps.subpic_w[i] - 1;
                let top = sps.subpic_y[i];
                let bottom = top + sps.subpic_h[i] - 1;
                let get = |v: &Vec<u32>, i: u32| {
                    v.get(i as usize)
                        .copied()
                        .ok_or(Error::Invalid("subpicture outside picture"))
                };
                let width_tiles =
                    get(&p.ctu_to_tile_col, right)? + 1 - get(&p.ctu_to_tile_col, left)?;
                let height_tiles =
                    get(&p.ctu_to_tile_row, bottom)? + 1 - get(&p.ctu_to_tile_row, top)?;
                let tile_row = p.ctu_to_tile_row[top as usize] as usize;
                let row_height = p.tile_row_bd[tile_row + 1] - p.tile_row_bd[tile_row];
                if height_tiles == 1 && sps.subpic_h[i] < row_height {
                    add_ctus(
                        &mut p.slice_map[i],
                        left,
                        left + sps.subpic_w[i],
                        top,
                        top + sps.subpic_h[i],
                        w,
                    );
                } else {
                    let tx = p.ctu_to_tile_col[left as usize];
                    let ty = p.ctu_to_tile_row[top as usize];
                    for j in 0..height_tiles {
                        for k in 0..width_tiles {
                            let (x0, x1) = (
                                p.tile_col_bd[(tx + k) as usize],
                                p.tile_col_bd[(tx + k + 1) as usize],
                            );
                            let (y0, y1) = (
                                p.tile_row_bd[(ty + j) as usize],
                                p.tile_row_bd[(ty + j + 1) as usize],
                            );
                            add_ctus(&mut p.slice_map[i], x0, x1, y0, y1, w);
                        }
                    }
                }
            }
        } else {
            for ty in 0..p.num_tile_rows() as usize {
                for tx in 0..p.num_tile_cols() as usize {
                    add_ctus(
                        &mut p.slice_map[0],
                        p.tile_col_bd[tx],
                        p.tile_col_bd[tx + 1],
                        p.tile_row_bd[ty],
                        p.tile_row_bd[ty + 1],
                        w,
                    );
                }
            }
        }
    } else {
        let n = p.num_slices_in_pic as usize;
        p.slice_map = vec![Vec::new(); n];
        let cols = p.num_tile_cols();
        let rows = p.num_tile_rows();
        let mut i = 0usize;
        while i < n {
            let tx = rect[i].tile_idx % cols;
            let ty = rect[i].tile_idx / cols;
            if i == n - 1 {
                rect[i].width_tiles = cols - tx;
                rect[i].height_tiles = rows - ty;
                rect[i].num_slices_in_tile = 1;
            }
            if rect[i].width_tiles > 1 || rect[i].height_tiles > 1 {
                for j in 0..rect[i].height_tiles {
                    for k in 0..rect[i].width_tiles {
                        let (x0, x1) = (
                            *p.tile_col_bd
                                .get((tx + k) as usize)
                                .ok_or(Error::Invalid("slice tiles"))?,
                            *p.tile_col_bd
                                .get((tx + k + 1) as usize)
                                .ok_or(Error::Invalid("slice tiles"))?,
                        );
                        let (y0, y1) = (
                            *p.tile_row_bd
                                .get((ty + j) as usize)
                                .ok_or(Error::Invalid("slice tiles"))?,
                            *p.tile_row_bd
                                .get((ty + j + 1) as usize)
                                .ok_or(Error::Invalid("slice tiles"))?,
                        );
                        add_ctus(&mut p.slice_map[i], x0, x1, y0, y1, w);
                    }
                }
            } else {
                let num_in_tile = rect[i].num_slices_in_tile;
                let mut y = p.tile_row_bd[ty as usize];
                let (x0, x1) = (p.tile_col_bd[tx as usize], p.tile_col_bd[tx as usize + 1]);
                for _ in 0..num_in_tile.saturating_sub(1) {
                    let h = rect[i].height_ctus;
                    add_ctus(&mut p.slice_map[i], x0, x1, y, y + h, w);
                    y += h;
                    i += 1;
                    check(i >= n, "Invalid rectangular slice signalling")?;
                }
                check(
                    y >= p.tile_row_bd[ty as usize + 1],
                    "Invalid rectangular slice signalling",
                )?;
                rect[i].height_ctus = p.tile_row_bd[ty as usize + 1] - y;
                add_ctus(
                    &mut p.slice_map[i],
                    x0,
                    x1,
                    y,
                    p.tile_row_bd[ty as usize + 1],
                    w,
                );
            }
            i += 1;
        }
    }
    let mut all: Vec<u32> = p.slice_map.iter().flatten().copied().collect();
    let total = (p.width_ctus * p.height_ctus) as usize;
    check(all.len() < total, "Slice map contains too few CTUs")?;
    check(all.len() > total, "Slice map contains too many CTUs")?;
    all.sort_unstable();
    for k in 1..all.len() {
        check(all[k] > all[k - 1] + 1, "CTU missing in slice map")?;
        check(all[k] == all[k - 1], "CTU duplicated in slice map")?;
    }
    Ok(())
}

fn init_subpics(p: &mut Pps, sps: &Sps) -> Result<(), Error> {
    if p.subpic_id_mapping_present {
        check(
            p.num_subpics != sps.num_subpics,
            "pps_num_subpics_minus1 shall be equal to sps_num_subpics_minus1",
        )?;
    } else {
        p.num_subpics = sps.num_subpics;
    }
    let sub_w = sps.sub_width_c();
    let sub_h = sps.sub_height_c();
    for i in 0..sps.num_subpics as usize {
        let conf = &sps.conf_win;
        check(
            i64::from(sps.subpic_x[i] * sps.ctu_size)
                >= i64::from(sps.max_width) - i64::from(conf.right * sub_w),
            "No subpicture can be located completely outside of the conformance cropping window",
        )?;
        check(
            u64::from(sps.subpic_x[i] + sps.subpic_w[i]) * u64::from(sps.ctu_size)
                <= u64::from(conf.left * sub_w),
            "No subpicture can be located completely outside of the conformance cropping window",
        )?;
        check(
            i64::from(sps.subpic_y[i] * sps.ctu_size)
                >= i64::from(sps.max_height) - i64::from(conf.bottom * sub_h),
            "No subpicture can be located completely outside of the conformance cropping window",
        )?;
        check(
            u64::from(sps.subpic_y[i] + sps.subpic_h[i]) * u64::from(sps.ctu_size)
                <= u64::from(conf.top * sub_h),
            "No subpicture can be located completely outside of the conformance cropping window",
        )?;
    }
    p.subpics.clear();
    for i in 0..p.num_subpics as usize {
        let id = if sps.subpic_id_mapping_explicit {
            if p.subpic_id_mapping_present {
                p.subpic_ids[i]
            } else {
                sps.subpic_id[i]
            }
        } else {
            i as u32
        };
        let ctu = p.ctu_size;
        let left = sps.subpic_x[i] * ctu;
        let right = (p.width - 1).min((sps.subpic_x[i] + sps.subpic_w[i]) * ctu - 1);
        let top = sps.subpic_y[i] * ctu;
        let bottom = (p.height - 1).min((sps.subpic_y[i] + sps.subpic_h[i]) * ctu - 1);
        let mut sub = SubPic {
            id,
            ctu_x: sps.subpic_x[i],
            ctu_y: sps.subpic_y[i],
            width_ctus: sps.subpic_w[i],
            height_ctus: sps.subpic_h[i],
            num_slices: 0,
            treated_as_pic: sps.subpic_treated_as_pic[i],
            loop_filter_across: sps.loop_filter_across_subpic[i],
            left,
            right,
            top,
            bottom,
        };
        if p.num_slices_in_pic == 1 {
            check(
                p.num_subpics != 1,
                "only one slice in picture, but number of subpic is not one",
            )?;
            sub.num_slices = 1;
        } else {
            let mut count = 0;
            let mut last: i64 = -1;
            let mut first_after = p.num_slices_in_pic as i64;
            for (j, slice) in p.slice_map.iter().enumerate() {
                let c = slice[0];
                let (x, y) = (c % p.width_ctus, c / p.width_ctus);
                if x >= sub.ctu_x
                    && x < sub.ctu_x + sub.width_ctus
                    && y >= sub.ctu_y
                    && y < sub.ctu_y + sub.height_ctus
                {
                    count += 1;
                    last = j as i64;
                } else if first_after == p.num_slices_in_pic as i64 && last != -1 {
                    first_after = j as i64;
                }
            }
            check(
                first_after < last,
                "The signalling order of slices shall follow the coding order",
            )?;
            sub.num_slices = count;
        }
        p.subpics.push(sub);
    }
    Ok(())
}

pub fn parse_pps(r: &mut BitReader, sps_list: &[Option<Sps>]) -> Result<Pps, Error> {
    let mut p = Pps {
        id: r.read(6)?,
        ..Default::default()
    };
    p.sps_id = r.code_range(4, 0, 15, "pps_seq_parameter_set_id")?;
    let sps = sps_list[p.sps_id as usize]
        .as_ref()
        .ok_or(Error::Invalid("SPS missing"))?;
    let sub_w = sps.check_sub_width_c();
    let sub_h = sps.check_sub_height_c();
    p.mixed_nalu_types = r.flag()?;
    let ctb = sps.ctu_size;
    let min_cb = 1u32 << sps.log2_min_cb_size;
    p.width = r.uvlc_range(1, sps.max_width, "pps_pic_width_in_luma_samples")?;
    check(
        p.width & (7u32.max(min_cb - 1)) != 0,
        "pps_pic_width_in_luma_samples not a multiple of 8 or MinCbSizeY",
    )?;
    check(
        !sps.res_change_in_clvs && p.width != sps.max_width,
        "pps_pic_width_in_luma_samples",
    )?;
    check(
        sps.wraparound && ctb / min_cb + 1 > (p.width / min_cb).wrapping_sub(1),
        "wraparound width",
    )?;
    p.height = r.uvlc_range(1, sps.max_height, "pps_pic_height_in_luma_samples")?;
    check(
        p.height & (7u32.max(min_cb - 1)) != 0,
        "pps_pic_height_in_luma_samples not a multiple of 8 or MinCbSizeY",
    )?;
    check(
        !sps.res_change_in_clvs && p.height != sps.max_height,
        "pps_pic_height_in_luma_samples",
    )?;
    p.width_ctus = p.width.div_ceil(ctb);
    p.height_ctus = p.height.div_ceil(ctb);
    p.ctu_size = ctb;
    p.conf_win_present = r.flag()?;
    check(
        p.conf_win_present && p.width == sps.max_width && p.height == sps.max_height,
        "pps_conformance_window_flag shall be equal to 0",
    )?;
    if p.conf_win_present {
        let left = r.uvlc()?;
        let right = r.uvlc()?;
        let top = r.uvlc()?;
        let bottom = r.uvlc()?;
        check(
            u64::from(sub_w) * (u64::from(left) + u64::from(right)) >= u64::from(p.width),
            "pps_conf_win_left_offset + pps_conf_win_right_offset too large",
        )?;
        check(
            u64::from(sub_h) * (u64::from(top) + u64::from(bottom)) >= u64::from(p.height),
            "pps_conf_win_top_offset + pps_conf_win_bottom_offset too large",
        )?;
        p.conf_win = Window {
            left,
            right,
            top,
            bottom,
        };
    }
    let scaling_window = r.flag()?;
    check(
        !sps.rpr_enabled && scaling_window,
        "pps_scaling_window_explicit_signalling_flag",
    )?;
    if scaling_window {
        let w = p.width as i64;
        let h = p.height as i64;
        let left = i64::from(r.svlc()?);
        let right = i64::from(r.svlc()?);
        let top = i64::from(r.svlc()?);
        let bottom = i64::from(r.svlc()?);
        let sw = i64::from(sub_w);
        let sh = i64::from(sub_h);
        for (v, s, dim) in [
            (left, sw, w),
            (right, sw, w),
            (top, sh, h),
            (bottom, sh, h),
            (left + right, sw, w),
            (top + bottom, sh, h),
        ] {
            check(
                s * v < -dim * 15 || s * v >= dim,
                "pps_scaling_win offset out of bounds",
            )?;
        }
    }
    p.output_flag_present = r.flag()?;
    p.no_pic_partition = r.flag()?;
    check(
        (sps.num_subpics > 1 || p.mixed_nalu_types) && p.no_pic_partition,
        "pps_no_pic_partition_flag shall be equal to 0",
    )?;
    p.subpic_id_mapping_present = r.flag()?;
    check(
        (!sps.subpic_id_mapping_explicit || sps.subpic_id_mapping_present)
            && p.subpic_id_mapping_present,
        "pps_subpic_id_mapping_present_flag shall be equal to 0",
    )?;
    check(
        sps.subpic_id_mapping_explicit
            && !sps.subpic_id_mapping_present
            && !p.subpic_id_mapping_present,
        "pps_subpic_id_mapping_present_flag shall be equal to 1",
    )?;
    if p.subpic_id_mapping_present {
        if !p.no_pic_partition {
            let n = r.uvlc_range(0, 599, "pps_num_subpics_minus1")?;
            check(
                n != sps.num_subpics - 1,
                "pps_num_subpics_minus1 shall be equal to sps_num_subpics_minus1",
            )?;
            p.num_subpics = n + 1;
        } else {
            p.num_subpics = 1;
        }
        let len = r.uvlc_range(0, 15, "pps_subpic_id_len_minus1")? + 1;
        check(
            len != sps.subpic_id_len,
            "pps_subpic_id_len_minus1 shall be equal to sps_subpic_id_len_minus1",
        )?;
        check(
            (1u64 << len) < u64::from(p.num_subpics),
            "pps_subpic_id_len too short",
        )?;
        p.subpic_ids = (0..p.num_subpics)
            .map(|_| r.read(len))
            .collect::<Result<_, _>>()?;
    } else {
        p.subpic_ids = (0..600)
            .map(|i| {
                if sps.subpic_id_mapping_explicit {
                    sps.subpic_id.get(i).copied().unwrap_or(0)
                } else {
                    i as u32
                }
            })
            .collect();
    }
    for i in 0..p.num_subpics as usize {
        for j in 0..i {
            check(
                p.subpic_ids[i] == p.subpic_ids[j],
                "SubpicIdVal[ i ] shall not be equal to SubpicIdVal[ j ]",
            )?;
        }
    }
    let mut rect_slices: Vec<RectSlice> = Vec::new();
    if !p.no_pic_partition {
        let log2 = r.code_range(2, 0, 2, "pps_log2_ctu_size_minus5")?;
        check(
            log2 + 5 != sps.log2_ctu_size,
            "pps_log2_ctu_size_minus5 shall be equal to sps_log2_ctu_size_minus5",
        )?;
        p.log2_ctu_size = log2 + 5;
        let exp_cols = r.uvlc_range(0, p.width_ctus - 1, "pps_num_exp_tile_columns_minus1")? + 1;
        let exp_rows = r.uvlc_range(0, p.height_ctus - 1, "pps_num_exp_tile_rows_minus1")? + 1;
        check(
            exp_cols > 20,
            "Number of explicit tile columns exceeds valid range",
        )?;
        let cols: Vec<u32> = (0..exp_cols)
            .map(|_| Ok(r.uvlc_range(0, p.width_ctus - 1, "pps_tile_column_width_minus1")? + 1))
            .collect::<Result<_, Error>>()?;
        let rows: Vec<u32> = (0..exp_rows)
            .map(|_| Ok(r.uvlc_range(0, p.height_ctus - 1, "pps_tile_row_height_minus1")? + 1))
            .collect::<Result<_, Error>>()?;
        init_tiles(&mut p, cols, rows)?;
        if p.num_tiles() > 1 {
            p.loop_filter_across_tiles = r.flag()?;
            p.rect_slice = r.flag()?;
            check(
                (sps.subpic_info_present || p.mixed_nalu_types) && !p.rect_slice,
                "pps_rect_slice_flag shall be equal to 1",
            )?;
        } else {
            p.loop_filter_across_tiles = false;
            p.rect_slice = true;
        }
        if p.rect_slice {
            p.single_slice_per_subpic = r.flag()?;
        }
        if p.rect_slice && !p.single_slice_per_subpic {
            let n_minus1 = r.uvlc_range(0, 999, "pps_num_slices_in_pic_minus1")?;
            p.num_slices_in_pic = n_minus1 + 1;
            let tile_idx_delta_present = if n_minus1 > 1 { r.flag()? } else { false };
            rect_slices = (0..p.num_slices_in_pic)
                .map(|_| RectSlice {
                    tile_idx: 0,
                    width_tiles: 0,
                    height_tiles: 0,
                    num_slices_in_tile: 0,
                    height_ctus: 0,
                })
                .collect();
            let cols = p.num_tile_cols();
            let rows = p.num_tile_rows();
            let row_height = |p: &Pps, t: u32| {
                p.tile_row_bd[(t / cols) as usize + 1] - p.tile_row_bd[(t / cols) as usize]
            };
            let mut tile_idx: i64 = 0;
            let mut i = 0usize;
            while i + 1 < p.num_slices_in_pic as usize {
                let t = tile_idx as u32;
                rect_slices[i].tile_idx = t;
                rect_slices[i].width_tiles = if t % cols != cols - 1 {
                    r.uvlc_range(0, cols - 1, "pps_slice_width_in_tiles_minus1")? + 1
                } else {
                    1
                };
                if t / cols != rows - 1 && (tile_idx_delta_present || t.is_multiple_of(cols)) {
                    rect_slices[i].height_tiles =
                        r.uvlc_range(0, rows - 1, "pps_slice_height_in_tiles_minus1")? + 1;
                } else if t / cols == rows - 1 {
                    rect_slices[i].height_tiles = 1;
                } else {
                    rect_slices[i].height_tiles = if i > 0 {
                        rect_slices[i - 1].height_tiles
                    } else {
                        0
                    };
                }
                if rect_slices[i].width_tiles == 1 && rect_slices[i].height_tiles == 1 {
                    let rh = row_height(&p, t);
                    if rh > 1 {
                        let num_exp = r.uvlc_range(0, rh - 1, "pps_num_exp_slices_in_tile")?;
                        if num_exp == 0 {
                            rect_slices[i].num_slices_in_tile = 1;
                            rect_slices[i].height_ctus = rh;
                        } else {
                            let mut remaining = rh as i64;
                            let mut last = 0u32;
                            let mut j = 0usize;
                            let mut heights = Vec::new();
                            while j < num_exp as usize {
                                let h =
                                    r.uvlc_range(0, rh - 1, "pps_exp_slice_height_in_ctus_minus1")?
                                        + 1;
                                heights.push(h);
                                remaining -= i64::from(h);
                                last = h;
                                j += 1;
                            }
                            // vvdec keeps these unsigned; a negative remainder wraps.
                            let mut rem = remaining as u32;
                            let uniform = last;
                            while rem >= uniform {
                                heights.push(uniform);
                                rem -= uniform;
                                check(heights.len() > 1000, "too many slices in tile")?;
                            }
                            if rem > 0 {
                                heights.push(rem);
                            }
                            let count = heights.len();
                            for (k, h) in heights.into_iter().enumerate() {
                                let slot = rect_slices.get_mut(i + k).ok_or(Error::Invalid(
                                    "Number of slices exceeds pps_num_slices_in_pic",
                                ))?;
                                slot.height_ctus = h;
                                slot.num_slices_in_tile = count as u32;
                                slot.width_tiles = 1;
                                slot.height_tiles = 1;
                                slot.tile_idx = t;
                            }
                            i += count - 1;
                        }
                    } else {
                        rect_slices[i].num_slices_in_tile = 1;
                        rect_slices[i].height_ctus = rh;
                    }
                }
                if i < n_minus1 as usize {
                    if tile_idx_delta_present {
                        let tiles = p.num_tiles() as i32;
                        let delta =
                            r.svlc_range(-tiles + 1, tiles - 1, "pps_tile_idx_delta_val")?;
                        check(
                            delta == 0,
                            "When present, the value of pps_tile_idx_delta_val[ i ] shall not be equal to 0.",
                        )?;
                        tile_idx += i64::from(delta);
                        check(
                            tile_idx < 0 || tile_idx >= i64::from(p.num_tiles()),
                            "Invalid tile_idx_delta.",
                        )?;
                    } else {
                        tile_idx += i64::from(rect_slices[i].width_tiles);
                        if tile_idx % i64::from(cols) == 0 {
                            tile_idx += i64::from(rect_slices[i].height_tiles.wrapping_sub(1))
                                * i64::from(cols);
                        }
                    }
                }
                check(
                    tile_idx < 0 || tile_idx >= i64::from(p.num_tiles()),
                    "Invalid tile_idx_delta.",
                )?;
                i += 1;
            }
            let last = p.num_slices_in_pic as usize - 1;
            rect_slices[last].tile_idx = tile_idx as u32;
        }
        if !p.rect_slice || p.single_slice_per_subpic || p.num_slices_in_pic > 1 {
            p.loop_filter_across_slices = r.flag()?;
        }
    } else {
        p.single_slice_per_subpic = true;
    }
    p.cabac_init_present = r.flag()?;
    p.num_ref_idx_default[0] = r.uvlc_range(0, 14, "pps_num_ref_idx_default_active_minus1")? + 1;
    p.num_ref_idx_default[1] = r.uvlc_range(0, 14, "pps_num_ref_idx_default_active_minus1")? + 1;
    p.rpl1_idx_present = r.flag()?;
    p.weighted_pred = r.flag()?;
    check(
        !sps.weighted_pred && p.weighted_pred,
        "pps_weighted_pred_flag shall be equal to 0",
    )?;
    p.weighted_bipred = r.flag()?;
    check(
        !sps.weighted_bipred && p.weighted_bipred,
        "pps_weighted_bipred_flag shall be equal to 0",
    )?;
    p.wraparound = r.flag()?;
    check(
        (!sps.wraparound || ctb / min_cb + 1 > (p.width / min_cb).wrapping_sub(1)) && p.wraparound,
        "pps_ref_wraparound_enabled_flag shall be equal to 0",
    )?;
    if p.wraparound {
        r.uvlc_range(
            0,
            (p.width / min_cb) - (ctb / min_cb) - 2,
            "pps_pic_width_minus_wraparound_offset",
        )?;
    }
    p.init_qp_minus26 = r.svlc_range(-(26 + sps.qp_bd_offset), 37, "pps_init_qp_minus26")?;
    p.cu_qp_delta = r.flag()?;
    p.chroma_tool_offsets = r.flag()?;
    check(
        sps.chroma_format_idc == 0 && p.chroma_tool_offsets,
        "pps_chroma_tool_offsets_present_flag shall be equal to 0",
    )?;
    p.chroma_qp_offset_list = vec![[0; 3]];
    if p.chroma_tool_offsets {
        p.cb_qp_offset = r.svlc_range(-12, 12, "pps_cb_qp_offset")?;
        p.cr_qp_offset = r.svlc_range(-12, 12, "pps_cr_qp_offset")?;
        p.joint_cbcr_qp_offset_present = r.flag()?;
        check(
            (sps.chroma_format_idc == 0 || !sps.joint_cbcr) && p.joint_cbcr_qp_offset_present,
            "pps_joint_cbcr_qp_offset_present_flag shall be equal to 0",
        )?;
        if p.joint_cbcr_qp_offset_present {
            p.joint_cbcr_qp_offset = r.svlc_range(-12, 12, "pps_joint_cbcr_qp_offset_value")?;
        }
        p.slice_chroma_qp_offsets = r.flag()?;
        p.cu_chroma_qp_offset_list = r.flag()?;
        if p.cu_chroma_qp_offset_list {
            let len = r.uvlc_range(0, 5, "pps_chroma_qp_offset_list_len_minus1")? + 1;
            for _ in 0..len {
                let cb = r.svlc_range(-12, 12, "pps_cb_qp_offset_list")?;
                let cr = r.svlc_range(-12, 12, "pps_cr_qp_offset_list")?;
                let joint = if p.joint_cbcr_qp_offset_present {
                    r.svlc_range(-12, 12, "pps_joint_cbcr_qp_offset_list")?
                } else {
                    0
                };
                p.chroma_qp_offset_list.push([cb, cr, joint]);
            }
        }
    }
    p.deblocking_control_present = r.flag()?;
    if p.deblocking_control_present {
        p.deblocking_override_enabled = r.flag()?;
        p.deblocking_disabled = r.flag()?;
        if !p.no_pic_partition && p.deblocking_override_enabled {
            p.dbf_info_in_ph = r.flag()?;
        }
        if !p.deblocking_disabled {
            let beta = r.svlc_range(-12, 12, "pps_luma_beta_offset_div2")?;
            let tc = r.svlc_range(-12, 12, "pps_luma_tc_offset_div2")?;
            p.beta_offset_div2 = [beta; 3];
            p.tc_offset_div2 = [tc; 3];
            if p.chroma_tool_offsets {
                p.beta_offset_div2[1] = r.svlc_range(-12, 12, "pps_cb_beta_offset_div2")?;
                p.tc_offset_div2[1] = r.svlc_range(-12, 12, "pps_cb_tc_offset_div2")?;
                p.beta_offset_div2[2] = r.svlc_range(-12, 12, "pps_cr_beta_offset_div2")?;
                p.tc_offset_div2[2] = r.svlc_range(-12, 12, "pps_cr_tc_offset_div2")?;
            }
        }
    }
    if !p.no_pic_partition {
        p.rpl_info_in_ph = r.flag()?;
        p.sao_info_in_ph = r.flag()?;
        p.alf_info_in_ph = r.flag()?;
        if (p.weighted_pred || p.weighted_bipred) && p.rpl_info_in_ph {
            p.wp_info_in_ph = r.flag()?;
        }
        p.qp_delta_info_in_ph = r.flag()?;
    }
    p.ph_extension = r.flag()?;
    p.sh_extension = r.flag()?;
    if r.flag()? {
        while r.more_rbsp_data()? {
            r.flag()?;
        }
    }
    r.trailing_bits()?;
    if p.width == sps.max_width && p.height == sps.max_height {
        p.conf_win = sps.conf_win.clone();
    }
    // finalizePPSPartitioning
    if p.no_pic_partition {
        p.log2_ctu_size = sps.log2_ctu_size;
        let (wc, hc) = (p.width_ctus, p.height_ctus);
        init_tiles(&mut p, vec![wc], vec![hc])?;
        p.rect_slice = true;
        p.num_slices_in_pic = 1;
        rect_slices = vec![RectSlice {
            tile_idx: 0,
            width_tiles: 0,
            height_tiles: 0,
            num_slices_in_tile: 0,
            height_ctus: 0,
        }];
        init_rect_slice_map(&mut p, sps, &mut rect_slices)?;
        check(
            p.num_subpics >= 2,
            "error, no picture partitions, but have equal to or more than 2 sub pictures",
        )?;
    } else if p.rect_slice {
        init_rect_slice_map(&mut p, sps, &mut rect_slices)?;
    }
    init_subpics(&mut p, sps)?;
    if p.wraparound {
        check(!sps.wraparound, "pps_ref_wraparound_enabled_flag")?;
    }
    Ok(p)
}

#[derive(Clone, Debug, Default)]
pub struct AlfParam {
    pub new_filter: [bool; 2],
    pub new_cc: [bool; 2],
    pub nonlinear_luma: bool,
    pub nonlinear_chroma: bool,
    pub num_luma_filters: u32,
    pub coeff_delta_idx: [u8; 25],
    /// 25 filters x 13 coefficients (the last is the implicit centre).
    pub luma_coeff: Vec<i16>,
    pub luma_clip: Vec<i16>,
    pub num_alt_chroma: u32,
    /// 8 alternatives x 7 coefficients.
    pub chroma_coeff: Vec<i16>,
    pub chroma_clip: Vec<i16>,
    pub cc_count: [u32; 2],
    /// [component][filter][8]
    pub cc_coeff: [[[i16; 8]; 4]; 2],
}

#[derive(Clone, Debug, Default)]
pub struct LmcsParam {
    pub min_bin: u32,
    pub max_bin: u32,
    pub delta_cw_bits: u32,
    pub bin_cw_delta: [i32; 16],
    pub chroma_offset: i32,
}

#[derive(Clone, Debug)]
pub struct ScalingList {
    pub coef: Vec<Vec<i32>>,
    pub dc: [i32; 28],
}

impl Default for ScalingList {
    fn default() -> Self {
        Self {
            coef: (0..28)
                .map(|id| vec![0; scaling_matrix_size(id) * scaling_matrix_size(id)])
                .collect(),
            dc: [0; 28],
        }
    }
}

pub fn scaling_matrix_size(id: usize) -> usize {
    if id < 2 {
        2
    } else if id < 8 {
        4
    } else {
        8
    }
}

#[derive(Clone, Debug)]
pub enum ApsData {
    Alf(AlfParam),
    Lmcs(LmcsParam),
    Scaling(ScalingList),
}

#[derive(Clone, Debug)]
pub struct Aps {
    pub kind: u32,
    pub id: u32,
    pub chroma_present: bool,
    pub data: ApsData,
}

/// Diagonal up-right scan positions (raster indices) of a w x h block.
pub fn diag_scan(w: usize, h: usize) -> Vec<u16> {
    let mut out = Vec::with_capacity(w * h);
    let (mut x, mut y) = (0isize, 0isize);
    while out.len() < w * h {
        while y >= 0 {
            if (x as usize) < w && (y as usize) < h {
                out.push((y as usize * w + x as usize) as u16);
            }
            y -= 1;
            x += 1;
        }
        y = x;
        x = 0;
    }
    out
}

fn alf_filter_coeffs(
    r: &mut BitReader,
    param: &mut AlfParam,
    chroma: bool,
    alt: usize,
) -> Result<(), Error> {
    let num_coeff = if chroma { 7 } else { 13 };
    let num_filters = if chroma {
        1
    } else {
        param.num_luma_filters as usize
    };
    for f in 0..num_filters {
        for j in 0..num_coeff - 1 {
            let abs = r.uvlc()?;
            check(abs > 128, "alf_coeff_abs")?;
            let mut v = abs as i32;
            if abs != 0 && r.flag()? {
                v = -v;
            }
            check(!(-128..=127).contains(&v), "AlfCoeff out of range")?;
            if chroma {
                param.chroma_coeff[alt * 7 + j] = v as i16;
            } else {
                param.luma_coeff[f * 13 + j] = v as i16;
            }
        }
        if chroma {
            param.chroma_coeff[alt * 7 + 6] = 1 << 6;
        } else {
            param.luma_coeff[f * 13 + 12] = 1 << 6;
        }
    }
    let clip = if chroma {
        param.nonlinear_chroma
    } else {
        param.nonlinear_luma
    };
    if clip {
        for f in 0..num_filters {
            for j in 0..num_coeff - 1 {
                let v = r.read(2)? as i16;
                if chroma {
                    param.chroma_clip[alt * 7 + j] = v;
                } else {
                    param.luma_clip[f * 13 + j] = v;
                }
            }
        }
    }
    Ok(())
}

fn parse_scaling_list(r: &mut BitReader, chroma_present: bool) -> Result<ScalingList, Error> {
    let mut list = ScalingList::default();
    for id in 0..28usize {
        let luma = id % 3 == 2 || id == 27;
        if !(chroma_present || luma) {
            continue;
        }
        let copy = r.flag()?;
        let pred_mode = if !copy { r.flag()? } else { false };
        let mut delta = 0usize;
        if (copy || pred_mode) && id != 0 && id != 2 && id != 8 {
            let max = if id < 2 {
                id
            } else if id < 8 {
                id - 2
            } else {
                id - 8
            };
            delta = r.uvlc_range(0, max as u32, "scaling_list_pred_id_delta")? as usize;
        }
        let size = scaling_matrix_size(id);
        let ref_id = id - delta;
        let dc_pred;
        let mut pred: Vec<i32>;
        if !copy && !pred_mode {
            pred = vec![8; size * size];
            dc_pred = 8;
        } else if delta == 0 {
            pred = vec![16; size * size];
            dc_pred = 16;
        } else {
            pred = list.coef[ref_id].clone();
            dc_pred = if ref_id > 13 {
                list.dc[ref_id]
            } else {
                pred[0]
            };
        }
        if copy {
            if id >= 14 {
                list.dc[id] = dc_pred;
            }
            list.coef[id] = pred;
            continue;
        }
        let mut next = 0i32;
        if id > 13 {
            let dc = r.svlc_range(-128, 127, "scaling_list_dc_coef")?;
            next += dc;
            list.dc[id] = (dc_pred + dc) & 255;
            check(
                list.dc[id] <= 0,
                "The value of ScalingMatrixDcRec shall be greater than 0.",
            )?;
        }
        let scan8 = diag_scan(8, 8);
        let scan = diag_scan(size, size);
        for i in 0..size * size {
            let (x, y) = (scan8[i] as usize % 8, scan8[i] as usize / 8);
            if !(id > 25 && x >= 4 && y >= 4) {
                next += r.svlc_range(-128, 127, "scaling_list_delta_coef")?;
            }
            let pos = scan[i] as usize;
            pred[pos] = (pred[pos] + next) & 255;
            check(
                pred[pos] <= 0,
                "The value of ScalingMatrixRec shall be greater than 0.",
            )?;
        }
        list.coef[id] = pred;
    }
    Ok(list)
}

/// Returns `None` for APS types vvdec ignores.
pub fn parse_aps(r: &mut BitReader) -> Result<Option<Aps>, Error> {
    let kind = r.read(3)?;
    let id = r.read(5)?;
    let chroma_present = r.flag()?;
    let data = match kind {
        0 => {
            check(id > 7, "adaptation_parameter_set_id for ALF_APS")?;
            let mut p = AlfParam {
                luma_coeff: vec![0; 25 * 13],
                luma_clip: vec![0; 25 * 13],
                chroma_coeff: vec![0; 8 * 7],
                chroma_clip: vec![0; 8 * 7],
                ..Default::default()
            };
            p.new_filter[0] = r.flag()?;
            if chroma_present {
                p.new_filter[1] = r.flag()?;
                p.new_cc[0] = r.flag()?;
                p.new_cc[1] = r.flag()?;
            }
            check(
                !p.new_filter[0] && !p.new_filter[1] && !p.new_cc[0] && !p.new_cc[1],
                "one of the ALF filter signal flags shall be nonzero",
            )?;
            if p.new_filter[0] {
                p.nonlinear_luma = r.flag()?;
                let n_minus1 = r.uvlc_range(0, 24, "alf_luma_num_filters_signalled_minus1")?;
                p.num_luma_filters = n_minus1 + 1;
                if n_minus1 > 0 {
                    let len = ceil_log2(n_minus1 + 1);
                    for f in 0..25 {
                        p.coeff_delta_idx[f] =
                            r.code_range(len, 0, n_minus1, "alf_luma_coeff_delta_idx")? as u8;
                    }
                }
                alf_filter_coeffs(r, &mut p, false, 0)?;
            }
            if p.new_filter[1] {
                p.nonlinear_chroma = r.flag()?;
                let alts = r.uvlc_range(0, 7, "alf_chroma_num_alts_minus1")? + 1;
                p.num_alt_chroma = alts;
                for alt in 0..alts as usize {
                    alf_filter_coeffs(r, &mut p, true, alt)?;
                }
            }
            for cc in 0..2 {
                if p.new_cc[cc] {
                    let count = r.uvlc()?;
                    check(count > 3, "alf_cc_filters_signalled_minus1")?;
                    p.cc_count[cc] = count + 1;
                    for f in 0..=count as usize {
                        for i in 0..7 {
                            let code = r.read(3)?;
                            p.cc_coeff[cc][f][i] = if code != 0 {
                                let v = 1i16 << (code - 1);
                                if r.flag()? { -v } else { v }
                            } else {
                                0
                            };
                        }
                    }
                }
            }
            ApsData::Alf(p)
        }
        1 => {
            check(id > 3, "adaptation_parameter_set_id for LMCS_APS")?;
            let mut p = LmcsParam {
                min_bin: r.uvlc_range(0, 15, "lmcs_min_bin_idx")?,
                ..Default::default()
            };
            let delta_max = r.uvlc_range(0, 15, "lmcs_delta_max_bin_idx")?;
            p.max_bin = 15 - delta_max;
            check(
                p.max_bin < p.min_bin,
                "The value of LmcsMaxBinIdx shall be greater than or equal to lmcs_min_bin_idx.",
            )?;
            p.delta_cw_bits = r.uvlc_range(0, 14, "lmcs_delta_cw_prec_minus1")? + 1;
            for i in p.min_bin..=p.max_bin {
                let abs = r.read(p.delta_cw_bits)? as i32;
                p.bin_cw_delta[i as usize] = if abs != 0 && r.flag()? { -abs } else { abs };
            }
            if chroma_present {
                let abs = r.read(3)? as i32;
                p.chroma_offset = if abs > 0 && r.flag()? { -abs } else { abs };
            }
            ApsData::Lmcs(p)
        }
        2 => {
            check(id > 7, "adaptation_parameter_set_id for SCALING_APS")?;
            ApsData::Scaling(parse_scaling_list(r, chroma_present)?)
        }
        _ => return Ok(None),
    };
    if r.flag()? {
        while r.more_rbsp_data()? {
            r.flag()?;
        }
    }
    r.trailing_bits()?;
    Ok(Some(Aps {
        kind,
        id,
        chroma_present,
        data,
    }))
}

#[derive(Clone, Debug, Default)]
pub struct PicHeader {
    pub gdr_or_irap: bool,
    pub non_ref: bool,
    pub gdr: bool,
    pub inter_allowed: bool,
    pub intra_allowed: bool,
    pub pps_id: u32,
    pub poc_lsb: u32,
    pub poc_msb_present: bool,
    pub poc_msb_val: u32,
    pub alf_enabled: [bool; 3],
    pub alf_aps_ids_luma: Vec<u32>,
    pub alf_aps_id_chroma: u32,
    pub ccalf_enabled: [bool; 2],
    pub ccalf_aps_id: [u32; 2],
    pub lmcs_enabled: bool,
    pub lmcs_aps_id: u32,
    pub chroma_residual_scale: bool,
    pub explicit_scaling_list: bool,
    pub scaling_list_aps_id: u32,
    pub vb_present: bool,
    pub vb_pos_x: Vec<u32>,
    pub vb_pos_y: Vec<u32>,
    pub rpl: [RefPicList; 2],
    pub rpl_idx: [i32; 2],
    pub split_cons_override: bool,
    pub min_qt: Constraints,
    pub max_mtt_depth: Constraints,
    pub max_bt: Constraints,
    pub max_tt: Constraints,
    pub cu_qp_delta_subdiv: [u32; 2],
    pub cu_chroma_qp_offset_subdiv: [u32; 2],
    pub temporal_mvp: bool,
    pub col_from_l0: bool,
    pub col_ref_idx: u32,
    pub qp_delta: i32,
    pub joint_cbcr_sign: bool,
    pub sao_enabled: [bool; 2],
    pub deblocking_override: bool,
    pub deblocking_disabled: bool,
    pub beta_offset_div2: [i32; 3],
    pub tc_offset_div2: [i32; 3],
    pub num_l0_weights: u32,
    pub num_l1_weights: u32,
    /// -1 when the picture is not a GDR picture.
    pub recovery_poc_cnt: i32,
    pub pic_output_flag: bool,
    pub mvd_l1_zero: bool,
    pub dis_bdof: bool,
    pub dis_dmvr: bool,
    pub dis_prof: bool,
    pub dis_frac_mmvd: bool,
    pub max_num_affine_merge_cand: u32,
}

fn parse_pic_or_slice_rpl(
    r: &mut BitReader,
    sps: &Sps,
    pps: &Pps,
    rpl: &mut [RefPicList; 2],
    rpl_idx: &mut [i32; 2],
    is_ph: bool,
    ph_inter_allowed: bool,
) -> Result<(), Error> {
    let mut sps_flag = [false; 2];
    for list in 0..2 {
        let num = sps.rpl_lists[list].len() as u32;
        if num > 0 && (list == 0 || pps.rpl1_idx_present) {
            sps_flag[list] = r.flag()?;
        } else if num == 0 {
            sps_flag[list] = false;
        } else if !pps.rpl1_idx_present && list == 1 {
            sps_flag[list] = sps_flag[0];
        }
        if sps_flag[list] {
            let mut idx = 0i32;
            if num == 1 {
                idx = 0;
            } else if list == 1 && !pps.rpl1_idx_present && num > 1 {
                idx = rpl_idx[0];
            }
            if num > 1 && (list == 0 || pps.rpl1_idx_present) {
                idx = r.code_range(ceil_log2(num), 0, num - 1, "rpl_idx")? as i32;
            }
            check(idx < 0 || idx > num as i32 - 1, "rpl_idx out of range")?;
            rpl[list] = sps.rpl_lists[list][idx as usize].clone();
            rpl_idx[list] = idx;
        } else {
            rpl[list] = parse_ref_pic_list(r, sps, true)?;
            rpl_idx[list] = -1;
        }
        if is_ph {
            check(
                pps.rpl_info_in_ph && ph_inter_allowed && rpl[list].num_entries == 0 && list == 0,
                "num_ref_entries[ 0 ] shall be greater than 0",
            )?;
        }
        let ltrp_in_header = rpl[list].ltrp_in_header;
        for j in 0..rpl[list].entries.len() {
            if !rpl[list].entries[j].lt {
                continue;
            }
            if ltrp_in_header {
                rpl[list].entries[j].id = r.read(sps.bits_for_poc)? as i32;
            }
            let present = r.flag()?;
            rpl[list].entries[j].msb_present = present;
            rpl[list].entries[j].msb_cycle = 0;
            if present {
                rpl[list].entries[j].msb_cycle =
                    r.uvlc_range(0, 1 << (32 - sps.bits_for_poc), "delta_poc_msb_cycle_lt")? as i32;
            }
        }
    }
    Ok(())
}

/// One weighted-prediction entry (vvdec's `WPScalingParam`).
#[derive(Clone, Copy, Debug)]
pub struct WpParam {
    pub present: bool,
    pub log2_denom: u32,
    pub weight: i32,
    pub offset: i32,
}

impl Default for WpParam {
    /// vvdec's `resetWpScaling` state.
    fn default() -> Self {
        Self {
            present: false,
            log2_denom: 0,
            weight: 1,
            offset: 0,
        }
    }
}

/// Weights per list, reference index and component.
pub type WpTable = [[[WpParam; 3]; 16]; 2];

fn parse_pred_weight_table(
    r: &mut BitReader,
    sps: &Sps,
    pps: &Pps,
    rpl: &[RefPicList; 2],
    num_active: [u32; 2],
    weights: Option<&mut (u32, u32)>,
    mut table: Option<&mut WpTable>,
) -> Result<(), Error> {
    let chroma = sps.chroma_format_idc != 0;
    let luma_denom = r.uvlc_range(0, 7, "luma_log2_weight_denom")?;
    let mut chroma_denom = 0u32;
    if chroma {
        let delta = r.svlc()?;
        check(
            !(0..=7).contains(&(luma_denom as i32 + delta)),
            "luma_log2_weight_denom + delta_chroma_log2_weight_denom",
        )?;
        chroma_denom = (luma_denom as i32 + delta) as u32;
    }
    let mut sum = 0u32;
    let mut counts = (0u32, 0u32);
    for list in 0..2 {
        let entries = rpl[list].num_entries;
        let mut num = num_active[list];
        if list == 0 {
            if pps.wp_info_in_ph {
                num = r.uvlc_range(1, 15.min(entries), "num_l0_weights")?;
                counts.0 = num;
            }
        } else if !pps.weighted_bipred || (pps.wp_info_in_ph && entries == 0) {
            num = 0;
        } else if pps.wp_info_in_ph && entries > 0 {
            num = r.uvlc_range(1, 15.min(entries), "num_l1_weights")?;
            counts.1 = num;
        }
        let mut luma_flags = Vec::new();
        for _ in 0..num {
            let f = r.flag()?;
            sum += u32::from(f);
            luma_flags.push(f);
        }
        let mut chroma_flags = vec![false; num as usize];
        if chroma {
            for f in chroma_flags.iter_mut() {
                *f = r.flag()?;
                sum += 2 * u32::from(*f);
            }
        }
        for i in 0..num as usize {
            let mut wp = [WpParam::default(); 3];
            wp[0].present = luma_flags[i];
            wp[0].log2_denom = luma_denom;
            wp[0].weight = 1 << luma_denom;
            wp[0].offset = 0;
            if luma_flags[i] {
                wp[0].weight += r.svlc_range(-128, 127, "delta_luma_weight")?;
                wp[0].offset = r.svlc_range(-128, 127, "luma_offset")?;
            }
            for c in 1..3 {
                wp[c].present = chroma_flags[i];
                wp[c].log2_denom = chroma_denom;
                wp[c].weight = 1 << chroma_denom;
                wp[c].offset = 0;
            }
            if chroma_flags[i] {
                for c in 1..3 {
                    wp[c].weight += r.svlc_range(-128, 127, "delta_chroma_weight")?;
                    let delta = r.svlc_range(-512, 508, "delta_chroma_offset")?;
                    wp[c].offset =
                        (128 + delta - ((128 * wp[c].weight) >> chroma_denom)).clamp(-128, 127);
                }
            }
            if let Some(t) = table.as_deref_mut()
                && i < 16
            {
                t[list][i] = wp;
            }
        }
        if let Some(t) = table.as_deref_mut() {
            for i in num as usize..16 {
                for c in 0..3 {
                    t[list][i][c].present = false;
                }
            }
        }
    }
    check(sum > 24, "sumWeightFlags shall be less than or equal to 24")?;
    if let Some(w) = weights {
        *w = counts;
    }
    Ok(())
}

pub fn parse_picture_header(
    r: &mut BitReader,
    sps_list: &[Option<Sps>],
    pps_list: &[Option<Pps>],
    trailing: bool,
) -> Result<PicHeader, Error> {
    let mut ph = PicHeader {
        intra_allowed: true,
        rpl_idx: [-1, -1],
        recovery_poc_cnt: -1,
        pic_output_flag: true,
        mvd_l1_zero: true,
        ..Default::default()
    };
    ph.gdr_or_irap = r.flag()?;
    ph.non_ref = r.flag()?;
    if ph.gdr_or_irap {
        ph.gdr = r.flag()?;
    }
    ph.inter_allowed = r.flag()?;
    if ph.inter_allowed {
        ph.intra_allowed = r.flag()?;
    }
    check(
        !ph.inter_allowed && !ph.intra_allowed,
        "Invalid picture without intra or inter slice",
    )?;
    ph.pps_id = r.uvlc_range(0, 63, "ph_pic_parameter_set_id")?;
    let pps = pps_list[ph.pps_id as usize]
        .as_ref()
        .ok_or(Error::Invalid("Invalid PPS"))?;
    let sps = sps_list[pps.sps_id as usize]
        .as_ref()
        .ok_or(Error::Invalid("Invalid SPS"))?;
    let ctb_log2 = sps.log2_ctu_size;
    let min_cb_log2 = sps.log2_min_cb_size;
    ph.poc_lsb = r.read(sps.bits_for_poc)?;
    if ph.gdr {
        ph.recovery_poc_cnt = r.uvlc_range(0, 1 << sps.bits_for_poc, "ph_recovery_poc_cnt")? as i32;
    }
    for &present in &sps.extra_ph_bits {
        if present {
            r.flag()?;
        }
    }
    if sps.poc_msb_flag {
        ph.poc_msb_present = r.flag()?;
        if ph.poc_msb_present {
            ph.poc_msb_val = r.read(sps.poc_msb_len)?;
        }
    }
    if sps.alf && pps.alf_info_in_ph {
        ph.alf_enabled[0] = r.flag()?;
        if ph.alf_enabled[0] {
            let n = r.code_range(3, 0, 7, "ph_num_alf_aps_ids_luma")?;
            ph.alf_aps_ids_luma = (0..n).map(|_| r.read(3)).collect::<Result<_, _>>()?;
            if sps.chroma_format_idc != 0 {
                ph.alf_enabled[1] = r.flag()?;
                ph.alf_enabled[2] = r.flag()?;
            }
            if ph.alf_enabled[1] || ph.alf_enabled[2] {
                ph.alf_aps_id_chroma = r.read(3)?;
            }
            if sps.ccalf {
                ph.ccalf_enabled[0] = r.flag()?;
                if ph.ccalf_enabled[0] {
                    ph.ccalf_aps_id[0] = r.read(3)?;
                }
                ph.ccalf_enabled[1] = r.flag()?;
                if ph.ccalf_enabled[1] {
                    ph.ccalf_aps_id[1] = r.read(3)?;
                }
            }
        }
    }
    if sps.lmcs {
        ph.lmcs_enabled = r.flag()?;
        if ph.lmcs_enabled {
            ph.lmcs_aps_id = r.read(2)?;
            if sps.chroma_format_idc != 0 {
                ph.chroma_residual_scale = r.flag()?;
            }
        }
    }
    if sps.scaling_list {
        ph.explicit_scaling_list = r.flag()?;
        if ph.explicit_scaling_list {
            ph.scaling_list_aps_id = r.read(3)?;
        }
    }
    if sps.virtual_boundaries_enabled && !sps.virtual_boundaries_present {
        ph.vb_present = r.flag()?;
        if ph.vb_present {
            let nv = r.uvlc_range(
                0,
                if pps.width <= 8 { 0 } else { 3 },
                "ph_num_ver_virtual_boundaries",
            )?;
            let mut prev = 0u32;
            for i in 0..nv {
                let pos = (r.uvlc_range(
                    0,
                    pps.width.div_ceil(8).saturating_sub(2),
                    "ph_virtual_boundary_pos_x_minus1",
                )? + 1)
                    << 3;
                check(
                    i > 0 && pos < prev + sps.ctu_size,
                    "vertical virtual boundary distance",
                )?;
                ph.vb_pos_x.push(pos);
                prev = pos;
            }
            let nh = r.uvlc_range(
                0,
                if pps.height <= 8 { 0 } else { 3 },
                "ph_num_hor_virtual_boundaries",
            )?;
            let mut prev = 0u32;
            for i in 0..nh {
                let pos = (r.uvlc_range(
                    0,
                    pps.height.div_ceil(8).saturating_sub(2),
                    "ph_virtual_boundary_pos_y_minus1",
                )? + 1)
                    << 3;
                check(
                    i > 0 && pos < prev + sps.ctu_size,
                    "horizontal virtual boundary distance",
                )?;
                ph.vb_pos_y.push(pos);
                prev = pos;
            }
            check(
                nv + nh == 0,
                "ph_num_ver_virtual_boundaries + ph_num_hor_virtual_boundaries shall be greater than 0",
            )?;
        }
    } else if sps.virtual_boundaries_present {
        ph.vb_present = true;
        ph.vb_pos_x = sps.vb_pos_x.clone();
        ph.vb_pos_y = sps.vb_pos_y.clone();
        for i in 1..ph.vb_pos_x.len() {
            check(
                ph.vb_pos_x[i] < ph.vb_pos_x[i - 1] + sps.ctu_size,
                "vertical virtual boundary distance",
            )?;
        }
        for i in 1..ph.vb_pos_y.len() {
            check(
                ph.vb_pos_y[i] < ph.vb_pos_y[i - 1] + sps.ctu_size,
                "horizontal virtual boundary distance",
            )?;
        }
    }
    if pps.output_flag_present && !ph.non_ref {
        ph.pic_output_flag = r.flag()?;
    }
    if pps.rpl_info_in_ph {
        let mut rpl = ph.rpl.clone();
        let mut idx = ph.rpl_idx;
        parse_pic_or_slice_rpl(r, sps, pps, &mut rpl, &mut idx, true, ph.inter_allowed)?;
        ph.rpl = rpl;
        ph.rpl_idx = idx;
    }
    if sps.split_cons_override {
        ph.split_cons_override = r.flag()?;
    }
    let mut min_qt = sps.min_qt;
    let mut max_depth = sps.max_mtt_depth;
    let mut max_bt = sps.max_bt;
    let mut max_tt = sps.max_tt;
    if ph.intra_allowed {
        let mut min_qt_log2_y = min_qt[0].trailing_zeros();
        if ph.split_cons_override {
            min_qt_log2_y = r.uvlc_range(
                0,
                6.min(ctb_log2) - min_cb_log2,
                "ph_log2_diff_min_qt_min_cb_intra_slice_luma",
            )? + min_cb_log2;
            min_qt[0] = 1 << min_qt_log2_y;
            max_depth[0] = r.uvlc_range(
                0,
                2 * (ctb_log2 - min_cb_log2),
                "ph_max_mtt_hierarchy_depth_intra_slice_luma",
            )?;
            max_tt[0] = min_qt[0];
            max_bt[0] = min_qt[0];
            if max_depth[0] != 0 {
                let bt_lim = if sps.dual_tree {
                    6.min(ctb_log2)
                } else {
                    ctb_log2
                } as i64
                    - i64::from(min_qt_log2_y);
                let v = r.uvlc()?;
                check(
                    i64::from(v) > bt_lim,
                    "ph_log2_diff_max_bt_min_qt_intra_slice_luma",
                )?;
                max_bt[0] <<= v;
                let v = r.uvlc()?;
                check(
                    i64::from(v) > 6.min(ctb_log2) as i64 - i64::from(min_qt_log2_y),
                    "ph_log2_diff_max_tt_min_qt_intra_slice_luma",
                )?;
                max_tt[0] <<= v;
            }
            if sps.dual_tree {
                let min_qt_c = r.uvlc_range(
                    0,
                    6.min(ctb_log2) - min_cb_log2,
                    "ph_log2_diff_min_qt_min_cb_intra_slice_chroma",
                )? + min_cb_log2;
                min_qt[2] = 1 << min_qt_c;
                max_depth[2] = r.uvlc_range(
                    0,
                    2 * (ctb_log2 - min_cb_log2),
                    "ph_max_mtt_hierarchy_depth_intra_slice_chroma",
                )?;
                max_tt[2] = min_qt[2];
                max_bt[2] = min_qt[2];
                if max_depth[2] != 0 {
                    let lim = 6.min(ctb_log2) as i64 - i64::from(min_qt_c);
                    let v = r.uvlc()?;
                    check(
                        i64::from(v) > lim,
                        "ph_log2_diff_max_bt_min_qt_intra_slice_chroma",
                    )?;
                    max_bt[2] <<= v;
                    let v = r.uvlc()?;
                    check(
                        i64::from(v) > lim,
                        "ph_log2_diff_max_tt_min_qt_intra_slice_chroma",
                    )?;
                    max_tt[2] <<= v;
                }
            }
        }
        let lim = 2 * (ctb_log2 as i64 - i64::from(min_qt_log2_y) + i64::from(max_depth[0]));
        if pps.cu_qp_delta {
            let v = r.uvlc()?;
            check(i64::from(v) > lim, "ph_cu_qp_delta_subdiv_intra_slice")?;
            ph.cu_qp_delta_subdiv[0] = v;
        }
        if pps.cu_chroma_qp_offset_list {
            let v = r.uvlc()?;
            check(
                i64::from(v) > lim,
                "ph_cu_chroma_qp_offset_subdiv_intra_slice",
            )?;
            ph.cu_chroma_qp_offset_subdiv[0] = v;
        }
    }
    if ph.inter_allowed {
        let mut min_qt_log2_inter = min_cb_log2;
        if ph.split_cons_override {
            min_qt_log2_inter += r.uvlc_range(
                0,
                6.min(ctb_log2) - min_cb_log2,
                "ph_log2_diff_min_qt_min_cb_inter_slice",
            )?;
            min_qt[1] = 1 << min_qt_log2_inter;
            max_depth[1] = r.uvlc_range(
                0,
                2 * (ctb_log2 - min_cb_log2),
                "ph_max_mtt_hierarchy_depth_inter_slice",
            )?;
            max_tt[1] = min_qt[1];
            max_bt[1] = min_qt[1];
            if max_depth[1] != 0 {
                max_bt[1] <<= r.uvlc_range(
                    0,
                    ctb_log2 - min_qt_log2_inter,
                    "ph_log2_diff_max_bt_min_qt_inter_slice",
                )?;
                let v = r.uvlc()?;
                check(
                    i64::from(v) > 6.min(ctb_log2) as i64 - i64::from(min_qt_log2_inter),
                    "ph_log2_diff_max_tt_min_qt_inter_slice",
                )?;
                max_tt[1] <<= v;
            }
        }
        let lim = 2 * (ctb_log2 as i64 - i64::from(min_qt_log2_inter) + i64::from(max_depth[1]));
        if pps.cu_qp_delta {
            let v = r.uvlc()?;
            check(i64::from(v) > lim, "ph_cu_qp_delta_subdiv_inter_slice")?;
            ph.cu_qp_delta_subdiv[1] = v;
        }
        if pps.cu_chroma_qp_offset_list {
            let v = r.uvlc()?;
            check(
                i64::from(v) > lim,
                "ph_cu_chroma_qp_offset_subdiv_inter_slice",
            )?;
            ph.cu_chroma_qp_offset_subdiv[1] = v;
        }
        if sps.temporal_mvp {
            ph.temporal_mvp = r.flag()?;
            if ph.temporal_mvp && pps.rpl_info_in_ph {
                ph.col_from_l0 = if ph.rpl[1].num_entries > 0 {
                    r.flag()?
                } else {
                    true
                };
                if (ph.col_from_l0 && ph.rpl[0].num_entries > 1)
                    || (!ph.col_from_l0 && ph.rpl[1].num_entries > 1)
                {
                    let v = r.uvlc()?;
                    let list = if ph.col_from_l0 { 0 } else { 1 };
                    check(v > ph.rpl[list].num_entries - 1, "ph_collocated_ref_idx")?;
                    ph.col_ref_idx = v;
                }
            }
        }
        ph.max_num_affine_merge_cand = if sps.affine {
            sps.max_num_affine_merge_cand
        } else {
            u32::from(sps.sbtmvp && ph.temporal_mvp)
        };
        if sps.mmvd_fullpel_only {
            ph.dis_frac_mmvd = r.flag()?;
        }
        let presence = !pps.rpl_info_in_ph || ph.rpl[1].num_entries > 0;
        ph.dis_bdof = if sps.bdof_control_in_ph {
            true
        } else {
            !sps.bdof
        };
        ph.dis_dmvr = if sps.dmvr_control_in_ph {
            true
        } else {
            !sps.dmvr
        };
        if presence {
            ph.mvd_l1_zero = r.flag()?;
            if sps.bdof_control_in_ph {
                ph.dis_bdof = r.flag()?;
            }
            if sps.dmvr_control_in_ph {
                ph.dis_dmvr = r.flag()?;
            }
        }
        ph.dis_prof = if sps.prof_control_in_ph {
            r.flag()?
        } else {
            !sps.prof
        };
        if (pps.weighted_pred || pps.weighted_bipred) && pps.wp_info_in_ph {
            let mut counts = (0, 0);
            let rpl = ph.rpl.clone();
            parse_pred_weight_table(r, sps, pps, &rpl, [0, 0], Some(&mut counts), None)?;
            ph.num_l0_weights = counts.0;
            ph.num_l1_weights = counts.1;
        }
    }
    ph.min_qt = min_qt;
    ph.max_mtt_depth = max_depth;
    ph.max_bt = max_bt;
    ph.max_tt = max_tt;
    if pps.qp_delta_info_in_ph {
        ph.qp_delta = r.svlc()?;
        let qp = 26 + pps.init_qp_minus26 + ph.qp_delta;
        check(
            qp < -sps.qp_bd_offset || qp > 63,
            "The value of SliceQpY shall be in the range of -QpBdOffset to +63, inclusive.",
        )?;
    }
    if sps.joint_cbcr {
        ph.joint_cbcr_sign = r.flag()?;
    }
    if sps.sao && pps.sao_info_in_ph {
        ph.sao_enabled[0] = r.flag()?;
        if sps.chroma_format_idc != 0 {
            ph.sao_enabled[1] = r.flag()?;
        }
    }
    if pps.dbf_info_in_ph {
        ph.deblocking_override = r.flag()?;
    }
    ph.deblocking_disabled = if pps.deblocking_disabled && ph.deblocking_override {
        false
    } else {
        pps.deblocking_disabled
    };
    ph.beta_offset_div2[0] = pps.beta_offset_div2[0];
    ph.tc_offset_div2[0] = pps.tc_offset_div2[0];
    if ph.deblocking_override {
        if !pps.deblocking_disabled {
            ph.deblocking_disabled = r.flag()?;
        }
        if !ph.deblocking_disabled {
            ph.beta_offset_div2[0] = r.svlc_range(-12, 12, "ph_luma_beta_offset_div2")?;
            ph.tc_offset_div2[0] = r.svlc_range(-12, 12, "ph_luma_tc_offset_div2")?;
        }
    }
    if ph.deblocking_override && !ph.deblocking_disabled && pps.chroma_tool_offsets {
        ph.beta_offset_div2[1] = r.svlc_range(-12, 12, "ph_cb_beta_offset_div2")?;
        ph.tc_offset_div2[1] = r.svlc_range(-12, 12, "ph_cb_tc_offset_div2")?;
        ph.beta_offset_div2[2] = r.svlc_range(-12, 12, "ph_cr_beta_offset_div2")?;
        ph.tc_offset_div2[2] = r.svlc_range(-12, 12, "ph_cr_tc_offset_div2")?;
    } else {
        for c in 1..3 {
            ph.beta_offset_div2[c] = if pps.chroma_tool_offsets {
                pps.beta_offset_div2[c]
            } else {
                ph.beta_offset_div2[0]
            };
            ph.tc_offset_div2[c] = if pps.chroma_tool_offsets {
                pps.tc_offset_div2[c]
            } else {
                ph.tc_offset_div2[0]
            };
        }
    }
    if pps.ph_extension {
        let len = r.uvlc_range(0, 256, "ph_extension_length")?;
        for _ in 0..len {
            r.read(8)?;
        }
    }
    if trailing {
        r.trailing_bits()?;
    }
    Ok(ph)
}

#[derive(Clone, Debug, Default)]
pub struct SliceHeader {
    pub ph_in_sh: bool,
    pub subpic_id: u32,
    pub slice_addr: u32,
    /// CTU raster addresses covered by this slice, in coding order.
    pub ctus: Vec<u32>,
    pub slice_type: u32,
    pub alf_enabled: [bool; 3],
    pub alf_aps_ids_luma: Vec<u32>,
    pub alf_aps_id_chroma: u32,
    pub ccalf_enabled: [bool; 2],
    pub ccalf_aps_id: [u32; 2],
    pub lmcs_used: bool,
    pub explicit_scaling_list_used: bool,
    pub qp: i32,
    pub chroma_qp_delta: [i32; 3],
    pub cu_chroma_qp_offset_enabled: bool,
    pub sao_enabled: [bool; 2],
    pub deblocking_disabled: bool,
    pub beta_offset_div2: [i32; 3],
    pub tc_offset_div2: [i32; 3],
    pub dep_quant: bool,
    pub sign_data_hiding: bool,
    pub ts_residual_coding_disabled: bool,
    /// Entry point offsets, already corrected for emulation prevention bytes.
    pub entry_points: Vec<u32>,
    /// Byte offset of slice data within the NAL unit RBSP.
    pub data_offset: usize,
    pub nal_type: u32,
    pub no_output_of_prior_pics: bool,
    pub rpl: [RefPicList; 2],
    pub rpl_idx: [i32; 2],
    pub num_ref_idx: [u32; 2],
    pub cabac_init: bool,
    pub col_from_l0: bool,
    pub col_ref_idx: u32,
    pub wp: WpTable,
}

pub const B_SLICE: u32 = 0;
pub const P_SLICE: u32 = 1;
pub const I_SLICE: u32 = 2;

pub struct SliceContext<'a> {
    pub sps: &'a Sps,
    pub pps: &'a Pps,
    pub ph: &'a PicHeader,
    pub aps: &'a [Vec<Option<Aps>>],
}

/// Parses a slice header. `nal_type` selects IDR/CRA/GDR handling;
/// `removed` holds emulation-prevention byte positions of the NAL payload.
pub fn parse_slice_header(
    r: &mut BitReader,
    nal_type: u32,
    ctx: &SliceContext,
    ph_in_sh: bool,
    removed: &[usize],
    payload_offset: usize,
) -> Result<SliceHeader, Error> {
    let (sps, pps, ph) = (ctx.sps, ctx.pps, ctx.ph);
    let mut sh = SliceHeader {
        ph_in_sh,
        nal_type,
        rpl_idx: [-1, -1],
        ..Default::default()
    };
    if ph_in_sh {
        check(
            pps.rpl_info_in_ph,
            "When sh_picture_header_in_slice_header_flag is equal to 1, rpl_info_in_ph_flag shall be equal to 0",
        )?;
        check(
            pps.dbf_info_in_ph,
            "When sh_picture_header_in_slice_header_flag is equal to 1, dbf_info_in_ph_flag shall be equal to 0",
        )?;
        check(
            pps.sao_info_in_ph,
            "When sh_picture_header_in_slice_header_flag is equal to 1, sao_info_in_ph_flag shall be equal to 0",
        )?;
        check(
            pps.alf_info_in_ph,
            "When sh_picture_header_in_slice_header_flag is equal to 1, alf_info_in_ph_flag shall be equal to 0",
        )?;
        check(
            pps.wp_info_in_ph,
            "When sh_picture_header_in_slice_header_flag is equal to 1, wp_info_in_ph_flag shall be equal to 0",
        )?;
        check(
            pps.qp_delta_info_in_ph,
            "When sh_picture_header_in_slice_header_flag is equal to 1, qp_delta_info_in_ph_flag shall be equal to 0",
        )?;
        check(
            sps.subpic_info_present,
            "When sps_subpic_info_present_flag is equal to 1, the value of sh_picture_header_in_slice_header_flag shall be equal to 0",
        )?;
    }
    check(
        sps.subpic_info_present
            && sps.virtual_boundaries_enabled
            && !sps.virtual_boundaries_present,
        "sps_virtual_boundaries_present_flag shall be equal 1",
    )?;
    let chroma = sps.chroma_format_idc != 0;
    if sps.subpic_info_present {
        sh.subpic_id = r.read(sps.subpic_id_len)?;
    }
    let num_tiles = pps.num_tiles();
    let mut slice_addr = 0u32;
    if !pps.rect_slice {
        if num_tiles > 1 {
            slice_addr =
                r.code_range(ceil_log2(num_tiles), 0, num_tiles - 1, "sh_slice_address")?;
        }
    } else {
        let sub = &pps.subpics[pps.subpic_idx_from_id(sh.subpic_id)];
        if sub.num_slices > 1 {
            slice_addr = r.code_range(
                ceil_log2(sub.num_slices),
                0,
                sub.num_slices - 1,
                "sh_slice_address",
            )?;
        }
    }
    for &present in &sps.extra_sh_bits {
        if present {
            r.flag()?;
        }
    }
    let mut num_tiles_in_slice = 1;
    if !pps.rect_slice && num_tiles as i64 - i64::from(slice_addr) > 1 {
        num_tiles_in_slice = r.uvlc_range(0, num_tiles - 1, "sh_num_tiles_in_slice_minus1")? + 1;
    }
    if !pps.rect_slice {
        check(slice_addr >= num_tiles, "Invalid slice address")?;
        for t in slice_addr..slice_addr + num_tiles_in_slice {
            let tx = t % pps.num_tile_cols();
            let ty = t / pps.num_tile_cols();
            check(
                ty >= pps.num_tile_rows(),
                "Number of tiles in slice exceeds the remaining number of tiles in picture",
            )?;
            add_ctus(
                &mut sh.ctus,
                pps.tile_col_bd[tx as usize],
                pps.tile_col_bd[tx as usize + 1],
                pps.tile_row_bd[ty as usize],
                pps.tile_row_bd[ty as usize + 1],
                pps.width_ctus,
            );
        }
        sh.slice_addr = slice_addr;
    } else {
        let mut idx = slice_addr;
        let sub_idx = pps.subpic_idx_from_id(sh.subpic_id);
        for s in &pps.subpics[..sub_idx] {
            idx += s.num_slices;
        }
        sh.ctus = pps
            .slice_map
            .get(idx as usize)
            .cloned()
            .ok_or(Error::Invalid("slice index"))?;
        sh.slice_addr = idx;
    }
    sh.slice_type = if ph.inter_allowed {
        r.uvlc_range(0, 2, "sh_slice_type")?
    } else {
        I_SLICE
    };
    check(
        !ph.intra_allowed && sh.slice_type == I_SLICE,
        "When ph_intra_slice_allowed_flag is equal to 0, the value of sh_slice_type shall be equal to 0 or 1.",
    )?;
    // IDR_W_RADL (7), IDR_N_LP (8), CRA (9), GDR (10)
    if (7..=10).contains(&nal_type) {
        sh.no_output_of_prior_pics = r.flag()?;
    }
    // inherit from picture header
    let mut rpl = [RefPicList::default(), RefPicList::default()];
    let mut rpl_idx = [-1i32; 2];
    if pps.rpl_info_in_ph {
        rpl = ph.rpl.clone();
        rpl_idx = ph.rpl_idx;
    }
    if pps.qp_delta_info_in_ph {
        sh.qp = 26 + pps.init_qp_minus26 + ph.qp_delta;
    }
    sh.deblocking_disabled = ph.deblocking_disabled;
    sh.beta_offset_div2[0] = ph.beta_offset_div2[0];
    sh.tc_offset_div2[0] = ph.tc_offset_div2[0];
    for c in 1..3 {
        sh.beta_offset_div2[c] = if pps.chroma_tool_offsets {
            ph.beta_offset_div2[c]
        } else {
            sh.beta_offset_div2[0]
        };
        sh.tc_offset_div2[c] = if pps.chroma_tool_offsets {
            ph.tc_offset_div2[c]
        } else {
            sh.tc_offset_div2[0]
        };
    }
    sh.sao_enabled = ph.sao_enabled;
    sh.alf_enabled = ph.alf_enabled;
    sh.alf_aps_ids_luma = ph.alf_aps_ids_luma.clone();
    sh.alf_aps_id_chroma = ph.alf_aps_id_chroma;
    sh.ccalf_enabled = ph.ccalf_enabled;
    sh.ccalf_aps_id = ph.ccalf_aps_id;
    sh.lmcs_used = if ph_in_sh { ph.lmcs_enabled } else { false };
    sh.explicit_scaling_list_used = if ph_in_sh {
        ph.explicit_scaling_list
    } else {
        false
    };
    let alf_aps = |id: u32| -> Option<&AlfParam> {
        match ctx.aps[0].get(id as usize)?.as_ref()?.data {
            ApsData::Alf(ref p) => Some(p),
            _ => None,
        }
    };
    if sps.alf && !pps.alf_info_in_ph {
        sh.alf_enabled[0] = r.flag()?;
        if sh.alf_enabled[0] {
            let n = r.read(3)?;
            sh.alf_aps_ids_luma.clear();
            for _ in 0..n {
                let id = r.read(3)?;
                let aps = alf_aps(id).ok_or(Error::Invalid("referenced APS not found"))?;
                check(
                    !aps.new_filter[0],
                    "alf_luma_filter_signal_flag of the referenced APS shall be equal to 1",
                )?;
                sh.alf_aps_ids_luma.push(id);
            }
            if chroma {
                sh.alf_enabled[1] = r.flag()?;
                sh.alf_enabled[2] = r.flag()?;
            }
            if sh.alf_enabled[1] || sh.alf_enabled[2] {
                sh.alf_aps_id_chroma = r.read(3)?;
                let aps = alf_aps(sh.alf_aps_id_chroma)
                    .ok_or(Error::Invalid("referenced APS not found"))?;
                check(
                    !aps.new_filter[1],
                    "alf_chroma_filter_signal_flag of the referenced APS shall be equal to 1",
                )?;
            }
            if sps.ccalf {
                for cc in 0..2 {
                    sh.ccalf_enabled[cc] = r.flag()?;
                    if sh.ccalf_enabled[cc] {
                        sh.ccalf_aps_id[cc] = r.read(3)?;
                        let aps = alf_aps(sh.ccalf_aps_id[cc])
                            .ok_or(Error::Invalid("referenced APS not found"))?;
                        check(
                            !aps.new_cc[cc],
                            "alf_cc_filter_signal_flag of the referenced APS shall be equal to 1",
                        )?;
                    }
                }
            }
        }
    }
    if ph.lmcs_enabled && !ph_in_sh {
        sh.lmcs_used = r.flag()?;
    }
    if ph.explicit_scaling_list && !ph_in_sh {
        sh.explicit_scaling_list_used = r.flag()?;
    }
    let idr = nal_type == 7 || nal_type == 8;
    if pps.rpl_info_in_ph {
    } else if idr && !sps.idr_rpl_present {
        rpl = [RefPicList::default(), RefPicList::default()];
    } else {
        parse_pic_or_slice_rpl(r, sps, pps, &mut rpl, &mut rpl_idx, false, false)?;
    }
    let intra = sh.slice_type == I_SLICE;
    let is_b = sh.slice_type == B_SLICE;
    let is_p = sh.slice_type == P_SLICE;
    let mut override_flag = true;
    let mut active_minus1 = [0u32; 2];
    if (!intra && rpl[0].num_entries > 1) || (is_b && rpl[1].num_entries > 1) {
        override_flag = r.flag()?;
        if override_flag {
            for i in 0..if is_b { 2 } else { 1 } {
                if rpl[i].num_entries > 1 {
                    active_minus1[i] = r.uvlc_range(0, 14, "sh_num_ref_idx_active_minus1")?;
                }
            }
        }
    }
    let mut num_ref_idx = [0u32; 2];
    for i in 0..2 {
        if is_b || (is_p && i == 0) {
            num_ref_idx[i] = if override_flag {
                active_minus1[i] + 1
            } else if rpl[i].num_entries >= pps.num_ref_idx_default[i] {
                pps.num_ref_idx_default[i]
            } else {
                rpl[i].num_entries
            };
        }
    }
    if is_p || is_b {
        check(
            num_ref_idx[0] == 0,
            "Number of active entries in RPL0 of P or B picture shall be greater than 0",
        )?;
        if is_b {
            check(
                num_ref_idx[1] == 0,
                "Number of active entries in RPL1 of B picture shall be greater than 0",
            )?;
        }
    }
    sh.col_from_l0 = if is_b { ph.col_from_l0 } else { true };
    sh.col_ref_idx = if pps.rpl_info_in_ph {
        ph.col_ref_idx
    } else {
        0
    };
    if !intra {
        if pps.cabac_init_present {
            sh.cabac_init = r.flag()?;
        }
        if ph.temporal_mvp && !pps.rpl_info_in_ph {
            if is_b {
                sh.col_from_l0 = r.flag()?;
            }
            let col_from_l0 = sh.col_from_l0;
            if (col_from_l0 && num_ref_idx[0] > 1) || (!col_from_l0 && num_ref_idx[1] > 1) {
                let list = if col_from_l0 { 0 } else { 1 };
                sh.col_ref_idx = r.uvlc_range(0, num_ref_idx[list] - 1, "sh_collocated_ref_idx")?;
            }
        }
        if !pps.wp_info_in_ph && ((pps.weighted_pred && is_p) || (pps.weighted_bipred && is_b)) {
            parse_pred_weight_table(r, sps, pps, &rpl, num_ref_idx, None, Some(&mut sh.wp))?;
        }
        if pps.wp_info_in_ph {
            check(
                pps.weighted_pred && is_p && num_ref_idx[0] > ph.num_l0_weights,
                "NumRefIdxActive[ 0 ] shall be less than or equal to the value of NumWeightsL0",
            )?;
            check(
                pps.weighted_bipred && is_b && num_ref_idx[0] > ph.num_l0_weights,
                "NumRefIdxActive[ 0 ] shall be less than or equal to the value of NumWeightsL0",
            )?;
            check(
                pps.weighted_bipred && is_b && num_ref_idx[1] > ph.num_l1_weights,
                "NumRefIdxActive[ 1 ] shall be less than or equal to the value of NumWeightsL1",
            )?;
        }
    }
    sh.rpl = rpl.clone();
    sh.rpl_idx = rpl_idx;
    sh.num_ref_idx = num_ref_idx;
    if !pps.qp_delta_info_in_ph {
        let delta = r.svlc()?;
        let qp = 26 + pps.init_qp_minus26 + delta;
        check(
            qp < -sps.qp_bd_offset || qp > MAX_QP,
            "SliceQpY out of range",
        )?;
        sh.qp = qp;
    }
    if pps.slice_chroma_qp_offsets {
        sh.chroma_qp_delta[0] = r.svlc_range(-12, 12, "sh_cb_qp_offset")?;
        check(
            !(-12..=12).contains(&(sh.chroma_qp_delta[0] + pps.cb_qp_offset)),
            "pps_cb_qp_offset + sh_cb_qp_offset",
        )?;
        sh.chroma_qp_delta[1] = r.svlc_range(-12, 12, "sh_cr_qp_offset")?;
        check(
            !(-12..=12).contains(&(sh.chroma_qp_delta[1] + pps.cr_qp_offset)),
            "pps_cr_qp_offset + sh_cr_qp_offset",
        )?;
        if sps.joint_cbcr {
            sh.chroma_qp_delta[2] = r.svlc_range(-12, 12, "sh_joint_cbcr_qp_offset")?;
            check(
                !(-12..=12).contains(&(sh.chroma_qp_delta[2] + pps.joint_cbcr_qp_offset)),
                "pps_joint_cbcr_qp_offset_value + sh_joint_cbcr_qp_offset",
            )?;
        }
    }
    if pps.cu_chroma_qp_offset_list {
        sh.cu_chroma_qp_offset_enabled = r.flag()?;
    }
    if sps.sao && !pps.sao_info_in_ph {
        sh.sao_enabled[0] = r.flag()?;
        if chroma {
            sh.sao_enabled[1] = r.flag()?;
        }
    }
    let mut dbf_override = false;
    if pps.deblocking_override_enabled && !pps.dbf_info_in_ph {
        dbf_override = r.flag()?;
    }
    sh.deblocking_disabled = if pps.deblocking_disabled && dbf_override {
        false
    } else {
        ph.deblocking_disabled
    };
    if dbf_override {
        if !pps.deblocking_disabled {
            sh.deblocking_disabled = r.flag()?;
        }
        if !sh.deblocking_disabled {
            sh.beta_offset_div2[0] = r.svlc_range(-12, 12, "sh_luma_beta_offset_div2")?;
            sh.tc_offset_div2[0] = r.svlc_range(-12, 12, "sh_luma_tc_offset_div2")?;
        }
    }
    if dbf_override && !sh.deblocking_disabled && pps.chroma_tool_offsets {
        sh.beta_offset_div2[1] = r.svlc_range(-12, 12, "sh_cb_beta_offset_div2")?;
        sh.tc_offset_div2[1] = r.svlc_range(-12, 12, "sh_cb_tc_offset_div2")?;
        sh.beta_offset_div2[2] = r.svlc_range(-12, 12, "sh_cr_beta_offset_div2")?;
        sh.tc_offset_div2[2] = r.svlc_range(-12, 12, "sh_cr_tc_offset_div2")?;
    } else if pps.chroma_tool_offsets {
        for c in 1..3 {
            sh.beta_offset_div2[c] = ph.beta_offset_div2[c];
            sh.tc_offset_div2[c] = ph.tc_offset_div2[c];
        }
    } else {
        for c in 1..3 {
            sh.beta_offset_div2[c] = sh.beta_offset_div2[0];
            sh.tc_offset_div2[c] = sh.tc_offset_div2[0];
        }
    }
    if sps.dep_quant {
        sh.dep_quant = r.flag()?;
    }
    if sps.sign_data_hiding && !sh.dep_quant {
        sh.sign_data_hiding = r.flag()?;
    }
    if sps.transform_skip && !sh.dep_quant && !sh.sign_data_hiding {
        sh.ts_residual_coding_disabled = r.flag()?;
    }
    if pps.sh_extension {
        let len = r.uvlc_range(0, 256, "sh_slice_header_extension_length")?;
        for _ in 0..len {
            r.read(8)?;
        }
    }
    // Entry points: tile starts and, with WPP, CTU row starts.
    let mut num_entry_points = 0usize;
    if sps.entry_points_present && !sh.ctus.is_empty() {
        let w = pps.width_ctus;
        let (mut px, mut py) = (sh.ctus[0] % w, sh.ctus[0] / w);
        for &addr in &sh.ctus[1..] {
            let (x, y) = (addr % w, addr / w);
            if pps.ctu_to_tile_row[y as usize] != pps.ctu_to_tile_row[py as usize]
                || pps.ctu_to_tile_col[x as usize] != pps.ctu_to_tile_col[px as usize]
                || (y != py && sps.entropy_coding_sync)
            {
                num_entry_points += 1;
            }
            px = x;
            py = y;
        }
    }
    let mut offsets = Vec::with_capacity(num_entry_points);
    if num_entry_points > 0 {
        let len = r.uvlc_range(0, 31, "sh_entry_offset_len_minus1")? + 1;
        for _ in 0..num_entry_points {
            offsets.push(r.read(len)?.wrapping_add(1));
        }
    }
    r.byte_alignment()?;
    sh.data_offset = r.byte_pos();
    if !offsets.is_empty() {
        // Positions in `removed` are relative to the NAL payload start; the
        // slice header ends at `data_offset` RBSP bytes, which is later in
        // the escaped payload by the number of bytes removed before it.
        let mut end = sh.data_offset + payload_offset;
        for &pos in removed {
            if pos < end {
                end += 1;
            }
        }
        let mut prev = 0u32;
        for offset in offsets.iter_mut() {
            let current = prev.wrapping_add(*offset);
            let count = removed
                .iter()
                .filter(|&&pos| pos >= (prev as usize) + end && pos < (current as usize) + end)
                .count() as u32;
            *offset = offset.wrapping_sub(count);
            prev = current;
        }
    }
    sh.entry_points = offsets;
    Ok(sh)
}
