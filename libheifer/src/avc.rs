// SPDX-License-Identifier: LGPL-3.0-or-later
//! AVC (H.264) sample decoding through the vendored Rust-only rusty_h264 decoder.
//!
//! libheif's only AVC decoder is its OpenH264 plugin. This adapter reproduces
//! that plugin's observable behavior: the length-prefixed to Annex B conversion
//! (including its start-code emulation handling), OpenH264's acceptance rules
//! for sequence parameter sets and CAVLC levels, 4:2:0 8-bit output with flat
//! chroma for monochrome streams, and the plugin's error codes and messages.
use crate::{
    context::{ContextError, Document},
    decoding::DecodeOptions,
    image::Image,
};

fn plugin_error(subcode: i32, message: &str) -> ContextError {
    ContextError::new(
        7,
        subcode,
        format!(
            "Decoder plugin generated an error: {}: {message}",
            crate::error_text::subcode_text(subcode)
        ),
    )
}

fn decoder_error() -> ContextError {
    plugin_error(0, "OpenH264 decoder error")
}

/// The plugin's conversion of four-byte length-prefixed NAL units to an Annex B
/// stream. It reproduces the native loop exactly, including its emulation
/// "check" that only inspects the first bytes of each unit.
fn annex_b(input: &[u8]) -> Result<Vec<u8>, ContextError> {
    let eof = || plugin_error(100, "Insufficient input data");
    let mut out = Vec::with_capacity(input.len() + input.len() / 8);
    let mut idx = 0usize;
    while idx < input.len() {
        // `indata.size() - 4 < idx` with unsigned arithmetic; push rejects < 4 bytes.
        if input.len() - 4 < idx {
            return Err(eof());
        }
        let mut size =
            u32::from_be_bytes([input[idx], input[idx + 1], input[idx + 2], input[idx + 3]])
                as usize;
        idx += 4;
        if input.len() < size || input.len() - size < idx {
            return Err(eof());
        }
        out.extend_from_slice(&[0, 0, 1]);
        let mut check = true;
        while check && size >= 3 {
            check = false;
            // The native loop compares the unit's first three bytes on every
            // iteration; a match inserts an emulation prevention byte after two
            // leading zero bytes and restarts. Otherwise the loop runs out.
            // The inner `for (i = 0; i < size - 3; ...)` runs only when size > 3.
            if size > 3 && input[idx] == 0 && input[idx + 1] == 0 && input[idx + 2] <= 3 {
                out.extend_from_slice(&[0, 0, 3]);
                idx += 2;
                size -= 2;
                check = true;
            }
        }
        if size == 0 {
            return Err(plugin_error(2006, "Invalid input data"));
        }
        out.extend_from_slice(&input[idx..idx + size]);
        idx += size;
    }
    Ok(out)
}

