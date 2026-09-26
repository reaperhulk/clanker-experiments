// SPDX-License-Identifier: LGPL-3.0-or-later
//! Sequence tracks, sample tables and owned metadata packets.
use crate::{
    context::{Context, ContextError},
    sequence_sample::RawSample,
    tai::{ClockInfo, TaiProperty},
    writing::{boxed, full, number},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
pub type Result<T> = std::result::Result<T, ContextError>;
pub type SharedTrack = Arc<Mutex<Track>>;
#[derive(Clone, Debug)]
pub struct TrackOptions {
    pub timescale: u32,
    pub interleaved: bool,
    pub tai_presence: i32,
    pub clock: Option<Box<ClockInfo>>,
    pub content_presence: i32,
    pub content_id: Vec<u8>,
}
impl Default for TrackOptions {
    fn default() -> Self {
        Self {
            timescale: 90000,
            interleaved: false,
            tai_presence: 0,
            clock: None,
            content_presence: 0,
            content_id: Vec::new(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AuxType {
    pub kind: u32,
    pub parameter: u32,
}
#[derive(Default)]
pub struct Auxiliary {
    pub sizes: Vec<u8>,
    pub data: Vec<u8>,
    pub offsets: Vec<u64>,
}
impl Auxiliary {
    fn add(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > 255 {
            return Err(ContextError::new(
                9,
                0,
                "Error during encoding or writing output file: Unspecified: Encoded sample auxiliary information exceeds maximum size",
            ));
        }
        self.sizes.push(bytes.len() as u8);
        self.data.extend(bytes);
        Ok(())
    }
    fn finish(&mut self, data: &mut Vec<u8>, interleaved: bool) {
        if !self.data.is_empty() {
            self.offsets.push(data.len() as u64);
            data.extend(&self.data);
            if interleaved {
                self.data.clear();
            }
        }
    }
    fn boxes(&self, kind: [u8; 4], base: u64) -> Vec<u8> {
        let fixed = self
            .sizes
            .first()
            .copied()
            .filter(|x| *x != 0 && self.sizes.iter().all(|y| y == x))
            .unwrap_or(0);
        let mut b = kind.to_vec();
        b.extend([0; 4]);
        b.push(fixed);
        let count = if fixed == 0 {
            self.sizes
                .iter()
                .rposition(|s| *s != 0)
                .map_or(0, |i| i + 1)
        } else {
            self.sizes.len()
        };
        number(&mut b, count as u64, 4);
        if fixed == 0 {
            b.extend(&self.sizes[..count]);
        }
        let mut out = full(*b"saiz", 0, 1, &b);
        b = kind.to_vec();
        b.extend([0; 4]);
        number(&mut b, self.offsets.len() as u64, 4);
        for offset in &self.offsets {
            number(&mut b, offset + base, 4);
        }
        out.extend(full(*b"saio", 0, 1, &b));
        out
    }
}
pub struct Track {
    pub id: u32,
    pub active_encoder: usize,
    pub encoded_frames: u32,
    pub decode_failed: bool,
    pub handler: u32,
    pub reported_handler: u32,
    pub options: TrackOptions,
    pub dimensions: (u16, u16),
    pub header_dimensions: (u16, u16),
    pub uri: Option<Vec<u8>>,
    pub entry_kind: u32,
    pub sample_description: Vec<u8>,
    pub references: Vec<(u32, Vec<u32>)>,
    pub child_order: Vec<[u8; 4]>,
    pub sizes: Vec<u32>,
    pub durations: Vec<u32>,
    pub pending: Vec<u8>,
    pub offsets: Vec<u64>,
    pub aux_tai: Auxiliary,
    pub aux_content: Auxiliary,
    pub finalizations: usize,
    pub media_duration: u64,
    pub movie_duration: u64,
    pub edits: Vec<u64>,
    pub repetitions: u32,
    pub next: u32,
    pub output_count: u64,
    pub input: Option<Arc<dyn crate::context::Input>>,
    pub ranges: Vec<(u64, u32)>,
    pub aux_ranges: Vec<(AuxType, Vec<(u64, u8)>)>,
    pub reservations: Vec<crate::security::Reservation>,
    pub aux_types: Vec<AuxType>,
    pub first_clock: Option<Box<ClockInfo>>,
    pub auxiliary_urn: Vec<u8>,
    pub alpha: bool,
    /// Decoder of every sample: libheif gives a chunk a new decoder unless its
    /// sample description index repeats the previous chunk's.
    pub sample_chunks: Vec<u32>,
    /// libheif's stateful decoding loop (built-in AVC or registered plugins).
    pub stateful: Option<Box<StatefulTrack>>,
}
/// State of libheif's `Track_Visual::decode_next_image_sample` loop over its
/// decoder plugins: registered plugins, or the built-in AVC decoder emulating
/// the OpenH264 plugin.
#[derive(Default)]
pub struct StatefulTrack {
    decoders: std::sync::Mutex<Vec<Option<GroupDecoder>>>,
    next_decoded: u64,
    next_output: u64,
    flushed: bool,
}
/// The plugin selected for one decoder (a run of chunks sharing a sample
/// description).
enum GroupDecoder {
    #[cfg(feature = "avc")]
    Avc(Box<crate::avc::SequenceDecoder>),
    #[cfg(feature = "vvc")]
    Vvc(Box<crate::vvc::heif::SequenceDecoder>),
    Plugin(Box<dyn crate::decoding::SequenceStream>),
}
impl Track {
    fn new(
        id: u32,
        handler: u32,
        dimensions: (u16, u16),
        uri: Option<Vec<u8>>,
        options: TrackOptions,
    ) -> Self {
        let child_order = if options.content_id.is_empty() {
            Vec::new()
        } else {
            vec![*b"meta"]
        };
        Self {
            id,
            active_encoder: 0,
            encoded_frames: 0,
            decode_failed: false,
            handler,
            reported_handler: 0,
            options,
            dimensions: (0, 0),
            header_dimensions: dimensions,
            uri,
            entry_kind: 0,
            sample_description: Vec::new(),
            references: Vec::new(),
            child_order,
            sizes: Vec::new(),
            durations: Vec::new(),
            pending: Vec::new(),
            offsets: Vec::new(),
            aux_tai: Auxiliary::default(),
            aux_content: Auxiliary::default(),
            finalizations: 0,
            media_duration: 0,
            movie_duration: 0,
            edits: Vec::new(),
            repetitions: 1,
            next: 0,
            output_count: 0,
            input: None,
            ranges: Vec::new(),
            aux_ranges: Vec::new(),
            reservations: Vec::new(),
            aux_types: Vec::new(),
            first_clock: None,
            auxiliary_urn: Vec::new(),
            alpha: false,
            sample_chunks: Vec::new(),
            stateful: None,
        }
    }
    pub fn visual(&self) -> bool {
        self.handler != u32::from_be_bytes(*b"meta")
    }
    pub fn add_reference(&mut self, kind: u32, id: u32) {
        if self.references.is_empty() {
            self.child_order.push(*b"tref");
        }
        if let Some((_, ids)) = self.references.iter_mut().find(|x| x.0 == kind) {
            ids.push(id);
        } else {
            self.references.push((kind, vec![id]));
        }
    }
    pub fn references(&self, kind: u32) -> &[u32] {
        self.references
            .iter()
            .find(|x| x.0 == kind)
            .map_or(&[], |x| x.1.as_slice())
    }
    pub fn add_raw(&mut self, sample: &RawSample) -> Result<()> {
        self.add_sample(sample, true)
    }
    fn add_sample(&mut self, sample: &RawSample, content_present: bool) -> Result<()> {
        if self.entry_kind == 0 {
            self.entry_kind = u32::from_be_bytes(*b"urim");
            let mut b = vec![0; 8];
            let mut uri = self.uri.clone().unwrap_or_default();
            uri.push(0);
            b.extend(full(*b"uri ", 0, 0, &uri));
            if self.options.tai_presence != 0
                && let Some(clock) = &self.options.clock
            {
                b.extend(boxed(
                    *b"taic",
                    &TaiProperty::Clock(**clock).property().data,
                ));
            }
            self.sample_description = boxed(*b"urim", &b);
        }
        self.pending.extend(sample.data());
        self.sizes.push(sample.data().len() as u32);
        if sample.metadata.duration == 0 {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: Sample duration may not be 0",
            ));
        }
        self.durations.push(sample.metadata.duration);
        if self.options.tai_presence != 0 {
            if let Some(t) = &sample.timestamp {
                self.aux_tai
                    .add(&TaiProperty::Timestamp(**t).property().data[4..])?;
            } else if self.options.tai_presence == 1 {
                self.aux_tai.add(&[])?;
            } else {
                return Err(ContextError::new(
                    9,
                    0,
                    "Error during encoding or writing output file: Unspecified: Mandatory TAI timestamp missing",
                ));
            }
        }
        if self.options.content_presence != 0 {
            if !content_present {
                if self.options.content_presence == 1 {
                    self.aux_content.add(&[])?;
                } else {
                    return Err(ContextError::new(
                        9,
                        0,
                        "Error during encoding or writing output file: Unspecified: Mandatory ContentID missing",
                    ));
                }
            } else {
                let mut data = sample.metadata.content_id.clone();
                data.push(0);
                self.aux_content.add(&data)?;
            }
        }
        self.next = self.next.wrapping_add(1);
        Ok(())
    }
    pub fn next_raw(&mut self) -> Result<RawSample> {
        if u64::from(self.next) >= self.output_count || self.ranges.is_empty() {
            return Err(ContextError::new(
                13,
                0,
                "End of sequence: Unspecified: End of sequence",
            ));
        }
        let idx = self.next as usize % self.ranges.len();
        let mut s = RawSample::default();
        let (offset, size) = self.ranges[idx];
        s.set_data(&self.read_range(offset, u64::from(size))?)
            .map_err(|_| ContextError::new(6, 0, "Out of memory"))?;
        s.metadata.duration = self.durations.get(idx).copied().unwrap_or(0);
        for (kind, ranges) in &self.aux_ranges {
            if let Some(&(offset, size)) = ranges.get(idx) {
                if size == 0 {
                    continue;
                }
                let bytes = self.read_range(offset, u64::from(size))?;
                if kind.kind == u32::from_be_bytes(*b"suid") {
                    s.metadata.content_id = decode_string(&bytes)?;
                }
                if kind.kind == u32::from_be_bytes(*b"stai") {
                    if bytes.len() != 9 {
                        return Err(invalid("Wrong size of TAI timestamp data"));
                    }
                    let mut data = vec![0; 4];
                    data.extend_from_slice(&bytes);
                    if let TaiProperty::Timestamp(t) = TaiProperty::parse(*b"itai", &data)? {
                        s.timestamp = Some(Box::new(t));
                    }
                }
            }
        }
        self.next = self.next.wrapping_add(1);
        Ok(s)
    }
    fn read_range(&self, offset: u64, size: u64) -> Result<std::borrow::Cow<'_, [u8]>> {
        let length = self.input.as_ref().map_or(0, |x| x.length());
        let end = offset
            .checked_add(size)
            .ok_or_else(|| invalid("Chunk file offset overflows 64-bit range."))?;
        if end > length {
            return Err(ContextError::invalid(
                100,
                &format!(
                    "Unexpected end of file: File range {offset}..{end} is beyond end of file."
                ),
            ));
        }
        self.input
            .as_ref()
            .ok_or_else(|| invalid("Missing sequence input"))?
            .read_range(offset, size)
    }
    pub fn first_uri(&self) -> Result<&[u8]> {
        if self.entry_kind == 0 {
            return Err(invalid("This track has no sample entries."));
        }
        if self.entry_kind != u32::from_be_bytes(*b"urim") {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: This cluster is no 'urim' sample entry.",
            ));
        }
        self.uri
            .as_deref()
            .ok_or_else(|| invalid("The 'urim' box has no 'uri' child box."))
    }
}
pub struct Sequences {
    pub initialized: bool,
    pub before_meta: bool,
    pub timescale: u32,
    pub duration: u64,
    pub repetitions: u32,
    pub tracks: BTreeMap<u32, SharedTrack>,
    pub data: Vec<u8>,
    pub next_id: u32,
    pub visual_id: u32,
}
impl Default for Sequences {
    fn default() -> Self {
        Self {
            initialized: false,
            before_meta: false,
            timescale: 0,
            duration: 0,
            repetitions: 1,
            tracks: BTreeMap::new(),
            data: Vec::new(),
            next_id: 0,
            visual_id: 0,
        }
    }
}
impl Context {
    pub fn init_sequence(&mut self) {
        if !self.sequences.initialized {
            self.sequences.initialized = true;
            self.sequences.before_meta = self.items.layout.lock().unwrap().meta.is_empty();
        }
    }
    pub fn add_track(
        &mut self,
        handler: u32,
        dimensions: (u16, u16),
        uri: Option<Vec<u8>>,
        options: TrackOptions,
    ) -> Result<SharedTrack> {
        self.init_sequence();
        let id = self.items.layout.lock().unwrap().mint(1)?;
        let track = Arc::new(Mutex::new(Track::new(
            id, handler, dimensions, uri, options,
        )));
        self.sequences.next_id = id.wrapping_add(1);
        self.sequences.tracks.insert(id, track.clone());
        Ok(track)
    }
}
impl Sequences {
    pub fn get(&self, id: u32) -> Option<SharedTrack> {
        if id == 0 {
            self.tracks
                .get(&self.visual_id)
                .or_else(|| self.tracks.values().next())
                .cloned()
        } else {
            self.tracks.get(&id).cloned()
        }
    }
    pub fn finalize(&mut self) {
        self.duration = 0;
        for track in self.tracks.values() {
            let mut t = track.lock().unwrap();
            t.offsets.push(self.data.len() as u64);
            self.data.append(&mut t.pending);
            if t.options.tai_presence != 0 {
                let interleaved = t.options.interleaved;
                t.aux_tai.finish(&mut self.data, interleaved);
            }
            if t.options.content_presence != 0 {
                let interleaved = t.options.interleaved;
                t.aux_content.finish(&mut self.data, interleaved);
            }
            t.finalizations += 1;
            t.media_duration = t.durations.iter().map(|x| u64::from(*x)).sum();
            if self.timescale == 0 {
                self.timescale = t.options.timescale;
            }
            let segment = if t.options.timescale == 0 {
                0
            } else {
                t.media_duration.wrapping_mul(u64::from(self.timescale))
                    / u64::from(t.options.timescale)
            };
            t.movie_duration = if self.repetitions == 0 {
                u64::MAX
            } else {
                segment.saturating_mul(u64::from(self.repetitions))
            };
            if self.repetitions != 1 && !t.child_order.contains(b"edts") {
                t.child_order.push(*b"edts");
            }
            if t.child_order.contains(b"edts") {
                t.edits.push(segment);
            }
            self.duration = self.duration.max(t.movie_duration);
        }
    }
    pub fn moov(&self, base: u64) -> Vec<u8> {
        if !self.initialized {
            return Vec::new();
        }
        let version = u8::from(self.duration > u64::from(u32::MAX));
        let n = if version == 1 { 8 } else { 4 };
        let mut b = vec![0; n * 2];
        number(&mut b, u64::from(self.timescale), 4);
        number(&mut b, self.duration, n);
        number(&mut b, 0x10000, 4);
        number(&mut b, 0x100, 2);
        b.extend([0; 10]);
        matrix(&mut b);
        b.extend([0; 24]);
        number(&mut b, u64::from(self.next_id), 4);
        let mut out = full(*b"mvhd", version, 0, &b);
        for t in self.tracks.values() {
            let t = t.lock().unwrap();
            out.extend(t.serialize(base));
            if t.duplicate_references() {
                break;
            }
        }
        boxed(*b"moov", &out)
    }
}
fn matrix(b: &mut Vec<u8>) {
    for x in [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000] {
        number(b, x, 4);
    }
}
fn handler(kind: u32) -> Vec<u8> {
    let mut b = vec![0; 4];
    number(&mut b, u64::from(kind), 4);
    b.extend([0; 13]);
    full(*b"hdlr", 0, 0, &b)
}
fn invalid(s: &str) -> ContextError {
    ContextError::invalid(0, &format!("Unspecified: {s}"))
}
impl Track {
    fn duplicate_references(&self) -> bool {
        self.references.iter().any(|(_, ids)| {
            ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        })
    }
    fn serialize(&self, base: u64) -> Vec<u8> {
        let v = u8::from(self.movie_duration > u64::from(u32::MAX));
        let n = if v == 1 { 8 } else { 4 };
        let mut b = vec![0; n * 2];
        number(&mut b, u64::from(self.id), 4);
        b.extend([0; 4]);
        number(&mut b, self.movie_duration, n);
        b.extend([0; 12]);
        number(&mut b, 0x100, 2);
        b.extend([0; 2]);
        matrix(&mut b);
        number(&mut b, u64::from(self.header_dimensions.0) << 16, 4);
        number(&mut b, u64::from(self.header_dimensions.1) << 16, 4);
        let mut out = full(*b"tkhd", v, 7, &b);
        let v = u8::from(self.media_duration > u64::from(u32::MAX));
        let n = if v == 1 { 8 } else { 4 };
        b = vec![0; n * 2];
        number(&mut b, u64::from(self.options.timescale), 4);
        number(&mut b, self.media_duration, n);
        number(&mut b, 0x55cb, 2);
        b.extend([0; 2]);
        let mut mdia = full(*b"mdhd", v, 0, &b);
        mdia.extend(handler(self.handler));
        let url = full(*b"url ", 0, 1, &[]);
        let mut dref = vec![0, 0, 0, 1];
        dref.extend(url);
        let mut minf = boxed(*b"dinf", &full(*b"dref", 0, 0, &dref));
        b = vec![0, 0, 0, u8::from(self.entry_kind != 0)];
        b.extend(&self.sample_description);
        let mut stbl = full(*b"stsd", 0, 0, &b);
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for &d in &self.durations {
            if let Some(last) = runs.last_mut().filter(|x| x.1 == d) {
                last.0 += 1;
            } else {
                runs.push((1, d));
            }
        }
        b = Vec::new();
        number(&mut b, runs.len() as u64, 4);
        for (count, d) in runs {
            number(&mut b, u64::from(count), 4);
            number(&mut b, u64::from(d), 4);
        }
        stbl.extend(full(*b"stts", 0, 0, &b));
        b = vec![0, 0, 0, u8::from(self.entry_kind != 0)];
        if self.entry_kind != 0 {
            number(&mut b, 1, 4);
            number(&mut b, self.sizes.len() as u64, 4);
            number(&mut b, 1, 4);
        }
        stbl.extend(full(*b"stsc", 0, 0, &b));
        let fixed = self
            .sizes
            .first()
            .copied()
            .filter(|x| *x != 0 && self.sizes.iter().all(|y| y == x))
            .unwrap_or(0);
        b = Vec::new();
        number(&mut b, u64::from(fixed), 4);
        number(&mut b, self.sizes.len() as u64, 4);
        if fixed == 0 {
            for &s in &self.sizes {
                number(&mut b, u64::from(s), 4);
            }
        }
        stbl.extend(full(*b"stsz", 0, 0, &b));
        b = Vec::new();
        number(&mut b, self.offsets.len() as u64, 4);
        for offset in &self.offsets {
            number(&mut b, base + offset, 4);
        }
        stbl.extend(full(*b"stco", 0, 0, &b));
        for _ in 0..self.finalizations {
            if self.options.tai_presence != 0 {
                stbl.extend(self.aux_tai.boxes(*b"stai", base));
            }
            if self.options.content_presence != 0 {
                stbl.extend(self.aux_content.boxes(*b"suid", base));
            }
        }
        minf.extend(boxed(*b"stbl", &stbl));
        minf.extend(if self.visual() {
            full(*b"vmhd", 0, 1, &[0; 8])
        } else {
            full(*b"nmhd", 0, 1, &[])
        });
        mdia.extend(boxed(*b"minf", &minf));
        out.extend(boxed(*b"mdia", &mdia));
        for kind in &self.child_order {
            match kind {
                b"meta" => {
                    let mut meta = handler(u32::from_be_bytes(*b"meta"));
                    let mut infe = vec![0, 1, 0, 0];
                    infe.extend(b"uri \0urn:uuid:15beb8e4-944d-5fc6-a3dd-cb5a7e655c73\0");
                    let mut iinf = vec![0, 1];
                    iinf.extend(full(*b"infe", 2, 0, &infe));
                    meta.extend(full(*b"iinf", 0, 0, &iinf));
                    let mut iloc = vec![0x44, 0x40, 0, 1, 0, 1, 0, 1, 0, 0];
                    iloc.extend([0; 4]);
                    iloc.extend([0, 1]);
                    iloc.extend([0; 4]);
                    number(&mut iloc, (self.options.content_id.len() + 1) as u64, 4);
                    let iloc_box = full(*b"iloc", 1, 0, &iloc);
                    let mut data = self.options.content_id.clone();
                    data.push(0);
                    meta.extend(boxed(*b"idat", &data));
                    meta.extend(iloc_box);
                    out.extend(full(*b"meta", 0, 0, &meta));
                }
                b"tref" => {
                    if self.references.iter().any(|(_, ids)| {
                        ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
                    }) {
                        break;
                    }
                    let mut refs = Vec::new();
                    for (kind, ids) in &self.references {
                        let mut b = Vec::new();
                        for id in ids {
                            number(&mut b, u64::from(*id), 4);
                        }
                        refs.extend(boxed(kind.to_be_bytes(), &b));
                    }
                    out.extend(boxed(*b"tref", &refs));
                }
                b"edts" => {
                    let v = u8::from(self.edits.iter().any(|x| *x > u64::from(u32::MAX)));
                    let n = if v == 1 { 8 } else { 4 };
                    let mut b = Vec::new();
                    number(&mut b, self.edits.len() as u64, 4);
                    for &d in &self.edits {
                        number(&mut b, d, n);
                        number(&mut b, 0, n);
                        number(&mut b, 0x10000, 4);
                    }
                    out.extend(boxed(*b"edts", &full(*b"elst", v, 1, &b)));
                }
                _ => {}
            }
        }
        boxed(*b"trak", &out)
    }
}

