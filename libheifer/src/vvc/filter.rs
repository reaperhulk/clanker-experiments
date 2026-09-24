// SPDX-License-Identifier: LGPL-3.0-or-later
//! In-loop filters: LMCS inverse luma mapping, deblocking, SAO and ALF.
use super::Error;
use super::pic::Picture;
use super::ps::{AlfParam, PicHeader, Pps, SliceHeader, Sps};
use super::recon::Lmcs;

pub fn loop_filter(
    pic: &mut Picture,
    sps: &Sps,
    pps: &Pps,
    ph: &PicHeader,
    slices: &[SliceHeader],
    lmcs: Option<&Lmcs>,
    _alf: &[Option<AlfParam>; 8],
) -> Result<(), Error> {
    if let Some(l) = lmcs
        && sps.lmcs
    {
        let size = 1i32 << pic.ctu_log2;
        for addr in 0..pic.ctus.len() {
            let s = pic.ctus[addr].slice.ok_or(Error::Invalid("picture incomplete"))? as usize;
            if !slices[s].lmcs_used {
                continue;
            }
            let (cx, cy) = ((addr as u32 % pic.width_ctus) as i32, (addr as u32 / pic.width_ctus) as i32);
            let (x0, y0) = (cx * size, cy * size);
            let plane = &mut pic.planes[0];
            for y in y0..(y0 + size).min(pic.height) {
                for x in x0..(x0 + size).min(pic.width) {
                    let v = plane.at(x, y);
                    plane.set(x, y, l.inv_lut[v as usize]);
                }
            }
        }
    }
    if std::env::var_os("VVC_NO_DEBLOCK").is_none() {
        super::deblock::deblock(pic, sps, pps, ph, slices);
    }
    if std::env::var_os("VVC_NO_SAO").is_none() {
        super::sao::sao(pic, sps, pps, ph);
    }
    Ok(())
}
