// SPDX-License-Identifier: LGPL-3.0-or-later
//! Metadata compression using Rust-only zlib and brotli implementations.
use crate::{context::ContextError, security::Budget};
use std::sync::Arc;
use zlib_rs::{Inflate, InflateFlush, Status};
type Result<T> = std::result::Result<T, ContextError>;

pub fn compress(data: &[u8], method: i32) -> Result<Vec<u8>> {
    if method == 5 {
        return Ok(compress_brotli(data));
    }
    if !matches!(method, 3 | 4) {
        return Ok(data.to_vec());
    }
    Ok(crate::deflate_compat::compress(data, method == 4))
}

/// zlib-rs replaces the error its fast decoding loop reports with "repeated
/// call with bad state" when that loop fails. zlib keeps the original message,
/// so replay the stream with an output buffer below the fast loop's minimum
/// (260 bytes); the slower loop reports the error without overwriting it.
fn original_message(data: &[u8], zlib: bool) -> Option<&'static str> {
    let mut codec = Inflate::new(zlib, 15);
    let mut buffer = [0u8; 256];
    loop {
        let before = (codec.total_in(), codec.total_out());
        match codec.decompress(
            &data[codec.total_in() as usize..],
            &mut buffer,
            InflateFlush::NoFlush,
        ) {
            Err(_) => return codec.error_message(),
            Ok(Status::StreamEnd) => return None,
            Ok(_) if (codec.total_in(), codec.total_out()) == before => return None,
            Ok(_) => {}
        }
    }
}

pub fn decompress(data: Vec<u8>, method: i32, budget: &Arc<Budget>) -> Result<Vec<u8>> {
    if method == 0 {
        return Ok(data);
    }
    if method == 5 {
        return decompress_brotli(&data, budget);
    }
    if !matches!(method, 3 | 4) {
        return Err(ContextError::new(
            3,
            3005,
            "Unsupported file-type: Unsupported header compression method",
        ));
    }
    if data.is_empty() {
        return Err(ContextError::invalid(
            150,
            "Invalid data in generic compression inflation: Empty zlib compressed data.",
        ));
    }
    let mut codec = Inflate::new(method == 4, 15);
    let mut output = Vec::new();
    let mut allocations = Vec::new();
    let mut buffer = vec![0u8; 8192];
    loop {
        let before = codec.total_out();
        let status = codec
            .decompress(
                &data[codec.total_in() as usize..],
                &mut buffer,
                InflateFlush::NoFlush,
            )
            .map_err(|e| {
                let code = match e {
                    zlib_rs::InflateError::NeedDict { .. } => 2,
                    zlib_rs::InflateError::StreamError => -2,
                    zlib_rs::InflateError::DataError => -3,
                    zlib_rs::InflateError::MemError => -4,
                };
                ContextError::invalid(
                    150,
                    &format!(
                        "Invalid data in generic compression inflation: Error performing zlib inflate: {} ({code})\n",
                        match codec.error_message() {
                            Some(m) if m.starts_with("repeated call with bad state") => {
                                original_message(&data, method == 4).unwrap_or(m)
                            }
                            m => m.unwrap_or("NULL"),
                        }
                    ),
                )
            })?;
        if status == Status::BufError {
            // The reference accepts a truncated stream once input is exhausted.
            if codec.total_in() == data.len() as u64 {
                break;
            }
            let size = buffer
                .len()
                .checked_mul(2)
                .ok_or_else(|| ContextError::new(6, 0, "Memory allocation error"))?;
            let limit = budget
                .limits
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .max_memory_block_size;
            if limit != 0 && size as u64 > limit {
                return Err(ContextError::new(
                    6,
                    1000,
                    "Memory allocation error: Security limit exceeded: zlib inflate scratch buffer exceeds the maximum block size",
                ));
            }
            buffer.resize(size, 0);
            continue;
        }
        let written = (codec.total_out() - before) as usize;
        allocations.push(
            budget
                .reserve(written as u64, "zlib/deflate decompression output")
                .map_err(|e| ContextError::new(e.code, e.subcode, e.message.to_string_lossy()))?,
        );
        output
            .try_reserve_exact(written)
            .map_err(|_| ContextError::new(6, 0, "Memory allocation error"))?;
        output.extend_from_slice(&buffer[..written]);
        if status == Status::StreamEnd {
            break;
        }
    }
    Ok(output)
}