pub fn decode(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
) -> Result<Image, ContextError> {
    if options.decoder_id.is_some_and(|id| id != b"rusty_h264") {
        return Err(ContextError::new(
            11,
            0,
            "Error while loading plugin: Unspecified: No decoder with that ID found.",
        ));
    }
    let container = document.container()?;
    let config = container
        .property(id, *b"avcC")
        .ok()
        .and_then(crate::avc_config::parse_configuration)
        .unwrap_or_default();
    // libheif tightens the pixel limit to the ispe size padded by one
    // macroblock, then rejects coded SPS sizes beyond it before decoding.
    let info = &document.images[&id];
    let mut limits = document.current_limits();
    if info.ispe.0 != 0
        && info.ispe.1 != 0
        && let Some(padded) = (u64::from(info.ispe.0) + 16).checked_mul(u64::from(info.ispe.1) + 16)
    {
        let maximum = padded.max(65536);
        if limits.max_image_size_pixels == 0 || maximum < limits.max_image_size_pixels {
            limits.max_image_size_pixels = maximum;
        }
    }
    if let Some((width, height)) = crate::avc_config::coded_size(&config)? {
        limits.check_image_size(width, height)?;
    }
    let mut data = config.header_nals();
    data.extend_from_slice(&crate::decoding::decoder_payload(document, id)?);
    if data.is_empty() {
        return Err(ContextError::invalid(
            0,
            "Unspecified: Input with empty data extent.",
        ));
    }
    if data.len() < 4 {
        return Err(plugin_error(2006, "Invalid input data"));
    }
    let accepted = crate::avc_openh264::accept(&annex_b(&data)?).map_err(|_| decoder_error())?;
    // One data call, then flush calls until a picture comes out. With error
    // concealment disabled, an incomplete picture decoded in OpenH264's data
    // call is an error (see `decode_call`); in its flush call it is not.
    let mut decoder = rusty_h264_decoder::Decoder::new();
    let mut reorder = Reorder::default();
    let picture = match decode_call(&mut reorder, &mut decoder, &accepted, 0, true)? {
        Some(picture) => picture,
        None => reorder.flush_frame().ok_or_else(|| {
            plugin_error(
                0,
                "Decoding the input data did not give a decompressed image.",
            )
        })?,
    };
    frame_image(&picture.frame, document, limits.max_image_size_pixels)
}

/// The plugin's output image: I420 planes added with `heif_image_add_plane_safe`
/// under the given pixel limit (the luma plane is checked first).
fn frame_image(
    frame: &rusty_h264_decoder::YuvFrame,
    document: &Document,
    maximum: u64,
) -> Result<Image, ContextError> {
    let (width, height) = (frame.width as u32, frame.height as u32);
    if maximum != 0 && height != 0 && maximum / u64::from(height) < u64::from(width) {
        return Err(ContextError::new(
            6,
            1000,
            format!(
                "Memory allocation error: Security limit exceeded: Allocating an image of size {width}x{height} exceeds the security limit of {maximum} pixels"
            ),
        ));
    }
    let mut image = Image::new(width, height, 0, 1)?;
    image.budget = Some(document.budget.clone());
    let planes: [(&[u8], u32, u32); 3] = [
        (&frame.y, width, height),
        (&frame.u, width.div_ceil(2), height.div_ceil(2)),
        (&frame.v, width.div_ceil(2), height.div_ceil(2)),
    ];
    for (channel, (samples, w, h)) in planes.into_iter().enumerate() {
        image.add_plane(channel as i32, w, h, 8)?;
        let plane = image.plane_mut(channel as i32).unwrap();
        let stride = plane.stride;
        let row = w as usize;
        for y in 0..h as usize {
            plane.data_mut()[y * stride..y * stride + row]
                .copy_from_slice(&samples[y * row..y * row + row]);
        }
    }
    Ok(image)
}

/// A decoded picture waiting in OpenH264's output (reordering) list.
struct Buffered {
    frame: rusty_h264_decoder::YuvFrame,
    poc: i32,
    seq: i32,
    dts: u32,
    user_data: u64,
}

/// libheif's OpenH264 plugin over one track chunk: packets are queued by
/// `push`, and each `decode_next` runs `DecodeFrameNoDelay` on one packet (or
/// `FlushFrame` after end of input), with OpenH264's single-threaded output
/// ordering (`ReorderPicturesInDisplay`).
pub struct SequenceDecoder {
    syntax: crate::avc_openh264::Syntax,
    decoder: rusty_h264_decoder::Decoder,
    queue: std::collections::VecDeque<(Vec<u8>, u64)>,
    eof: bool,
    reorder: Reorder,
}

impl Default for SequenceDecoder {
    fn default() -> Self {
        SequenceDecoder {
            syntax: crate::avc_openh264::Syntax::default(),
            decoder: rusty_h264_decoder::Decoder::new(),
            queue: std::collections::VecDeque::new(),
            eof: false,
            reorder: Reorder::default(),
        }
    }
}

