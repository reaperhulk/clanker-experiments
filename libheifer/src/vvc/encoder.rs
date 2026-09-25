// SPDX-License-Identifier: LGPL-3.0-or-later
//! All-intra VVC (H.266) encoder for still images.
//!
//! The encoder writes a restricted toolset: 64x64 CTUs split by quadtree
//! only, the 67 regular intra luma modes, the five non-CCLM chroma modes,
//! DCT-II transforms with scalar quantization, and the deblocking filter.
//! Decisions reuse the decoder's partitioner, context derivations and
//! reconstruction ([`super::recon::reconstruct_cu`]), so the encoder's
//! reconstruction is the decoder's by construction; the syntax writer
//! mirrors the parser in [`super::ctu`].
use super::Error;
use super::bits::{BitReader, BitWriter, escape};
use super::cabac::{BinSink, CabacWriter, Contexts, Estimator, rem_abs_bins};
use super::ctu::{CoeffCtx, Partitioner, SliceInfo, SliceInter, Split, intra_mpms};
use super::ctx;
use super::pic::SaoParam;
use super::pic::*;
use super::ps::{self, PicHeader, Pps, SliceHeader, Sps};
use super::recon;
use super::tables::*;

/// Encoder settings.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Slice QP, 0..=63.
    pub qp: i32,
    /// chroma_format_idc: 0 (4:0:0), 1 (4:2:0), 2 (4:2:2) or 3 (4:4:4).
    pub chroma: u32,
    /// Sample aspect ratio signalled as Extended_SAR in the VUI.
    pub sar: Option<(u16, u16)>,
    /// Encoder effort, 0 (fastest) to 2.
    pub effort: u32,
    /// Enables the deblocking filter.
    pub deblocking: bool,
    /// Picture rate for the level choice.
    pub fps: f64,
}

/// An 8-bit picture whose dimensions are multiples of 8.
pub struct Picture8<'a> {
    pub width: u32,
    pub height: u32,
    /// Y, Cb and Cr planes, tightly packed (Cb and Cr absent for 4:0:0).
    pub planes: [&'a [u8]; 3],
}

/// sps_max_mtt_hierarchy_depth_intra_slice_luma.
const MAX_MTT_DEPTH: u32 = 2;

/// Ternary splits are tried when the binary split in the same direction
/// costs at most this factor more than the best choice, per effort.
const TT_MARGIN: [f64; 3] = [1.0, 1.02, 1.1];

const NAL_SPS: u32 = 15;
const NAL_PPS: u32 = 16;
const NAL_IDR_N_LP: u32 = 8;

fn nal(kind: u32, rbsp: &[u8]) -> Vec<u8> {
    let mut out = vec![0, ((kind << 3) | 1) as u8];
    out.extend_from_slice(&escape(rbsp));
    out
}

/// general_level_idc: the lowest level whose MaxLumaPs, maximum dimension
/// and MaxLumaSr (at `fps` pictures per second) admit the picture (H.266
/// tables A.1 and A.2).
fn level_idc(width: u32, height: u32, fps: f64) -> u32 {
    const LEVELS: [(u32, u64, u64); 13] = [
        (16, 36_864, 552_960),
        (32, 122_880, 3_686_400),
        (35, 245_760, 7_372_800),
        (48, 552_960, 16_588_800),
        (51, 983_040, 33_177_600),
        (64, 2_228_224, 66_846_720),
        (67, 2_228_224, 133_693_440),
        (80, 8_912_896, 267_386_880),
        (83, 8_912_896, 534_773_760),
        (86, 8_912_896, 1_069_547_520),
        (96, 35_651_584, 1_069_547_520),
        (99, 35_651_584, 2_139_095_040),
        (102, 35_651_584, 4_278_190_080),
    ];
    let ps = u64::from(width) * u64::from(height);
    for &(idc, max_ps, max_sr) in &LEVELS {
        let max_dim = ((max_ps * 8) as f64).sqrt() as u64;
        if ps <= max_ps
            && u64::from(width) <= max_dim
            && u64::from(height) <= max_dim
            && ps as f64 * fps <= max_sr as f64
        {
            return idc;
        }
    }
    105
}

/// general_profile_idc: Main 10 for 4:0:0 and 4:2:0, Main 10 4:4:4
/// otherwise.
pub fn profile_idc(chroma: u32) -> u32 {
    if chroma <= 1 { 1 } else { 33 }
}

fn write_sps(s: &Settings, width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.write(0, 4); // sps_seq_parameter_set_id
    w.write(0, 4); // sps_video_parameter_set_id
    w.write(0, 3); // sps_max_sublayers_minus1
    w.write(s.chroma, 2);
    w.write(1, 2); // sps_log2_ctu_size_minus5: 64x64
    w.flag(true); // sps_ptl_dpb_hrd_params_present_flag
    // profile_tier_level( 1, 0 )
    w.write(profile_idc(s.chroma), 7);
    w.flag(false); // general_tier_flag
    w.write(level_idc(width, height, s.fps), 8);
    w.flag(true); // ptl_frame_only_constraint_flag
    w.flag(false); // ptl_multilayer_enabled_flag
    w.flag(false); // gci_present_flag
    w.align_zero(); // gci_alignment_zero_bit
    w.write(0, 8); // ptl_num_sub_profiles
    w.flag(false); // sps_gdr_enabled_flag
    w.flag(false); // sps_ref_pic_resampling_enabled_flag
    w.uvlc(width);
    w.uvlc(height);
    w.flag(false); // sps_conformance_window_flag
    w.flag(false); // sps_subpic_info_present_flag
    w.uvlc(0); // sps_bitdepth_minus8
    w.flag(false); // sps_entropy_coding_sync_enabled_flag
    w.flag(false); // sps_entry_point_offsets_present_flag
    w.write(4, 4); // sps_log2_max_pic_order_cnt_lsb_minus4
    w.flag(false); // sps_poc_msb_cycle_flag
    w.write(0, 2); // sps_num_extra_ph_bytes
    w.write(0, 2); // sps_num_extra_sh_bytes
    // dpb_parameters( 0, 0 )
    w.uvlc(0); // dpb_max_dec_pic_buffering_minus1
    w.uvlc(0); // dpb_max_num_reorder_pics
    w.uvlc(0); // dpb_max_latency_increase_plus1
    w.uvlc(0); // sps_log2_min_luma_coding_block_size_minus2: 4
    w.flag(false); // sps_partition_constraints_override_enabled_flag
    w.uvlc(1); // sps_log2_diff_min_qt_min_cb_intra_slice_luma: MinQt 8
    w.uvlc(MAX_MTT_DEPTH); // sps_max_mtt_hierarchy_depth_intra_slice_luma
    w.uvlc(2); // sps_log2_diff_max_bt_min_qt_intra_slice_luma: MaxBt 32
    w.uvlc(2); // sps_log2_diff_max_tt_min_qt_intra_slice_luma: MaxTt 32
    if s.chroma != 0 {
        w.flag(false); // sps_qtbtt_dual_tree_intra_flag
    }
    w.uvlc(1); // sps_log2_diff_min_qt_min_cb_inter_slice
    w.uvlc(0); // sps_max_mtt_hierarchy_depth_inter_slice
    w.flag(true); // sps_max_luma_transform_size_64_flag
    w.flag(false); // sps_transform_skip_enabled_flag
    w.flag(true); // sps_mts_enabled_flag
    w.flag(false); // sps_explicit_mts_intra_enabled_flag: implicit MTS
    w.flag(false); // sps_explicit_mts_inter_enabled_flag
    w.flag(true); // sps_lfnst_enabled_flag
    if s.chroma != 0 {
        w.flag(false); // sps_joint_cbcr_enabled_flag
        w.flag(true); // sps_same_qp_table_for_chroma_flag
        // vvenc's default chroma QP mapping: (17,17) (22,23) (34,35) (42,39).
        w.svlc(17 - 26); // sps_qp_table_start_minus26
        w.uvlc(2); // sps_num_points_in_qp_table_minus1
        for (din, dout) in [(4u32, 6u32), (11, 12), (7, 4)] {
            w.uvlc(din); // sps_delta_qp_in_val_minus1
            w.uvlc(dout ^ din); // sps_delta_qp_diff_val
        }
    }
    w.flag(s.deblocking); // sps_sao_enabled_flag
    w.flag(false); // sps_alf_enabled_flag
    w.flag(false); // sps_lmcs_enabled_flag
    w.flag(false); // sps_weighted_pred_flag
    w.flag(false); // sps_weighted_bipred_flag
    w.flag(false); // sps_long_term_ref_pics_flag
    w.flag(false); // sps_idr_rpl_present_flag
    w.flag(true); // sps_rpl1_same_as_rpl0_flag
    w.uvlc(0); // sps_num_ref_pic_lists[ 0 ]
    w.flag(false); // sps_ref_wraparound_enabled_flag
    w.flag(false); // sps_temporal_mvp_enabled_flag
    w.flag(false); // sps_amvr_enabled_flag
    w.flag(false); // sps_bdof_enabled_flag
    w.flag(false); // sps_smvd_enabled_flag
    w.flag(false); // sps_dmvr_enabled_flag
    w.flag(false); // sps_mmvd_enabled_flag
    w.uvlc(0); // sps_six_minus_max_num_merge_cand
    w.flag(false); // sps_sbt_enabled_flag
    w.flag(false); // sps_affine_enabled_flag
    w.flag(false); // sps_bcw_enabled_flag
    w.flag(false); // sps_ciip_enabled_flag
    w.flag(false); // sps_gpm_enabled_flag
    w.uvlc(0); // sps_log2_parallel_merge_level_minus2
    w.flag(false); // sps_isp_enabled_flag
    w.flag(true); // sps_mrl_enabled_flag
    w.flag(true); // sps_mip_enabled_flag
    if s.chroma != 0 {
        w.flag(true); // sps_cclm_enabled_flag
    }
    if s.chroma == 1 {
        w.flag(true); // sps_chroma_horizontal_collocated_flag
        w.flag(false); // sps_chroma_vertical_collocated_flag
    }
    w.flag(false); // sps_palette_enabled_flag
    // No sps_act_enabled_flag: the maximum transform size is 64.
    w.flag(false); // sps_ibc_enabled_flag
    w.flag(false); // sps_ladf_enabled_flag
    w.flag(false); // sps_explicit_scaling_list_enabled_flag
    w.flag(false); // sps_dep_quant_enabled_flag
    w.flag(false); // sps_sign_data_hiding_enabled_flag
    w.flag(false); // sps_virtual_boundaries_enabled_flag
    w.flag(false); // sps_timing_hrd_params_present_flag
    w.flag(false); // sps_field_seq_flag
    match s.sar {
        Some((sw, sh)) => {
            w.flag(true); // sps_vui_parameters_present_flag
            let mut v = BitWriter::default();
            v.flag(true); // vui_progressive_source_flag
            v.flag(false); // vui_interlaced_source_flag
            v.flag(false); // vui_non_packed_constraint_flag
            v.flag(false); // vui_non_projected_constraint_flag
            v.flag(true); // vui_aspect_ratio_info_present_flag
            v.flag(false); // vui_aspect_ratio_constant_flag
            v.write(255, 8); // vui_aspect_ratio_idc: Extended_SAR
            v.write(u32::from(sw), 16);
            v.write(u32::from(sh), 16);
            v.flag(false); // vui_overscan_info_present_flag
            v.flag(false); // vui_colour_description_present_flag
            v.flag(false); // vui_chroma_loc_info_present_flag
            // vui_payload_bit_equal_to_one and alignment
            v.trailing_bits();
            w.uvlc(v.data.len() as u32 - 1); // sps_vui_payload_size_minus1
            w.align_zero();
            for b in v.data {
                w.write(u32::from(b), 8);
            }
        }
        None => w.flag(false),
    }
    w.flag(false); // sps_extension_present_flag
    w.trailing_bits();
    w.data
}