/// `BrotliDecoderErrorString` of the reference's brotli (1.1.0).
fn brotli_error_string(code: i32) -> &'static str {
    const FORMAT: [&str; 16] = [
        "_ERROR_FORMAT_EXUBERANT_NIBBLE",
        "_ERROR_FORMAT_RESERVED",
        "_ERROR_FORMAT_EXUBERANT_META_NIBBLE",
        "_ERROR_FORMAT_SIMPLE_HUFFMAN_ALPHABET",
        "_ERROR_FORMAT_SIMPLE_HUFFMAN_SAME",
        "_ERROR_FORMAT_CL_SPACE",
        "_ERROR_FORMAT_HUFFMAN_SPACE",
        "_ERROR_FORMAT_CONTEXT_MAP_REPEAT",
        "_ERROR_FORMAT_BLOCK_LENGTH_1",
        "_ERROR_FORMAT_BLOCK_LENGTH_2",
        "_ERROR_FORMAT_TRANSFORM",
        "_ERROR_FORMAT_DICTIONARY",
        "_ERROR_FORMAT_WINDOW_BITS",
        "_ERROR_FORMAT_PADDING_1",
        "_ERROR_FORMAT_PADDING_2",
        "_ERROR_FORMAT_DISTANCE",
    ];
    match code {
        0 => "_NO_ERROR",
        1 => "_SUCCESS",
        2 => "_NEEDS_MORE_INPUT",
        3 => "_NEEDS_MORE_OUTPUT",
        -16..=-1 => FORMAT[(-code - 1) as usize],
        -18 => "_ERROR_COMPOUND_DICTIONARY",
        -19 => "_ERROR_DICTIONARY_NOT_SET",
        -20 => "_ERROR_INVALID_ARGUMENTS",
        -21 => "_ERROR_ALLOC_CONTEXT_MODES",
        -22 => "_ERROR_ALLOC_TREE_GROUPS",
        -25 => "_ERROR_ALLOC_CONTEXT_MAP",
        -26 => "_ERROR_ALLOC_RING_BUFFER_1",
        -27 => "_ERROR_ALLOC_RING_BUFFER_2",
        -30 => "_ERROR_ALLOC_BLOCK_TYPE_TREES",
        -31 => "_ERROR_UNREACHABLE",
        _ => "INVALID",
    }
}

/// libheif's `decompress_brotli`: stream through a 256 KiB output buffer,
/// charging each flushed chunk to the security limits.
fn decompress_brotli(data: &[u8], budget: &Arc<Budget>) -> Result<Vec<u8>> {
    use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};
    let mut state = BrotliState::new_strict(
        StandardAlloc::default(),
        StandardAlloc::default(),
        StandardAlloc::default(),
    );
    let mut buffer = vec![0u8; 1 << 18];
    let (mut available_in, mut input_offset, mut total) = (data.len(), 0, 0);
    let mut output = Vec::new();
    let mut allocations = Vec::new();
    loop {
        let (mut available_out, mut output_offset) = (buffer.len(), 0);
        let result = BrotliDecompressStream(
            &mut available_in,
            &mut input_offset,
            data,
            &mut available_out,
            &mut output_offset,
            &mut buffer,
            &mut total,
            &mut state,
        );
        let fail = |message: String| {
            ContextError::invalid(
                150,
                &format!("Invalid data in generic compression inflation: {message}"),
            )
        };
        match result {
            BrotliResult::NeedsMoreOutput | BrotliResult::ResultSuccess => {
                allocations.push(
                    budget
                        .reserve(output_offset as u64, "brotli decompression output")
                        .map_err(|e| {
                            ContextError::new(e.code, e.subcode, e.message.to_string_lossy())
                        })?,
                );
                output
                    .try_reserve_exact(output_offset)
                    .map_err(|_| ContextError::new(6, 0, "Memory allocation error"))?;
                output.extend_from_slice(&buffer[..output_offset]);
                if matches!(result, BrotliResult::ResultSuccess) {
                    return Ok(output);
                }
            }
            BrotliResult::NeedsMoreInput => {
                return Err(fail(
                    "Error performing brotli inflate - insufficient data.\n".into(),
                ));
            }
            BrotliResult::ResultFailure => {
                return Err(fail(format!(
                    "Error performing brotli inflate - {}\n",
                    brotli_error_string(state.error_code as i32)
                )));
            }
        }
    }
}

/// libheif's `compress_brotli`: the encoder's defaults (quality 11, window 22)
/// over the whole input with BROTLI_OPERATION_FINISH.
fn compress_brotli(data: &[u8]) -> Vec<u8> {
    use brotli::enc::encode::{BrotliEncoderOperation, BrotliEncoderStateStruct};
    let mut state = BrotliEncoderStateStruct::new(brotli::enc::StandardAlloc::default());
    let mut output = Vec::new();
    let mut buffer = vec![0u8; 1 << 18];
    let (mut available_in, mut input_offset) = (data.len(), 0);
    loop {
        let (mut available_out, mut output_offset) = (buffer.len(), 0);
        if !state.compress_stream(
            BrotliEncoderOperation::BROTLI_OPERATION_FINISH,
            &mut available_in,
            data,
            &mut input_offset,
            &mut available_out,
            &mut buffer,
            &mut output_offset,
            &mut None,
            &mut |_, _, _, _| (),
        ) {
            // libheif returns an empty vector when the encoder fails.
            return Vec::new();
        }
        output.extend_from_slice(&buffer[..output_offset]);
        if state.is_finished() {
            return output;
        }
    }
}
