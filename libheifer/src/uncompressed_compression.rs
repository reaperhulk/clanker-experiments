// SPDX-License-Identifier: LGPL-3.0-or-later
//! ISO 23001-17 generic compression and compressed-unit addressing.
use super::{Result, unspecified};
use crate::{
    container::Container,
    context::ContextError,
    security::{Budget, Limits},
};
use std::sync::Arc;
struct Reader<'a> {
    data: &'a [u8],
    failed: bool,
}
impl Reader<'_> {
    fn number(&mut self, n: usize) -> u64 {
        if self.data.len() < n {
            self.failed = true;
            self.data = &[];
            return 0;
        }
        let v = self.data[..n]
            .iter()
            .fold(0, |v, b| (v << 8) | u64::from(*b));
        self.data = &self.data[n..];
        v
    }
    fn version(&mut self, kind: &str) -> Result<()> {
        let v = self.number(4) >> 24;
        if v != 0 {
            return Err(ContextError::new(
                4,
                3002,
                format!(
                    "Unsupported feature: Unsupported data version: {kind} box data version {v} is not implemented yet"
                ),
            ));
        }
        Ok(())
    }
    fn finish(&self) -> Result<()> {
        if self.failed {
            Err(ContextError::invalid(100, "Unexpected end of file"))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Compression {
    kind: [u8; 4],
    unit: u8,
}
impl Compression {
    pub(crate) fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader {
            data,
            failed: false,
        };
        r.version("cmpC")?;
        let kind = (r.number(4) as u32).to_be_bytes();
        let unit = r.number(1) as u8;
        if unit > 4 {
            return Err(ContextError::new(
                2,
                2005,
                "Invalid input: Unsupported parameter: Unsupported cmpC compressed unit type",
            ));
        }
        r.finish()?;
        Ok(Self { kind, unit })
    }
    fn inflate(&self, data: Vec<u8>, budget: &Arc<Budget>) -> Result<Vec<u8>> {
        let method = match &self.kind {
            b"zlib" => 4,
            b"defl" => 3,
            b"brot" => 5,
            _ => {
                return Err(ContextError::new(
                    4,
                    3006,
                    format!(
                        "Unsupported feature: Unsupported generic compression method: cannot decode unci item with unsupported compression type: {}\n",
                        u32::from_be_bytes(self.kind)
                    ),
                ));
            }
        };
        crate::compression::decompress(data, method, budget)
    }
}
pub(crate) fn units(data: &[u8], limits: Option<&Limits>) -> Result<Vec<(u64, u64)>> {
    let mut r = Reader {
        data,
        failed: false,
    };
    r.version("icef")?;
    let codes = r.number(1);
    let offset_code = (codes >> 5) as usize;
    let size_code = ((codes >> 2) & 7) as usize;
    let count = r.number(4);
    if offset_code > 4 {
        return Err(ContextError::new(
            5,
            2005,
            "Usage error: Unsupported parameter: Unsupported icef unit offset code",
        ));
    }
    if size_code > 4 {
        return Err(ContextError::new(
            5,
            2005,
            "Usage error: Unsupported parameter: Unsupported icef unit size code",
        ));
    }
    let off_width = [0, 2, 3, 4, 8][offset_code];
    let size_width = [1, 2, 3, 4, 8][size_code];
    if count * (off_width + size_width) as u64 > r.data.len() as u64 {
        return Err(ContextError::invalid(
            100,
            &format!(
                "Unexpected end of file: icef box declares {count} units, but only {} were contained in the file",
                r.data.len() / ((off_width + size_width) * 64)
            ),
        ));
    }
    if let Some(limits) = limits {
        let budget = Arc::new(Budget::new(Arc::new(std::sync::RwLock::new(*limits))));
        let _reservation = budget.reserve(count * 16, "icef box compressed unit infos")?;
    }
    let mut out = Vec::new();
    out.try_reserve_exact(count as usize)
        .map_err(|_| crate::error::Error::ALLOCATION)?;
    let mut implied = 0;
    for _ in 0..count {
        let offset = if offset_code == 0 {
            implied
        } else {
            r.number(off_width)
        };
        let size = r.number(size_width);
        if size >= u64::MAX - offset {
            return Err(ContextError::invalid(
                2006,
                "Invalid parameter value: icef unit offset + size exceeds 64 bit range",
            ));
        }
        if offset_code == 0 {
            implied += size;
        }
        r.finish()?;
        out.push((offset, size));
    }
    r.finish()?;
    Ok(out)
}
pub(crate) struct Source<'a, 'b> {
    container: &'a Container<'b>,
    id: u32,
    compression: Option<Compression>,
    units: Option<Vec<(u64, u64)>>,
    budget: Arc<Budget>,
}
impl<'a, 'b> Source<'a, 'b> {
    pub(crate) fn new(
        container: &'a Container<'b>,
        id: u32,
        budget: Option<Arc<Budget>>,
    ) -> Result<Self> {
        Ok(Self {
            container,
            id,
            compression: container
                .property(id, *b"cmpC")
                .ok()
                .map(Compression::parse)
                .transpose()?,
            units: container
                .property(id, *b"icef")
                .ok()
                .map(|data| units(data, None))
                .transpose()?,
            budget: budget.unwrap_or_else(|| {
                Arc::new(Budget::new(Arc::new(std::sync::RwLock::new(
                    container.limits,
                ))))
            }),
        })
    }
    pub(crate) fn range(&self, start: u64, size: u64, tile: u64) -> Result<Vec<u8>> {
        let Some(c) = self.compression else {
            return self.container.payload_window(self.id, start, size);
        };
        if c.unit == 2
            && let Some(units) = &self.units
        {
            let (offset, len) = units
                .get(tile as usize)
                .ok_or_else(|| unspecified("no icef-box entry for tile index"))?;
            return c.inflate(
                self.container.payload_window(self.id, *offset, *len)?,
                &self.budget,
            );
        }
        let compressed = self.container.payload(self.id)?;
        let mut data = if let Some(units) = &self.units {
            let mut data = Vec::new();
            let mut reservations = Vec::new();
            for (offset, len) in units {
                if *offset > compressed.len() as u64 || *len > compressed.len() as u64 - offset {
                    return Err(unspecified("incomplete data in unci image"));
                }
                let part = c.inflate(
                    compressed[*offset as usize..(offset + len) as usize].to_vec(),
                    &self.budget,
                )?;
                reservations.push(
                    self.budget
                        .reserve(part.len() as u64, "unci icef decompressed units")?,
                );
                data.try_reserve(part.len())
                    .map_err(|_| crate::error::Error::ALLOCATION)?;
                data.extend_from_slice(&part);
            }
            data
        } else {
            c.inflate(compressed, &self.budget)?
        };
        if start > data.len() as u64 || size > data.len() as u64 - start {
            return Err(unspecified("Data range out of existing range"));
        }
        data.copy_within(start as usize..(start + size) as usize, 0);
        data.truncate(size as usize);
        Ok(data)
    }
}