fn write_pps(s: &Settings, width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.write(0, 6); // pps_pic_parameter_set_id
    w.write(0, 4); // pps_seq_parameter_set_id
    w.flag(false); // pps_mixed_nalu_types_in_pic_flag
    w.uvlc(width);
    w.uvlc(height);
    w.flag(false); // pps_conformance_window_flag
    w.flag(false); // pps_scaling_window_explicit_signalling_flag
    w.flag(false); // pps_output_flag_present_flag
    w.flag(true); // pps_no_pic_partition_flag
    w.flag(false); // pps_subpic_id_mapping_present_flag
    w.flag(false); // pps_cabac_init_present_flag
    w.uvlc(0); // pps_num_ref_idx_default_active_minus1[ 0 ]
    w.uvlc(0); // pps_num_ref_idx_default_active_minus1[ 1 ]
    w.flag(false); // pps_rpl1_idx_present_flag
    w.flag(false); // pps_weighted_pred_flag
    w.flag(false); // pps_weighted_bipred_flag
    w.flag(false); // pps_ref_wraparound_enabled_flag
    w.svlc(s.qp - 26); // pps_init_qp_minus26
    w.flag(false); // pps_cu_qp_delta_enabled_flag
    w.flag(false); // pps_chroma_tool_offsets_present_flag
    w.flag(!s.deblocking); // pps_deblocking_filter_control_present_flag
    if !s.deblocking {
        w.flag(false); // pps_deblocking_filter_override_enabled_flag
        w.flag(true); // pps_deblocking_filter_disabled_flag
    }
    w.flag(false); // pps_picture_header_extension_present_flag
    w.flag(false); // pps_slice_header_extension_present_flag
    w.flag(false); // pps_extension_flag
    w.trailing_bits();
    w.data
}

/// The slice header with the picture header in it, up to and including
/// byte_alignment().
fn write_slice_header(s: &Settings) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.flag(true); // sh_picture_header_in_slice_header_flag
    // picture_header_structure( )
    w.flag(true); // ph_gdr_or_irap_pic_flag
    w.flag(false); // ph_non_ref_pic_flag
    w.flag(false); // ph_gdr_pic_flag
    w.flag(false); // ph_inter_slice_allowed_flag
    w.uvlc(0); // ph_pic_parameter_set_id
    w.write(0, 8); // ph_pic_order_cnt_lsb
    w.flag(false); // sh_no_output_of_prior_pics_flag
    w.svlc(0); // sh_qp_delta
    if s.deblocking {
        w.flag(true); // sh_sao_luma_used_flag
        if s.chroma != 0 {
            w.flag(true); // sh_sao_chroma_used_flag
        }
    }
    w.trailing_bits(); // byte_alignment( )
    w.data
}

/// Parsed parameter sets for the encoder's own headers.
struct Headers {
    sps: Sps,
    pps: Pps,
    ph: PicHeader,
    sh: SliceHeader,
}

fn parse_headers(sps_rbsp: &[u8], pps_rbsp: &[u8], sh_rbsp: &[u8]) -> Result<Headers, Error> {
    let sps = ps::parse_sps(&mut BitReader::new(sps_rbsp))?;
    let mut sps_list = vec![None; 16];
    sps_list[0] = Some(sps.clone());
    let pps = ps::parse_pps(&mut BitReader::new(pps_rbsp), &sps_list)?;
    let mut pps_list = vec![None; 64];
    pps_list[0] = Some(pps.clone());
    let mut r = BitReader::new(sh_rbsp);
    if !r.flag()? {
        return Err(Error::Invalid("picture header not in slice header"));
    }
    let ph = ps::parse_picture_header(&mut r, &sps_list, &pps_list, false)?;
    let aps: Vec<Vec<Option<ps::Aps>>> = vec![vec![None; 8], vec![None; 8], vec![None; 8]];
    let ctx = ps::SliceContext {
        sps: &sps,
        pps: &pps,
        ph: &ph,
        aps: &aps,
    };
    let sh = ps::parse_slice_header(&mut r, NAL_IDR_N_LP, &ctx, true, &[], 0)?;
    if sh.data_offset != sh_rbsp.len() {
        return Err(Error::Invalid("slice header length"));
    }
    Ok(Headers { sps, pps, ph, sh })
}

/// Encodes a picture; returns the SPS, PPS and slice NAL units without
/// start codes, and the reconstruction before in-loop filtering.
pub fn encode(input: &Picture8, s: &Settings) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>), Error> {
    let (w, h) = (input.width, input.height);
    if w == 0 || h == 0 || w % 8 != 0 || h % 8 != 0 || s.chroma > 3 {
        return Err(Error::Invalid("encoder input size"));
    }
    let sps_rbsp = write_sps(s, w, h);
    let pps_rbsp = write_pps(s, w, h);
    let sh_rbsp = write_slice_header(s);
    let hd = parse_headers(&sps_rbsp, &pps_rbsp, &sh_rbsp)?;
    let inter = SliceInter {
        refs: Default::default(),
        info: Default::default(),
        col: None,
        check_ldc: false,
        bidir: false,
        sym_ref: [-1, -1],
    };
    let alf: [Option<ps::AlfParam>; 8] = Default::default();
    let si = SliceInfo {
        sps: &hd.sps,
        pps: &hd.pps,
        ph: &hd.ph,
        sh: &hd.sh,
        slice_idx: 0,
        alf_aps: &alf,
        lmcs: None,
        scaling: None,
        inter: &inter,
    };
    let mut pic = Picture::new(&hd.sps, &hd.pps);
    let fmt = pic.fmt;
    let src: Vec<Plane> = (0..fmt.num_comp())
        .map(|c| {
            let (sx, sy) = fmt.scale(c);
            let (pw, ph) = ((w >> sx) as usize, (h >> sy) as usize);
            let mut p = Plane::new(pw, ph);
            for (d, &v) in p.data.iter_mut().zip(&input.planes[c][..pw * ph]) {
                *d = i16::from(v);
            }
            p
        })
        .collect();
    let mut enc = Enc {
        si: &si,
        src,
        qp: hd.sh.qp,
        lambda: 0.57 * 2f64.powf(f64::from(hd.sh.qp - 12) / 3.0),
        effort: s.effort,
        rdoq: true,
    };
    let ctx0 = Contexts::new(2, hd.sh.qp);
    let ctu = hd.sps.ctu_size as i32;
    let ctu_area = |addr: u32| {
        let (cx, cy) = (addr % hd.pps.width_ctus, addr / hd.pps.width_ctus);
        fmt.unit(cx as i32 * ctu, cy as i32 * ctu, ctu, ctu)
    };
    // Phase 1: decide every coding tree with estimated rates.
    let mut trees = Vec::with_capacity(hd.sh.ctus.len());
    let mut est = Estimator::new(ctx0.clone());
    for &addr in &hd.sh.ctus {
        pic.ctus[addr as usize].slice = Some(0);
        let mut part = Partitioner::new(&pic, &si, ctu_area(addr), 0, 0);
        trees.push(enc.search(&mut pic, &mut part, &mut est)?);
    }
    // In-loop filter decisions on the deblocked reconstruction.
    let sao = if s.deblocking {
        let mut deblocked = pic.clone();
        super::deblock::deblock(
            &mut deblocked,
            &hd.sps,
            &hd.pps,
            &hd.ph,
            std::slice::from_ref(&hd.sh),
        );
        enc.sao_decisions(&deblocked)
    } else {
        Vec::new()
    };
    // Phase 2: rebuild the picture as the decoder parses it while writing it.
    let mut pic = Picture::new(&hd.sps, &hd.pps);
    let mut writer = CabacWriter::new(ctx0);
    let n = hd.sh.ctus.len();
    for (i, &addr) in hd.sh.ctus.iter().enumerate() {
        pic.ctus[addr as usize].slice = Some(0);
        if let Some(d) = sao.get(addr as usize) {
            write_sao(&mut writer, &pic, addr, d);
        }
        let mut part = Partitioner::new(&pic, &si, ctu_area(addr), 0, 0);
        let mut it = trees[i].iter();
        enc.write_tree(&mut pic, &mut part, &mut writer, &mut it)?;
        if i == n - 1 {
            writer.term(1); // end_of_slice_one_bit
        }
    }
    let mut slice = sh_rbsp;
    slice.extend(writer.finish());
    let recon = pic
        .planes
        .iter()
        .map(|p| p.data.iter().map(|&v| v as u8).collect())
        .collect();
    Ok((
        vec![
            nal(NAL_SPS, &sps_rbsp),
            nal(NAL_PPS, &pps_rbsp),
            nal(NAL_IDR_N_LP, &slice),
        ],
        recon,
    ))
}

/// A CTU's SAO decision: merged from the left (0) or above (1) CTU, or
/// its own parameters (Cb and Cr share the type and edge class).
#[derive(Clone, Copy, Default)]
struct SaoDecision {
    merge: Option<u8>,
    p: [SaoParam; 3],
}

/// Per-category statistics of (source - deblocked) differences.
#[derive(Clone, Copy, Default)]
struct SaoStats {
    /// Edge classes 0..4 (types 1..4) by edge index 0..5; band offsets by
    /// band 0..32.
    eo: [[(i64, i64); 5]; 4],
    bo: [(i64, i64); 32],
}

fn sao_stats(rec: &Plane, src: &Plane, bx: i32, by: i32, w: i32, h: i32) -> SaoStats {
    let mut st = SaoStats::default();
    let (pw, ph) = (rec.width as i32, rec.height as i32);
    let r = |x: i32, y: i32| i32::from(rec.at(x, y));
    const DIRS: [[(i32, i32); 2]; 4] = [
        [(-1, 0), (1, 0)],
        [(0, -1), (0, 1)],
        [(-1, -1), (1, 1)],
        [(1, -1), (-1, 1)],
    ];
    for y in by..by + h {
        for x in bx..bx + w {
            let v = r(x, y);
            let d = i64::from(src.at(x, y)) - i64::from(v);
            let b = &mut st.bo[(v >> 3) as usize];
            b.0 += d;
            b.1 += 1;
            for (k, dir) in DIRS.iter().enumerate() {
                let (ax, ay) = (x + dir[0].0, y + dir[0].1);
                let (cx, cy) = (x + dir[1].0, y + dir[1].1);
                if ax < 0
                    || ay < 0
                    || cx < 0
                    || cy < 0
                    || ax >= pw
                    || cx >= pw
                    || ay >= ph
                    || cy >= ph
                {
                    continue;
                }
                let e = (v - r(ax, ay)).signum() + (v - r(cx, cy)).signum();
                let c = &mut st.eo[k][(e + 2) as usize];
                c.0 += d;
                c.1 += 1;
            }
        }
    }
    st
}

impl<'a, 's> Enc<'a, 's> {
    /// Distortion change and bypass bins of applying offset `o` to a
    /// category with difference sum `sum` over `count` samples.
    fn sao_offset(&self, sum: i64, count: i64, sign: i32, weight: f64) -> (i32, f64, u32) {
        let mut best = (0, 0.0, 1);
        if count == 0 {
            return best;
        }
        let target = (sum as f64 / count as f64).round() as i32 * sign;
        let mut best_cost = self.lambda / weight;
        for o in 1..=target.clamp(0, 7) {
            let v = f64::from(o * sign);
            let dd = (count as f64 * v * v - 2.0 * v * sum as f64) * weight;
            let bins = (o + 1).min(7) as u32;
            let cost = dd + self.lambda * f64::from(bins);
            if cost < best_cost {
                best_cost = cost;
                best = (o, dd, bins);
            }
        }
        best
    }

