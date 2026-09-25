// SPDX-License-Identifier: LGPL-3.0-or-later
//! Built-in AVC encoding with the rusty_h264 encoder (crates.io, unmodified).
//!
//! libheif encodes AVC with its x264 plugin. rusty_h264-encoder is an
//! independent encoder, so its bitstreams differ from x264's; the C plugin
//! record reproduces the x264 plugin's parameters, input checks, padding and
//! packet interface, and this module turns 8-bit 4:2:0 samples into H.264 NAL
//! units. The encoder writes no VUI, so the SPS is rewritten here with the VUI
//! fields x264 writes (colour signalling, sample aspect ratio, timing and
//! bitstream restrictions) and the level x264 would choose.

use rusty_h264_common::{Profile, YuvFrame};
use rusty_h264_encoder::{Encoder, EncoderConfig, Preset};

/// Colour signalling written to the SPS VUI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VuiSignal {
    pub full_range: bool,
    /// (colour primaries, transfer characteristics, matrix coefficients)
    pub description: Option<(u8, u8, u8)>,
    /// Sample aspect ratio (not 1:1, both below 65536).
    pub sar: Option<(u16, u16)>,
}

/// Encoder settings derived from the plugin parameters.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// libheif quality, 0..=100.
    pub quality: i32,
    /// rusty_h264's search tier for the x264 preset.
    pub speed: Speed,
    /// CABAC entropy coding (x264 turns it off for preset ultrafast and tune
    /// fastdecode).
    pub cabac: bool,
    /// Baseline profile: x264 chooses it when CABAC and B-frames are off, which
    /// among its presets happens only with ultrafast.
    pub baseline: bool,
    pub vui: VuiSignal,
}

/// The encoder preset an x264 preset maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Fast,
    Balanced,
    Quality,
}

/// One 8-bit 4:2:0 picture; `planes` are tightly packed Y, Cb and Cr.
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub planes: [&'a [u8]; 3],
}

/// The constant intra QP for a libheif quality.
///
/// x264 maps quality to CRF (100 - quality) / 2 and codes a lone I-frame a
/// few QPs below the CRF (its ipratio), less so at high CRF. rusty_h264-encoder
/// has no CRF mode; this constant QP tracks x264's luma PSNR at the same
/// quality on photographic content (`tools/bench_hevc_encoding.py --codec avc`).
pub fn quality_to_qp(quality: i32) -> u8 {
    let crf = f64::from(100 - quality.clamp(0, 100)) / 2.0;
    (crf - 3.0 + 0.07 * (crf - 15.0).max(0.0))
        .round()
        .clamp(0.0, 51.0) as u8
}

/// x264's automatic level: the first level whose frame size and DPB limits
/// admit the picture (`x264_validate_levels`; the MB rate is negligible at
/// the 1/25 frame rate libheif's plugin uses for still images). Level 1b is
/// written as level 1.1 with constraint_set3 for Baseline and Main profile,
/// as x264 does.
/// Also returns the level's vertical MV range, from which x264 derives the
/// VUI's maximum MV length.
fn level(width: u32, height: u32, dpb_frames: u32) -> (u8, bool, u32) {
    // (level_idc, frame size in MBs, DPB in MBs, MV range)
    const LEVELS: [(u8, u32, u32, u32); 20] = [
        (10, 99, 396, 64),
        (9, 99, 396, 64),
        (11, 396, 900, 128),
        (12, 396, 2376, 128),
        (13, 396, 2376, 128),
        (20, 396, 2376, 128),
        (21, 792, 4752, 256),
        (22, 1620, 8100, 256),
        (30, 1620, 8100, 256),
        (31, 3600, 18000, 512),
        (32, 5120, 20480, 512),
        (40, 8192, 32768, 512),
        (41, 8192, 32768, 512),
        (42, 8704, 34816, 512),
        (50, 22080, 110400, 512),
        (51, 36864, 184320, 512),
        (52, 36864, 184320, 512),
        (60, 139264, 696320, 8192),
        (61, 139264, 696320, 8192),
        (62, 139264, 696320, 8192),
    ];
    let (mw, mh) = (width.div_ceil(16), height.div_ceil(16));
    let mbs = mw * mh;
    let fits = |&(_, frame, dpb, _): &(u8, u32, u32, u32)| {
        frame >= mbs && frame * 8 >= mw * mw && frame * 8 >= mh * mh && dpb >= mbs * dpb_frames
    };
    let chosen = LEVELS.iter().position(fits).unwrap_or(LEVELS.len() - 1);
    let (idc, _, _, mv_range) = LEVELS[chosen];
    match idc {
        9 => (11, true, mv_range),
        idc => (idc, false, mv_range),
    }
}

