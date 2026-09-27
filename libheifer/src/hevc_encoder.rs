// SPDX-License-Identifier: LGPL-3.0-or-later
//! Built-in HEVC encoding with the pure Rust hpvca encoder (crates.io).
//!
//! libheif encodes HEVC with its x265 plugin. hpvca is an independent
//! all-intra encoder, so its bitstreams differ from x265's; the C plugin
//! record reproduces the x265 plugin's parameters, input checks, padding and
//! packet interface, and this module turns planar samples into HEVC NAL units.
//! hpvca's public API produces a HEIC file; the parameter sets are taken from
//! its `hvcC` box and the slice from its item data.

/// Colour signalling written to the SPS VUI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VuiSignal {
    pub full_range: bool,
    /// (colour primaries, transfer characteristics, matrix coefficients)
    pub description: Option<(u8, u8, u8)>,
}

/// Encoder settings derived from the plugin parameters.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// libheif quality, 0..=100.
    pub quality: i32,
    pub lossless: bool,
    /// hpvca's slower search tier (x265 presets from "slow" upwards).
    pub slow: bool,
    /// Disable variance-boost adaptive quantization (x265 tune "psnr").
    pub psnr: bool,
    pub vui: VuiSignal,
}

/// One picture: planar samples at `bit_depth`, luma first. `chroma` is the
/// libheif chroma value (0 monochrome, 1 4:2:0, 2 4:2:2, 3 4:4:4).
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub chroma: i32,
    pub planes: [&'a [u16]; 3],
}

/// hpvca's quality → QP mapping (`hevc::quality_to_qp`).
fn hpvca_qp(quality: u8) -> i32 {
    ((100 - i32::from(quality.clamp(1, 100))) * 41 / 99 + 10).min(51)
}

/// The hpvca quality for a libheif quality.
///
/// x265 maps quality to CRF (100 - quality) / 2. hpvca has no rate control;
/// its quality selects a constant QP in 10..=51. The target QP is the CRF
/// minus one, which matches x265's luma PSNR on photographic content
/// (`tools/bench_hevc_encoding.py`); qualities that would need a QP below 10
/// get 10.
pub fn quality_to_hpvca(quality: i32) -> u8 {
    let target = (f64::from(100 - quality.clamp(0, 100)) / 2.0 - 1.0).round() as i32;
    (1..=100u8)
        .min_by_key(|&q| ((hpvca_qp(q) - target).abs(), std::cmp::Reverse(q)))
        .unwrap_or(100)
}

fn cicp(vui: &VuiSignal) -> Option<hpvca::Cicp> {
    use hpvca::{MatrixCoefficients as M, Primaries as P, TransferFunction as T};
    let (p, t, m) = vui.description?;
    let primaries = match p {
        1 => P::Bt709,
        2 => P::Unspecified,
        4 => P::Bt470M,
        5 => P::Bt470Bg,
        6 => P::Bt601,
        7 => P::Smpte240,
        8 => P::GenericFilm,
        9 => P::Bt2020,
        10 => P::Xyz,
        11 => P::Smpte431,
        12 => P::Smpte432,
        22 => P::Ebu3213,
        _ => return None,
    };
    let transfer = match t {
        1 => T::Bt709,
        2 => T::Unspecified,
        4 => T::Bt470M,
        5 => T::Bt470Bg,
        6 => T::Bt601,
        7 => T::Smpte240,
        8 => T::Linear,
        9 => T::Log100,
        10 => T::Log100sqrt10,
        11 => T::Iec61966,
        12 => T::Bt1361,
        13 => T::Srgb,
        14 => T::Bt202010bit,
        15 => T::Bt202012bit,
        16 => T::Smpte2084,
        17 => T::Smpte428,
        18 => T::Hlg,
        _ => return None,
    };
    let matrix = match m {
        0 => M::Identity,
        1 => M::Bt709,
        2 => M::Unspecified,
        4 => M::Fcc,
        5 => M::Bt470Bg,
        6 => M::Smpte170m,
        7 => M::Smpte240m,
        8 => M::YCgCo,
        9 => M::Bt2020Ncl,
        10 => M::Bt2020Cl,
        11 => M::Smpte2085,
        12 => M::ChromaticityDerivedNCL,
        13 => M::ChromaticityDerivedCL,
        14 => M::ICtCp,
        _ => return None,
    };
    Some(hpvca::Cicp {
        primaries,
        transfer,
        matrix,
        full_range: vui.full_range,
    })
}

fn yuv(picture: &Picture) -> Result<hpvca::Yuv, String> {
    let chroma = match picture.chroma {
        0 => hpvca::ChromaFormat::Monochrome,
        1 => hpvca::ChromaFormat::Yuv420,
        2 => hpvca::ChromaFormat::Yuv422,
        3 => hpvca::ChromaFormat::Yuv444,
        _ => return Err("unsupported chroma format".into()),
    };
    let bits = match picture.bit_depth {
        8 => hpvca::BitDepth::Eight,
        10 => hpvca::BitDepth::Ten,
        12 => hpvca::BitDepth::Twelve,
        _ => return Err("unsupported bit depth".into()),
    };
    let (cb, cr) = if picture.chroma == 0 {
        (Vec::new(), Vec::new())
    } else {
        (picture.planes[1].to_vec(), picture.planes[2].to_vec())
    };
    hpvca::Yuv::from_planes(
        picture.planes[0].to_vec(),
        cb,
        cr,
        picture.width,
        picture.height,
        chroma,
        bits,
    )
    .map_err(|e| e.to_string())
}