    /// Cost (distortion change plus rate) and parameters of the best SAO
    /// setting of type `ty` (0 off, 1 band, 2..=5 edge classes) for one
    /// component.
    fn sao_candidate(&self, st: &SaoStats, ty: usize, weight: f64) -> (f64, SaoParam) {
        let mut p = SaoParam::default();
        match ty {
            0 => (0.0, p),
            1 => {
                let mut best = (f64::MAX, p);
                for start in 0..32 {
                    let mut cost = self.lambda * (1.0 + 1.0 + 5.0);
                    let mut off = [0i32; 5];
                    for i in 0..4 {
                        let (sum, count) = st.bo[(start + i) % 32];
                        let pos = self.sao_offset(sum, count, 1, weight);
                        let neg = self.sao_offset(sum, count, -1, weight);
                        let (o, dd, bins) = if pos.1 + self.lambda * f64::from(pos.2)
                            <= neg.1 + self.lambda * f64::from(neg.2)
                        {
                            pos
                        } else {
                            (-neg.0, neg.1, neg.2)
                        };
                        off[i] = o;
                        cost += dd + self.lambda * f64::from(bins + u32::from(o != 0));
                    }
                    if cost < best.0 {
                        best = (
                            cost,
                            SaoParam {
                                mode: 1,
                                type_idc: 0,
                                band_pos: start as u8,
                                offset: off,
                            },
                        );
                    }
                }
                best
            }
            _ => {
                let k = ty - 2;
                let mut cost = self.lambda * (1.0 + 1.0 + 2.0);
                let mut off = [0i32; 5];
                for (i, &(e, sign)) in [(0usize, 1), (1, 1), (3, -1), (4, -1)].iter().enumerate() {
                    let (sum, count) = st.eo[k][e];
                    let (o, dd, bins) = self.sao_offset(sum, count, sign, weight);
                    off[if i < 2 { i } else { i + 1 }] = o * sign;
                    cost += dd + self.lambda * f64::from(bins);
                }
                p = SaoParam {
                    mode: 1,
                    type_idc: (k + 1) as u8,
                    band_pos: 0,
                    offset: off,
                };
                (cost, p)
            }
        }
    }

    /// Distortion change of a component's parameters on its statistics.
    fn sao_apply_cost(st: &SaoStats, p: &SaoParam, weight: f64) -> f64 {
        let dd = |(sum, count): (i64, i64), o: i32| {
            let v = f64::from(o);
            (count as f64 * v * v - 2.0 * v * sum as f64) * weight
        };
        match (p.mode, p.type_idc) {
            (0, _) => 0.0,
            (_, 0) => (0..4)
                .map(|i| dd(st.bo[(p.band_pos as usize + i) % 32], p.offset[i]))
                .sum(),
            (_, t) => (0..5)
                .map(|e| dd(st.eo[t as usize - 1][e], p.offset[e]))
                .sum(),
        }
    }

    /// SAO decisions for every CTU of the deblocked picture.
    fn sao_decisions(&self, pic: &Picture) -> Vec<SaoDecision> {
        let size = 1i32 << pic.ctu_log2;
        let wc = pic.width_ctus as usize;
        let n = pic.ctus.len();
        let mut out: Vec<SaoDecision> = Vec::with_capacity(n);
        let ncomp = pic.fmt.num_comp();
        for addr in 0..n {
            let (cx, cy) = ((addr % wc) as i32, (addr / wc) as i32);
            let stats: Vec<SaoStats> = (0..ncomp)
                .map(|c| {
                    let (sx, sy) = pic.fmt.scale(c);
                    let (x0, y0) = (cx * size, cy * size);
                    let (w, h) = (size.min(pic.width - x0), size.min(pic.height - y0));
                    sao_stats(
                        &pic.planes[c],
                        &self.src[c],
                        x0 >> sx,
                        y0 >> sy,
                        w >> sx,
                        h >> sy,
                    )
                })
                .collect();
            // Luma: off, band or one of the edge classes.
            let wl = self.weight(0);
            let mut luma = (0.0, SaoParam::default());
            for ty in 1..6 {
                let c = self.sao_candidate(&stats[0], ty, wl);
                if c.0 < luma.0 {
                    luma = c;
                }
            }
            let mut own = SaoDecision {
                merge: None,
                p: [luma.1, SaoParam::default(), SaoParam::default()],
            };
            let mut own_cost = luma.0 + self.lambda;
            if ncomp > 1 {
                let wc_ = self.weight(1);
                let mut chroma = (0.0, [SaoParam::default(); 2]);
                for ty in 1..6 {
                    let a = self.sao_candidate(&stats[1], ty, wc_);
                    let b = self.sao_candidate(&stats[2], ty, wc_);
                    if a.0 + b.0 < chroma.0 {
                        chroma = (a.0 + b.0, [a.1, b.1]);
                    }
                }
                own.p[1] = chroma.1[0];
                own.p[2] = chroma.1[1];
                own_cost += chroma.0 + self.lambda;
            }
            let mut best = (own_cost, own);
            for (m, nb) in [
                (0u8, (cx > 0).then(|| addr - 1)),
                (1, (cy > 0).then(|| addr - wc)),
            ] {
                let Some(nb) = nb else { continue };
                let p = out[nb].p;
                let cost: f64 = (0..ncomp)
                    .map(|c| Self::sao_apply_cost(&stats[c], &p[c], self.weight(c)))
                    .sum::<f64>()
                    + self.lambda * f64::from(m + 1);
                if cost < best.0 {
                    best = (cost, SaoDecision { merge: Some(m), p });
                }
            }
            out.push(best.1);
        }
        out
    }
}

/// sao( ) for one CTU.
fn write_sao<S: BinSink>(s: &mut S, pic: &Picture, addr: u32, d: &SaoDecision) {
    let wc = pic.width_ctus;
    let (cx, cy) = (addr % wc, addr / wc);
    if cx > 0 {
        s.bin(ctx::SAO_MERGE_FLAG, u32::from(d.merge == Some(0)));
    }
    if d.merge != Some(0) && cy > 0 {
        s.bin(ctx::SAO_MERGE_FLAG, u32::from(d.merge == Some(1)));
    }
    if d.merge.is_some() {
        return;
    }
    for c in 0..pic.fmt.num_comp() {
        let p = d.p[c];
        if c != 2 {
            s.bin(ctx::SAO_TYPE_IDX, u32::from(p.mode != 0));
            if p.mode != 0 {
                s.ep(u32::from(p.type_idc != 0));
            }
        }
        if p.mode == 0 {
            continue;
        }
        let mags: [i32; 4] = if p.type_idc == 0 {
            [p.offset[0], p.offset[1], p.offset[2], p.offset[3]]
        } else {
            [p.offset[0], p.offset[1], -p.offset[3], -p.offset[4]]
        };
        for &m in &mags {
            let m = m.unsigned_abs();
            for _ in 0..m {
                s.ep(1);
            }
            if m < 7 {
                s.ep(0);
            }
        }
        if p.type_idc == 0 {
            for &m in &mags {
                if m != 0 {
                    s.ep(u32::from(m < 0));
                }
            }
            s.eps(u32::from(p.band_pos), 5);
        } else if c != 2 {
            s.eps(u32::from(p.type_idc) - 1, 2);
        }
    }
}

fn clear_map(pic: &mut Picture, area: &UnitArea) {
    let b = area.blk[0];
    let x1 = ((b.x + b.w).min(pic.width) as usize).div_ceil(4);
    let y1 = ((b.y + b.h).min(pic.height) as usize).div_ceil(4);
    for y in (b.y as usize / 4)..y1 {
        for x in (b.x as usize / 4)..x1 {
            pic.cu_map[0][y * pic.map_w + x] = NONE;
            pic.cu_map[1][y * pic.map_w + x] = NONE;
        }
    }
}

/// A decided coding tree node in parsing order: a split or a coding unit.
#[derive(Clone)]
enum Node {
    Split(Split),
    Leaf(Box<CuData>),
}

#[derive(Clone, Default)]
struct CuData {
    intra_dir: [u8; 2],
    /// Matrix intra prediction (intra_dir[0] is then the MIP mode).
    mip: bool,
    mip_transposed: bool,
    /// Reference line index (0, 1 or 2 for lines 0, 1 and 3).
    mrl: u8,
    lfnst: u8,
    cbf: u8,
    coeff: [Vec<i32>; 3],
}

struct Enc<'a, 's> {
    si: &'a SliceInfo<'s>,
    src: Vec<Plane>,
    qp: i32,
    lambda: f64,
    effort: u32,
    rdoq: bool,
}

/// State saved to undo a trial encoding of an area.
struct Snapshot {
    cus: usize,
    tus: usize,
    num_cus: Vec<(usize, u32, u32)>,
    planes: Vec<Vec<i16>>,
}

impl<'a, 's> Enc<'a, 's> {
    fn snapshot(&self, pic: &Picture, area: &UnitArea) -> Snapshot {
        let planes = (0..pic.fmt.num_comp())
            .map(|c| read_block(&pic.planes[c], &clip(area.blk[c], &pic.planes[c])))
            .collect();
        Snapshot {
            cus: pic.cus.len(),
            tus: pic.tus.len(),
            num_cus: pic
                .ctus
                .iter()
                .enumerate()
                .filter(|(_, c)| c.slice.is_some())
                .map(|(i, c)| (i, c.num_cus, c.num_tus))
                .collect(),
            planes,
        }
    }

    fn restore(&self, pic: &mut Picture, area: &UnitArea, snap: &Snapshot) {
        pic.cus.truncate(snap.cus);
        pic.tus.truncate(snap.tus);
        for &(i, c, t) in &snap.num_cus {
            pic.ctus[i].num_cus = c;
            pic.ctus[i].num_tus = t;
        }
        clear_map(pic, area);
        for c in 0..pic.fmt.num_comp() {
            let b = clip(area.blk[c], &pic.planes[c]);
            write_block(&mut pic.planes[c], &b, &snap.planes[c]);
        }
    }

    /// Phase 1: chooses between coding the partitioner's area as one coding
    /// unit and each allowed split; leaves the chosen reconstruction in `pic`.
    fn search(
        &mut self,
        pic: &mut Picture,
        part: &mut Partitioner,
        est: &mut Estimator,
    ) -> Result<Vec<Node>, Error> {
        let area = *part.area();
        let can = part.can_split(pic);
        let snap = self.snapshot(pic, &area);
        let start = est.clone();
        // The best choice so far and the state needed to reinstate it.
        let mut best: Option<(f64, Vec<Node>, Estimator)> = None;
        let mut best_state: Option<(Snapshot, Vec<Cu>, Vec<Tu>)> = None;
        // Whether `pic` holds the best candidate's reconstruction.
        let mut current_is_best = false;
        let mut tried = false;
        if can[0] {
            let mut e = start.clone();
            write_split(&mut e, pic, part, Split::None);
            let (cost, data) = self.code_cu(pic, part, &mut e)?;
            best = Some((cost, vec![Node::Leaf(Box::new(data))], e));
            tried = true;
            current_is_best = true;
        }
        let splits = [
            Split::Quad,
            Split::Horz,
            Split::Vert,
            Split::TriH,
            Split::TriV,
        ];
        let mut costs = [f64::MAX; 5];
        for (k, &split) in splits.iter().enumerate() {
            // Implicit splits at the picture boundary are always considered.
            if !can[k + 1] || (can[0] && !split_ok(pic, &area, split)) {
                continue;
            }
            // Ternary splits only when the binary split in the same
            // direction came close to the best choice.
            if can[0] && k >= 3 {
                let best_cost = best.as_ref().map_or(f64::MAX, |b| b.0);
                if costs[k - 2] > best_cost * TT_MARGIN[self.effort.min(2) as usize] {
                    continue;
                }
            }
            if tried {
                if current_is_best {
                    best_state = Some((
                        self.snapshot(pic, &area),
                        pic.cus[snap.cus..].to_vec(),
                        pic.tus[snap.tus..].to_vec(),
                    ));
                }
                self.restore(pic, &area, &snap);
            }
            let mut e = start.clone();
            write_split(&mut e, pic, part, split);
            let mut nodes = vec![Node::Split(split)];
            part.split(split, pic);
            let chan = pic.chan_area(0);
            loop {
                let b = part.area().blk[0];
                if chan.contains(b.x, b.y) {
                    nodes.extend(self.search(pic, part, &mut e)?);
                }
                if !part.next_part(pic, false) {
                    break;
                }
            }
            part.exit_split(pic);
            tried = true;
            let cost = self.distortion(pic, &area) + self.lambda * e.bits as f64 / 32768.0;
            costs[k] = cost;
            current_is_best = best.as_ref().is_none_or(|b| cost < b.0);
            if current_is_best {
                best = Some((cost, nodes, e));
            }
        }
        let (_, nodes, e) = best.ok_or(Error::Invalid("no partitioning allowed"))?;
        if !current_is_best {
            let (keep, cus, tus) = best_state.ok_or(Error::Invalid("lost best candidate"))?;
            // A previous candidate won: reinstate its coding units and
            // reconstruction.
            self.restore(pic, &area, &snap);
            write_block_all(pic, &area, &keep);
            for (i, c, t) in keep.num_cus {
                pic.ctus[i].num_cus = c;
                pic.ctus[i].num_tus = t;
            }
            pic.cus.extend(cus);
            pic.tus.extend(tus);
            for id in snap.cus..pic.cus.len() {
                pic.fill_map(id as u32);
            }
        }
        *est = e;
        Ok(nodes)
    }