/// Encodes one intra picture into SPS, PPS and slice NAL units (without start
/// codes, with emulation prevention).
pub fn encode(picture: &Picture, settings: &Settings) -> Result<Vec<Vec<u8>>, String> {
    let (width, height) = (picture.width as usize, picture.height as usize);
    let mut cfg = EncoderConfig::new(width, height);
    // x264_param_apply_profile(.., "main") on 8-bit input: no 8x8 transform;
    // the profile then follows the coding tools.
    cfg.profile = if settings.baseline {
        Profile::ConstrainedBaseline
    } else {
        Profile::Main
    };
    cfg.transform_8x8 = false;
    cfg.cabac = settings.cabac && !settings.baseline;
    cfg.bframes = 0;
    cfg.num_ref_frames = 1;
    cfg.gop_size = 1;
    cfg.min_keyint = 1;
    cfg.scenecut = 0;
    cfg.lookahead = 0;
    cfg.mbtree = false;
    cfg.bitrate = 0;
    cfg.framerate = 25.0;
    cfg.i_qp_offset = 0;
    cfg.qp = quality_to_qp(settings.quality);
    cfg.preset = match settings.speed {
        Speed::Fast => Preset::Fast,
        Speed::Balanced => Preset::Balanced,
        Speed::Quality => Preset::Quality,
    };
    let (level_idc, constraint_set3, mv_range) = level(picture.width, picture.height, 1);
    cfg.level_idc = level_idc;
    let mut encoder = Encoder::new(cfg).map_err(|e| e.to_string())?;
    let frame = YuvFrame {
        width,
        height,
        y: picture.planes[0].to_vec(),
        u: picture.planes[1].to_vec(),
        v: picture.planes[2].to_vec(),
    };
    let mut stream = encoder.try_encode(&frame).map_err(|e| e.to_string())?;
    stream.extend(encoder.try_flush().map_err(|e| e.to_string())?);
    annex_b_units(&stream)
        .into_iter()
        .map(|unit| {
            if unit.first().map(|h| h & 0x1f) == Some(7) {
                rewrite_sps(
                    unit,
                    settings.baseline,
                    level_idc,
                    constraint_set3,
                    mv_range,
                    &settings.vui,
                )
            } else {
                Ok(unit.to_vec())
            }
        })
        .collect()
}

/// The NAL units of an Annex B byte stream.
fn annex_b_units(stream: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= stream.len() {
        if stream[i] == 0 && stream[i + 1] == 0 && stream[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let mut end = starts.get(n + 1).map_or(stream.len(), |&s| s - 3);
            // Trailing zero bytes belong to the next start code.
            while end > start && stream[end - 1] == 0 {
                end -= 1;
            }
            &stream[start..end]
        })
        .filter(|unit| !unit.is_empty())
        .collect()
}

fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 64);
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bits: usize,
}

impl BitWriter {
    fn bit(&mut self, bit: bool) {
        if self.bits.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if bit {
            *self.bytes.last_mut().unwrap() |= 0x80 >> (self.bits % 8);
        }
        self.bits += 1;
    }
    fn bits(&mut self, value: u32, count: u32) {
        for i in (0..count).rev() {
            self.bit(value >> i & 1 != 0);
        }
    }
    fn ue(&mut self, value: u32) {
        let v = u64::from(value) + 1;
        let length = 64 - v.leading_zeros();
        self.bits(0, length - 1);
        for i in (0..length).rev() {
            self.bit(v >> i & 1 != 0);
        }
    }
}

