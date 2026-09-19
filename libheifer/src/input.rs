// SPDX-License-Identifier: LGPL-3.0-or-later
//! Retained random-access input. Snapshot container headers and metadata;
//! media bytes are fetched through the retained source when requested.
use crate::context::{ContextError, Input, header};
use crate::error::Error;
use std::{
    borrow::Cow,
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::Mutex,
};
type Result<T> = std::result::Result<T, ContextError>;
fn eof() -> ContextError {
    ContextError::invalid(100, "Unexpected end of file")
}
fn take<R: Read + Seek>(source: &mut R, offset: u64, size: u64) -> Result<Vec<u8>> {
    let size = usize::try_from(size).map_err(|_| ContextError::from(Error::ALLOCATION))?;
    let mut data = Vec::new();
    data.try_reserve_exact(size)
        .map_err(|_| ContextError::from(Error::ALLOCATION))?;
    data.resize(size, 0);
    source
        .seek(SeekFrom::Start(offset))
        .and_then(|_| source.read_exact(&mut data))
        .map_err(|_| eof())?;
    Ok(data)
}
pub struct Snapshot {
    pub bytes: Vec<u8>,
    regions: Vec<(usize, u64, usize)>,
    pub length: u64,
}
/// A potentially growing source. Requests return the available end position.
pub trait RangeSource: Read + Seek {
    fn request_range(&mut self, start: u64, end: u64) -> u64;
}
impl Snapshot {
    pub fn read_ranges<R: RangeSource>(source: &mut R) -> Result<Self> {
        let mut out = Self {
            bytes: Vec::new(),
            regions: Vec::new(),
            length: u64::MAX,
        };
        let mut available = source.request_range(0, 1024);
        if available < 32 {
            return Ok(out);
        }
        let prefix = take(source, 0, 32)?;
        let Ok(first) = header(&prefix) else {
            out.append(0, &prefix)?;
            return Ok(out);
        };
        if first.kind != *b"ftyp"
            || first.size == 0
            || first.size > available
            || first.size < first.header as u64
        {
            out.append(0, &prefix)?;
            return Ok(out);
        }
        out.append(0, &take(source, 0, first.size)?)?;
        let mut offset = first.size;
        let mut found = false;
        loop {
            let end = offset.checked_add(32).ok_or_else(eof)?;
            if end > available {
                available = source.request_range(offset, end);
            }
            if end > available {
                if found {
                    break;
                }
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: Insufficient input data",
                ));
            }
            let mut prefix = take(source, offset, 32)?;
            let Ok(h) = header(&prefix) else {
                out.append(offset, &prefix)?;
                break;
            };
            if h.size != 0 && h.size < h.header as u64 {
                out.append(offset, &prefix)?;
                break;
            }
            if matches!(&h.kind, b"meta" | b"moov" | b"mini") {
                let (code, name, category) = match &h.kind {
                    b"meta" => (104, "meta", "No 'meta' box"),
                    b"moov" => (151, "moov", "No 'moov' box"),
                    _ => (149, "mini", "Unsupported or invalid 'mini' box"),
                };
                let end = if h.size == 0 {
                    available = source.request_range(offset, u64::MAX);
                    if available <= offset {
                        return Err(ContextError::invalid(
                            code,
                            &format!("{category}: Cannot read {name} box with unspecified size"),
                        ));
                    }
                    available
                } else {
                    offset.checked_add(h.size).ok_or_else(|| {
                        ContextError::invalid(
                            code,
                            &format!("{category}: Cannot read {name} box with invalid size"),
                        )
                    })?
                };
                if end > available {
                    available = source.request_range(offset, end);
                }
                if end > available {
                    return Err(ContextError::invalid(
                        code,
                        &format!("{category}: Cannot read full {name} box"),
                    ));
                }
                out.append(offset, &take(source, offset, end - offset)?)?;
                found = true;
            } else {
                if h.size != 0 {
                    prefix[..4].copy_from_slice(&32u32.to_be_bytes());
                }
                out.append(offset, &prefix)?;
            }
            if h.size == 0 {
                break;
            }
            offset = offset.checked_add(h.size).ok_or_else(|| {
                ContextError::invalid(0, "Unspecified: Box size too large, integer overflow")
            })?;
        }
        Ok(out)
    }
    fn append(&mut self, offset: u64, bytes: &[u8]) -> Result<()> {
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| ContextError::from(Error::ALLOCATION))?;
        self.regions.push((self.bytes.len(), offset, bytes.len()));
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn read<R: Read + Seek>(source: &mut R, length: u64) -> Result<Self> {
        let mut out = Self {
            bytes: Vec::new(),
            regions: Vec::new(),
            length,
        };
        let prefix = take(source, 0, length.min(32))?;
        let Ok(first) = header(&prefix) else {
            out.append(0, &prefix)?;
            return Ok(out);
        };
        if length < 32
            || first.kind != *b"ftyp"
            || first.size == 0
            || first.size > 1024.min(length)
            || first.size < first.header as u64
        {
            out.append(0, &prefix)?;
            return Ok(out);
        }
        let bytes = take(source, 0, first.size)?;
        out.append(0, &bytes)?;
        let mut offset = first.size;
        while offset < length {
            let remaining = length - offset;
            let prefix = take(source, offset, remaining.min(32))?;
            if remaining < 32 {
                out.append(offset, &prefix)?;
                break;
            }
            let Ok(h) = header(&prefix) else {
                out.append(offset, &prefix)?;
                break;
            };
            if h.size != 0 && h.size < h.header as u64 {
                out.append(offset, &prefix)?;
                break;
            }
            let size = if h.size == 0 { remaining } else { h.size };
            if matches!(&h.kind, b"meta" | b"moov" | b"mini") {
                if size > remaining {
                    out.append(offset, &prefix)?;
                    break;
                }
                let bytes = take(source, offset, size)?;
                out.append(offset, &bytes)?;
            } else {
                // Non-metadata boxes are skipped by the native layout reader.
                // Media extent offsets remain in the original source domain.
                let mut skipped = prefix;
                if h.size != 0 {
                    skipped[..4].copy_from_slice(&32u32.to_be_bytes());
                }
                out.append(offset, &skipped)?;
            }
            if h.size == 0 {
                break;
            }
            let Some(next) = offset.checked_add(h.size) else {
                break;
            };
            offset = next;
        }
        Ok(out)
    }
    pub fn original_offset(&self, data: &[u8]) -> u64 {
        let at = data.as_ptr() as usize - self.bytes.as_ptr() as usize;
        self.regions
            .iter()
            .rev()
            .find(|(start, _, len)| at >= *start && at < start + len)
            .map_or(at as u64, |(start, offset, _)| offset + (at - start) as u64)
    }
}
pub struct FileInput {
    snapshot: Snapshot,
    file: Mutex<File>,
}
impl FileInput {
    pub fn new(mut file: File) -> Result<Self> {
        let length = file.seek(SeekFrom::End(0)).map_err(|_| eof())?;
        let snapshot = Snapshot::read(&mut file, length)?;
        Ok(Self {
            snapshot,
            file: Mutex::new(file),
        })
    }
}
impl Input for FileInput {
    fn bytes(&self) -> &[u8] {
        &self.snapshot.bytes
    }
    fn length(&self) -> u64 {
        self.snapshot.length
    }
    fn original_offset(&self, data: &[u8]) -> u64 {
        self.snapshot.original_offset(data)
    }
    fn read_range(&self, offset: u64, size: u64) -> Result<Cow<'_, [u8]>> {
        if offset.checked_add(size).is_none_or(|n| n > self.length()) {
            return Err(eof());
        }
        Ok(Cow::Owned(take(
            &mut *self.file.lock().unwrap(),
            offset,
            size,
        )?))
    }
}
