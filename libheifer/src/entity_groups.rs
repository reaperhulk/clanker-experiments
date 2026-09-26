// SPDX-License-Identifier: LGPL-3.0-or-later
//! Ordered entity groups; public snapshots own their member arrays.
use crate::{context::ContextError, security::Limits};
#[derive(Clone)]
pub struct EntityGroup {
    pub id: u32,
    pub kind: u32,
    pub entities: Vec<u32>,
}
#[derive(Default)]
pub struct EntityGroups {
    pub children: usize,
    pub groups: Vec<EntityGroup>,
}
fn parse_group_inner(
    kind: [u8; 4],
    data: &[u8],
    limits: &Limits,
) -> Result<Option<EntityGroup>, ContextError> {
    if !matches!(&kind, b"altr" | b"ster" | b"pymd") {
        return Ok(None);
    }
    if data.len() < 4 {
        return Err(ContextError::invalid(100, "Unexpected end of file"));
    }
    // Scalar reads past the range return zero. The base reader's error is not
    // returned by this parser after a successfully read full-box header.
    let read = |n| {
        data.get(n..n + 4)
            .map_or(0, |v| u32::from_be_bytes(v.try_into().unwrap()))
    };
    let id = read(4);
    let count = read(8);
    let available = data.len().saturating_sub(12) / 4;
    if count as usize > available {
        return Err(ContextError::invalid(
            100,
            &format!(
                "Unexpected end of file: entity group box should contain {count} entities, but we can only read {available} entities."
            ),
        ));
    }
    if limits.max_size_entity_group != 0 && count > limits.max_size_entity_group {
        return Err(ContextError::invalid(
            1000,
            &format!(
                "Security limit exceeded: entity group box contains {count} entities, but the security limit is set to {} entities.",
                limits.max_size_entity_group
            ),
        ));
    }
    if kind == *b"ster" && count != 2 {
        return Err(ContextError::invalid(
            101,
            "Invalid box size: 'ster' entity group does not exists of exactly two images",
        ));
    }
    let entities = (0..count as usize).map(|i| read(12 + 4 * i)).collect();
    Ok(Some(EntityGroup {
        id,
        kind: u32::from_be_bytes(kind),
        entities,
    }))
}

/// A failed pyramid box is retained as an ignorable error child, not a group.
pub fn parse_group(
    kind: [u8; 4],
    data: &[u8],
    limits: &Limits,
) -> Result<Option<EntityGroup>, ContextError> {
    match parse_group_inner(kind, data, limits) {
        Err(_) if kind == *b"pymd" => Ok(None),
        result => result,
    }
}
