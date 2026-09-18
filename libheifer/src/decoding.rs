// SPDX-License-Identifier: LGPL-3.0-or-later
//! Item decoding and container metadata orchestration.
use crate::{
    color::Nclx,
    context::{ContextError, Document},
    image::Image,
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

#[derive(Clone)]
pub(crate) struct DecodeState {
    pub ids: BTreeSet<u32>,
    count: Arc<AtomicU32>,
}

pub trait DecodeCallbacks: Sync {
    fn start(&self, step: i32, maximum: i32);
    fn progress(&self, step: i32, value: i32);
    fn end(&self, step: i32);
    fn canceled(&self) -> bool;
}
#[derive(Clone, Copy)]
pub struct DecodeOptions<'a> {
    pub callbacks: Option<&'a dyn DecodeCallbacks>,
    pub decoder_id: Option<&'a [u8]>,
    pub max_decoding_threads: i32,
    pub ignore_transformations: bool,
    pub strict: bool,
    pub output_nclx: Option<Nclx>,
    pub profile_passthrough: bool,
    pub convert_hdr_to_8bit: bool,
    pub color_conversion: crate::color::ColorConversionOptions,
}
impl Default for DecodeOptions<'_> {
    fn default() -> Self {
        Self {
            callbacks: None,
            decoder_id: None,
            max_decoding_threads: 4,
            ignore_transformations: false,
            strict: false,
            output_nclx: None,
            profile_passthrough: false,
            convert_hdr_to_8bit: false,
            color_conversion: crate::color::ColorConversionOptions {
                only_use_preferred_chroma_algorithm: 0,
                ..Default::default()
            },
        }
    }
}
impl From<crate::error::Error> for ContextError {
    fn from(e: crate::error::Error) -> Self {
        Self::new(e.code, e.subcode, e.message.to_string_lossy())
    }
}
pub fn unsupported_conversion() -> ContextError {
    ContextError::new(4, 3003, "Unsupported feature: Unsupported color conversion")
}

pub fn decode(
    document: &Document,
    id: u32,
    colorspace: i32,
    chroma: i32,
    options: DecodeOptions,
) -> Result<Image, ContextError> {
    verify_references(document, id)?;
    let mut visiting = DecodeState {
        ids: BTreeSet::new(),
        count: Arc::new(AtomicU32::new(0)),
    };
    let image = decode_native(document, id, &options, &mut visiting)?;
    let target_cs = if colorspace == 99 {
        image.colorspace
    } else {
        colorspace
    };
    let target_chroma = if chroma == 99 { image.chroma } else { chroma };
    let defaults = Nclx {
        primaries: 1,
        transfer: 13,
        matrix: 6,
        full_range: true,
    };
    let source_profile = image
        .color
        .nclx
        .filter(|n| n.is_defined())
        .unwrap_or(defaults);
    let passthrough = options.output_nclx.is_none() && options.profile_passthrough;
    let requested = options.output_nclx.unwrap_or(if passthrough {
        source_profile
    } else {
        defaults
    });
    if target_cs != image.colorspace
        || target_chroma != image.chroma
        || (options.convert_hdr_to_8bit && image.plane(0).is_some_and(|p| p.bit_depth > 8))
        || (!passthrough && requested != source_profile)
    {
        return crate::conversion::convert(
            image,
            target_cs,
            target_chroma,
            requested,
            if options.convert_hdr_to_8bit { 8 } else { 0 },
            options.color_conversion,
        )
        .map_err(Into::into);
    }
    Ok(image)
}

fn children_for(
    container: &crate::container::Container<'_>,
    document: &Document,
    id: u32,
) -> Vec<u32> {
    let mut result = container
        .items
        .get(&id)
        .and_then(|i| i.references.get(b"dimg"))
        .cloned()
        .unwrap_or_default();
    for item in container.items.values() {
        if item
            .references
            .get(b"auxl")
            .is_some_and(|r| r.contains(&id))
            && container.property(item.id, *b"auxC").ok().is_some_and(|p| {
                matches!(
                    p.get(4..)
                        .unwrap_or_default()
                        .split(|b| *b == 0)
                        .next()
                        .unwrap_or_default(),
                    b"urn:mpeg:avc:2015:auxid:1"
                        | b"urn:mpeg:hevc:2015:auxid:1"
                        | b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha"
                )
            })
        {
            result.push(item.id);
        }
    }
    result.retain(|id| document.images.contains_key(id));
    result
}