    /// Sets a coding unit's decided modes and coefficients and
    /// reconstructs it.
    fn apply(&self, pic: &mut Picture, cu_id: u32, d: &CuData) -> Result<(), Error> {
        set_modes(pic, cu_id, d);
        let tu = pic.cus[cu_id as usize].first_tu as usize;
        set_coeffs(pic, tu, d.cbf, &d.coeff);
        recon::reconstruct_cu(pic, self.si, cu_id)
    }

    fn distortion(&self, pic: &Picture, area: &UnitArea) -> f64 {
        let mut d = 0f64;
        for c in 0..pic.fmt.num_comp() {
            let b = clip(area.blk[c], &pic.planes[c]);
            d += sse(&pic.planes[c], &self.src[c], &b) as f64 * self.weight(c);
        }
        d
    }

    /// Distortion weight of a component: VTM's chroma weighting
    /// 2^((QpY - QpC) / 3).
    fn weight(&self, comp: usize) -> f64 {
        if comp == 0 {
            1.0
        } else {
            2f64.powf(f64::from(self.qp - self.comp_qp(comp)) / 3.0)
        }
    }

    /// Codes the partitioner's area as one intra coding unit; returns its
    /// rate-distortion cost and data, with the estimator advanced.
    fn code_cu(
        &mut self,
        pic: &mut Picture,
        part: &mut Partitioner,
        est: &mut Estimator,
    ) -> Result<(f64, CuData), Error> {
        let cu_id = add_cu(pic, self.si, part, self.qp);
        add_tus(pic, self.si, part, cu_id);
        let fmt = pic.fmt;
        let lb = pic.cus[cu_id as usize].blk[0];
        let mpm = intra_mpms(pic, cu_id, false);
        // Rough luma mode decision on prediction SATD.
        let mut rough: Vec<(f64, u8)> = Vec::new();
        let sad_lambda = self.lambda.sqrt();
        let orig: Vec<i16> = read_block(&self.src[0], &lb);
        let eval = |pic: &mut Picture, modes: Vec<u8>, rough: &mut Vec<(f64, u8)>| {
            let modes: Vec<u8> = modes
                .into_iter()
                .filter(|m| !rough.iter().any(|r| r.1 == *m))
                .collect();
            let preds = recon::predict_modes(pic, self.si, cu_id, 0, &modes, 0);
            for (m, pred) in modes.into_iter().zip(preds) {
                if rough.iter().any(|r| r.1 == m) {
                    continue;
                }
                let satd = satd_blk(&pred, &orig, lb.w as usize, lb.h as usize) as f64;
                let bits = if let Some(i) = mpm.iter().position(|&x| x == m) {
                    1.0 + (i.min(4) + 1) as f64
                } else {
                    6.0
                };
                rough.push((satd + sad_lambda * bits, m));
            }
        };
        let step = if self.effort == 0 { 4 } else { 2 };
        let mut first: Vec<u8> = (2..=66u8).step_by(step).collect();
        first.extend([PLANAR, DC]);
        eval(pic, first, &mut rough);
        rough.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut refine: Vec<u8> = Vec::new();
        for &(_, m) in rough.iter().take(2) {
            if m >= 2 {
                for d in 1..step as i32 {
                    for mm in [m as i32 - d, m as i32 + d] {
                        if (2..=66).contains(&mm) {
                            refine.push(mm as u8);
                        }
                    }
                }
            }
        }
        refine.extend(mpm);
        eval(pic, refine, &mut rough);
        rough.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n_full = match self.effort {
            0 => 1,
            1 => 2,
            _ => 3,
        };
        // Rough MIP mode decision.
        let num_mip = mip_modes(lb.w, lb.h) as u8;
        let mip_list: Vec<(u8, bool)> =
            (0..num_mip).flat_map(|m| [(m, false), (m, true)]).collect();
        let mip_preds = recon::predict_mip(pic, self.si, cu_id, &mip_list)?;
        let mip_bits = 2.0 + f64::from(num_mip).log2();
        let mut mip_rough: Vec<(f64, u8, bool)> = mip_list
            .iter()
            .zip(&mip_preds)
            .map(|(&(m, t), pred)| {
                let satd = satd_blk(pred, &orig, lb.w as usize, lb.h as usize) as f64;
                (satd + sad_lambda * mip_bits, m, t)
            })
            .collect();
        mip_rough.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Full rate-distortion over the best candidates, luma only, chroma DM.
        let mut cands: Vec<(f64, CuData)> = rough
            .iter()
            .take(n_full)
            .map(|&(c, m)| {
                (
                    c,
                    CuData {
                        intra_dir: [m, DM_CHROMA],
                        ..Default::default()
                    },
                )
            })
            .collect();
        let worst = cands.last().map_or(f64::MAX, |c| c.0);
        for &(c, m, t) in mip_rough.iter().take(n_full.min(2)) {
            if c < worst * 1.05 {
                cands.push((
                    c,
                    CuData {
                        intra_dir: [m, DM_CHROMA],
                        mip: true,
                        mip_transposed: t,
                        ..Default::default()
                    },
                ));
            }
        }
        // Other reference lines, with the non-planar most probable modes.
        if lb.y & ((1 << pic.ctu_log2) - 1) != 0 {
            let mut mrl_best: Option<(f64, u8, u8)> = None;
            for mrl in 1..=2u8 {
                let preds = recon::predict_modes(pic, self.si, cu_id, 0, &mpm[1..], mrl);
                for (i, pred) in preds.iter().enumerate() {
                    let satd = satd_blk(pred, &orig, lb.w as usize, lb.h as usize) as f64;
                    let c = satd + sad_lambda * (3.0 + (i + 1).min(4) as f64);
                    if mrl_best.is_none_or(|b| c < b.0) {
                        mrl_best = Some((c, mpm[i + 1], mrl));
                    }
                }
            }
            if let Some((c, m, mrl)) = mrl_best
                && c < worst * 1.05
            {
                cands.push((
                    c,
                    CuData {
                        intra_dir: [m, DM_CHROMA],
                        mrl,
                        ..Default::default()
                    },
                ));
            }
        }
        let mut best: Option<(f64, CuData, Estimator)> = None;
        for (_, mut d) in cands {
            let (cost, e) = self.rd_cu(pic, cu_id, &mut d, est, fmt.chroma != 0)?;
            if best.as_ref().is_none_or(|b| cost < b.0) {
                best = Some((cost, d, e));
            }
        }
        let (mut cost, mut data, mut e) = best.ok_or(Error::NoPicture)?;
        let luma = data.intra_dir[0];
        let with_luma = |intra_dir: [u8; 2], data: &CuData| CuData {
            intra_dir,
            mip: data.mip,
            mip_transposed: data.mip_transposed,
            mrl: data.mrl,
            ..Default::default()
        };
        // Chroma mode decision with the chosen luma mode.
        if fmt.chroma != 0 {
            set_modes(pic, cu_id, &data);
            let luma_col = recon::co_located_intra_luma_mode(pic, self.si, cu_id);
            let mut list = [PLANAR, VER, HOR, DC];
            for m in list.iter_mut() {
                if *m == luma_col {
                    *m = VDIA;
                }
            }
            let modes: Vec<u8> = std::iter::once(DM_CHROMA).chain(list).collect();
            let mut scores = vec![0f64; modes.len()];
            for c in 1..3 {
                let b = pic.cus[cu_id as usize].blk[c];
                let orig = read_block(&self.src[c], &b);
                let preds = recon::predict_modes(pic, self.si, cu_id, c, &modes, 0);
                for (sc, pred) in scores.iter_mut().zip(preds) {
                    *sc += satd_blk(&pred, &orig, b.w as usize, b.h as usize) as f64;
                }
            }
            // Cross-component modes predict from the final luma
            // reconstruction.
            self.apply(pic, cu_id, &data)?;
            let mut modes = modes;
            let tu = pic.cus[cu_id as usize].first_tu as usize;
            for lm in [LM_CHROMA, MDLM_L, MDLM_T] {
                pic.cus[cu_id as usize].intra_dir[1] = lm;
                set_coeffs(pic, tu, 0, &Default::default());
                recon::reconstruct_cu_comps(pic, self.si, cu_id, 6)?;
                let mut sc = 0f64;
                for c in 1..3 {
                    let b = pic.cus[cu_id as usize].blk[c];
                    let pred: Vec<i32> = read_block(&pic.planes[c], &b)
                        .iter()
                        .map(|&v| i32::from(v))
                        .collect();
                    let orig = read_block(&self.src[c], &b);
                    sc += satd_blk(&pred, &orig, b.w as usize, b.h as usize) as f64;
                }
                modes.push(lm);
                scores.push(sc);
            }
            let mut cbest: Option<(f64, u8)> = None;
            for (&cm, &sc) in modes.iter().zip(&scores) {
                let bits = match cm {
                    DM_CHROMA => 2.0,
                    LM_CHROMA => 2.0,
                    MDLM_L | MDLM_T => 3.0,
                    _ => 4.0,
                };
                let v = sc + sad_lambda * bits;
                if cbest.is_none_or(|b| v < b.0) {
                    cbest = Some((v, cm));
                }
            }
            let cm = cbest.map_or(DM_CHROMA, |b| b.1);
            if cm != DM_CHROMA {
                let mut d = with_luma([luma, cm], &data);
                let (c2, e2) = self.rd_cu(pic, cu_id, &mut d, est, true)?;
                if c2 < cost {
                    cost = c2;
                    data = d;
                    e = e2;
                }
            }
        }
        // LFNST with the chosen modes.
        for lfnst in 1..=2 {
            let mut d = CuData {
                lfnst,
                ..with_luma(data.intra_dir, &data)
            };
            let (c2, e2) = self.rd_cu(pic, cu_id, &mut d, est, true)?;
            if c2 < cost {
                cost = c2;
                data = d;
                e = e2;
            }
        }
        self.apply(pic, cu_id, &data)?;
        *est = e;
        Ok((cost, data))
    }

