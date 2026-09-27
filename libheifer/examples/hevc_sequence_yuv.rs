// SPDX-License-Identifier: LGPL-3.0-or-later
//! Decode a whole Annex B HEVC stream with oxideav-h265 (the candidate's
//! range-extension decoder) and write the output pictures to stdout in output
//! order as planar YUV: 8-bit samples, or 16-bit little-endian for every
//! plane when any plane is above 8 bits, as the HM reference decoder writes
//! its reconstruction.
use oxideav_h265::picture::Plane;
use oxideav_h265::sequence::{DecodedFrame, SequenceDecoder};
use std::io::Write;

fn write(out: &mut impl Write, frame: &DecodedFrame) -> std::io::Result<()> {
    let picture = frame.output_picture();
    let planes: &[Plane] = if picture.chroma_array_type() == 0 {
        &[Plane::Luma]
    } else {
        &[Plane::Luma, Plane::Cb, Plane::Cr]
    };
    let wide = planes.iter().any(|&plane| picture.bit_depth(plane) > 8);
    let mut bytes = Vec::new();
    for &plane in planes {
        for &s in picture.plane(plane) {
            if wide {
                bytes.extend_from_slice(&(s as u16).to_le_bytes());
            } else {
                bytes.push(s as u8);
            }
        }
    }
    out.write_all(&bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(std::env::args_os().nth(1).ok_or("input path required")?)?;
    let mut decoder = SequenceDecoder::new();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    // Output order: PicOrderCntVal within a coded video sequence, bumping once
    // more pictures are pending than sps_max_num_reorder_pics allows.
    let mut pending: Vec<DecodedFrame> = Vec::new();
    let mut count = 0;
    let mut bump =
        |pending: &mut Vec<DecodedFrame>, keep: usize, out: &mut _| -> std::io::Result<()> {
            pending.sort_by_key(|f| (f.cvs_index, f.poc));
            while pending.len() > keep {
                let frame = pending.remove(0);
                if frame.output {
                    write(out, &frame)?;
                    count += 1;
                }
            }
            Ok(())
        };
    for unit in oxideav_h265::NalIter::new(&data) {
        decoder.push_nal_unit(unit?)?;
        let decoded = decoder.take_decoded();
        if let Some(cvs) = decoded.last().map(|f| f.cvs_index) {
            // A new coded video sequence outputs everything before it.
            if pending.iter().any(|f| f.cvs_index < cvs) {
                let (older, newer) = pending.drain(..).partition(|f| f.cvs_index < cvs);
                let mut older: Vec<_> = older;
                bump(&mut older, 0, &mut out)?;
                pending = newer;
            }
            pending.extend(decoded);
            let keep = decoder.max_num_reorder_pics().unwrap_or(0) as usize;
            bump(&mut pending, keep, &mut out)?;
        }
    }
    decoder.flush()?;
    pending.extend(decoder.take_decoded());
    bump(&mut pending, 0, &mut out)?;
    out.flush()?;
    eprintln!("{count} pictures");
    Ok(())
}