// Use a heap worklist: untrusted derivation depth must not consume the stack.
fn verify_references(document: &Document, root: u32) -> Result<(), ContextError> {
    let container = document.container()?;
    let mut on_path = BTreeSet::from([root]);
    let mut verified = BTreeSet::new();
    let mut stack = vec![(root, children_for(&container, document, root).into_iter())];
    while let Some((id, children)) = stack.last_mut() {
        if let Some(child) = children.next() {
            if on_path.contains(&child) {
                return Err(ContextError::invalid(
                    2008,
                    "Image reference cycle: Image reference cycle",
                ));
            }
            if !verified.contains(&child) {
                on_path.insert(child);
                let next = children_for(&container, document, child);
                stack.push((child, next.into_iter()));
            }
        } else {
            on_path.remove(id);
            verified.insert(*id);
            stack.pop();
        }
    }
    if crate::brands::has_compatible_brand(document.input.bytes(), *b"miaf") == 1 {
        let mut verified = BTreeSet::new();
        let mut pending = vec![(root, 2, false)];
        while let Some((id, max_rank, parent_identity)) = pending.pop() {
            let Some(item) = container.items.get(&id) else {
                continue;
            };
            let identity = item.kind == *b"iden";
            let rank = match &item.kind {
                b"grid" => 1,
                b"iovl" => 2,
                _ => 0,
            };
            if identity && parent_identity {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: MIAF: an 'iden' image is derived directly from another 'iden' image",
                ));
            }
            if !identity && rank > max_rank {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: MIAF: derived-image dependencies are not in the order allowed by ISO/IEC 23000-22",
                ));
            }
            if !verified.insert((id, max_rank, parent_identity)) || (!identity && rank == 0) {
                continue;
            }
            let child_rank = if identity { max_rank } else { rank - 1 };
            let dimg = item
                .references
                .get(b"dimg")
                .map(Vec::as_slice)
                .unwrap_or_default();
            for child in children_for(&container, document, id).into_iter().rev() {
                pending.push(if dimg.contains(&child) {
                    (child, child_rank, identity)
                } else {
                    (child, 2, false)
                });
            }
        }
    }

    Ok(())
}