    /// Transforms, quantizes and reconstructs a coding unit with the modes
    /// in `d`, filling its coefficients; returns the cost and the estimator
    /// after its syntax.
    fn rd_cu(
        &self,
        pic: &mut Picture,
        cu_id: u32,
        d: &mut CuData,
        est: &Estimator,
        _chroma: bool,
    ) -> Result<(f64, Estimator), Error> {
        let tu = pic.cus[cu_id as usize].first_tu as usize;
        set_modes(pic, cu_id, d);
        set_coeffs(pic, tu, 0, &Default::default());
        recon::reconstruct_cu(pic, self.si, cu_id)?;
        let blk = pic.tus[tu].blk;
        let mut cbf = 0u8;
        let mut coeff: [Vec<i32>; 3] = Default::default();
        for c in 0..pic.fmt.num_comp() {
            let b = blk[c];
            if !b.valid() {
                continue;
            }
            let pred = read_block(&pic.planes[c], &b);
            let orig = read_block(&self.src[c], &b);
            let res: Vec<i32> = orig
                .iter()
                .zip(&pred)
                .map(|(&o, &p)| i32::from(o) - i32::from(p))
                .collect();
            let qp = self.comp_qp(c);
            let tr = if c == 0 && d.lfnst == 0 && !d.mip {
                implicit_mts(b.w, b.h)
            } else {
                (0, 0)
            };
            let mut tc = forward(&res, b.w as usize, b.h as usize, 8, tr);
            if c == 0 && d.lfnst != 0 {
                if b.w < 4 || b.h < 4 {
                    return Ok((f64::MAX, est.clone()));
                }
                let mode = if d.mip { PLANAR } else { d.intra_dir[0] };
                fwd_lfnst(&mut tc, b.w, b.h, mode, d.lfnst);
            }
            let levels = if self.rdoq {
                let signs = tc.iter().map(|&v| v < 0);
                rdoq(
                    &tc,
                    b.w,
                    b.h,
                    c,
                    &pic.cus[cu_id as usize],
                    &est.ctx,
                    qp,
                    self.lambda / self.weight(c),
                )
                .into_iter()
                .zip(signs)
                .map(|(l, neg)| if neg { -l } else { l })
                .collect()
            } else {
                quantize(&tc, b.w, b.h, qp, 8)
            };
            if levels.iter().any(|&v| v != 0) {
                cbf |= 1 << c;
                coeff[c] = levels;
            }
        }
        d.cbf = cbf;
        d.coeff = coeff;
        // A chosen LFNST index must be signalled, which needs coefficients
        // within its region.
        if d.lfnst != 0 && !lfnst_signalled(pic, cu_id, d) {
            return Ok((f64::MAX, est.clone()));
        }
        set_coeffs(pic, tu, cbf, &d.coeff);
        recon::reconstruct_cu(pic, self.si, cu_id)?;
        let mut e = est.clone();
        write_cu(&mut e, pic, self.si, cu_id, d);
        let cu = &pic.cus[cu_id as usize];
        let area = UnitArea { blk: cu.blk };
        let dist = self.distortion(pic, &area);
        Ok((dist + self.lambda * e.bits as f64 / 32768.0, e))
    }

    fn comp_qp(&self, comp: usize) -> i32 {
        if comp == 0 {
            return self.qp;
        }
        let sps = self.si.sps;
        let off = sps.qp_bd_offset;
        let qpi = self.qp.clamp(-off, 63);
        (sps.chroma_qp_table[comp - 1][(qpi + off) as usize] + off).clamp(0, 63 + off)
    }

    /// Phase 2: rebuilds the decided tree as the decoder parses it, writing
    /// its syntax.
    fn write_tree<'n>(
        &self,
        pic: &mut Picture,
        part: &mut Partitioner,
        w: &mut CabacWriter,
        it: &mut impl Iterator<Item = &'n Node>,
    ) -> Result<(), Error> {
        let node = it.next().ok_or(Error::Invalid("coding tree underrun"))?;
        match node {
            &Node::Split(split) => {
                write_split(w, pic, part, split);
                part.split(split, pic);
                let chan = pic.chan_area(0);
                loop {
                    let b = part.area().blk[0];
                    if chan.contains(b.x, b.y) {
                        self.write_tree(pic, part, w, it)?;
                    }
                    if !part.next_part(pic, false) {
                        break;
                    }
                }
                part.exit_split(pic);
            }
            Node::Leaf(d) => {
                write_split(w, pic, part, Split::None);
                let cu_id = add_cu(pic, self.si, part, self.qp);
                add_tus(pic, self.si, part, cu_id);
                self.apply(pic, cu_id, d)?;
                write_cu(w, pic, self.si, cu_id, d);
            }
        }
        Ok(())
    }
}

fn write_block_all(pic: &mut Picture, area: &UnitArea, snap: &Snapshot) {
    for c in 0..pic.fmt.num_comp() {
        let b = clip(area.blk[c], &pic.planes[c]);
        write_block(&mut pic.planes[c], &b, &snap.planes[c]);
    }
}

fn clip(b: Area, p: &Plane) -> Area {
    Area::new(
        b.x,
        b.y,
        b.w.min(p.width as i32 - b.x).max(0),
        b.h.min(p.height as i32 - b.y).max(0),
    )
}

fn read_block(p: &Plane, b: &Area) -> Vec<i16> {
    let mut out = Vec::with_capacity((b.w * b.h).max(0) as usize);
    for y in b.y..b.y + b.h {
        let row = y as usize * p.stride;
        out.extend_from_slice(&p.data[row + b.x as usize..row + (b.x + b.w) as usize]);
    }
    out
}

fn write_block(p: &mut Plane, b: &Area, data: &[i16]) {
    let w = b.w.max(0) as usize;
    for (i, y) in (b.y..b.y + b.h).enumerate() {
        let row = y as usize * p.stride + b.x as usize;
        p.data[row..row + w].copy_from_slice(&data[i * w..i * w + w]);
    }
}

fn sse(a: &Plane, b: &Plane, r: &Area) -> u64 {
    let mut s = 0u64;
    for y in r.y..r.y + r.h {
        let row = y as usize * a.stride;
        for x in r.x..r.x + r.w {
            let d = i64::from(a.data[row + x as usize]) - i64::from(b.data[row + x as usize]);
            s += (d * d) as u64;
        }
    }
    s
}

/// Sum of absolute Hadamard-transformed differences between a prediction
/// and a block, over 4x4 blocks (8x8 where the block allows).
fn satd_blk(pred: &[i32], orig: &[i16], w: usize, h: usize) -> u64 {
    let mut total = 0u64;
    if w.is_multiple_of(8) && h.is_multiple_of(8) {
        let mut d = [[0i32; 8]; 8];
        for by in (0..h).step_by(8) {
            for bx in (0..w).step_by(8) {
                for (y, row) in d.iter_mut().enumerate() {
                    let i = (by + y) * w + bx;
                    let (p, o) = (&pred[i..i + 8], &orig[i..i + 8]);
                    for x in 0..8 {
                        row[x] = p[x] - i32::from(o[x]);
                    }
                    hadamard8(row);
                }
                let mut s = 0u64;
                for x in 0..8 {
                    let mut col = [
                        d[0][x], d[1][x], d[2][x], d[3][x], d[4][x], d[5][x], d[6][x], d[7][x],
                    ];
                    hadamard8(&mut col);
                    s += col.iter().map(|v| u64::from(v.unsigned_abs())).sum::<u64>();
                }
                total += (s + 2) >> 2;
            }
        }
    } else {
        let mut d = [[0i32; 4]; 4];
        for by in (0..h).step_by(4) {
            for bx in (0..w).step_by(4) {
                for (y, row) in d.iter_mut().enumerate() {
                    let i = (by + y) * w + bx;
                    for x in 0..4 {
                        row[x] = pred[i + x] - i32::from(orig[i + x]);
                    }
                    hadamard4(row);
                }
                let mut s = 0u64;
                for x in 0..4 {
                    let mut col = [d[0][x], d[1][x], d[2][x], d[3][x]];
                    hadamard4(&mut col);
                    s += col.iter().map(|v| u64::from(v.unsigned_abs())).sum::<u64>();
                }
                total += (s + 1) >> 1;
            }
        }
    }
    total
}

#[inline(always)]
fn hadamard4(v: &mut [i32; 4]) {
    let (a, b, c, d) = (v[0] + v[1], v[0] - v[1], v[2] + v[3], v[2] - v[3]);
    *v = [a + c, b + d, a - c, b - d];
}

#[inline(always)]
fn hadamard8(v: &mut [i32; 8]) {
    let mut t = [0i32; 8];
    for k in 0..4 {
        t[k] = v[2 * k] + v[2 * k + 1];
        t[k + 4] = v[2 * k] - v[2 * k + 1];
    }
    let mut u = [0i32; 8];
    for k in 0..2 {
        u[k] = t[2 * k] + t[2 * k + 1];
        u[k + 2] = t[2 * k] - t[2 * k + 1];
        u[k + 4] = t[4 + 2 * k] + t[4 + 2 * k + 1];
        u[k + 6] = t[4 + 2 * k] - t[4 + 2 * k + 1];
    }
    for k in 0..4 {
        v[k] = u[2 * k] + u[2 * k + 1];
        v[k + 4] = u[2 * k] - u[2 * k + 1];
    }
}

fn log2(v: usize) -> i32 {
    (usize::BITS - 1 - v.leading_zeros()) as i32
}

/// Forward transform (VTM's partial butterflies' precision) with the
/// horizontal and vertical types of `recon::matrix` (0 DCT-II, 2 DST-VII),
/// zeroing the frequencies above 32 as 64-point transforms require.
fn forward(res: &[i32], w: usize, h: usize, bd: i32, tr: (u8, u8)) -> Vec<i32> {
    let s1 = log2(w) + bd - 9;
    let s2 = log2(h) + 6;
    let mh = recon::matrix(tr.0, w);
    let mv = recon::matrix(tr.1, h);
    let (kw, kh) = (w.min(32), h.min(32));
    let mut tmp = vec![0i32; kw * h];
    for y in 0..h {
        let row = &res[y * w..y * w + w];
        for k in 0..kw {
            let m = &mh[k * w..k * w + w];
            let acc: i64 = row
                .iter()
                .zip(m)
                .map(|(&r, &c)| i64::from(r) * i64::from(c))
                .sum();
            tmp[y * kw + k] = ((acc + (1 << (s1 - 1))) >> s1) as i32;
        }
    }
    let mut out = vec![0i32; w * h];
    for x in 0..kw {
        for k in 0..kh {
            let m = &mv[k * h..k * h + h];
            let mut acc = 0i64;
            for y in 0..h {
                acc += i64::from(tmp[y * kw + x]) * i64::from(m[y]);
            }
            out[k * w + x] = ((acc + (1 << (s2 - 1))) >> s2) as i32;
        }
    }
    out
}

/// Implicit MTS transform types of an intra luma block (`tr_types`): DST-VII
/// for dimensions of 4 to 16.
fn implicit_mts(w: i32, h: i32) -> (u8, u8) {
    let t = |n: i32| if (4..=16).contains(&n) { 2 } else { 0 };
    (t(w), t(h))
}

const QUANT_SCALES: [[i64; 6]; 2] = [
    [26214, 23302, 20560, 18396, 16384, 14564],
    [18396, 16384, 14564, 13107, 11651, 10280],
];

/// Scalar quantization with an intra rounding offset of 171/512.
fn quantize(coeff: &[i32], w: i32, h: i32, qp: i32, bd: i32) -> Vec<i32> {
    let (lw, lh) = (log2(w as usize), log2(h as usize));
    let sqrt_adj = (lw + lh) & 1 == 1;
    let tshift = 15 - bd - ((lw + lh) >> 1) - i32::from(sqrt_adj);
    let (per, rem) = (qp / 6, qp % 6);
    let qbits = 14 + per + tshift;
    let scale = QUANT_SCALES[usize::from(sqrt_adj)][rem as usize];
    let offset = 171i64 << (qbits - 9);
    coeff
        .iter()
        .map(|&c| {
            let a = (i64::from(c).abs() * scale + offset) >> qbits;
            let a = a.min(32767) as i32;
            if c < 0 { -a } else { a }
        })
        .collect()
}