fn decode_string(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    if data.last() != Some(&0) {
        return Err(invalid("utf8string not null-terminated"));
    }
    if data[..data.len() - 1].contains(&0) {
        return Err(invalid("utf8string with null character"));
    }
    Ok(data[..data.len() - 1].to_vec())
}
fn num(b: &[u8], at: usize, n: usize) -> Result<u64> {
    let data = b
        .get(at..at + n)
        .ok_or_else(|| ContextError::invalid(100, "Unexpected end of file"))?;
    Ok(data.iter().fold(0, |v, c| (v << 8) | u64::from(*c)))
}
fn sub(b: &[u8], at: usize) -> Result<&[u8]> {
    b.get(at..)
        .ok_or_else(|| ContextError::invalid(100, "Unexpected end of file"))
}
fn find<'a>(b: &[([u8; 4], &'a [u8])], kind: [u8; 4]) -> Option<&'a [u8]> {
    b.iter().find(|x| x.0 == kind).map(|x| x.1)
}
fn need<'a>(b: &[([u8; 4], &'a [u8])], kind: [u8; 4]) -> Result<&'a [u8]> {
    find(b, kind).ok_or_else(|| {
        invalid(&format!(
            "Track has no '{}' box.",
            String::from_utf8_lossy(&kind)
        ))
    })
}
fn entries(b: &[u8], width: usize, start: usize) -> Result<Vec<Vec<u64>>> {
    let count = num(b, start - 4, 4)? as usize;
    let bytes = count
        .checked_mul(width * 4)
        .ok_or_else(|| invalid("Table length overflow"))?;
    let body = b
        .get(
            start
                ..start
                    .checked_add(bytes)
                    .ok_or_else(|| invalid("Table length overflow"))?,
        )
        .ok_or_else(|| ContextError::invalid(100, "Unexpected end of file"))?;
    body.chunks_exact(width * 4)
        .map(|row| (0..width).map(|i| num(row, i * 4, 4)).collect())
        .collect()
}
impl Context {
    pub fn read_sequences(&mut self, input: Arc<dyn crate::context::Input>) -> Result<()> {
        use crate::context::children;
        let mut data = input.bytes();
        let mut movie = None;
        while data.len() >= 8 {
            let mut n = num(data, 0, 4)?;
            let mut start = 8;
            if n == 1 {
                n = num(data, 8, 8)?;
                start = 16;
            }
            if n == 0 {
                n = data.len() as u64;
            }
            if n < start as u64 || n > data.len() as u64 {
                break;
            }
            if &data[4..8] == b"moov" {
                movie = Some(&data[start..n as usize]);
            }
            data = &data[n as usize..];
        }
        let Some(movie) = movie else {
            self.sequences.timescale = 0;
            self.sequences.duration = 0;
            return Ok(());
        };
        let boxes = children(movie)?;
        validate_sequence_boxes(movie, &self.limits.read().unwrap())?;
        let mvhd =
            find(&boxes, *b"mvhd").ok_or_else(|| invalid("No mvhd box in image sequence."))?;
        let n = if num(mvhd, 0, 1)? == 1 { 8 } else { 4 };
        self.sequences.timescale = num(mvhd, 4 + 2 * n, 4)? as u32;
        self.sequences.duration = num(mvhd, 8 + 2 * n, n)?;
        self.sequences.initialized = true;
        let indefinite = self.sequences.duration
            == if n == 8 {
                u64::MAX
            } else {
                u64::from(u32::MAX)
            };
        self.sequences.tracks.clear();
        let limits = *self.limits.read().unwrap();
        for (_, data) in boxes.iter().filter(|x| x.0 == *b"trak") {
            let boxes = children(data)?;
            let mdia = children(need(&boxes, *b"mdia")?)?;
            let hd = need(&mdia, *b"hdlr")?;
            let handler = num(hd, 8, 4)? as u32;
            if ![
                u32::from_be_bytes(*b"meta"),
                u32::from_be_bytes(*b"pict"),
                u32::from_be_bytes(*b"vide"),
                u32::from_be_bytes(*b"auxv"),
            ]
            .contains(&handler)
            {
                continue;
            }
            let tkhd = need(&boxes, *b"tkhd")?;
            let n = if num(tkhd, 0, 1)? == 1 { 8 } else { 4 };
            let id = num(tkhd, 4 + 2 * n, 4)? as u32;
            let mut t = Track::new(id, handler, (0, 0), None, TrackOptions::default());
            t.reported_handler = handler;
            t.input = Some(input.clone());
            let minf = children(need(&mdia, *b"minf")?)?;
            let mdhd = need(&mdia, *b"mdhd")?;
            let n = if num(mdhd, 0, 1)? == 1 { 8 } else { 4 };
            t.options.timescale = num(mdhd, 4 + 2 * n, 4)? as u32;
            t.media_duration = num(mdhd, 8 + 2 * n, n)?;
            let stbl = children(need(&minf, *b"stbl")?)?;
            let stsd = need(&stbl, *b"stsd")?;
            let stsc = need(&stbl, *b"stsc")?;
            let stco = need(&stbl, *b"stco")?;
            let stsz = need(&stbl, *b"stsz")?;
            let count = num(stsz, 8, 4)?;
            if limits.max_sequence_frames != 0 && count > u64::from(limits.max_sequence_frames) {
                return Err(ContextError::new(
                    6,
                    1000,
                    "Memory allocation error: Security limit exceeded: Number of 'stsz' samples exceeds the maximum number of sequence frames.",
                ));
            }
            let stts = need(&stbl, *b"stts")?;
            let timing = entries(stts, 2, 8)?;
            if timing.iter().map(|x| x[0]).sum::<u64>() != count {
                return Err(invalid(
                    "Number of samples in 'stts' and 'stsz' is inconsistent.",
                ));
            }
            for (size, reason) in [
                (16, "the sequence chunk sample-range tables"),
                (48, "the sequence presentation timeline"),
            ] {
                t.reservations
                    .push(self.budget.reserve(count * size, reason).map_err(|e| {
                        ContextError::new(e.code, e.subcode, e.message.to_string_lossy())
                    })?);
            }
            t.durations
                .try_reserve(count as usize)
                .map_err(|_| ContextError::new(6, 0, "Out of memory"))?;
            for row in timing {
                t.durations
                    .extend(std::iter::repeat_n(row[1] as u32, row[0] as usize));
            }
            let fixed = num(stsz, 4, 4)? as u32;
            if fixed != 0 {
                t.sizes = vec![fixed; count as usize];
            } else {
                t.sizes = entries(stsz, 1, 12)?
                    .into_iter()
                    .map(|x| x[0] as u32)
                    .collect();
            }
            let descriptions = children(sub(stsd, 8)?)?;
            if let Some((kind, b)) = descriptions.first() {
                t.entry_kind = u32::from_be_bytes(*kind);
                t.sample_description = boxed(*kind, b);
                if kind == b"urim" {
                    let properties = children(sub(b, 8)?)?;
                    t.uri = find(&properties, *b"uri ")
                        .map(|x| {
                            sub(x, 4)
                                .map(|v| v.split(|x| *x == 0).next().unwrap_or_default().to_vec())
                        })
                        .transpose()?;
                }
            }
            let offsets = entries(stco, 1, 8)?;
            let mapping = entries(stsc, 3, 8)?;
            let mut chunks = Vec::new();
            let (mut decoder, mut previous) = (0u32, None);
            for (idx, off) in offsets.iter().enumerate() {
                let row = mapping
                    .iter()
                    .rev()
                    .find(|r| r[0] <= idx as u64 + 1)
                    .ok_or_else(|| invalid("'stco' box references a non-existing chunk."))?;
                let description = descriptions
                    .get(row[2].wrapping_sub(1) as usize)
                    .ok_or_else(|| {
                        invalid("Track references a non-existing sample description.")
                    })?;
                if t.ranges.len() as u64 + row[1] > count {
                    return Err(invalid(
                        "Number of samples in 'stsc' box exceeds sample sizes in 'stsz' box.",
                    ));
                }
                let start = t.ranges.len();
                let mut pos = off[0];
                if previous.is_some_and(|p| p != row[2]) {
                    decoder += 1;
                }
                previous = Some(row[2]);
                for _ in 0..row[1] {
                    let size = t.sizes[t.ranges.len()];
                    t.ranges.push((pos, size));
                    t.sample_chunks.push(decoder);
                    pos = pos
                        .checked_add(u64::from(size))
                        .ok_or_else(|| invalid("Chunk file offset overflows 64-bit range."))?;
                }
                chunks.push((start, t.ranges.len()));
                let visual = matches!(
                    &description.0,
                    b"uncv"
                        | b"hvc1"
                        | b"hev1"
                        | b"av01"
                        | b"avc1"
                        | b"vvc1"
                        | b"vvi1"
                        | b"jpeg"
                        | b"j2k1"
                );
                if t.visual() && !visual {
                    return Err(invalid(
                        "Visual track sample description does not match visual track.",
                    ));
                }
                let properties = if description.0 == *b"urim" {
                    children(sub(description.1, 8)?)?
                } else if visual {
                    children(sub(description.1, 78)?)?
                } else {
                    Vec::new()
                };
                if t.first_clock.is_none()
                    && let Some(b) = find(&properties, *b"taic")
                    && let TaiProperty::Clock(c) = TaiProperty::parse(*b"taic", b)?
                {
                    t.first_clock = Some(Box::new(c));
                }
                if let Some(b) = find(&properties, *b"auxi") {
                    t.auxiliary_urn = sub(b, 4)?
                        .split(|x| *x == 0)
                        .next()
                        .unwrap_or_default()
                        .to_vec();
                }
                if t.visual() {
                    t.dimensions = (
                        num(description.1, 24, 2)? as u16,
                        num(description.1, 26, 2)? as u16,
                    );
                }
            }
            if t.ranges.len() as u64 != count {
                return Err(invalid(
                    "Number of samples covered by 'stsc' does not match 'stsz'/'stts'.",
                ));
            }
            let saiz: Vec<_> = stbl
                .iter()
                .filter(|x| x.0 == *b"saiz")
                .map(|x| x.1)
                .collect();
            let saio: Vec<_> = stbl
                .iter()
                .filter(|x| x.0 == *b"saio")
                .map(|x| x.1)
                .collect();
            if saiz.len() != saio.len() {
                return Err(invalid("Boxes 'saiz' and `saio` must come in pairs."));
            }
            for sizebox in saiz {
                let flags = num(sizebox, 1, 3)?;
                let (kind, parameter, at) = if flags & 1 != 0 {
                    (num(sizebox, 4, 4)? as u32, num(sizebox, 8, 4)? as u32, 12)
                } else {
                    (0, 0, 4)
                };
                let fixed = num(sizebox, at, 1)?;
                let n = num(sizebox, at + 1, 4)? as usize;
                let offsets = saio
                    .iter()
                    .find(|b| {
                        let f = num(b, 1, 3).unwrap_or(0);
                        if f & 1 != 0 {
                            num(b, 4, 4).ok() == Some(u64::from(kind))
                                && num(b, 8, 4).ok() == Some(u64::from(parameter))
                        } else {
                            kind == 0 && parameter == 0
                        }
                    })
                    .ok_or_else(|| invalid("'saiz' box without matching 'saio' box."))?;
                let oa = if num(offsets, 1, 3)? & 1 != 0 { 12 } else { 4 };
                let no = num(offsets, oa, 4)? as usize;
                if no != 1 && no != chunks.len() {
                    return Err(invalid("Invalid number of chunks in 'saio' box."));
                }
                if no != 1 && chunks.is_empty() && n > 0 {
                    return Err(invalid(
                        "'saiz' box references samples but no chunks exist.",
                    ));
                }
                if n as u64 > count {
                    return Err(invalid(
                        "Number of samples in 'saiz' box exceeds actual number of samples.",
                    ));
                }
                let width = if num(offsets, 0, 1)? == 1 { 8 } else { 4 };
                let mut ranges = Vec::new();
                let mut pos = if no > 0 {
                    num(offsets, oa + 4, width)?
                } else {
                    0
                };
                let mut chunk = 0;
                for i in 0..n {
                    if no != 1 && chunk < chunks.len() && i >= chunks[chunk].1 {
                        chunk += 1;
                        if chunk >= chunks.len() {
                            break;
                        }
                        pos = num(offsets, oa + 4 + chunk * width, width)?;
                    }
                    let size = if fixed == 0 {
                        num(sizebox, at + 5 + i, 1)?
                    } else {
                        fixed
                    };
                    ranges.push((pos, size as u8));
                    pos = pos
                        .checked_add(size)
                        .ok_or_else(|| invalid("Chunk file offset overflows 64-bit range."))?;
                }
                if kind == u32::from_be_bytes(*b"stai") || kind == u32::from_be_bytes(*b"suid") {
                    t.aux_ranges.push((AuxType { kind, parameter }, ranges));
                }
            }
            t.aux_ranges.sort_by_key(|(k, _)| {
                if k.kind == u32::from_be_bytes(*b"stai") {
                    0
                } else {
                    1
                }
            });
            t.aux_types = t.aux_ranges.iter().map(|x| x.0).collect();
            if let Some(tref) = find(&boxes, *b"tref") {
                for (kind, b) in children(tref)? {
                    let mut ids = Vec::new();
                    for d in b.chunks_exact(4) {
                        ids.push(num(d, 0, 4)? as u32);
                    }
                    if ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len() {
                        return Err(invalid("'tref' has double references"));
                    }
                    t.references.push((u32::from_be_bytes(kind), ids));
                }
            }
            if let Some(meta) = find(&boxes, *b"meta") {
                let boxes = children(sub(meta, 4)?)?;
                if let Some(data) = find(&boxes, *b"idat") {
                    t.options.content_id = decode_string(data)?;
                }
            }
            t.output_count = count;
            if let Some(edts) = find(&boxes, *b"edts") {
                let edts = children(edts)?;
                if let Some(elst) = find(&edts, *b"elst") {
                    t.repetitions = 0;
                    let n = if num(elst, 0, 1)? == 1 { 8 } else { 4 };
                    if num(elst, 4, 4)? == 1
                        && num(elst, 1, 3)? & 1 != 0
                        && t.options.timescale == self.sequences.timescale
                        && num(elst, 8 + n, n)? == 0
                        && num(elst, 8, n)? == t.media_duration
                    {
                        if t.media_duration == 0 {
                            return Err(invalid("Track duration is zero."));
                        }
                        let reps = self.sequences.duration / t.media_duration;
                        t.repetitions = if indefinite {
                            u32::MAX
                        } else {
                            reps.min(u64::from(u32::MAX)) as u32
                        };
                        let cap = if limits.max_sequence_frames == 0 {
                            u32::MAX
                        } else {
                            limits.max_sequence_frames
                        };
                        t.output_count = count.saturating_mul(reps).min(u64::from(cap));
                    }
                }
            }
            if t.visual() && self.sequences.visual_id == 0 {
                self.sequences.visual_id = id;
            }
            self.items.layout.lock().unwrap().mark(1, id);
            self.sequences
                .tracks
                .entry(id)
                .or_insert_with(|| Arc::new(Mutex::new(t)));
        }
        Ok(())
    }
}

