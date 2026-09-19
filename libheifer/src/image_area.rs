// SPDX-License-Identifier: LGPL-3.0-or-later
// Geometry and allocation semantics follow libheif 1.23.4 pixelimage.cc.
use super::{Error, Image, Plane};
use crate::security::Budget;
use std::sync::Arc;

impl Image {
    /// Replicate edge samples into physical padding. Logical image dimensions
    /// remain unchanged. Upstream exposes resized plane dimensions only when
    /// allocation is necessary; component descriptions retain their old sizes.
    pub fn extend_padding(&mut self, width: u32, height: u32) -> Result<(), Error> {
        self.extend_area(width, height, false, None)
    }

    /// Enlarge visible planes, filling new bytes with zero (128 for 8-bit Cb/Cr).
    pub fn extend_with_zero(&mut self, width: u32, height: u32) -> Result<(), Error> {
        self.extend_area(width, height, true, None)
    }

    fn extend_area(
        &mut self,
        width: u32,
        height: u32,
        zero: bool,
        budget: Option<&Arc<Budget>>,
    ) -> Result<(), Error> {
        let components = match self.chroma {
            10 | 12 | 14 => 3,
            11 | 13 | 15 => 4,
            _ => 1,
        };
        // Process in storage order: earlier planes stay changed if a later one
        // fails a size or allocation check.
        for index in 0..self.planes.len() {
            let (sx, sy) = self.subsampling(self.planes[index].channel);
            let (w, h) = (width.div_ceil(sx), height.div_ceil(sy));
            let p = &mut self.planes[index];
            if (p.width != self.width || p.height != self.height) && !matches!(p.channel, 1 | 2) {
                return Err(Error::new(
                    4,
                    0,
                    if zero {
                        c"Unsupported feature: Unspecified: Cannot extend an image with non-uniform component sizes."
                    } else {
                        c"Unsupported feature: Unspecified: Cannot extend padding for an image with non-uniform component sizes."
                    },
                ));
            }
            let (old_w, old_h, bytes) = (p.width, p.height, p.bytes_per_pixel);
            let reallocate = p.mem_width < w || p.mem_height < h;
            // Native shrinking-width memset/copies can underflow or overrun.
            // Reject these undefined inputs without exposing Rust panics to C.
            if (zero && w < old_w) || (reallocate && (w < old_w || h < old_h)) {
                return Err(Error::new(
                    5,
                    2006,
                    c"Usage error: Invalid parameter value: Cannot shrink image storage",
                ));
            }
            if reallocate {
                let mut dest = Plane::new(p.channel, w, h, p.bit_depth.into(), components, budget)?;
                dest.datatype = p.datatype;
                dest.component_ids = p.component_ids.clone();
                let row_bytes = old_w as usize * bytes;
                for y in 0..old_h as usize {
                    let from = y * p.stride;
                    let to = y * dest.stride;
                    dest.data_mut()[to..to + row_bytes]
                        .copy_from_slice(&p.data()[from..from + row_bytes]);
                }
                *p = dest;
            }
            let fill = if bytes == 1 && matches!(p.channel, 1 | 2) {
                128
            } else {
                0
            };
            for y in 0..old_h as usize {
                let row = y * p.stride;
                if zero {
                    p.data_mut()[row + old_w as usize * bytes..row + w as usize * bytes].fill(fill);
                } else {
                    let last = row + (old_w as usize - 1) * bytes;
                    for x in old_w..w {
                        p.data_mut()
                            .copy_within(last..last + bytes, row + x as usize * bytes);
                    }
                }
            }
            for y in old_h..h {
                let row = y as usize * p.stride;
                let len = w as usize * bytes;
                if zero {
                    p.data_mut()[row..row + len].fill(fill);
                } else {
                    let last = (old_h as usize - 1) * p.stride;
                    p.data_mut().copy_within(last..last + len, row);
                }
            }
            if zero {
                p.width = w;
                p.height = h;
                for id in &p.component_ids {
                    if let Some(d) = self.component_ids.find_mut(*id) {
                        d.width = w;
                        d.height = h;
                    }
                }
            }
        }
        if zero {
            self.width = width;
            self.height = height;
        }
        Ok(())
    }

    /// Extract an area using the reference's ceiling chroma offsets. Requests
    /// extending past the right or bottom edge are filled by extend_with_zero.
    pub fn extract_area(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        budget: Option<Arc<Budget>>,
    ) -> Result<Self, Error> {
        if x >= self.width || y >= self.height {
            return Err(Error::new(5, 2006, c"Usage error: Invalid parameter value: extract_image_area: top-left position is outside the image"));
        }
        if !self.standard_planes() {
            return Err(Error::new(4, 0, c"Unsupported feature: Unspecified: Extracting an area from an image with non-standard plane sizes is not supported"));
        }
        let (w, h) = (width.min(self.width - x), height.min(self.height - y));
        let mut out = Self::new(w, h, self.colorspace, self.chroma)?.with_budget(budget.clone());
        for source in &self.planes {
            let (sx, sy) = self.subsampling(source.channel);
            let bytes = (usize::from(source.bit_depth).next_power_of_two() / 8).max(1);
            let mut dest = Plane::new(
                source.channel,
                w.div_ceil(sx),
                h.div_ceil(sy),
                source.bit_depth.into(),
                source.bytes_per_pixel / bytes,
                budget.as_ref(),
            )?;
            dest.datatype = source.datatype;
            dest.component_ids = source.component_ids.clone();
            out.planes.push(dest);
        }
        out.component_ids = self.component_ids.clone();
        for d in &mut out.component_ids.descriptions {
            let (sx, sy) = self.subsampling(d.channel);
            d.width = w.div_ceil(sx);
            d.height = h.div_ceil(sy);
        }
        out.color = self.color.try_clone()?;
        out.sensor = self.sensor.clone();
        out.sample = self.sample.clone();
        out.projection = self.projection;
        out.tai_timestamp = self.tai_timestamp;
        out.pixel_aspect_ratio = self.pixel_aspect_ratio;
        out.premultiplied_alpha = self.premultiplied_alpha;
        let channels: std::collections::BTreeSet<_> = self.channels().collect();
        for channel in channels {
            let source = self.plane(channel).unwrap();
            let dest = out.plane_mut(channel).unwrap();
            let (sx, sy) = self.subsampling(channel);
            let (xs, ys) = (x.div_ceil(sx), y.div_ceil(sy));
            let copy_w = w.div_ceil(sx).min(self.width.div_ceil(sx) - xs);
            let copy_h = h.div_ceil(sy).min(self.height.div_ceil(sy) - ys);
            // Upstream narrows storage bits to uint8_t, including 256 -> 0.
            let bytes = (source.bytes_per_pixel * 8) as u8 as usize / 8;
            for row in 0..copy_h as usize {
                let from = (ys as usize + row) * source.stride + xs as usize * bytes;
                let to = row * dest.stride;
                let len = copy_w as usize * bytes;
                dest.data_mut()[to..to + len].copy_from_slice(&source.data()[from..from + len]);
            }
        }
        out.extend_area(width, height, true, budget.as_ref())?;
        Ok(out)
    }
}
