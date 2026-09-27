// SPDX-License-Identifier: LGPL-3.0-or-later
//! JPEG2000 property validation, including native error ordering.
use crate::context::ContextError;

pub(crate) fn validate(
    kind: [u8; 4],
    data: &[u8],
    max_components: u32,
) -> Result<(), ContextError> {
    let mut at = 0;
    let mut truncated = false;
    let mut read = |n: usize| {
        if at + n > data.len() {
            at = data.len();
            truncated = true;
            0u16
        } else {
            let value = data[at..at + n]
                .iter()
                .fold(0u16, |v, &b| (v << 8) | u16::from(b));
            at += n;
            value
        }
    };
    match &kind {
        b"cdef" | b"j2kL" => {
            let count = read(2);
            if kind == *b"cdef" && max_components != 0 && u32::from(count) > max_components {
                return Err(ContextError::invalid(
                    1000,
                    &format!(
                        "Security limit exceeded: cdef box wants to define {count} JPEG-2000 channels, but the security limit is set to {max_components} components"
                    ),
                ));
            }
            let available = data.len().saturating_sub(2) / if kind == *b"cdef" { 6 } else { 5 };
            if usize::from(count) > available {
                let message = if kind == *b"cdef" {
                    format!(
                        "cdef box wants to define {count} JPEG-2000 channels, but file only contains {available} components"
                    )
                } else {
                    format!(
                        "j2kL box wants to define {count}JPEG-2000 layers, but the box only contains {available} layers entries"
                    )
                };
                return Err(ContextError::invalid(
                    100,
                    &format!("Unexpected end of file: {message}"),
                ));
            }
        }
        b"cmap" => truncated = !data.len().is_multiple_of(4),
        b"pclr" => {
            let entries = read(2);
            let columns = read(1);
            let mut bytes_per_entry = 0;
            for _ in 0..columns {
                let depth = read(1);
                let message = if depth & 128 != 0 {
                    Some("pclr with signed data is not supported")
                } else if depth > 16 {
                    Some("pclr more than 16 bits per channel is not supported")
                } else {
                    None
                };
                if let Some(message) = message {
                    return Err(ContextError::new(
                        4,
                        3002,
                        format!("Unsupported feature: Unsupported data version: {message}"),
                    ));
                }
                bytes_per_entry += if depth <= 8 { 1 } else { 2 };
            }
            if bytes_per_entry != 0 && usize::from(entries) > (data.len() - at) / bytes_per_entry {
                return Err(ContextError::invalid(
                    100,
                    "Unexpected end of file: pclr box declares more entries than the box contains",
                ));
            }
        }
        _ => {}
    }
    if truncated {
        Err(ContextError::invalid(100, "Unexpected end of file"))
    } else {
        Ok(())
    }
}