/// Quantizer step and distortion scale of a transform block.
struct QuantParams {
    scale: i64,
    qbits: i32,
    /// Squared quantizer step in pixel-domain units: the SSE of one level of
    /// error.
    step2: f64,
}

fn quant_params(w: i32, h: i32, qp: i32, bd: i32) -> QuantParams {
    let (lw, lh) = (log2(w as usize), log2(h as usize));
    let sqrt_adj = (lw + lh) & 1 == 1;
    let tshift = 15 - bd - ((lw + lh) >> 1) - i32::from(sqrt_adj);
    let (per, rem) = (qp / 6, qp % 6);
    let qbits = 14 + per + tshift;
    let scale = QUANT_SCALES[usize::from(sqrt_adj)][rem as usize];
    // Coefficients carry a gain of 2^(15 - bd - (lw + lh) / 2).
    let gain = f64::from(15 - bd) - f64::from(lw + lh) / 2.0;
    let step = 2f64.powi(qbits) / scale as f64;
    QuantParams {
        scale,
        qbits,
        step2: step * step / 2f64.powf(2.0 * gain),
    }
}

/// Rate-distortion optimized quantization: greedy level decisions in coding
/// order with context-accurate rate estimates, coded-subblock decisions and
/// a final choice of the last position (or no coefficients) by exact rate.
#[allow(clippy::too_many_arguments)]
fn rdoq(
    coeff: &[i32],
    w: i32,
    h: i32,
    comp: usize,
    cu: &Cu,
    ctx: &Contexts,
    qp: i32,
    lambda: f64,
) -> Vec<i32> {
    let q = quant_params(w, h, qp, 8);
    let n = (w * h) as usize;
    let div = 2f64.powi(q.qbits);
    let lf: Vec<f64> = coeff
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if (i as i32 % w) >= 32 || (i as i32 / w) >= 32 {
                0.0
            } else {
                (i64::from(c).abs() * q.scale) as f64 / div
            }
        })
        .collect();
    let l0: Vec<i32> = lf.iter().map(|&v| ((v + 0.5) as i32).min(32767)).collect();
    let dist = |i: usize, l: i32| -> f64 {
        let e = lf[i] - f64::from(l);
        e * e * q.step2
    };
    let ch = usize::from(comp != 0);
    let mut cc = CoeffCtx::new(w, h, ch, false, comp == 0, cu, MTS_DCT2, false, false);
    let n_real = (w.min(32) * h.min(32)) as usize;
    let Some(last) = (0..n_real).rev().find(|&p| l0[cc.scan[p] as usize] != 0) else {
        return vec![0; n];
    };
    cc.scan_pos_last = last;
    let bits = |b: u32| f64::from(b) / 32768.0;
    let mut levels = vec![0i32; n];
    let mut work = vec![0i32; n];
    let mut sub_set = (last >> cc.log2_cg_size) as i32;
    while sub_set >= 0 {
        cc.init_subblock(sub_set as usize);
        let before = (cc.clone(), work.clone());
        let min_sub = cc.min_sub_pos as i32;
        let is_last = cc.is_last();
        let first_sig = if is_last {
            cc.scan_pos_last as i32
        } else {
            cc.max_sub_pos as i32
        };
        let group_inferred = is_last || min_sub == 0;
        cc.set_sig_group();
        let infer_sig = if first_sig != cc.scan_pos_last as i32 {
            if cc.sub_set != 0 { min_sub } else { -1 }
        } else {
            first_sig
        };
        let mut rem_bins = cc.reg_bin_limit;
        let mut num_nz = 0;
        let mut coded_cost = 0f64;
        let mut zero_dist = 0f64;
        let mut gt2: Vec<usize> = Vec::new();
        let mut next = first_sig;
        while next >= min_sub && rem_bins >= 4 {
            let blk = cc.scan[next as usize] as usize;
            let big = l0[blk];
            let forced = next == last as i32 || (num_nz == 0 && next == infer_sig);
            let inferred = num_nz == 0 && next == infer_sig;
            let sig_id = (!inferred).then(|| cc.sig_ctx(blk, 0));
            let off = cc.ctx_offset_abs();
            let rice = GO_RICE_PARS[cc.template_abs_sum(blk, &work, 4) as usize];
            let mut cands = vec![big, big - 1, if big <= 2 { 0 } else { big }];
            if forced {
                cands.iter_mut().for_each(|l| *l = (*l).max(1));
            }
            cands.retain(|&l| l >= 0);
            cands.sort_unstable();
            cands.dedup();
            let mut best = (f64::MAX, 0);
            for &l in &cands {
                let mut r = sig_id.map_or(0, |id| ctx.cost(id, u32::from(l > 0)));
                if l > 0 {
                    r += 32768 + ctx.cost(cc.gtx1_ctx(off), u32::from(l > 1));
                    if l > 1 {
                        r += ctx.cost(cc.par_ctx(off), ((l - 2) & 1) as u32)
                            + ctx.cost(cc.gtx2_ctx(off), u32::from(l > 3));
                        if l > 3 {
                            let rem = ((l - 4 - ((l - 2) & 1)) >> 1) as u32;
                            r += 32768 * rem_abs_bins(rem, rice, 5, cc.max_log2_range);
                        }
                    }
                }
                let cost = dist(blk, l) + lambda * bits(r);
                if cost < best.0 {
                    best = (cost, l);
                }
            }
            let l = best.1;
            coded_cost += best.0;
            zero_dist += dist(blk, 0);
            levels[blk] = l;
            rem_bins -= i32::from(!inferred);
            if l > 0 {
                num_nz += 1;
                rem_bins -= 1;
                let first = if l > 1 {
                    rem_bins -= 2;
                    if l > 3 {
                        gt2.push(blk);
                    }
                    2 + ((l - 2) & 1) + 2 * i32::from(l > 3)
                } else {
                    1
                };
                cc.abs_val_1st_pass(blk, &mut work, first);
            }
            next -= 1;
        }
        cc.reg_bin_limit = rem_bins;
        for &p in &gt2 {
            work[p] = levels[p];
        }
        while next >= min_sub {
            let blk = cc.scan[next as usize] as usize;
            let big = l0[blk];
            let forced = next == last as i32 || (num_nz == 0 && next == infer_sig);
            let rice = GO_RICE_PARS[cc.template_abs_sum(blk, &work, 0) as usize];
            let pos0 = 1u32 << rice;
            let mut cands = vec![big, big - 1, 0];
            if forced {
                cands.iter_mut().for_each(|l| *l = (*l).max(1));
            }
            cands.retain(|&l| l >= 0);
            let mut best = (f64::MAX, 0);
            for &l in &cands {
                let tc = l as u32;
                let code = if tc == 0 {
                    pos0
                } else if tc <= pos0 {
                    tc - 1
                } else {
                    tc
                };
                let r = 32768 * (rem_abs_bins(code, rice, 5, cc.max_log2_range) + u32::from(l > 0));
                let cost = dist(blk, l) + lambda * bits(r);
                if cost < best.0 {
                    best = (cost, l);
                }
            }
            let l = best.1;
            coded_cost += best.0;
            zero_dist += dist(blk, 0);
            levels[blk] = l;
            if l > 0 {
                num_nz += 1;
                work[blk] = l;
            }
            next -= 1;
        }
        if !group_inferred {
            let coded = coded_cost + lambda * bits(ctx.cost(cc.sig_group_ctx, 1));
            let zero = zero_dist + lambda * bits(ctx.cost(cc.sig_group_ctx, 0));
            if num_nz == 0 || zero < coded {
                (cc, work) = before;
                for p in min_sub..=first_sig {
                    levels[cc.scan[p as usize] as usize] = 0;
                }
            }
        }
        sub_set -= 1;
    }
    // The last position: truncations at the highest non-zero positions, or
    // no coefficients, by exact rate.
    let cbf_ctx = [ctx::QT_CBF0, ctx::QT_CBF1, ctx::QT_CBF2][comp];
    let nz: Vec<usize> = (0..n_real)
        .rev()
        .filter(|&p| levels[cc.scan[p] as usize] != 0)
        .take(4)
        .collect();
    let base_dist: f64 = (0..n).map(|i| dist(i, levels[i])).sum();
    let mut best = (
        (0..n).map(|i| dist(i, 0)).sum::<f64>() + lambda * bits(ctx.cost(cbf_ctx, 0)),
        None,
    );
    let mut extra = 0f64;
    for (k, &p) in nz.iter().enumerate() {
        if k > 0 {
            let b = cc.scan[nz[k - 1]] as usize;
            extra += dist(b, 0) - dist(b, levels[b]);
        }
        let mut t = levels.clone();
        for &sp in &nz[..k] {
            t[cc.scan[sp] as usize] = 0;
        }
        let _ = p;
        let mut e = Estimator::new(ctx.clone());
        e.bin(cbf_ctx, 1);
        write_residual(&mut e, &t, w, h, comp, cu);
        let cost = base_dist + extra + lambda * bits(e.bits as u32);
        if cost < best.0 {
            best = (cost, Some(t));
        }
    }
    best.1.unwrap_or_else(|| vec![0; n])
}

fn set_modes(pic: &mut Picture, cu_id: u32, d: &CuData) {
    let cu = &mut pic.cus[cu_id as usize];
    cu.intra_dir = d.intra_dir;
    cu.mip = d.mip;
    cu.mip_transposed = d.mip_transposed;
    cu.mrl = d.mrl;
    cu.lfnst = d.lfnst;
}

fn set_coeffs(pic: &mut Picture, tu: usize, cbf: u8, coeff: &[Vec<i32>; 3]) {
    let cu = pic.tus[tu].cu as usize;
    pic.cus[cu].root_cbf = cbf != 0;
    pic.cus[cu].plane_cbf = [cbf & 1 != 0, cbf & 2 != 0, cbf & 4 != 0];
    let t = &mut pic.tus[tu];
    t.cbf = cbf;
    for c in 0..3 {
        if cbf >> c & 1 != 0 {
            let b = t.blk[c];
            t.coeff[c] = coeff[c].clone();
            t.max_scan[c] = max_scan(&coeff[c], b.w, b.h);
        } else {
            t.coeff[c] = Vec::new();
            t.max_scan[c] = (0, 0);
        }
    }
}

/// The decoder's `max_scan` for a coefficient block: the bottom-right
/// corner of the coded coefficient groups, or (0, 0) when only DC is coded.
fn max_scan(coeff: &[i32], w: i32, h: i32) -> (i32, i32) {
    let scan = super::ctu::grouped_scan_cached(w, h);
    let n = (w.min(32) * h.min(32)) as usize;
    let Some(last) = (0..n).rev().find(|&s| coeff[scan[s] as usize] != 0) else {
        return (0, 0);
    };
    if last == 0 {
        return (0, 0);
    }
    let (lcw, lch) = LOG2_SBB_SIZE[log2(w as usize) as usize][log2(h as usize) as usize];
    let (mut mx, mut my) = (0, 0);
    for s in 0..=last {
        let p = scan[s] as i32;
        if coeff[p as usize] != 0 {
            mx = mx.max((p % w) >> lcw);
            my = my.max((p / w) >> lch);
        }
    }
    (((mx + 1) << lcw) - 1, ((my + 1) << lch) - 1)
}