/// Replaces the SPS's `vui_parameters_present_flag = 0` (the last bit before
/// the RBSP stop bit) with x264's VUI, and writes x264's profile, constraint
/// flags and level.
fn rewrite_sps(
    unit: &[u8],
    baseline: bool,
    level_idc: u8,
    constraint_set3: bool,
    mv_range: u32,
    vui: &VuiSignal,
) -> Result<Vec<u8>, String> {
    let rbsp = unescape(&unit[1..]);
    let bit = |i: usize| rbsp[i / 8] >> (7 - i % 8) & 1 != 0;
    let total = rbsp.len() * 8;
    let stop = (0..total)
        .rev()
        .find(|&i| bit(i))
        .ok_or("SPS without a stop bit")?;
    if stop == 0 || bit(stop - 1) {
        return Err("SPS already carries a VUI".into());
    }
    let mut w = BitWriter::default();
    for i in 0..stop - 1 {
        w.bit(bit(i));
    }
    // The profile and constraint byte as x264 writes them: constraint_set0
    // for Baseline, constraint_set1 for Baseline and Main, constraint_set3
    // for level 1b.
    w.bytes[0] = if baseline { 66 } else { 77 };
    w.bytes[1] = if baseline { 0xc0 } else { 0x40 } | if constraint_set3 { 0x10 } else { 0 };
    w.bytes[2] = level_idc;
    w.bit(true); // vui_parameters_present_flag
    match vui.sar {
        Some((sw, sh)) => {
            const TABLE: [(u16, u16); 16] = [
                (1, 1),
                (12, 11),
                (10, 11),
                (16, 11),
                (40, 33),
                (24, 11),
                (20, 11),
                (32, 11),
                (80, 33),
                (18, 11),
                (15, 11),
                (64, 33),
                (160, 99),
                (4, 3),
                (3, 2),
                (2, 1),
            ];
            w.bit(true);
            match TABLE.iter().position(|&s| s == (sw, sh)) {
                Some(i) => w.bits(i as u32 + 1, 8),
                None => {
                    w.bits(255, 8);
                    w.bits(u32::from(sw), 16);
                    w.bits(u32::from(sh), 16);
                }
            }
        }
        None => w.bit(false),
    }
    w.bit(false); // overscan_info_present_flag
    // x264 keeps only the code points its tables know and writes 2 otherwise.
    let (p, t, m) = vui.description.unwrap_or((2, 2, 2));
    let (p, t, m) = (
        if p <= 12 { p } else { 2 },
        if t <= 18 { t } else { 2 },
        if m <= 14 { m } else { 2 },
    );
    let description = (p, t, m) != (2, 2, 2);
    let signal = vui.full_range || description;
    w.bit(signal);
    if signal {
        w.bits(5, 3); // video_format: unspecified
        w.bit(vui.full_range);
        w.bit(description);
        if description {
            w.bits(u32::from(p), 8);
            w.bits(u32::from(t), 8);
            w.bits(u32::from(m), 8);
        }
    }
    w.bit(false); // chroma_loc_info_present_flag
    // Timing from libheif's 1/25 frame rate: x264's timebase 25/1.
    w.bit(true);
    w.bits(25, 32); // num_units_in_tick
    w.bits(2, 32); // time_scale
    w.bit(true); // fixed_frame_rate_flag
    w.bit(false); // nal_hrd_parameters_present_flag
    w.bit(false); // vcl_hrd_parameters_present_flag
    w.bit(false); // pic_struct_present_flag
    w.bit(true); // bitstream_restriction_flag
    w.bit(true); // motion_vectors_over_pic_boundaries_flag
    w.ue(0); // max_bytes_per_pic_denom
    w.ue(0); // max_bits_per_mb_denom
    // (int)log2f(mv_range * 4 - 1) + 1
    let mv_length = 32 - (mv_range * 4 - 1).leading_zeros();
    w.ue(mv_length); // log2_max_mv_length_horizontal
    w.ue(mv_length); // log2_max_mv_length_vertical
    w.ue(0); // max_num_reorder_frames
    w.ue(1); // max_dec_frame_buffering
    w.bit(true); // rbsp_stop_one_bit
    let mut out = vec![unit[0]];
    out.extend(escape(&w.bytes));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_x264() {
        assert_eq!(level(64, 64, 1), (10, false, 64));
        assert_eq!(level(176, 144, 1), (10, false, 64));
        assert_eq!(level(352, 288, 1), (11, false, 128));
        assert_eq!(level(1920, 1080, 1), (40, false, 512));
    }

    #[test]
    fn escaping_round_trips() {
        let data = [0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 7];
        assert_eq!(unescape(&escape(&data)), data);
    }
}
