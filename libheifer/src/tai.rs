// SPDX-License-Identifier: LGPL-3.0-or-later
// TAI semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Versioned TAI clock descriptions and timestamp packets.
use crate::{context::ContextError, properties::Property};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockInfo {
    pub version: u8,
    pub time_uncertainty: u64,
    pub clock_resolution: u32,
    pub clock_drift_rate: i32,
    pub clock_type: u8,
}
impl Default for ClockInfo {
    fn default() -> Self {
        Self {
            version: 1,
            time_uncertainty: u64::MAX,
            clock_resolution: 0,
            clock_drift_rate: i32::MAX,
            clock_type: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub version: u8,
    pub tai_timestamp: u64,
    pub synchronization_state: u8,
    pub timestamp_generation_failure: u8,
    pub timestamp_is_modified: u8,
}
impl Default for Timestamp {
    fn default() -> Self {
        Self {
            version: 1,
            tai_timestamp: 0,
            synchronization_state: 0,
            timestamp_generation_failure: 0,
            timestamp_is_modified: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaiProperty {
    Clock(ClockInfo),
    Timestamp(Timestamp),
}
impl TaiProperty {
    pub fn parse(kind: [u8; 4], data: &[u8]) -> Result<Self, ContextError> {
        let needed = if kind == *b"taic" { 21 } else { 13 };
        if data.len() < needed {
            return Err(ContextError::invalid(100, "Unexpected end of file"));
        }
        let value = u64::from_be_bytes(data[4..12].try_into().unwrap());
        Ok(if kind == *b"taic" {
            Self::Clock(ClockInfo {
                version: 1,
                time_uncertainty: value,
                clock_resolution: u32::from_be_bytes(data[12..16].try_into().unwrap()),
                clock_drift_rate: i32::from_be_bytes(data[16..20].try_into().unwrap()),
                clock_type: data[20] >> 6,
            })
        } else {
            Self::Timestamp(Timestamp {
                version: 1,
                tai_timestamp: value,
                synchronization_state: u8::from(data[12] & 0x80 != 0),
                timestamp_generation_failure: u8::from(data[12] & 0x40 != 0),
                timestamp_is_modified: u8::from(data[12] & 0x20 != 0),
            })
        })
    }
    pub fn property(self) -> Property {
        let mut data = vec![0; 4];
        let kind = match self {
            Self::Clock(c) => {
                data.extend_from_slice(&c.time_uncertainty.to_be_bytes());
                data.extend_from_slice(&c.clock_resolution.to_be_bytes());
                data.extend_from_slice(&c.clock_drift_rate.to_be_bytes());
                data.push(c.clock_type.wrapping_shl(6));
                *b"taic"
            }
            Self::Timestamp(t) => {
                data.extend_from_slice(&t.tai_timestamp.to_be_bytes());
                data.push(
                    (u8::from(t.synchronization_state != 0) << 7)
                        | (u8::from(t.timestamp_generation_failure != 0) << 6)
                        | (u8::from(t.timestamp_is_modified != 0) << 5),
                );
                *b"itai"
            }
        };
        Property {
            kind,
            uuid: None,
            data,
            raw: false,
            tai: Some(self),
        }
    }
}
