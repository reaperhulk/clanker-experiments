// SPDX-License-Identifier: LGPL-3.0-or-later
//! Native JPEG marker ordering before entropy decoding.
use super::jpeg::codec_error;
use crate::context::ContextError;

struct Input<'a> {
    data: &'a [u8],
    at: usize,
}
impl Input<'_> {
    fn byte(&mut self) -> u8 {
        let value = self.data.get(self.at).copied().unwrap_or_else(|| {
            if (self.at - self.data.len()).is_multiple_of(2) {
                255
            } else {
                217
            }
        });
        self.at += 1;
        value
    }
    fn word(&mut self) -> usize {
        (usize::from(self.byte()) << 8) | usize::from(self.byte())
    }
    fn skip(&mut self, n: usize) {
        self.at += n;
    }
    fn marker(&mut self) -> u8 {
        loop {
            if self.byte() != 255 {
                continue;
            }
            let mut marker = self.byte();
            while marker == 255 {
                marker = self.byte();
            }
            if marker != 0 {
                return marker;
            }
        }
    }
}
#[derive(Default)]
pub(crate) struct Header {
    pub precision: u8,
    pub components: Vec<(u8, u8, u8)>,
    progressive: bool,
    lossless: bool,
    scan: Vec<(u8, u8)>,
    ss: u8,
    se: u8,
    ah: u8,
    al: u8,
    jfif: bool,
    adobe: Option<u8>,
}
impl Header {
    pub fn validate_decode(&self) -> Result<(), ContextError> {
        if self.components.len() != 1 && (self.components.len() != 3 || self.is_rgb()) {
            return Err(codec_error("Unsupported color conversion request"));
        }
        if self.progressive
            && (self.ss > self.se
                || self.se > 63
                || (self.ss == 0 && self.se != 0)
                || (self.ss != 0 && self.scan.len() != 1)
                || (self.ah != 0 && self.al + 1 != self.ah)
                || self.al > 13)
        {
            return Err(codec_error(format!(
                "Invalid progressive/lossless parameters Ss={} Se={} Ah={} Al={}",
                self.ss, self.se, self.ah, self.al
            )));
        }
        for &(_, tables) in &self.scan {
            if (!self.progressive || self.ss == 0) && tables >> 4 > 3 {
                return Err(codec_error(format!(
                    "Huffman table 0x{:02x} was not defined",
                    tables >> 4
                )));
            }
            if (!self.progressive || self.se > 0) && tables & 15 > 3 {
                return Err(codec_error(format!(
                    "Huffman table 0x{:02x} was not defined",
                    16 + (tables & 15)
                )));
            }
        }
        Ok(())
    }
    fn is_rgb(&self) -> bool {
        if self.jfif {
            return false;
        }
        if let Some(transform) = self.adobe {
            return transform == 0;
        }
        self.components
            .iter()
            .map(|c| c.0)
            .eq(b"RGB".iter().copied())
    }
}

pub(crate) fn read(data: &[u8]) -> Result<Header, ContextError> {
    let mut input = Input { data, at: 0 };
    let (a, b) = (input.byte(), input.byte());
    if (a, b) != (255, 216) {
        return Err(codec_error(format!(
            "Not a JPEG file: starts with 0x{a:02x} 0x{b:02x}"
        )));
    }
    let mut header = Header::default();
    let mut frame = false;
    loop {
        let marker = input.marker();
        if marker == 217 {
            return Err(codec_error(if frame {
                "Invalid JPEG file structure: missing SOS marker"
            } else {
                "JPEG datastream contains no image"
            }));
        }
        if marker == 216 {
            return Err(codec_error("Invalid JPEG file structure: two SOI markers"));
        }
        if (208..=215).contains(&marker) || marker == 1 {
            continue;
        }
        let length = input.word();
        let start = input.at;
        if matches!(marker,192..=195 | 197..=199 | 201..=203 | 205..=207) {
            if frame {
                return Err(codec_error("Invalid JPEG file structure: two SOF markers"));
            }
            header.precision = input.byte();
            let h = input.word();
            let w = input.word();
            let n = usize::from(input.byte());
            if h == 0 || w == 0 || n == 0 {
                return Err(codec_error("Empty JPEG image (DNL not supported)"));
            }
            if length != 8 + 3 * n {
                return Err(codec_error("Bogus marker length"));
            }
            for _ in 0..n {
                header
                    .components
                    .push((input.byte(), input.byte(), input.byte()));
            }
            header.progressive = matches!(marker, 194 | 198 | 202 | 206);
            header.lossless = matches!(marker, 195 | 199 | 203 | 207);
            frame = true;
        } else if marker == 218 {
            if !frame {
                return Err(codec_error("Invalid JPEG file structure: SOS before SOF"));
            }
            let n = usize::from(input.byte());
            if !(1..=4).contains(&n) || length != n * 2 + 6 {
                return Err(codec_error("Bogus marker length"));
            }
            for _ in 0..n {
                let component = input.byte();
                let tables = input.byte();
                if !header.components.iter().any(|c| c.0 == component)
                    || header.scan.iter().any(|c| c.0 == component)
                {
                    return Err(codec_error(format!(
                        "Invalid component ID {component} in SOS"
                    )));
                }
                header.scan.push((component, tables));
            }
            header.ss = input.byte();
            header.se = input.byte();
            let approx = input.byte();
            header.ah = approx >> 4;
            header.al = approx & 15;
            if (!header.lossless && header.precision != 8)
                || (header.lossless && !(2..=16).contains(&header.precision))
            {
                return Err(codec_error(format!(
                    "Unsupported JPEG data precision {}",
                    header.precision
                )));
            }
            if header
                .components
                .iter()
                .any(|&(_, s, _)| s >> 4 == 0 || s >> 4 > 4 || s & 15 == 0 || s & 15 > 4)
            {
                return Err(codec_error("Bogus sampling factors"));
            }
            return Ok(header);
        } else if marker == 219 {
            let mut remaining = length as i64 - 2;
            while remaining > 0 {
                let table = input.byte();
                if table & 15 > 3 {
                    return Err(codec_error(format!("Bogus DQT index {}", table & 15)));
                }
                let count = if table >> 4 != 0 { 128 } else { 64 };
                input.skip(count);
                remaining -= (count + 1) as i64;
            }
            if remaining != 0 {
                return Err(codec_error("Bogus marker length"));
            }
        } else if marker == 196 {
            let mut remaining = length as i64 - 2;
            while remaining > 16 {
                let index = input.byte();
                let count: usize = (0..16).map(|_| usize::from(input.byte())).sum();
                remaining -= 17;
                if count > 256 || count as i64 > remaining {
                    return Err(codec_error("Bogus Huffman table definition"));
                }
                input.skip(count);
                remaining -= count as i64;
                if index & !16 > 3 {
                    return Err(codec_error(format!("Bogus DHT index {}", index & !16)));
                }
            }
            if remaining != 0 {
                return Err(codec_error("Bogus marker length"));
            }
        } else {
            if length < 2 {
                return Err(codec_error("Bogus marker length"));
            }
            let end = start.saturating_add(length - 2).min(data.len());
            let segment = data.get(start..end).unwrap_or_default();
            if marker == 224 && segment.starts_with(b"JFIF\0") {
                header.jfif = true;
            }
            if marker == 238 && segment.starts_with(b"Adobe") && segment.len() >= 12 {
                header.adobe = Some(segment[11]);
            }
            input.skip(length - 2);
        }
    }
}
