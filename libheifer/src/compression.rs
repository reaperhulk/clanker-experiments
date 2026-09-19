// SPDX-License-Identifier: LGPL-3.0-or-later
//! Metadata compression using a Rust-only zlib implementation and allocator.
use crate::{context::ContextError, security::Budget};
use std::sync::Arc;
use zlib_rs::{Inflate, InflateFlush, Status};
type Result<T> = std::result::Result<T, ContextError>;

pub fn compress(data: &[u8], method: i32) -> Result<Vec<u8>> {
    if method == 5 {
        return Err(ContextError::new(
            4,
            3005,
            "Unsupported feature: Unsupported header compression method",
        ));
    }
    if !matches!(method, 3 | 4) {
        return Ok(data.to_vec());
    }
    Ok(crate::deflate_compat::compress(data, method == 4))
}

pub fn decompress(data: Vec<u8>, method: i32, budget: &Arc<Budget>) -> Result<Vec<u8>> {
    if method == 0 {
        return Ok(data);
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
                        codec.error_message().unwrap_or("NULL")
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