impl Track {
    pub fn encode_uncompressed(&mut self, image: &crate::image::Image) -> Result<()> {
        let (data, properties) = crate::uncompressed_encode::encode(image, 0)?;
        self.dimensions = (image.width as u16, image.height as u16);
        if self.entry_kind == 0 {
            self.entry_kind = u32::from_be_bytes(*b"uncv");
            let mut b = vec![0; 6];
            number(&mut b, 1, 2);
            b.extend([0; 16]);
            number(&mut b, u64::from(image.width), 2);
            number(&mut b, u64::from(image.height), 2);
            number(&mut b, 0x480000, 4);
            number(&mut b, 0x480000, 4);
            b.extend([0; 4]);
            number(&mut b, 1, 2);
            let mut name = [0; 32];
            name[0] = 11;
            name[1..12].copy_from_slice(b"iso23001-17");
            b.extend(name);
            number(&mut b, 24, 2);
            number(&mut b, 65535, 2);
            for (p, _) in properties {
                if matches!(
                    &p.kind,
                    b"cmpd"
                        | b"uncC"
                        | b"cmpC"
                        | b"icef"
                        | b"cpat"
                        | b"splz"
                        | b"sbpm"
                        | b"snuc"
                        | b"cloc"
                ) {
                    b.extend(boxed(p.kind, &p.data));
                }
            }
            b.extend(full(*b"ccst", 0, 0, &[0x80, 0, 0, 0]));
            if self.options.tai_presence != 0
                && let Some(c) = &self.options.clock
            {
                b.extend(boxed(*b"taic", &TaiProperty::Clock(**c).property().data));
            }
            self.sample_description = boxed(*b"uncv", &b);
        }
        self.encoded_frames = self.encoded_frames.wrapping_add(1);
        let mut sample = RawSample::default();
        sample.metadata = image.sample.clone();
        sample.timestamp = image.tai_timestamp.map(Box::new);
        sample.set_data(&data)?;
        self.add_sample(&sample, !image.sample.content_id.is_empty())
    }
    pub fn decode_next(
        &mut self,
        context: &Context,
        colorspace: i32,
        chroma: i32,
        options: crate::decoding::DecodeOptions,
        ignore_editlist: bool,
    ) -> Result<crate::image::Image> {
        // libheif drives every track through its decoder plugins. AVC and VVC
        // always do here (the built-in decoders emulate the OpenH264 and vvdec
        // plugins); other codecs do once a registered plugin is selected over
        // the built-in one.
        let format = match &self.entry_kind.to_be_bytes() {
            b"avc1" => 2,
            b"hvc1" | b"hev1" => 1,
            b"av01" => 4,
            b"vvc1" => 5,
            _ => 0,
        };
        if format == 2
            || format == 5
            || (format != 0
                && (self.stateful.is_some()
                    || options.decoder_provider.is_some_and(|p| {
                        matches!(p.select(format, options.decoder_id), Ok(Some(_)))
                    })))
        {
            return self.decode_next_stateful(
                format,
                context,
                colorspace,
                chroma,
                options,
                ignore_editlist,
            );
        }
        if !self.decode_failed && ignore_editlist && self.next as usize >= self.ranges.len() {
            return Err(ContextError::new(
                13,
                0,
                "End of sequence: Unspecified: End of sequence",
            ));
        }
        if self.decode_failed {
            return Err(ContextError::new(
                7,
                0,
                "Decoder plugin generated an error: Unspecified: Did not decode all frames",
            ));
        }
        let sample = match self.next_raw() {
            Ok(s) => s,
            Err(e) => {
                if self.entry_kind == u32::from_be_bytes(*b"uncv") {
                    self.next = self.next.wrapping_add(1);
                    self.decode_failed = u64::from(self.next) >= self.output_count;
                } else {
                    self.decode_failed = true;
                }
                return Err(e);
            }
        };
        let description = sub(&self.sample_description, 8)?;
        let properties = crate::context::children(sub(description, 78)?)?
            .into_iter()
            .filter(|(kind, _)| !matches!(kind, b"ccst" | b"taic" | b"auxi"))
            .map(|(kind, data)| (crate::encoding::property(kind, data.to_vec()), false))
            .collect();
        let kind = match &self.entry_kind.to_be_bytes() {
            b"uncv" => *b"unci",
            b"hvc1" => *b"hvc1",
            b"av01" => *b"av01",
            b"avc1" => *b"avc1",
            b"vvc1" => *b"vvc1",
            _ => self.entry_kind.to_be_bytes(),
        };
        let mut temporary = Context {
            budget: context.budget.clone(),
            limits: context.limits.clone(),
            ..Default::default()
        };
        let template = crate::image::Image::new(
            u32::from(self.dimensions.0),
            u32::from(self.dimensions.1),
            2,
            0,
        )?;
        let image = temporary.insert_encoded(
            &template,
            kind,
            sample.data().to_vec(),
            properties,
            &crate::encoding::Options::default(),
        )?;
        let document = temporary.decoding_document().unwrap();
        let result = crate::decoding::decode(&document, image.id, colorspace, chroma, options);
        match result {
            Ok(mut image) => {
                image.sample = sample.metadata;
                if self.entry_kind == u32::from_be_bytes(*b"uncv") && !self.durations.is_empty() {
                    image.sample.duration =
                        self.durations[self.next as usize % self.durations.len()];
                }
                image.tai_timestamp = sample.timestamp.map(|t| *t);
                Ok(image)
            }
            Err(e) => {
                self.next = self.next.wrapping_sub(1);
                Err(e)
            }
        }
    }
}

