// SPDX-License-Identifier: LGPL-3.0-or-later
//! Incremental grid and uncompressed tile construction.
use crate::{
    context::{Context, ContextError, ImageInfo},
    derived::Grid,
    encoding::{Options, description_properties, property},
    image::Image,
    properties::Property,
};
use std::sync::Arc;
type Result<T> = std::result::Result<T, ContextError>;
#[derive(Clone)]
pub enum TileEncoder {
    Grid {
        options: Options,
        compression: i32,
        orientation: i32,
    },
    Uncompressed {
        columns: u32,
        rows: u32,
        width: u32,
        height: u32,
        compression: i32,
        colorspace: i32,
        chroma: i32,
        components: Vec<crate::components::Description>,
        tile_size: usize,
        units: Vec<(u64, u64)>,
        next_offset: u64,
        icef: Option<usize>,
    },
}
fn ispe(w: u32, h: u32) -> Property {
    let mut d = vec![0; 4];
    d.extend(w.to_be_bytes());
    d.extend(h.to_be_bytes());
    property(*b"ispe", d)
}
fn color_properties(image: &Image, options: &Options) -> Vec<Property> {
    let mut result = Vec::new();
    if let Some(raw) = &image.color.raw {
        let mut d = raw.profile_type.to_be_bytes().to_vec();
        d.extend(&raw.data);
        result.push(property(*b"colr", d));
    }
    if let Some(n) = options.nclx
        && !options.no_nclx
        && (image.color.raw.is_none() || options.two_profiles)
    {
        let mut d = b"nclx".to_vec();
        d.extend(n.primaries.to_be_bytes());
        d.extend(n.transfer.to_be_bytes());
        d.extend(n.matrix.to_be_bytes());
        d.push(u8::from(n.full_range) << 7);
        result.push(property(*b"colr", d));
    }
    result
}
impl Context {
    pub fn encode_format(
        &mut self,
        image: &Image,
        format: i32,
        options: &Options,
        compression: i32,
    ) -> Result<Arc<ImageInfo>> {
        match format {
            8 => self.encode_uncompressed(image, options, compression),
            9 => self.encode_mask(image, options),
            _ => Err(ContextError::new(
                4,
                6003,
                "Unsupported feature: Support for this compression format has not been built in",
            )),
        }
    }
    pub(crate) fn add_orientation(&mut self, id: u32, orientation: i32) -> Result<()> {
        let (r, m) = match orientation {
            2 => (0, Some(1)),
            3 => (2, None),
            4 => (0, Some(0)),
            5 => (3, Some(1)),
            6 => (3, None),
            7 => (3, Some(0)),
            8 => (1, None),
            _ => (0, None),
        };
        if r != 0 {
            self.properties
                .add_to_file(id, property(*b"irot", vec![r]), true)?;
        }
        if let Some(m) = m {
            self.properties
                .add_to_file(id, property(*b"imir", vec![m]), true)?;
        }
        Ok(())
    }
    fn retained_property(&mut self, image: &ImageInfo, p: Property, essential: bool) -> Result<()> {
        // Native image objects retain their own box pointers even when the file deduplicates.
        let owned = Arc::new(p.clone());
        self.properties.add_to_file(image.id, p, essential)?;
        image.retained_properties.lock().unwrap().push(owned);
        Ok(())
    }
    pub fn add_grid(
        &mut self,
        size: (u32, u32),
        columns: u32,
        rows: u32,
        options: Options,
        compression: i32,
        full_tiles: Option<&[u32]>,
    ) -> Result<Arc<ImageInfo>> {
        let full = full_tiles.is_some();
        if !full && u64::from(columns) * u64::from(rows) > 65535 {
            return Err(ContextError::new(
                5,
                0,
                "Usage error: Unspecified: Too many tiles (maximum: 65535)",
            ));
        }
        let (w, h) = size;
        let wide = w > 65535 || h > 65535;
        let mut data = vec![0, u8::from(wide), (rows - 1) as u8, (columns - 1) as u8];
        for n in [w, h] {
            crate::writing::number(&mut data, u64::from(n), if wide { 4 } else { 2 });
        }
        self.items.layout.lock().unwrap().init_image();
        let mut item = crate::items::Item::new(*b"grid");
        item.hidden = false;
        let id = self.items.add_pending(item)?;
        self.items.append_written(id, 1, data)?;
        let tiles = full_tiles.map_or_else(|| vec![0; (rows * columns) as usize], <[u32]>::to_vec);
        self.items.add_reference(crate::items::Reference {
            from: id,
            kind: u32::from_be_bytes(*b"dimg"),
            to: tiles.clone(),
        });
        self.properties.add_to_file(id, ispe(w, h), false)?;
        let mut info = ImageInfo::new(id, *b"grid", if full { (w, h) } else { (0, 0) }, Vec::new());
        if full {
            info.retained_properties
                .get_mut()
                .unwrap()
                .push(Arc::new(ispe(w, h)));
        } else {
            info.width = w;
            info.height = h;
            info.grid = Some(Grid {
                columns,
                rows,
                width: w,
                height: h,
            });
            *info.grid_tiles.get_mut().unwrap() = tiles;
            let orientation = options.orientation;
            let mut options = options;
            options.orientation = 1;
            *info.tile_encoder.get_mut().unwrap() = Some(TileEncoder::Grid {
                options,
                compression,
                orientation,
            });
        }
        Ok(self.register_image(info))
    }
    pub fn finish_full_grid(
        &mut self,
        grid: &ImageInfo,
        first: &Image,
        first_id: u32,
    ) -> Result<()> {
        if let Some(p) = self
            .properties
            .get(first_id)?
            .into_iter()
            .find(|p| p.kind == *b"pixi")
            .cloned()
        {
            self.retained_property(grid, p, true)?;
        }
        let mut props = description_properties(first);
        let split = props
            .iter()
            .position(|p| p.kind == *b"prfr" || p.uuid == Some(crate::gimi::COMPONENT_UUID))
            .unwrap_or(props.len());
        let mut profiles = color_properties(
            first,
            &Options {
                nclx: first.color.nclx,
                two_profiles: true,
                ..Default::default()
            },
        );
        profiles.reverse();
        props.splice(split..split, profiles);
        for p in props {
            let essential = false;
            self.retained_property(grid, p, essential)?;
        }
        Ok(())
    }
    pub fn add_grid_tile(
        &mut self,
        grid: &ImageInfo,
        x: u32,
        y: u32,
        image: &Image,
        format: i32,
    ) -> Result<()> {
        let encoder = grid.tile_encoder.lock().unwrap().clone();
        let Some(TileEncoder::Grid {
            options,
            compression,
            orientation,
        }) = encoder
        else {
            return Err(ContextError::new(
                5,
                0,
                "Cannot add tile to a non-tiled image",
            ));
        };
        let tile = self.encode_format(image, format, &options, compression)?;
        self.items.items.get_mut(&tile.id).unwrap().hidden = true;
        let columns = grid.grid.unwrap().columns;
        let index = u64::from(y) * u64::from(columns) + u64::from(x);
        let mut ids = grid.grid_tiles.lock().unwrap();
        let slot = ids
            .get_mut(index as usize)
            .ok_or_else(|| ContextError::new(5, 2006, "Usage error: Invalid parameter value"))?;
        *slot = tile.id;
        drop(ids);
        if let Some(r) = self
            .items
            .references
            .iter_mut()
            .find(|r| r.from == grid.id && r.kind == u32::from_be_bytes(*b"dimg"))
        {
            r.to[index as usize] = tile.id;
        }
        if let Some(p) = self
            .properties
            .get(tile.id)?
            .into_iter()
            .find(|p| p.kind == *b"pixi")
            .cloned()
        {
            self.retained_property(grid, p, true)?;
        }
        if x == 0 && y == 0 {
            for p in description_properties(image) {
                let essential = false;
                self.retained_property(grid, p, essential)?;
            }
            for p in color_properties(image, &options) {
                self.retained_property(grid, p, false)?;
            }
            self.add_orientation(grid.id, orientation)?;
        }
        Ok(())
    }
    pub fn add_empty_uncompressed(
        &mut self,
        size: (u32, u32),
        tile: (u32, u32),
        compression: i32,
        options: &Options,
        prototype: &Image,
    ) -> Result<Arc<ImageInfo>> {
        let (w, h) = size;
        let (tw, th) = tile;
        if tw == 0 || th == 0 || w % tw != 0 || h % th != 0 {
            return Err(ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value: ISO 23001-17 image size must be an integer multiple of the tile size.",
            ));
        }
        let (cols, rows) = (w / tw, h / th);
        if cols == 0 || rows == 0 {
            return Err(ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value",
            ));
        }
        let scaled;
        let shape = if (prototype.width, prototype.height) == tile {
            prototype
        } else {
            scaled = prototype.scale(tw, th)?;
            &scaled
        };
        let (data, props) = crate::uncompressed_encode::encode_tiled(shape, cols, rows)?;
        self.items.layout.lock().unwrap().init_image();
        let mut item = crate::items::Item::new(*b"unci");
        item.hidden = false;
        let id = self.items.add_pending(item)?;
        let mut info = ImageInfo::new(id, *b"unci", (w, h), Vec::new());
        info.width = w;
        info.height = h;
        for (p, e) in props {
            self.retained_property(&info, p, e)?;
        }
        self.retained_property(&info, ispe(w, h), true)?;
        let icef = if compression != 0 {
            let kind = match compression {
                3 => *b"defl",
                4 => *b"zlib",
                5 => *b"brot",
                _ => [0; 4],
            };
            let mut d = vec![0; 4];
            d.extend(kind);
            d.push(2);
            self.retained_property(&info, property(*b"cmpC", d), true)?;
            let p = Arc::new(property(*b"icef", vec![0; 9]));
            let index = self.properties.boxes.len();
            self.properties.boxes.push(p.clone());
            self.properties.items.entry(id).or_default().push(index);
            self.properties.essential.entry(id).or_default().push(true);
            info.retained_properties.get_mut().unwrap().push(p);
            Some(index)
        } else {
            None
        };
        self.add_orientation(id, options.orientation)?;
        if compression == 0 {
            let len = data
                .len()
                .checked_mul(cols as usize)
                .and_then(|n| n.checked_mul(rows as usize))
                .ok_or(crate::error::Error::ALLOCATION)?;
            let mut zeros = Vec::new();
            zeros
                .try_reserve_exact(len)
                .map_err(|_| crate::error::Error::ALLOCATION)?;
            zeros.resize(len, 0);
            self.items.append_written(id, 0, zeros)?;
        }
        *info.tile_encoder.get_mut().unwrap() = Some(TileEncoder::Uncompressed {
            columns: cols,
            rows,
            width: tw,
            height: th,
            compression,
            colorspace: prototype.colorspace,
            chroma: prototype.chroma,
            components: prototype.component_ids.descriptions.clone(),
            tile_size: data.len(),
            units: Vec::new(),
            next_offset: 0,
            icef,
        });
        Ok(self.register_image(info))
    }
    pub fn add_uncompressed_tile(
        &mut self,
        info: &ImageInfo,
        x: u32,
        y: u32,
        image: &Image,
    ) -> Result<()> {
        let mut encoder = info.tile_encoder.lock().unwrap();
        let Some(TileEncoder::Uncompressed {
            columns,
            rows,
            width,
            height,
            compression,
            colorspace,
            chroma,
            components,
            tile_size,
            units,
            next_offset,
            icef,
        }) = encoder.as_mut()
        else {
            return Err(ContextError::new(
                5,
                0,
                "Cannot add tile to a non-tiled image",
            ));
        };
        if x >= *columns || y >= *rows {
            return Err(ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value: tile_x and/or tile_y are out of range.",
            ));
        }
        if (image.width, image.height) != (*width, *height) {
            return Err(ContextError::new(
                5,
                2006,
                "Usage error: Invalid parameter value: Tile image size does not match the tile size of the uncompressed image.",
            ));
        }
        if image.colorspace != *colorspace
            || image.chroma != *chroma
            || image.component_ids.descriptions.len() != components.len()
            || components.iter().any(|p| {
                image.component_ids.find(p.id).is_none_or(|d| {
                    d.channel != p.channel
                        || d.kind != p.kind
                        || d.bit_depth != p.bit_depth
                        || d.datatype != p.datatype
                        || d.has_data != p.has_data
                })
            })
        {
            return Err(ContextError::invalid(
                0,
                "Unspecified: Image does not match the component configuration of the uncompressed image it is added to",
            ));
        }
        let (data, _) = crate::uncompressed_encode::encode(image, 0)?;
        let index = (y * *columns + x) as usize;
        if *compression == 0 {
            let loc = self
                .items
                .locations
                .get_mut(&info.id)
                .ok_or_else(|| ContextError::invalid(117, "Item has no data"))?;
            let bytes = Arc::make_mut(loc.owned.as_mut().unwrap());
            let start = index * *tile_size;
            let end = start
                .checked_add(data.len())
                .ok_or(crate::error::Error::ALLOCATION)?;
            bytes
                .get_mut(start..end)
                .ok_or(crate::error::Error::ALLOCATION)?
                .copy_from_slice(&data);
        } else {
            if !matches!(*compression, 3 | 4) {
                return Err(ContextError::new(
                    4,
                    3006,
                    "Unsupported feature: Unsupported generic compression method: Unsupported unci compression method.",
                ));
            }
            let data = crate::compression::compress(&data, *compression)?;
            let size = data.len() as u64;
            self.items.append_written(info.id, 0, data)?;
            units.resize(units.len().max(index + 1), (0, 0));
            units[index] = (*next_offset, size);
            *next_offset += size;
            let encoded = icef_data(units);
            let index = icef.unwrap();
            let old = self.properties.boxes[index].clone();
            let p = Arc::new(property(*b"icef", encoded));
            self.properties.boxes[index] = p.clone();
            let mut retained = info.retained_properties.lock().unwrap();
            if let Some(slot) = retained.iter_mut().find(|p| Arc::ptr_eq(p, &old)) {
                *slot = p;
            }
        }
        Ok(())
    }
}
fn icef_data(units: &[(u64, u64)]) -> Vec<u8> {
    fn code(n: u64, offset: bool) -> u8 {
        if !offset && n <= 255 {
            0
        } else if n <= 65535 {
            1
        } else if n <= 0xffffff {
            2
        } else if n <= 0xffffffff {
            3
        } else {
            4
        }
    }
    let mut end = 0;
    let mut implicit = true;
    let mut oc = 1;
    let mut sc = 0;
    for &(o, n) in units {
        implicit &= o == end;
        end = end.saturating_add(n);
        oc = oc.max(code(o, true));
        sc = sc.max(code(n, false));
    }
    if implicit {
        oc = 0;
    }
    let mut d = vec![0; 4];
    d.push((oc << 5) | (sc << 2));
    d.extend((units.len() as u32).to_be_bytes());
    for &(o, n) in units {
        if oc != 0 {
            crate::writing::number(&mut d, o, if oc == 4 { 8 } else { usize::from(oc) + 1 });
        }
        crate::writing::number(&mut d, n, if sc == 4 { 8 } else { usize::from(sc) + 1 });
    }
    d
}