/// The decoder's addCU for a single-tree intra coding unit.
fn add_cu(pic: &mut Picture, si: &SliceInfo, part: &Partitioner, qp: i32) -> u32 {
    let area = *part.area();
    let ctu = pic.ctu_addr_of(area.blk[0].x, area.blk[0].y, 0);
    let ctud = &mut pic.ctus[ctu as usize];
    ctud.num_cus += 1;
    let cu = Cu {
        blk: area.blk,
        ch_type: 0,
        tree: part.tree,
        mode_type: part.mode_type,
        qt_depth: part.qt_depth,
        depth: part.depth,
        split_series: part.split_series(),
        slice: si.slice_idx,
        tile: 0,
        ctu,
        idx: ctud.num_cus,
        left: part.cu_left(),
        above: part.cu_above(),
        first_tu: pic.tus.len() as u32,
        intra_dir: [DC, 0],
        ref_idx: [-1, -1],
        pred: Pred::Intra,
        qp,
        ..Default::default()
    };
    let id = pic.cus.len() as u32;
    pic.cus.push(cu);
    pic.fill_map(id);
    id
}

/// The decoder's transform tree for an unsplit intra coding unit: one
/// transform unit (the coding unit never exceeds the 64x64 maximum).
fn add_tus(pic: &mut Picture, _si: &SliceInfo, part: &Partitioner, cu_id: u32) {
    let area = *part.area();
    let ctu = pic.cus[cu_id as usize].ctu as usize;
    pic.ctus[ctu].num_tus += 1;
    let idx = pic.ctus[ctu].num_tus;
    pic.tus.push(Tu {
        blk: area.blk,
        ch_type: 0,
        idx,
        cu: cu_id,
        ..Default::default()
    });
    pic.cus[cu_id as usize].num_tu += 1;
}

/// split_cu_flag, split_qt_flag, mtt_split_cu_vertical_flag and
/// mtt_split_cu_binary_flag (`split_cu_mode`).
fn write_split<S: BinSink>(s: &mut S, pic: &Picture, part: &Partitioner, split: Split) {
    let can = part.can_split(pic);
    let num_hor = u32::from(can[2]) + u32::from(can[4]);
    let num_ver = u32::from(can[3]) + u32::from(can[5]);
    let num_split = (u32::from(can[1]) << 1) + num_hor + num_ver;
    if can[0] && num_split == 0 {
        return;
    }
    let left = part.cu_left().map(|c| &pic.cus[c as usize]);
    let above = part.cu_above().map(|c| &pic.cus[c as usize]);
    let w = part.area().blk[0].w;
    let h = part.area().blk[0].h;
    let (l_h, l_qt) = left.map_or((0, 0), |c| (c.blk[0].h, c.qt_depth));
    let (a_w, a_qt) = above.map_or((0, 0), |c| (c.blk[0].w, c.qt_depth));
    let (has_l, has_a) = (left.is_some(), above.is_some());
    if can[0] {
        let mut ctx_split = usize::from(has_l && l_h < h) + usize::from(has_a && a_w < w);
        const OFFSET: [usize; 7] = [0, 0, 0, 3, 3, 6, 6];
        ctx_split += OFFSET[num_split as usize];
        s.bin(ctx::SPLIT_FLAG + ctx_split, u32::from(split != Split::None));
    }
    if split == Split::None {
        return;
    }
    let can_btt = num_hor != 0 || num_ver != 0;
    if can[1] && can_btt {
        let mut c =
            usize::from(has_l && l_qt > part.qt_depth) + usize::from(has_a && a_qt > part.qt_depth);
        c += if part.qt_depth < 2 { 0 } else { 3 };
        s.bin(ctx::SPLIT_QT_FLAG + c, u32::from(split == Split::Quad));
    }
    if split == Split::Quad {
        return;
    }
    let is_ver = matches!(split, Split::Vert | Split::TriV);
    if num_ver != 0 && num_hor != 0 {
        let mut c = 0;
        if num_ver == num_hor {
            if has_l && has_a {
                let dep_above = w >> a_w.ilog2();
                let dep_left = h >> l_h.ilog2();
                c = if dep_above == dep_left {
                    0
                } else if dep_above < dep_left {
                    1
                } else {
                    2
                };
            }
        } else if num_ver < num_hor {
            c = 3;
        } else {
            c = 4;
        }
        s.bin(ctx::SPLIT_HV_FLAG + c, u32::from(is_ver));
    }
    let can14 = if is_ver { can[5] } else { can[4] };
    let can12 = if is_ver { can[3] } else { can[2] };
    if can12 && can14 {
        let c = usize::from(part.mt_depth <= 1) + (usize::from(is_ver) << 1);
        s.bin(
            ctx::SPLIT12_FLAG + c,
            u32::from(matches!(split, Split::Horz | Split::Vert)),
        );
    }
}

/// Whether the encoder considers a split: every resulting chroma block keeps
/// at least 4x4 samples, so no split needs the local dual tree
/// (`mode_constraint`) or 2xN chroma blocks.
fn split_ok(pic: &Picture, area: &UnitArea, split: Split) -> bool {
    let b = area.blk[0];
    let (w, h) = match split {
        Split::Quad => (b.w / 2, b.h / 2),
        Split::Horz => (b.w, b.h / 2),
        Split::Vert => (b.w / 2, b.h),
        Split::TriH => (b.w, b.h / 4),
        Split::TriV => (b.w / 4, b.h),
        _ => return false,
    };
    let (sx, sy) = (pic.fmt.sx, pic.fmt.sy);
    pic.fmt.chroma == 0 || ((w >> sx) >= 4 && (h >> sy) >= 4)
}

/// intra_luma_ref_idx, coded below the first row of a CTU.
fn write_mrl<S: BinSink>(s: &mut S, pic: &Picture, cu: &Cu, mrl: u8) {
    if cu.ly() & ((1 << pic.ctu_log2) - 1) != 0 {
        s.bin(ctx::MULTI_REF_LINE_IDX, u32::from(mrl != 0));
        if mrl != 0 {
            s.bin(ctx::MULTI_REF_LINE_IDX + 1, u32::from(mrl == 2));
        }
    }
}

/// The number of MIP modes for a block size.
fn mip_modes(w: i32, h: i32) -> u32 {
    match super::ctu::mip_size_id(w, h) {
        0 => 16,
        1 => 8,
        _ => 6,
    }
}

/// truncated binary code (`trunc_bin`).
fn write_trunc_bin<S: BinSink>(s: &mut S, symbol: u32, max: u32) {
    let thresh = tb_max(max);
    let val = 1u32 << thresh;
    let b = max - val;
    if symbol < val - b {
        s.eps(symbol, thresh);
    } else {
        s.eps(symbol + val - b, thresh + 1);
    }
}

/// coding_unit( ) of an intra coding unit in an I slice.
fn write_cu<S: BinSink>(s: &mut S, pic: &Picture, si: &SliceInfo, cu_id: u32, d: &CuData) {
    let cu = &pic.cus[cu_id as usize];
    // intra_luma_mpm_flag, intra_luma_not_planar_flag, intra_luma_mpm_idx,
    // intra_luma_mpm_remainder
    // intra_mip_flag, intra_mip_transposed_flag, intra_mip_mode
    let mut ctx_id = usize::from(cu.left.is_some_and(|l| pic.cus[l as usize].mip))
        + usize::from(cu.above.is_some_and(|a| pic.cus[a as usize].mip));
    if cu.lw() > 2 * cu.lh() || cu.lh() > 2 * cu.lw() {
        ctx_id = 3;
    }
    s.bin(ctx::MIP_FLAG + ctx_id, u32::from(d.mip));
    let mut mpm = intra_mpms(pic, cu_id, false);
    let mode = cu.intra_dir[0];
    if d.mip {
        s.ep(u32::from(d.mip_transposed));
        write_trunc_bin(s, u32::from(mode), mip_modes(cu.lw(), cu.lh()));
    } else if let Some(idx) = mpm.iter().position(|&m| m == mode) {
        write_mrl(s, pic, cu, d.mrl);
        if d.mrl == 0 {
            s.bin(ctx::I_PRED_MODE0, 1);
            s.bin(ctx::INTRA_LUMA_PLANAR_FLAG + 1, u32::from(idx != 0));
        }
        if idx != 0 {
            for _ in 1..idx {
                s.ep(1);
            }
            if idx < 5 {
                s.ep(0);
            }
        }
    } else {
        write_mrl(s, pic, cu, 0);
        s.bin(ctx::I_PRED_MODE0, 0);
        mpm.sort_unstable();
        let mut sym = u32::from(mode);
        for &m in mpm.iter().rev() {
            if u32::from(m) < u32::from(mode) {
                sym -= 1;
            }
        }
        write_trunc_bin(s, sym, 67 - 6);
    }
    // intra_chroma_pred_mode
    if pic.fmt.chroma != 0 {
        let cm = cu.intra_dir[1];
        // cclm_mode_flag and cclm_mode_idx (the tree is never dual).
        let lm = [LM_CHROMA, MDLM_L, MDLM_T].iter().position(|&m| m == cm);
        s.bin(ctx::CCLM_MODE_FLAG, u32::from(lm.is_some()));
        if let Some(sym) = lm {
            s.bin(ctx::CCLM_MODE_IDX, u32::from(sym != 0));
            if sym != 0 {
                s.ep(sym as u32 - 1);
            }
        } else if cm == DM_CHROMA {
            s.bin(ctx::I_PRED_MODE1, 0);
        } else {
            s.bin(ctx::I_PRED_MODE1, 1);
            let mut list = [PLANAR, VER, HOR, DC];
            if !recon::is_dm_chroma_mip(pic, si, cu_id) {
                let luma = recon::co_located_intra_luma_mode(pic, si, cu_id);
                for m in list.iter_mut() {
                    if *m == luma {
                        *m = VDIA;
                        break;
                    }
                }
            }
            let idx = list.iter().position(|&m| m == cm).unwrap_or(0);
            s.eps(idx as u32, 2);
        }
    }
    // transform_unit( )
    let tu = &pic.tus[cu.first_tu as usize];
    let cbf = |c: usize| d.cbf >> c & 1 != 0;
    if pic.fmt.chroma != 0 {
        s.bin(ctx::QT_CBF1, u32::from(cbf(1)));
        s.bin(ctx::QT_CBF2 + usize::from(cbf(1)), u32::from(cbf(2)));
    }
    s.bin(ctx::QT_CBF0, u32::from(cbf(0)));
    for comp in 0..pic.fmt.num_comp() {
        if cbf(comp) {
            write_residual(s, &d.coeff[comp], tu.blk[comp].w, tu.blk[comp].h, comp, cu);
        }
    }
    // lfnst_idx
    if lfnst_signalled(pic, cu_id, d) {
        s.bin(ctx::LFNST_IDX, u32::from(d.lfnst != 0));
        if d.lfnst != 0 {
            s.bin(ctx::LFNST_IDX + 2, u32::from(d.lfnst == 2));
        }
    }
}

/// Whether lfnst_idx is coded for a single-tree intra coding unit with
/// these coefficients (`residual_lfnst_mode`): no block's last significant
/// scan position beyond the LFNST region, and one beyond DC.
fn lfnst_signalled(pic: &Picture, cu_id: u32, d: &CuData) -> bool {
    let cu = &pic.cus[cu_id as usize];
    if d.mip && !(cu.lw() >= 16 && cu.lh() >= 16) {
        return false;
    }
    let tu = &pic.tus[cu.first_tu as usize];
    let mut violates = false;
    let mut last_pos = false;
    for comp in 0..pic.fmt.num_comp() {
        let b = tu.blk[comp];
        if d.cbf >> comp & 1 == 0 || b.w < 4 || b.h < 4 {
            continue;
        }
        let scan = super::ctu::grouped_scan_cached(b.w, b.h);
        let n = (b.w.min(32) * b.h.min(32)) as usize;
        let last = (0..n)
            .rev()
            .find(|&p| d.coeff[comp][scan[p] as usize] != 0)
            .unwrap_or(0);
        let max = if (b.w == 4 && b.h == 4) || (b.w == 8 && b.h == 8) {
            7
        } else {
            15
        };
        violates |= last > max;
        last_pos |= last >= 1;
    }
    !violates && last_pos
}

