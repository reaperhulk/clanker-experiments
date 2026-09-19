// SPDX-License-Identifier: LGPL-3.0-or-later
// Box reader semantics adapted from libheif, Copyright Dirk Farin and contributors.
//! Structural parsing used by the brand probe. A non-ftyp first box still needs
//! parsing: the C API distinguishes end of data from all other parse errors.
//! Keep the stream boundary separate from the parent box boundary, and preserve
//! optional child errors and validation order. This is not an item interpreter.
use crate::context::ContextError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Failure {
    End,
    Other,
}
type Result<T = ()> = std::result::Result<T, Failure>;
impl From<ContextError> for Failure {
    fn from(error: ContextError) -> Self {
        if error.subcode == 100 {
            Self::End
        } else {
            Self::Other
        }
    }
}

struct Reader<'a> {
    input: &'a [u8],
    at: usize,
    end: usize,
    failed: bool,
}
impl Reader<'_> {
    fn number(&mut self, n: usize) -> u64 {
        if self.end - self.at < n {
            self.at = self.end;
            self.failed = true;
            return 0;
        }
        let value = self.input[self.at..self.at + n]
            .iter()
            .fold(0, |v, b| v << 8 | u64::from(*b));
        self.at += n;
        value
    }
    fn finish(&self) -> Result {
        if self.failed {
            Err(Failure::End)
        } else {
            Ok(())
        }
    }
    fn eof(&self) -> bool {
        self.at == self.end
    }
    fn version(&mut self, maximum: u8) -> Result<(u8, u32)> {
        let full = self.number(4) as u32;
        let version = (full >> 24) as u8;
        if version > maximum {
            return Err(Failure::Other);
        }
        Ok((version, full & 0x00ff_ffff))
    }
    fn string(&mut self) {
        let data = &self.input[self.at..self.end];
        self.at += data
            .iter()
            .position(|b| *b == 0)
            .map_or(data.len(), |n| n + 1);
    }
    fn header(&mut self) -> Result<Header> {
        let start = self.at;
        if self.input.len() - self.at < 8 {
            return Err(Failure::End);
        }
        let mut size = self.number(4);
        let kind = (self.number(4) as u32).to_be_bytes();
        let mut length = 8;
        if size == 1 {
            if self.input.len() - self.at < 8 {
                return Err(Failure::End);
            }
            size = self.number(4) << 32 | self.number(4);
            length += 8;
            if size > 0x0fff_ffff_ffff_ffff {
                return Err(Failure::Other);
            }
        }
        let mut uuid = None;
        if kind == *b"uuid" {
            if self.input.len() - self.at < 16 {
                return Err(Failure::End);
            }
            if self.end - self.at < 16 {
                self.at = self.end;
                self.failed = true;
            } else {
                uuid = Some(self.input[self.at..self.at + 16].try_into().unwrap());
                self.at += 16;
            }
            length += 16;
        }
        self.finish()?;
        Ok(Header {
            start,
            size,
            kind: crate::camera::kind(kind, uuid),
            length,
        })
    }
}
struct Header {
    start: usize,
    size: u64,
    kind: [u8; 4],
    length: u64,
}

pub(crate) fn first_box_truncated(input: &[u8]) -> bool {
    let mut r = Reader {
        input,
        at: 0,
        end: input.len(),
        failed: false,
    };
    matches!(read_box(&mut r, 0), Err((Failure::End, _)))
}

// The bool is true only for errors in the body of optional/ignorable boxes.
fn read_box(r: &mut Reader<'_>, level: usize) -> std::result::Result<(), (Failure, bool)> {
    let h = r.header().map_err(|e| (e, false))?;
    if level > 20 || (h.size != 0 && h.size < h.length) {
        return Err((Failure::Other, false));
    }
    let end = if h.size == 0 {
        r.end
    } else {
        let end = h.start as u64 + h.size;
        if end > r.input.len() as u64 {
            return Err((Failure::End, false));
        }
        if end > r.end as u64 {
            return Err((Failure::Other, false));
        }
        end as usize
    };
    let mut body = Reader {
        input: r.input,
        at: r.at,
        end,
        failed: false,
    };
    let result = parse(&mut body, h.kind, level + 1);
    r.at = end;
    let optional = matches!(
        &h.kind,
        b"irot" | b"imir" | b"clap" | b"pasp" | b"udes" | b"cmin" | b"cmex"
    );
    result.map_err(|e| (e, optional))
}

fn children(r: &mut Reader<'_>, level: usize, expected: Option<usize>) -> Result {
    if let Some(n) = expected {
        if n > 1000 {
            return Err(Failure::Other);
        }
        if n > (r.end - r.at) / 8 {
            return Err(Failure::End);
        }
    }
    let mut count = 0;
    while !r.eof() && !r.failed {
        if let Err((error, false)) = read_box(r, level) {
            return Err(error);
        }
        if expected.is_none() && count > 100 {
            return Err(Failure::Other);
        }
        count += 1;
        if expected == Some(count) {
            break;
        }
    }
    if expected.is_some_and(|n| count != n) {
        return Err(Failure::End);
    }
    r.finish()
}