impl Track {
    /// The sample entry's configuration properties (avcC, colr, ...).
    fn entry_properties(&self) -> Result<Vec<(crate::properties::Property, bool)>> {
        let description = sub(&self.sample_description, 8)?;
        Ok(crate::context::children(sub(description, 78)?)?
            .into_iter()
            .filter(|(kind, _)| !matches!(kind, b"ccst" | b"taic" | b"auxi"))
            .map(|(kind, data)| (crate::encoding::property(kind, data.to_vec()), false))
            .collect())
    }

    /// libheif's `Track_Visual::decode_next_image_sample`: samples are pushed
    /// into a stateful decoder per run of chunks sharing a sample description
    /// (configuration units with sample 0 only), frames are polled before each
    /// push, and the decoder is flushed at the end of the samples.
    fn decode_next_stateful(
        &mut self,
        format: i32,
        context: &Context,
        colorspace: i32,
        chroma: i32,
        options: crate::decoding::DecodeOptions,
        ignore_editlist: bool,
    ) -> Result<crate::image::Image> {
        let count = self.ranges.len() as u64;
        let limit = if ignore_editlist {
            count
        } else {
            self.output_count
        };
        let mut state = self.stateful.take().unwrap_or_default();
        let result = self.stateful_loop(
            &mut state, format, context, colorspace, chroma, options, count, limit,
        );
        self.stateful = Some(state);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn stateful_loop(
        &mut self,
        state: &mut StatefulTrack,
        format: i32,
        context: &Context,
        colorspace: i32,
        chroma: i32,
        options: crate::decoding::DecodeOptions,
        count: u64,
        limit: u64,
    ) -> Result<crate::image::Image> {
        if state.next_output >= limit || count == 0 {
            return Err(ContextError::new(
                13,
                0,
                "End of sequence: Unspecified: End of sequence",
            ));
        }
        let properties = self.entry_properties()?;
        let limits = *context
            .limits
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The sample entry as an item: configuration units, coded-size checks
        // and the still-image pipeline for decoded frames.
        let mut temporary = Context {
            budget: context.budget.clone(),
            limits: context.limits.clone(),
            ..Default::default()
        };
        let template = crate::image::Image::new(
            u32::from(self.dimensions.0),
            u32::from(self.dimensions.1),
            2,
            0,
        )?;
        let item = temporary.insert_encoded(
            &template,
            self.entry_kind.to_be_bytes(),
            Vec::new(),
            properties,
            &crate::encoding::Options::default(),
        )?;
        let document = temporary
            .decoding_document()
            .ok_or_else(|| invalid("Missing decoding document"))?;
        let container = document.container()?;
        let coded_size = match format {
            1 => container
                .property(item.id, *b"hvcC")
                .ok()
                .map(crate::hevc_config::coded_size)
                .transpose()?
                .flatten(),
            2 => container
                .property(item.id, *b"avcC")
                .ok()
                .and_then(crate::avc_config::parse_configuration)
                .map(|c| crate::avc_config::coded_size(&c))
                .transpose()?
                .flatten(),
            #[cfg(feature = "vvc")]
            5 => container
                .property(item.id, *b"vvcC")
                .ok()
                .map(crate::vvc::heif::config_coded_size)
                .transpose()?
                .flatten(),
            _ => None,
        };
        let mut decoded_idx = 0u64;
        let (image, sample_idx) = loop {
            let sample_idx = (state.next_decoded % count) as usize;
            let group = self.sample_chunks.get(sample_idx).copied().unwrap_or(0) as usize;
            let mut decoders = state
                .decoders
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if decoders.len() <= group {
                decoders.resize_with(group + 1, || None);
            }
            let slot = &mut decoders[group];
            if state.next_decoded != 0 {
                select_group(slot, format, &options)?;
                let frame = match slot.as_mut() {
                    #[cfg(feature = "avc")]
                    Some(GroupDecoder::Avc(decoder)) => {
                        let document = context_document(context)?;
                        decoder
                            .decode_next(&document, limits.max_image_size_pixels)?
                            .map(|(image, user)| {
                                decoded_idx = user;
                                image
                            })
                    }
                    #[cfg(feature = "vvc")]
                    Some(GroupDecoder::Vvc(decoder)) => {
                        let document = context_document(context)?;
                        decoder
                            .decode_next(&document, limits.max_image_size_pixels)?
                            .map(|(image, user)| {
                                decoded_idx = user;
                                image
                            })
                    }
                    Some(GroupDecoder::Plugin(stream)) => stream.next(&mut decoded_idx)?,
                    None => None,
                };
                if let Some(image) = frame {
                    break (image, sample_idx);
                }
                if state.flushed {
                    return Err(ContextError::new(
                        7,
                        0,
                        "Decoder plugin generated an error: Unspecified: Did not decode all frames",
                    ));
                }
            }
            if state.next_decoded < self.output_count {
                // The sample counts as pushed before the decoder is selected.
                state.next_decoded += 1;
                select_group(slot, format, &options)?;
                // libheif checks the configuration's coded size against the limits.
                if let Some((w, h)) = coded_size {
                    limits.check_image_size(w, h)?;
                }
                let (offset, size) = self.ranges[sample_idx];
                let sample = self.read_range(offset, u64::from(size))?.into_owned();
                let mut data = if sample_idx == 0 {
                    crate::decoding::codec_configuration(&container, item.id, format)?
                } else {
                    Vec::new()
                };
                data.extend_from_slice(&sample);
                match slot.as_mut() {
                    #[cfg(feature = "avc")]
                    Some(GroupDecoder::Avc(decoder)) => {
                        if data.is_empty() {
                            return Err(ContextError::invalid(
                                0,
                                "Unspecified: Input with empty data extent.",
                            ));
                        }
                        decoder.push(data, sample_idx as u64)?
                    }
                    #[cfg(feature = "vvc")]
                    Some(GroupDecoder::Vvc(decoder)) => {
                        if data.is_empty() {
                            return Err(ContextError::invalid(
                                0,
                                "Unspecified: Input with empty data extent.",
                            ));
                        }
                        decoder.push(&data, sample_idx as u64)?
                    }
                    Some(GroupDecoder::Plugin(stream)) => {
                        stream.push(&data, sample_idx as u64, &options)?
                    }
                    None => {}
                }
            } else {
                match slot.as_mut() {
                    #[cfg(feature = "avc")]
                    Some(GroupDecoder::Avc(decoder)) => decoder.flush(),
                    #[cfg(feature = "vvc")]
                    Some(GroupDecoder::Vvc(decoder)) => decoder.flush(),
                    Some(GroupDecoder::Plugin(stream)) => stream.flush()?,
                    None => {}
                }
                state.flushed = true;
            }
        };
        // libheif resets the flushed flag after the last frame of an edit-list
        // segment, before counting the output.
        if (state.next_output + 1).is_multiple_of(count) {
            state.flushed = false;
        }
        state.next_output += 1;
        self.next = state.next_output as u32;
        // Colour handling, transforms and metadata follow the still-image path
        // on the temporary item, which carries the decoded frame.
        *document.images[&item.id]
            .predecoded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(image);
        let still = crate::decoding::DecodeOptions {
            decoder_provider: None,
            decoder_id: None,
            ..options
        };
        let mut image = crate::decoding::decode(&document, item.id, colorspace, chroma, still)?;
        image.sample.duration = self.durations.get(sample_idx).copied().unwrap_or(0);
        let saved = self.next;
        self.next = decoded_idx as u32;
        let metadata = self.next_raw().map(|s| (s.metadata, s.timestamp));
        self.next = saved;
        if let Ok((metadata, timestamp)) = metadata {
            image.sample.content_id = metadata.content_id;
            image.tai_timestamp = timestamp.map(|t| *t);
        }
        Ok(image)
    }
}

/// libheif's `Decoder::require_decoder_plugin`: the plugin is selected once
/// per decoder, by the first push or poll; an old plugin version is reported
/// by that call only.
fn select_group(
    slot: &mut Option<GroupDecoder>,
    format: i32,
    options: &crate::decoding::DecodeOptions,
) -> Result<()> {
    if slot.is_some() {
        return Ok(());
    }
    let selected = match options.decoder_provider {
        Some(provider) => provider.select(format, options.decoder_id)?,
        None => None,
    };
    if let Some(decoder) = selected {
        let stream = decoder
            .sequence()
            .ok_or_else(|| ContextError::new(4, 3000, "Unsupported feature: Unsupported codec"))?;
        *slot = Some(GroupDecoder::Plugin(stream));
        return decoder.validate();
    }
    #[cfg(feature = "avc")]
    if format == 2 {
        *slot = Some(GroupDecoder::Avc(Box::default()));
        return Ok(());
    }
    #[cfg(feature = "vvc")]
    if format == 5 {
        *slot = Some(GroupDecoder::Vvc(Box::default()));
        return Ok(());
    }
    Err(ContextError::new(
        4,
        3000,
        "Unsupported feature: Unsupported codec",
    ))
}

#[cfg(any(feature = "avc", feature = "vvc"))]
fn context_document(context: &Context) -> Result<Arc<crate::context::Document>> {
    let mut temporary = Context {
        budget: context.budget.clone(),
        limits: context.limits.clone(),
        ..Default::default()
    };
    let template = crate::image::Image::new(1, 1, 2, 0)?;
    temporary.insert_encoded(
        &template,
        *b"avc1",
        Vec::new(),
        Vec::new(),
        &crate::encoding::Options::default(),
    )?;
    temporary
        .decoding_document()
        .ok_or_else(|| invalid("Missing decoding document"))
}

fn validate_sequence_boxes(data: &[u8], limits: &crate::security::Limits) -> Result<()> {
    use crate::context::children;
    let mut stack = children(data)?.into_iter().rev().collect::<Vec<_>>();
    let frame_limit = if limits.max_sequence_frames == 0 {
        u64::from(u32::MAX)
    } else {
        u64::from(limits.max_sequence_frames)
    };
    let frame_error = || {
        ContextError::new(
            6,
            1000,
            "Memory allocation error: Security limit exceeded: Security limit for maximum number of sequence frames exceeded",
        )
    };
    while let Some((kind, b)) = stack.pop() {
        let versioned = matches!(
            &kind,
            b"mvhd"
                | b"tkhd"
                | b"mdhd"
                | b"hdlr"
                | b"stsd"
                | b"stts"
                | b"stsc"
                | b"stsz"
                | b"stco"
                | b"nmhd"
                | b"vmhd"
                | b"ctts"
                | b"elst"
        );
        if versioned {
            let version = num(b, 0, 1)?;
            let max = if matches!(&kind, b"mvhd" | b"tkhd" | b"mdhd" | b"ctts" | b"elst") {
                1
            } else {
                0
            };
            if version > max {
                return Err(ContextError::new(
                    4,
                    3002,
                    format!(
                        "Unsupported feature: Unsupported data version: {} box data version {version} is not implemented yet",
                        String::from_utf8_lossy(&kind)
                    ),
                ));
            }
            let minimum = match &kind {
                b"mvhd" => 96 + 12 * version as usize,
                b"tkhd" => 84 + 12 * version as usize,
                b"mdhd" => 24 + 12 * version as usize,
                b"hdlr" => 24,
                b"vmhd" => 12,
                b"nmhd" => 4,
                _ => 8,
            };
            sub(b, minimum)?;
        }
        match &kind {
            b"stts" => {
                let count = num(b, 4, 4)?;
                if count > frame_limit {
                    return Err(frame_error());
                }
                let mut total = 0;
                for i in 0..count as usize {
                    let at = 8 + i * 8;
                    if at >= b.len() {
                        return Err(ContextError::invalid(
                            100,
                            &format!(
                                "Unexpected end of file: stts box should contain {count} entries, but box only contained {i} entries"
                            ),
                        ));
                    }
                    total += num(b, at, 4)?;
                    num(b, at + 4, 4)?;
                }
                if total > frame_limit {
                    return Err(frame_error());
                }
            }
            b"stsc" => {
                let count = num(b, 4, 4)?;
                if count == 0 {
                    return Err(invalid("'stsc' box with zero entries."));
                }
                if limits.max_sequence_frames != 0 && count > frame_limit {
                    return Err(invalid(
                        "Number of chunks in `stsc` box exceeds security limits of maximum number of frames.",
                    ));
                }
                for i in 0..count as usize {
                    let at = 8 + i * 12;
                    let _first = num(b, at, 4).unwrap_or(0);
                    let samples = num(b, at + 4, 4).unwrap_or(0);
                    let desc = num(b, at + 8, 4).unwrap_or(0);
                    if samples == 0 {
                        return Err(invalid("'stsc' box with zero samples per chunk entry."));
                    }
                    if desc == 0 {
                        return Err(invalid(
                            "'sample_description_index' in 'stsc' must not be 0.",
                        ));
                    }
                    if limits.max_sequence_frames != 0 && samples > frame_limit {
                        return Err(invalid(
                            "Number of chunk samples in `stsc` box exceeds security limits of maximum number of frames.",
                        ));
                    }
                }
            }
            b"stco" => {
                let count = num(b, 4, 4)?;
                if limits.max_sequence_frames != 0 && count > frame_limit {
                    return Err(invalid(
                        "Number of chunks in 'stco' box exceeds security limits of maximum number of frames.",
                    ));
                }
                sub(b, 8 + count as usize * 4)?;
            }
            b"stsz" => {
                let fixed = num(b, 4, 4)?;
                let count = num(b, 8, 4)?;
                if count > frame_limit {
                    return Err(frame_error());
                }
                if fixed == 0 {
                    for i in 0..count as usize {
                        let at = 12 + i * 4;
                        if at >= b.len() {
                            return Err(ContextError::invalid(
                                100,
                                &format!(
                                    "Unexpected end of file: stsz box should contain {count} entries, but box only contained {i} entries"
                                ),
                            ));
                        }
                        num(b, at, 4)?;
                    }
                }
            }
            b"stsd" => {
                let count = num(b, 4, 4)?;
                if limits.max_sample_description_box_entries != 0
                    && count > u64::from(limits.max_sample_description_box_entries)
                {
                    return Err(ContextError::new(
                        6,
                        1000,
                        format!(
                            "Memory allocation error: Security limit exceeded: stsd box contains {count} entries, which exceeds the security limit of {} items",
                            limits.max_sample_description_box_entries
                        ),
                    ));
                }
            }
            _ => {}
        }
        let prefix = match &kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf" => Some(0),
            b"meta" => Some(4),
            b"stsd" | b"urim" => Some(8),
            b"uncv" | b"hvc1" | b"hev1" | b"av01" | b"avc1" | b"vvc1" => Some(78),
            _ => None,
        };
        if let Some(prefix) = prefix {
            let mut nested = children(sub(b, prefix)?)?;
            nested.reverse();
            stack.extend(nested);
        }
    }
    Ok(())
}