/// Boxes as (type, payload).
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= data.len() {
        let size = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = data[at + 4..at + 8].try_into().unwrap();
        if size < 8 || at + size > data.len() {
            break;
        }
        out.push((kind, &data[at + 8..at + size]));
        at += size;
    }
    out
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Result<&'a [u8], String> {
    boxes(data)
        .into_iter()
        .find(|(k, _)| k == kind)
        .map(|(_, b)| b)
        .ok_or_else(|| format!("hpvca output without {}", String::from_utf8_lossy(kind)))
}

fn be(data: &[u8], at: usize, n: usize) -> Result<u64, String> {
    let bytes = data.get(at..at + n).ok_or("truncated hpvca output")?;
    Ok(bytes.iter().fold(0, |v, &b| (v << 8) | u64::from(b)))
}

/// The NAL units of hpvca's single-item HEIC output: the `hvcC` arrays, then
/// the length-prefixed units of item 1's data.
fn nal_units(file: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let meta = child(file, b"meta")?.get(4..).ok_or("truncated meta")?;
    let ipco = child(child(meta, b"iprp")?, b"ipco")?;
    let hvcc = child(ipco, b"hvcC")?;
    let mut units = Vec::new();
    let mut at = 23;
    for _ in 0..*hvcc.get(22).ok_or("truncated hvcC")? {
        let count = be(hvcc, at + 1, 2)?;
        at += 3;
        for _ in 0..count {
            let size = be(hvcc, at, 2)? as usize;
            units.push(
                hvcc.get(at + 2..at + 2 + size)
                    .ok_or("truncated hvcC")?
                    .to_vec(),
            );
            at += 2 + size;
        }
    }
    // iloc: the file extents of item 1.
    let iloc = child(meta, b"iloc")?;
    let version = *iloc.first().ok_or("truncated iloc")?;
    let sizes = be(iloc, 4, 2)? as usize;
    let (offset_size, length_size, base_size) = (sizes >> 12, (sizes >> 8) & 15, (sizes >> 4) & 15);
    let index_size = if version >= 1 { sizes & 15 } else { 0 };
    let id_size = if version < 2 { 2 } else { 4 };
    let items = be(iloc, 6, id_size)?;
    let mut at = 6 + id_size;
    let mut data = Vec::new();
    for _ in 0..items {
        let id = be(iloc, at, id_size)?;
        at += id_size + if version >= 1 { 2 } else { 0 } + 2;
        let base = be(iloc, at, base_size)?;
        at += base_size;
        let extents = be(iloc, at, 2)?;
        at += 2;
        for _ in 0..extents {
            at += index_size;
            let offset = be(iloc, at, offset_size)?;
            at += offset_size;
            let length = be(iloc, at, length_size)?;
            at += length_size;
            if id == 1 {
                let start = (base + offset) as usize;
                data.extend_from_slice(
                    file.get(start..start + length as usize)
                        .ok_or("hpvca item extent outside the file")?,
                );
            }
        }
    }
    let mut at = 0;
    while at < data.len() {
        let size = be(&data, at, 4)? as usize;
        units.push(
            data.get(at + 4..at + 4 + size)
                .ok_or("truncated slice")?
                .to_vec(),
        );
        at += 4 + size;
    }
    Ok(units)
}

/// Encodes one intra picture into VPS, SPS, PPS and slice NAL units (without
/// start codes, with emulation prevention).
pub fn encode(picture: &Picture, settings: &Settings) -> Result<Vec<Vec<u8>>, String> {
    let yuv = yuv(picture)?;
    let mut cfg = hpvca::EncodeConfig::new()
        .with_quality(quality_to_hpvca(settings.quality))
        .with_lossless(settings.lossless)
        .with_parallelism(hpvca::ParallelismStrategy::Single)
        .with_speed(if settings.slow {
            hpvca::Speed::Slow
        } else {
            hpvca::Speed::Fast
        })
        .with_threads(1)
        // With persistent Rice adaptation, hpvca 0.1.17 writes lossless
        // streams above 8 bits that libde265 and oxideav-h265 both find
        // malformed (libde265 conceals the error).
        .with_persistent_rice(false);
    // The VUI: a colour description only when hpvca knows all three code
    // points; without one it signals full range.
    cfg.color = hpvca::ColorMetadata {
        cicp: cicp(&settings.vui),
        icc: None,
    };
    if settings.psnr {
        cfg = cfg.with_variance_boost(6, 0.0, false);
    }
    let file = hpvca::encode_yuv(&yuv, &cfg).map_err(|e| e.to_string())?;
    nal_units(&file)
}