fn parse(r: &mut Reader<'_>, kind: [u8; 4], level: usize) -> Result {
    let data = &r.input[r.at..r.end];
    if let Some((error, _)) = crate::properties::parse_error(kind, data) {
        return Err(error.into());
    }
    match &kind {
        b"meta" => {
            r.version(0)?;
            return children(r, level, None);
        }
        b"iprp" | b"ipco" => return children(r, level, None),
        b"ftyp" => {
            // A zero-size ftyp is rejected even when the enclosing range has data.
            r.number(4);
            r.number(4);
            if data.len() < 8 || (data.len() - 8) / 4 > 1000 {
                return Err(Failure::Other);
            }
        }
        b"hdlr" => {
            r.version(0)?;
            for _ in 0..5 {
                r.number(4);
            }
        }
        b"pitm" => {
            let (v, _) = r.version(1)?;
            r.number(if v == 0 { 2 } else { 4 });
        }
        b"iinf" => {
            let (v, _) = r.version(255)?;
            let n = r.number(if v == 0 { 2 } else { 4 }) as usize;
            // Upstream returns success early for zero, including a failed count read.
            return if n == 0 {
                Ok(())
            } else {
                children(r, level, Some(n))
            };
        }
        b"infe" => {
            let (v, _) = r.version(3)?;
            r.number(if v == 3 { 4 } else { 2 });
            r.number(2);
            let kind = if v >= 2 {
                (r.number(4) as u32).to_be_bytes()
            } else {
                *b"mime"
            };
            r.string();
            if kind == *b"mime" {
                r.string();
                r.string();
            }
            if kind == *b"uri " {
                r.string();
            }
        }
        b"ipma" => {
            let (v, flags) = r.version(1)?;
            let n = r.number(4);
            if n > 1000 {
                return Err(Failure::Other);
            }
            for _ in 0..n {
                if r.eof() || r.failed {
                    break;
                }
                r.number(if v == 0 { 2 } else { 4 });
                let count = r.number(1);
                for _ in 0..count {
                    r.number(if flags & 1 != 0 { 2 } else { 1 });
                }
            }
        }
        b"iloc" => return locations(r),
        b"iref" => return references(r),
        b"ispe" => {
            r.version(0)?;
            r.number(4);
            r.number(4);
        }
        b"auxC" => {
            r.version(0)?;
            r.string();
        }
        b"pasp" => {
            r.number(4);
            r.number(4);
        }
        b"hvcC" | b"av1C" => {
            return crate::context::validate_property(kind, data).map_err(Into::into);
        }
        b"mskC" => {
            r.version(255)?;
            r.number(1);
        }
        b"taic" => {
            r.version(255)?;
            r.number(8);
            r.number(4);
            r.number(4);
            r.number(1);
        }
        b"itai" => {
            r.version(255)?;
            r.number(8);
            r.number(1);
        }
        _ => {}
    }
    r.finish()
}

fn locations(r: &mut Reader<'_>) -> Result {
    let (v, _) = r.version(2)?;
    let sizes = r.number(2);
    let offsets = (sizes >> 12) & 15;
    let lengths = (sizes >> 8) & 15;
    let base = (sizes >> 4) & 15;
    let index = if v == 0 { 0 } else { sizes & 15 };
    let count = r.number(if v < 2 { 2 } else { 4 });
    if count > 1000 {
        return Err(Failure::Other);
    }
    for _ in 0..count {
        if r.eof() {
            return Err(Failure::End);
        }
        r.number(if v < 2 { 2 } else { 4 });
        if v >= 1 {
            r.number(2);
        }
        r.number(2);
        if matches!(base, 4 | 8) {
            r.number(base as usize);
        }
        let extents = r.number(2);
        if extents > 32 {
            return Err(Failure::Other);
        }
        for _ in 0..extents {
            if r.eof() {
                return Err(Failure::End);
            }
            for n in [index, offsets, lengths] {
                if matches!(n, 4 | 8) {
                    r.number(n as usize);
                }
            }
        }
    }
    r.finish()
}

fn references(r: &mut Reader<'_>) -> Result {
    let (v, _) = r.version(1)?;
    let width = if v == 0 { 2 } else { 4 };
    let mut duplicate = false;
    for index in 0.. {
        if r.eof() {
            break;
        }
        if index >= 1000 {
            return Err(Failure::Other);
        }
        // These entries parse a header but do not constrain the following IDs
        // to its declared size. That behavior differs from normal child boxes.
        r.header()?;
        r.number(width);
        let count = r.number(2);
        if count == 0 || count > 1000 {
            return Err(Failure::Other);
        }
        let mut targets = std::collections::BTreeSet::new();
        for _ in 0..count {
            if r.eof() {
                return Err(Failure::End);
            }
            duplicate |= !targets.insert(r.number(width));
        }
    }
    if duplicate {
        return Err(Failure::Other);
    }
    r.finish()
}