impl SequenceDecoder {
    /// `openh264_push_data2`: packets under four bytes are refused.
    pub fn push(&mut self, data: Vec<u8>, user_data: u64) -> Result<(), ContextError> {
        if data.len() < 4 {
            return Err(plugin_error(2006, "Invalid input data"));
        }
        self.queue.push_back((data, user_data));
        Ok(())
    }

    /// `openh264_flush_data`.
    pub fn flush(&mut self) {
        self.eof = true;
    }

    /// `openh264_decode_next_image2`: `Ok(None)` when no picture is output.
    pub fn decode_next(
        &mut self,
        document: &Document,
        maximum: u64,
    ) -> Result<Option<(Image, u64)>, ContextError> {
        let output = if let Some((data, user_data)) = self.queue.pop_front() {
            let stream = annex_b(&data)?;
            self.reorder.dts = self.reorder.dts.wrapping_add(1);
            let accepted = self.syntax.accept(&stream).map_err(|_| decoder_error())?;
            decode_call(
                &mut self.reorder,
                &mut self.decoder,
                &accepted,
                user_data,
                false,
            )?
        } else if self.eof {
            self.reorder.flush_frame()
        } else {
            return Ok(None);
        };
        match output {
            Some(b) => Ok(Some((
                frame_image(&b.frame, document, maximum)?,
                b.user_data,
            ))),
            None => Ok(None),
        }
    }
}

/// One `DecodeFrameNoDelay` call over a packet's accepted units. Its data half
/// decodes every unit but the stream's final NAL, which waits for the flush
/// half. `ReorderPicturesInDisplay` runs once per half, with the last picture
/// completed in it and the slice header decoded last (a later picture's, when
/// that picture's first slices were decoded already); earlier pictures never
/// reach the output list. The final picture completes in the flush half, which
/// resets the output first, unless an SEI or delimiter ended its access unit
/// in the data half; then the flush half outputs nothing.
fn decode_call(
    reorder: &mut Reorder,
    decoder: &mut rusty_h264_decoder::Decoder,
    accepted: &crate::avc_openh264::Accepted,
    user_data: u64,
    still: bool,
) -> Result<Option<Buffered>, ContextError> {
    let units = accepted.units.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let frames = decoder
        .decode_units_all(&units, still)
        .map_err(|_| decoder_error())?;
    if frames.is_empty() {
        if accepted.constructed_early {
            return Err(decoder_error());
        }
        return Ok(None);
    }
    let info = |index: usize| accepted.slices.get(index).copied().flatten();
    let last_slice = accepted.slices.iter().rposition(Option::is_some);
    let mut frames = frames;
    let final_frame = match frames.last() {
        Some((index, _)) if !accepted.constructed_early && Some(*index) == last_slice => {
            frames.pop()
        }
        _ => None,
    };
    // Data half: picture starts advance the sequence number.
    let mut decoded = None;
    for (index, slice) in accepted.slices.iter().enumerate() {
        if let Some(slice) = slice
            && Some(index) != accepted.held
        {
            if slice.first_mb == 0 {
                reorder.start(slice);
            }
            decoded = Some(*slice);
        }
    }
    if let Some((_, frame)) = frames.pop() {
        // Its output (a baseline picture) is reset by the flush half.
        let _ = reorder.picture(frame, decoded.ok_or_else(decoder_error)?, user_data);
    }
    // Flush half.
    if let Some(slice) = accepted.held.and_then(info)
        && slice.first_mb == 0
    {
        reorder.start(&slice);
    }
    let out = match final_frame {
        Some((index, frame)) => {
            reorder.picture(frame, info(index).ok_or_else(decoder_error)?, user_data)
        }
        None => None,
    };
    Ok(if accepted.constructed_early {
        None
    } else {
        out
    })
}

/// OpenH264's single-threaded output list (`ReorderPicturesInDisplay`).
#[derive(Default)]
struct Reorder {
    dts: u32,
    seq: i32,
    active_sps: Option<usize>,
    list: Vec<Buffered>,
    has_b: bool,
    last_written_poc: Option<i32>,
    last_seq: i32,
}