pub(crate) fn decode_native(
    document: &Document,
    id: u32,
    options: &DecodeOptions,
    visiting: &mut DecodeState,
) -> Result<Image, ContextError> {
    if !visiting.ids.insert(id) {
        return Err(ContextError::invalid(
            0,
            "Unspecified: 'iref' has cyclic references",
        ));
    }
    let limits = document.current_limits();
    let budget = u64::from(limits.max_items)
        .saturating_mul(2)
        .min(u64::from(u32::MAX)) as u32;
    if budget != 0 && visiting.count.fetch_add(1, Ordering::Relaxed) >= budget {
        return Err(ContextError::invalid(
            1000,
            "Security limit exceeded: Too many derived-image decode operations (possible reference amplification)",
        ));
    }
    let info = document
        .images
        .get(&id)
        .ok_or_else(|| ContextError::invalid(2000, "Non-existing item ID referenced"))?;
    if let Some(error) = &info.error {
        return Err(error.clone());
    }
    let _item_lock = info
        .decode_mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if info.ispe.0 != 0 && info.ispe.1 != 0 {
        limits.check_image_size(info.ispe.0, info.ispe.1)?;
    }
    let container = document.container()?;
    let mut image = match &container.items[&id].kind {
        b"mski" => crate::mask::decode(&container, id, Some(document.budget.clone()))?,
        b"grid" => decode_grid(document, id, options, visiting)?,
        b"iovl" => crate::overlay::decode(document, id, options, visiting)?,
        b"iden" => {
            if !container.has_references {
                return Err(ContextError::invalid(
                    113,
                    "No 'iref' box: No iref box available, but needed for iden image",
                ));
            }
            let refs = container.items[&id]
                .references
                .get(b"dimg")
                .map(Vec::as_slice)
                .unwrap_or_default();
            if refs.len() > 1 {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: 'iden' image with more than one reference image",
                ));
            }
            let Some(&child) = refs.first() else {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: 'iden' image without 'dimg' reference",
                ));
            };
            if child == id {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: 'iden' image referring to itself",
                ));
            }
            if !document.images.contains_key(&child) {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: 'iden' image references unavailable image",
                ));
            }
            decode_native(document, child, options, visiting)?
        }
        #[cfg(feature = "hevc")]
        b"hvc1" => {
            // Each decode replaces the decoder's input extent. The buffer remains
            // owned by the image item between calls, but is reread on the next call.
            *info
                .decoder_input
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            if options.decoder_id.is_some_and(|id| id != b"rusty_h265") {
                return Err(ContextError::new(
                    11,
                    0,
                    "Error while loading plugin: Unspecified: No decoder with that ID found.",
                ));
            }
            let data = {
                let mut cached = info
                    .decoder_input
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if cached.as_ref().is_none_or(|input| input.data.is_empty()) {
                    let data = container.payload(id)?;
                    let reservation = document
                        .budget
                        .reserve(data.len() as u64, "decoder input buffer (iloc)")?;
                    *cached = Some(crate::context::DecoderInput {
                        data: std::sync::Arc::new(data),
                        _reservation: reservation,
                    });
                }
                cached.as_ref().unwrap().data.clone()
            };
            if data.is_empty() {
                return Err(ContextError::invalid(
                    0,
                    "Unspecified: Input with empty data extent.",
                ));
            }
            crate::hevc::decode_item_from_payload(
                &container,
                id,
                &data,
                Some(document.budget.clone()),
            )
            .map_err(|e| match e {
                crate::hevc::DecodeError::Container(e) => e.into(),
                crate::hevc::DecodeError::Image(e) => e.into(),
                e => ContextError::new(
                    7,
                    0,
                    format!("Decoder plugin generated an error: Unspecified: {e}"),
                ),
            })?
        }
        _ => {
            return Err(ContextError::new(
                4,
                3000,
                "Unsupported feature: Unsupported codec",
            ));
        }
    };
    if info.ispe.0 != 0 && info.ispe.1 != 0 && (image.width, image.height) != info.ispe {
        return Err(ContextError::invalid(
            129,
            "Invalid image size: Decoded image does not have the size signaled in the file.",
        ));
    }
    if !options.ignore_transformations {
        for (kind, p) in container.properties(id)? {
            match &kind {
                b"irot" if p.first().is_some_and(|a| a & 3 != 0) => {
                    image = image.rotate(p[0] & 3)?;
                }
                b"imir" if !p.is_empty() => {
                    image.mirror(p[0] & 1 != 0)?;
                }
                b"clap" => {
                    let (l, r, t, b) = crate::geometry::CleanAperture::parse(p)?
                        .crop(image.width, image.height)?;
                    image = image.crop(l, r, t, b)?;
                }
                b"iscl" => {
                    return Err(ContextError::new(
                        4,
                        0,
                        "Unsupported feature: Unspecified: Image scaling (iscl) transformative property is not yet supported",
                    ));
                }
                _ => {}
            }
        }
    }
    if info.has_alpha {
        for item in container.items.values() {
            if !item
                .references
                .get(b"auxl")
                .is_some_and(|ids| ids.contains(&id))
            {
                continue;
            }
            let Ok(aux) = container.property(item.id, *b"auxC") else {
                continue;
            };
            let aux_type = aux
                .get(4..)
                .unwrap_or_default()
                .split(|b| *b == 0)
                .next()
                .unwrap_or_default();
            if !matches!(
                aux_type,
                b"urn:mpeg:avc:2015:auxid:1"
                    | b"urn:mpeg:hevc:2015:auxid:1"
                    | b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha"
            ) {
                continue;
            }
            let mut alpha = decode_native(document, item.id, options, visiting)?;
            if alpha.width != image.width || alpha.height != image.height {
                alpha = alpha.scale(image.width, image.height)?;
            }
            image.transfer_plane(&mut alpha, 0, 6)?;
            break;
        }
    }
    let bitstream_nclx = image.color.nclx;
    image.color = info.color.try_clone()?;
    if image.color.nclx.is_none_or(|n| !n.is_defined()) {
        image.color.nclx = bitstream_nclx;
    }
    image.pixel_aspect_ratio = info.pixel_aspect.unwrap_or((1, 1));
    image.premultiplied_alpha = info.premultiplied_alpha;
    visiting.ids.remove(&id);
    Ok(image)
}

