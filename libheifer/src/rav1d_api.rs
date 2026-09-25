// SPDX-License-Identifier: LGPL-3.0-or-later
//! Owned wrapper over rav1d's public dav1d-compatible API (crates.io rav1d,
//! unmodified). This is the only module in the crate that uses `unsafe`: the
//! calls into rav1d's `extern "C"` functions and the reads of the picture
//! planes they return.
#![allow(unsafe_code)]
use rav1d::include::dav1d::{
    data::Dav1dData,
    dav1d::{Dav1dContext, Dav1dSettings},
    picture::Dav1dPicture,
};
use rav1d::src::lib as dav1d;
use std::{mem::MaybeUninit, ptr::NonNull};

/// A negative errno from rav1d.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub i32);

const EAGAIN: i32 = -11;

fn check(result: rav1d::Dav1dResult) -> Result<(), Error> {
    if result.0 < 0 {
        Err(Error(result.0))
    } else {
        Ok(())
    }
}

pub struct Decoder(Option<Dav1dContext>);

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
}

impl Decoder {
    pub fn new(threads: i32, strict: bool, max_pixels: u32) -> Result<Self, Error> {
        let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
        // SAFETY: `dav1d_default_settings` initializes every field.
        let mut settings = unsafe {
            dav1d::dav1d_default_settings(NonNull::new_unchecked(settings.as_mut_ptr()));
            settings.assume_init()
        };
        settings.n_threads = threads;
        settings.max_frame_delay = 0;
        settings.all_layers = 0;
        settings.strict_std_compliance = i32::from(strict);
        settings.frame_size_limit = max_pixels;
        let mut context: Option<Dav1dContext> = None;
        // SAFETY: both pointers are valid for the duration of the call.
        check(unsafe {
            dav1d::dav1d_open(
                Some(NonNull::from(&mut context)),
                Some(NonNull::from(&mut settings)),
            )
        })?;
        Ok(Self(context))
    }

    /// Sends one buffer (copied into rav1d-owned memory).
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut data = Dav1dData::default();
        // SAFETY: `data` is valid; on success the returned buffer holds
        // `bytes.len()` writable bytes owned by `data`.
        let buffer =
            unsafe { dav1d::dav1d_data_create(Some(NonNull::from(&mut data)), bytes.len()) };
        if buffer.is_null() {
            return Err(Error(-12));
        }
        // SAFETY: see above; the regions do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
        // SAFETY: the context is open and `data` is valid for reads and writes.
        let result =
            check(unsafe { dav1d::dav1d_send_data(self.0, Some(NonNull::from(&mut data))) });
        // SAFETY: releases whatever rav1d did not consume (a no-op when empty).
        unsafe { dav1d::dav1d_data_unref(Some(NonNull::from(&mut data))) };
        result
    }

    /// The next output picture, or `None` when rav1d needs more data.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, Error> {
        let mut picture = Dav1dPicture::default();
        // SAFETY: the context is open and `picture` is valid for writes.
        match check(unsafe { dav1d::dav1d_get_picture(self.0, Some(NonNull::from(&mut picture))) })
        {
            Err(Error(EAGAIN)) => return Ok(None),
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        let frame = copy_frame(&picture);
        // SAFETY: `picture` came from `dav1d_get_picture`.
        unsafe { dav1d::dav1d_picture_unref(Some(NonNull::from(&mut picture))) };
        frame.map(Some)
    }
}

fn copy_frame(picture: &Dav1dPicture) -> Result<Frame, Error> {
    let invalid = Error(-22);
    let width = u32::try_from(picture.p.w).map_err(|_| invalid)?;
    let height = u32::try_from(picture.p.h).map_err(|_| invalid)?;
    let bit_depth = u8::try_from(picture.p.bpc).map_err(|_| invalid)?;
    let layout = u8::try_from(picture.p.layout).map_err(|_| invalid)?;
    // SAFETY: a returned picture carries its sequence header until unref.
    let seq = unsafe { picture.seq_hdr.ok_or(invalid)?.as_ref() };
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
            .ok_or(invalid)?;
        let stride = picture.stride[usize::from(index != 0)];
        let origin = picture.data[index].ok_or(invalid)?.as_ptr().cast::<u8>();
        plane
            .try_reserve_exact(row.checked_mul(h).ok_or(invalid)?)
            .map_err(|_| Error(-12))?;
        for y in 0..h {
            let offset = (y as isize).checked_mul(stride).ok_or(invalid)?;
            // SAFETY: rav1d guarantees `h` rows of at least `row` bytes at
            // `stride` from the plane origin while the picture is referenced.
            plane.extend_from_slice(unsafe {
                std::slice::from_raw_parts(origin.offset(offset), row)
            });
        }
    }
    Ok(Frame {
        width,
        height,
        bit_depth,
        layout,
        planes,
        color_primaries: seq.pri as u8,
        transfer_characteristics: seq.trc as u8,
        matrix_coefficients: seq.mtrx as u8,
        full_range: seq.color_range != 0,
    })
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if self.0.is_some() {
            // SAFETY: closes the context opened in `new` exactly once.
            unsafe { dav1d::dav1d_close(Some(NonNull::from(&mut self.0))) };
        }
    }
}