impl Reorder {
    /// A picture starts decoding: an IDR or another SPS begins a new sequence.
    fn start(&mut self, slice: &crate::avc_openh264::SliceInfo) {
        if slice.idr || self.active_sps != Some(slice.sps_id) {
            self.seq = self.seq.wrapping_add(1);
            self.active_sps = Some(slice.sps_id);
        }
    }

    /// A completed picture, recorded with the current slice header; returns
    /// the picture released for output, if any.
    fn picture(
        &mut self,
        frame: rusty_h264_decoder::YuvFrame,
        slice: crate::avc_openh264::SliceInfo,
        user_data: u64,
    ) -> Option<Buffered> {
        let picture = Buffered {
            frame,
            poc: slice.poc_lsb,
            seq: self.seq,
            dts: self.dts,
            user_data,
        };
        if matches!(slice.profile, 66 | 83) {
            return Some(picture);
        }
        if self.list.len() >= 16 {
            // BufferingReadyPicture finds no free slot: the picture is dropped.
            return None;
        }
        if slice.slice_type == 1 {
            self.has_b = true;
        }
        self.list.push(picture);
        if !self.has_b && self.list.len() > 1 {
            self.release_no_reorder()
        } else {
            self.release_reorder(false, Some((slice.poc_lsb, self.seq)))
        }
    }

    /// `FlushFrame`: one picture per call.
    fn flush_frame(&mut self) -> Option<Buffered> {
        if self.has_b {
            self.release_reorder(true, None)
        } else {
            self.release_no_reorder()
        }
    }

    /// `ReleaseBufferedReadyPictureNoReorder`: the earliest decoded picture.
    fn release_no_reorder(&mut self) -> Option<Buffered> {
        let index = (0..self.list.len()).min_by_key(|&i| self.list[i].dts)?;
        let picture = self.list.remove(index);
        self.last_written_poc = Some(picture.poc);
        self.last_seq = picture.seq;
        Some(picture)
    }

    /// `ReleaseBufferedReadyPictureReorder`: the smallest (sequence, POC)
    /// picture, once ready; `current` is the last decoded picture's (POC,
    /// sequence number) when not flushing.
    fn release_reorder(&mut self, flush: bool, current: Option<(i32, i32)>) -> Option<Buffered> {
        if self.list.is_empty() {
            return None;
        }
        let mut index = 0;
        for i in 1..self.list.len() {
            let (a, b) = (&self.list[i], &self.list[index]);
            let earlier = if a.seq == b.seq {
                a.poc < b.poc
            } else {
                a.seq.wrapping_sub(b.seq) < 0
            };
            if earlier {
                index = i;
            }
        }
        if !flush {
            let (poc, seq) = (self.list[index].poc, self.list[index].seq);
            let (last_poc, last_seq) = current.unwrap_or((poc, seq));
            let ready = self.last_written_poc.is_some_and(|w| poc - w <= 1)
                || poc < last_poc
                || seq.wrapping_sub(last_seq) < 0;
            if !ready {
                return None;
            }
        }
        let picture = self.list.remove(index);
        self.last_written_poc = Some(picture.poc);
        self.last_seq = picture.seq;
        Some(picture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annex_b_matches_native_emulation_quirk() {
        // Leading 00 00 0x bytes gain an emulation byte; later ones do not.
        let input = [0, 0, 0, 6, 0, 0, 1, 0x65, 0, 0];
        assert_eq!(annex_b(&input).unwrap(), [0, 0, 1, 0, 0, 3, 1, 0x65, 0, 0]);
        assert_eq!(annex_b(&[0, 0, 0, 9]).unwrap_err().subcode, 100);
        assert_eq!(annex_b(&[0, 0, 0, 0]).unwrap_err().subcode, 2006);
    }
}