fn decode_grid(
    document: &Document,
    id: u32,
    options: &DecodeOptions<'_>,
    visiting: &DecodeState,
) -> Result<Image, ContextError> {
    let container = document.container()?;
    let grid = document.images[&id]
        .grid
        .as_ref()
        .expect("loaded grid header");
    let refs = &container.items[&id].references[b"dimg"];
    for child in refs {
        if !document.images.contains_key(child) {
            return Err(ContextError::invalid(
                119,
                &format!("Missing grid images: Tile image ID={child} is not a proper image."),
            ));
        }
    }
    let (w, h) = (grid.width, grid.height);
    document.current_limits().check_image_size(w, h)?;
    if let Some(c) = options.callbacks {
        c.start(0, refs.len() as i32);
        c.progress(0, 0);
    }
    use std::collections::VecDeque;
    use std::sync::Mutex;
    let warnings = Mutex::new(Vec::<ContextError>::new());
    let canvas = Mutex::new(None::<Image>);
    let progress = Mutex::new(0i32);
    let mut tile_size = None;
    let mut validate_tile =
        |index: usize, child: u32| -> Result<Option<(u32, u32, u32)>, ContextError> {
            let info = &document.images[&child];
            if let Some(e) = &info.error {
                if options.strict || index == 0 {
                    return Err(e.clone());
                }
                warnings.lock().unwrap().push(e.clone());
                // The pinned parallel implementation leaves skipped entries empty.
                return Ok(None);
            }
            let size = (info.width, info.height);
            if u64::from(size.0) * u64::from(grid.columns) < u64::from(w)
                || u64::from(size.1) * u64::from(grid.rows) < u64::from(h)
            {
                return Err(ContextError::invalid(
                    118,
                    "Invalid grid data: Grid tiles do not cover whole image",
                ));
            }
            if tile_size.is_some_and(|s| s != size) {
                return Err(ContextError::invalid(
                    118,
                    "Invalid grid data: Grid tiles have different sizes",
                ));
            }
            tile_size = Some(size);
            Ok(Some((
                child,
                index as u32 % grid.columns * size.0,
                index as u32 / grid.columns * size.1,
            )))
        };
    let process_tile = |tile: Option<(u32, u32, u32)>| -> Result<(), ContextError> {
        let decoded = if let Some((child, x, y)) = tile {
            decode_native(document, child, options, &mut visiting.clone())
                .map(|image| (image, x, y))
        } else {
            Err(ContextError::invalid(
                119,
                "Missing grid images: Missing grid image",
            ))
        };
        match decoded {
            Ok((image, x, y)) => {
                let mut output = canvas.lock().unwrap();
                if output.is_none() {
                    *output = Some(image.empty_canvas(w, h)?);
                }
                let out = output.as_mut().unwrap();
                if out.chroma != image.chroma {
                    return Err(ContextError::invalid(
                        127,
                        "Wrong tile image chroma format: Image tile has different chroma format than combined image",
                    ));
                }
                out.paste(&image, x, y)?;
            }
            Err(error) => {
                if options.strict {
                    return Err(error);
                }
                warnings.lock().unwrap().push(error);
            }
        }
        if let Some(callbacks) = options.callbacks {
            let mut counter = progress.lock().unwrap();
            *counter += 1;
            callbacks.progress(0, *counter);
        }
        Ok(())
    };
    let mut canceled = false;
    if options.max_decoding_threads > 0 {
        let mut tiles = Vec::with_capacity(refs.len());
        for (index, &child) in refs.iter().enumerate() {
            tiles.push(validate_tile(index, child)?);
        }
        std::thread::scope(|scope| -> Result<(), ContextError> {
            let mut pending = VecDeque::new();
            let join = |job: std::thread::ScopedJoinHandle<'_, Result<(), ContextError>>| {
                job.join().unwrap_or_else(|_| {
                    Err(ContextError::new(
                        7,
                        0,
                        "Decoder plugin generated an error: Unspecified: Rust decoder panicked",
                    ))
                })
            };
            for tile in tiles {
                if pending.len() >= options.max_decoding_threads as usize {
                    let result = join(pending.pop_front().unwrap());
                    if let Err(error) = result {
                        for job in pending {
                            let _ = join(job);
                        }
                        return Err(error);
                    }
                }
                canceled = options.callbacks.is_some_and(|c| c.canceled());
                let worker = &process_tile;
                pending.push_back(scope.spawn(move || worker(tile)));
                if canceled {
                    break;
                }
            }
            while let Some(job) = pending.pop_front() {
                if let Err(error) = join(job) {
                    for job in pending {
                        let _ = join(job);
                    }
                    return Err(error);
                }
            }
            Ok(())
        })?;
    } else {
        for (index, &child) in refs.iter().enumerate() {
            let tile = validate_tile(index, child)?;
            if tile.is_none() {
                continue;
            }
            canceled = options.callbacks.is_some_and(|c| c.canceled());
            process_tile(tile)?;
            if canceled {
                break;
            }
        }
    }
    if let Some(c) = options.callbacks {
        c.end(0);
    }
    if canceled {
        return Err(ContextError::new(
            12,
            0,
            "Canceled by user: Unspecified: Decoding the image was canceled",
        ));
    }
    let warnings = warnings.into_inner().unwrap();
    if let Some(mut image) = canvas.into_inner().unwrap() {
        image.warnings.extend(warnings.into_iter().map(Into::into));
        return Ok(image);
    }
    Err(warnings.into_iter().next().unwrap_or_else(|| {
        ContextError::invalid(118, "Invalid grid data: Grid image without tiles")
    }))
}
