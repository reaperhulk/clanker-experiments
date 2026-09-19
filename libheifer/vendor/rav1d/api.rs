// SPDX-License-Identifier: BSD-2-Clause
//! Safe owned Rust API added for libheifer; no foreign ABI calls.
#![forbid(unsafe_code)]
use crate::include::common::bitdepth::BitDepth8;
use crate::include::dav1d::{data::Rav1dData, dav1d::Rav1dSettings, picture::Rav1dPicture};
pub use crate::src::error::Rav1dError as Error;
use crate::src::{c_arc::CArc, c_box::CBox, internal::Rav1dContext, lib};
use std::sync::Arc;

pub struct Decoder(Option<Arc<Rav1dContext>>);
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    /// 0 monochrome, 1 4:2:0, 2 4:2:2, 3 4:4:4.
    pub layout: u8,
    pub planes: [Vec<u8>; 3],
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub full_range: bool,
    pub content_light: Option<(u16, u16)>,
}
impl Decoder {
    pub fn new(threads: i32, strict: bool, max_pixels: u32) -> Result<Self, Error> {
        let settings = Rav1dSettings {
            n_threads: threads,
            max_frame_delay: 0,
            all_layers: false,
            strict_std_compliance: strict,
            frame_size_limit: max_pixels,
            logger: None,
            ..Default::default()
        };
        Ok(Self(Some(lib::rav1d_open(&settings)?)))
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .map_err(|_| Error::ENOMEM)?;
        owned.extend_from_slice(bytes);
        let mut data = Rav1dData {
            data: Some(CArc::wrap(CBox::from_box(owned.into_boxed_slice()))?),
            ..Default::default()
        };
        lib::rav1d_send_data(self.0.as_ref().unwrap(), &mut data)
    }
    pub fn next_frame(&mut self) -> Result<Option<Frame>, Error> {
        let mut picture = Rav1dPicture::default();
        match lib::rav1d_get_picture(self.0.as_ref().unwrap(), &mut picture) {
            Err(Error::EAGAIN) => return Ok(None),
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        let width = u32::try_from(picture.p.w).map_err(|_| Error::EINVAL)?;
        let height = u32::try_from(picture.p.h).map_err(|_| Error::EINVAL)?;
        let bit_depth = picture.p.bpc;
        let layout = picture.p.layout as u8;
        let data = picture.data.as_ref().ok_or(Error::EINVAL)?;
        let seq = picture.seq_hdr.as_ref().ok_or(Error::EINVAL)?;
        let mut planes: [Vec<u8>; 3] = Default::default();
        for (index, plane) in planes
            .iter_mut()
            .enumerate()
            .take(if layout == 0 { 1 } else { 3 })
        {
            let w = if index != 0 && layout != 3 {
                width.div_ceil(2)
            } else {
                width
            } as usize;
            let h = if index != 0 && layout == 1 {
                height.div_ceil(2)
            } else {
                height
            } as usize;
            let row = w
                .checked_mul(if bit_depth > 8 { 2 } else { 1 })
                .ok_or(Error::ENOMEM)?;
            plane
                .try_reserve_exact(row.checked_mul(h).ok_or(Error::ENOMEM)?)
                .map_err(|_| Error::ENOMEM)?;
            let component = &data.data[index];
            let origin = component.pixel_offset::<BitDepth8>();
            let stride = picture.stride[usize::from(index != 0)];
            for y in 0..h {
                let offset = (y as isize).checked_mul(stride).ok_or(Error::EINVAL)?;
                let start = origin.checked_add_signed(offset).ok_or(Error::EINVAL)?;
                plane.extend_from_slice(&component.slice::<BitDepth8, _>(start..start + row));
            }
        }
        Ok(Some(Frame {
            width,
            height,
            bit_depth,
            layout,
            planes,
            color_primaries: seq.pri.0,
            transfer_characteristics: seq.trc.0,
            matrix_coefficients: seq.mtrx.0,
            full_range: seq.color_range != 0,
            content_light: picture
                .content_light
                .as_ref()
                .map(|p| (p.max_content_light_level, p.max_frame_average_light_level)),
        }))
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        if let Some(context) = self.0.take() {
            lib::rav1d_close(context);
        }
    }
}