/// Forward LFNST of a luma block's primary DCT-II coefficients (the
/// transpose of `recon::inv_lfnst`): the top-left region becomes 8 or 16
/// coefficients in the first scan positions; the rest are zero.
fn fwd_lfnst(coeff: &mut [i32], w: i32, h: i32, mode: u8, lfnst: u8) {
    let whge3 = w >= 8 && h >= 8;
    let scan: Vec<u16> = if whge3 {
        super::ps::diag_scan(4, 4)
            .iter()
            .map(|&p| ((p as i32 / 4) * w + p as i32 % 4) as u16)
            .collect()
    } else {
        super::ctu::grouped_scan_cached(w, h)
    };
    let mut mode = i32::from(mode);
    if mode >= 2 {
        const SHIFT: [i32; 6] = [0, 6, 10, 12, 14, 15];
        let delta = (log2(w as usize) - log2(h as usize)).unsigned_abs() as usize;
        if w > h && mode < 2 + SHIFT[delta] {
            mode += VDIA as i32 - 1;
        } else if h > w && mode > VDIA as i32 - SHIFT[delta] {
            mode -= VDIA as i32 + 1;
        }
    }
    let intra_mode = if mode < 0 {
        (mode + 14 + 67) as usize
    } else if mode >= 67 {
        (mode + 14) as usize
    } else {
        mode as usize
    };
    let transpose = intra_mode >= 67 + 14 || (intra_mode < 67 && intra_mode > DIA as usize);
    let sb = if whge3 { 8 } else { 4 };
    let small = (w == 4 && h == 4) || (w == 8 && h == 8);
    let zero_out = if small { 8 } else { 16 };
    let set = LFNST_LUT[intra_mode] as usize;
    let idx = usize::from(lfnst - 1);
    let tr_size = if sb > 4 { 48 } else { 16 };
    let wu = w as usize;
    // The region in the inverse transform's output order.
    let mut x = vec![0i32; tr_size];
    if transpose {
        if sb == 4 {
            for y in 0..4 {
                for k in 0..4 {
                    x[y + 4 * k] = coeff[y * wu + k];
                }
            }
        } else {
            for y in 0..8 {
                for k in 0..4 {
                    x[y + 8 * k] = coeff[y * wu + k];
                }
                if y < 4 {
                    for k in 0..4 {
                        x[y + 32 + 4 * k] = coeff[y * wu + 4 + k];
                    }
                }
            }
        }
    } else {
        let mut k = 0;
        for y in 0..sb {
            let s = if y < 4 { sb } else { 4 };
            for xx in 0..s {
                x[k] = coeff[y * wu + xx];
                k += 1;
            }
        }
    }
    let mut out = [0i32; 16];
    for (i, o) in out.iter_mut().enumerate().take(zero_out) {
        let mut acc = 0i64;
        for (j, &v) in x.iter().enumerate() {
            let m = if sb > 4 {
                LFNST_8X8[((set * 2 + idx) * 48 + j) * 16 + i]
            } else {
                LFNST_4X4[((set * 2 + idx) * 16 + j) * 16 + i]
            };
            acc += i64::from(v) * i64::from(m);
        }
        *o = ((acc + 64) >> 7) as i32;
    }
    coeff.fill(0);
    for i in 0..zero_out {
        coeff[scan[i] as usize] = out[i];
    }
}

/// residual_coding( ) without transform skip, dependent quantization or
/// sign hiding, mirroring the parser's context derivations.
fn write_residual<S: BinSink>(s: &mut S, coeff: &[i32], w: i32, h: i32, comp: usize, cu: &Cu) {
    let ch = usize::from(comp != 0);
    let mut cc = CoeffCtx::new(w, h, ch, false, comp == 0, cu, MTS_DCT2, false, false);
    let n_real = (w.min(32) * h.min(32)) as usize;
    let last = (0..n_real)
        .rev()
        .find(|&p| coeff[cc.scan[p] as usize] != 0)
        .unwrap_or(0);
    cc.scan_pos_last = last;
    // last_sig_coeff_x/y_prefix and suffix
    let blk = cc.scan[last] as i32;
    let (px, py) = ((blk % w) as u32, (blk / w) as u32);
    let (lx_ctx, ly_ctx) = if ch == 0 {
        (ctx::LASTX0, ctx::LASTY0)
    } else {
        (ctx::LASTX1, ctx::LASTY1)
    };
    let gx = GROUP_IDX[px as usize];
    let gy = GROUP_IDX[py as usize];
    for (g, max, base, off, shift) in [
        (gx, cc.max_last_x, lx_ctx, cc.last_off_x, cc.last_shift_x),
        (gy, cc.max_last_y, ly_ctx, cc.last_off_y, cc.last_shift_y),
    ] {
        for i in 0..g {
            s.bin(base + (off + (i >> shift)) as usize, 1);
        }
        if g < max {
            s.bin(base + (off + (g >> shift)) as usize, 0);
        }
    }
    for (g, p) in [(gx, px), (gy, py)] {
        if g > 3 {
            let count = (g - 2) >> 1;
            s.eps(p - MIN_IN_GROUP[g as usize], count);
        }
    }
    let n = (w * h) as usize;
    let mut work = vec![0i32; n];
    let mut sub_set = (last >> cc.log2_cg_size) as i32;
    while sub_set >= 0 {
        cc.init_subblock(sub_set as usize);
        write_subblock(s, &mut cc, coeff, &mut work);
        sub_set -= 1;
    }
}

fn write_subblock<S: BinSink>(s: &mut S, cc: &mut CoeffCtx, coeff: &[i32], work: &mut [i32]) {
    let min_sub = cc.min_sub_pos as i32;
    let is_last = cc.is_last();
    let first_sig = if is_last {
        cc.scan_pos_last as i32
    } else {
        cc.max_sub_pos as i32
    };
    let mut next = first_sig;
    let any = (min_sub..=first_sig).any(|p| coeff[cc.scan[p as usize] as usize] != 0);
    let inferred = is_last || min_sub == 0;
    if !inferred {
        s.bin(cc.sig_group_ctx, u32::from(any));
        if !any {
            return;
        }
    }
    cc.set_sig_group();
    let infer_sig = if next != cc.scan_pos_last as i32 {
        if cc.sub_set != 0 { min_sub } else { -1 }
    } else {
        next
    };
    let mut gt2_pos: Vec<(usize, i32)> = Vec::new();
    let mut sig_pos: Vec<usize> = Vec::new();
    let mut rem_bins = cc.reg_bin_limit;
    while next >= min_sub && rem_bins >= 4 {
        let blk = cc.scan[next as usize] as usize;
        let a = coeff[blk].abs();
        if !(sig_pos.is_empty() && next == infer_sig) {
            let ctx_id = cc.sig_ctx(blk, 0);
            s.bin(ctx_id, u32::from(a != 0));
            rem_bins -= 1;
        }
        if a != 0 {
            let off = cc.ctx_offset_abs();
            sig_pos.push(blk);
            let gt1 = a > 1;
            s.bin(cc.gtx1_ctx(off), u32::from(gt1));
            rem_bins -= 1;
            let first = if gt1 {
                let par = ((a - 2) & 1) as u32;
                s.bin(cc.par_ctx(off), par);
                rem_bins -= 1;
                let gt2 = a > 3;
                s.bin(cc.gtx2_ctx(off), u32::from(gt2));
                rem_bins -= 1;
                if gt2 {
                    gt2_pos.push((blk, a));
                }
                2 + par as i32 + 2 * i32::from(gt2)
            } else {
                1
            };
            cc.abs_val_1st_pass(blk, work, first);
        }
        next -= 1;
    }
    cc.reg_bin_limit = rem_bins;
    for &(pos, a) in &gt2_pos {
        let sum = cc.template_abs_sum(pos, work, 4);
        let rice = GO_RICE_PARS[sum as usize];
        let rem = ((a - work[pos]) >> 1) as u32;
        s.rem_abs(rem, rice, 5, cc.max_log2_range);
        work[pos] = a;
    }
    while next >= min_sub {
        let blk = cc.scan[next as usize] as usize;
        let sum = cc.template_abs_sum(blk, work, 0);
        let rice = GO_RICE_PARS[sum as usize];
        let pos0 = 1u32 << rice;
        let tc = coeff[blk].unsigned_abs();
        let code = if tc == 0 {
            pos0
        } else if tc <= pos0 {
            tc - 1
        } else {
            tc
        };
        s.rem_abs(code, rice, 5, cc.max_log2_range);
        if tc != 0 {
            work[blk] = tc as i32;
            sig_pos.push(blk);
        }
        next -= 1;
    }
    for &p in &sig_pos {
        s.ep(u32::from(coeff[p] < 0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, chroma: u32) -> Vec<Vec<u8>> {
        let fmt = Format::new(chroma);
        (0..fmt.num_comp())
            .map(|c| {
                let (sx, sy) = fmt.scale(c);
                let (pw, ph) = (w >> sx, h >> sy);
                let mut v = Vec::new();
                for y in 0..ph {
                    for x in 0..pw {
                        let f = ((x * 7 + y * 3 + c as u32 * 50) % 256) as f64;
                        let g =
                            (((x as f64 / 5.0).sin() + (y as f64 / 9.0).cos()) * 40.0) + f / 4.0;
                        v.push((g + 100.0).clamp(0.0, 255.0) as u8);
                    }
                }
                v
            })
            .collect()
    }

    #[test]
    fn decodes_to_reconstruction() {
        for (w, h, chroma) in [
            (64, 64, 1),
            (72, 40, 1),
            (8, 8, 0),
            (136, 72, 3),
            (24, 48, 2),
        ] {
            let planes = image(w, h, chroma);
            let empty: &[u8] = &[];
            let input = Picture8 {
                width: w,
                height: h,
                planes: [
                    &planes[0],
                    planes.get(1).map_or(empty, |p| p),
                    planes.get(2).map_or(empty, |p| p),
                ],
            };
            for qp in [22, 37] {
                let s = Settings {
                    qp,
                    chroma,
                    sar: None,
                    effort: 1,
                    deblocking: true,
                    fps: 25.0,
                };
                // Without deblocking the decoder reproduces the encoder's
                // reconstruction exactly.
                let raw = Settings {
                    deblocking: false,
                    ..s
                };
                let (nals, recon) = encode(&input, &raw).unwrap();
                let frame = super::super::decode_nals(nals.iter().map(|n| n.as_slice())).unwrap();
                for (c, r) in recon.iter().enumerate() {
                    let got: Vec<u8> = frame.planes[c].0.iter().map(|&v| v as u8).collect();
                    assert!(got == *r, "{w}x{h} {chroma} {qp}: component {c} differs");
                }
                let (nals, _) = encode(&input, &s).unwrap();
                let frame = super::super::decode_nals(nals.iter().map(|n| n.as_slice()))
                    .unwrap_or_else(|e| panic!("{w}x{h} {chroma} {qp}: {e:?}"));
                assert_eq!(frame.width, w);
                // Deblocked output stays close to the source.
                let y = &frame.planes[0];
                let mut err = 0f64;
                for yy in 0..h as usize {
                    for xx in 0..w as usize {
                        let d = f64::from(y.0[yy * y.1 + xx])
                            - f64::from(planes[0][yy * w as usize + xx]);
                        err += d * d;
                    }
                }
                let psnr = 10.0 * (255.0f64 * 255.0 / (err / f64::from(w * h))).log10();
                assert!(psnr > 25.0, "{w}x{h} {chroma} {qp}: {psnr}");
            }
        }
    }
}
