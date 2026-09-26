//! The irreversible multi-component transformation, as specified in
//! Annex G.2 and G.3.

use super::ComponentData;
use super::codestream::{Header, WaveletTransform};
use crate::error::{ColorError, Result, bail, err};
use crate::math::{Level, Simd, dispatch, f32x8};

/// Apply the inverse multi-component transform, as specified in G.2 and G.3.
pub(crate) fn apply_inverse(
    components: &mut [ComponentData],
    component_infos: &[super::codestream::ComponentInfo],
    header: &Header<'_>,
    rect: &super::rect::IntRect,
) -> Result<()> {
    if components.len() < 3 {
        return if header.strict {
            err!(ColorError::Mct)
        } else {
            Ok(())
        };
    }

    let (s, _) = components.split_at_mut(3);
    let [s0, s1, s2] = s else { unreachable!() };

    let transform = component_infos[0].wavelet_transform();

    if transform != component_infos[1].wavelet_transform()
        || component_infos[1].wavelet_transform() != component_infos[2].wavelet_transform()
    {
        bail!(ColorError::Mct);
    }

    if s0.container.len() != s1.container.len() || s1.container.len() != s2.container.len() {
        bail!(ColorError::Mct);
    }

    let size = &header.size_data;
    let width = size.image_width() as usize;
    let height = size.image_height() as usize;
    let sx = u64::from(size.x_shrink_factor) * u64::from(size.x_resolution_shrink_factor);
    let sy = u64::from(size.y_shrink_factor) * u64::from(size.y_resolution_shrink_factor);
    let x0 = (u64::from(rect.x0.saturating_sub(size.image_area_x_offset)).div_ceil(sx) as usize).min(width);
    let x1 = (u64::from(rect.x1.saturating_sub(size.image_area_x_offset)).div_ceil(sx) as usize).min(width);
    let y0 = (u64::from(rect.y0.saturating_sub(size.image_area_y_offset)).div_ceil(sy) as usize).min(height);
    let y1 = (u64::from(rect.y1.saturating_sub(size.image_area_y_offset)).div_ceil(sy) as usize).min(height);
    for y in y0..y1 {
        let range = y * width + x0..y * width + x1;
        apply_inner(transform, &mut s0.container[range.clone()], &mut s1.container[range.clone()], &mut s2.container[range]);
    }

    Ok(())
}

fn apply_inner(transform: WaveletTransform, s0: &mut [f32], s1: &mut [f32], s2: &mut [f32]) {
    dispatch!(Level::new(), simd => apply_inner_impl(simd, transform, s0, s1, s2));
}

#[inline(always)]
fn apply_inner_impl<S: Simd>(
    simd: S,
    transform: WaveletTransform,
    s0: &mut [f32],
    s1: &mut [f32],
    s2: &mut [f32],
) {
    let tail = s0.len() / 8 * 8;
    for i in tail..s0.len() {
        let (y0, y1, y2) = (s0[i], s1[i], s2[i]);
        match transform {
            WaveletTransform::Irreversible97 => {
                s0[i] = y2 * 1.402 + y0;
                s1[i] = y2 * -0.71414 + (y1 * -0.34413 + y0);
                s2[i] = y1 * 1.772 + y0;
            }
            WaveletTransform::Reversible53 => {
                let g = y0 - ((y2 + y1) * 0.25).floor();
                s0[i] = y2 + g;
                s1[i] = g;
                s2[i] = y1 + g;
            }
        }
    }
    match transform {
        // Irreversible MCT, specified in G.3.
        WaveletTransform::Irreversible97 => {
            for ((y0, y1), y2) in s0
                .chunks_exact_mut(8)
                .zip(s1.chunks_exact_mut(8))
                .zip(s2.chunks_exact_mut(8))
            {
                let y_0 = f32x8::from_slice(simd, y0);
                let y_1 = f32x8::from_slice(simd, y1);
                let y_2 = f32x8::from_slice(simd, y2);

                let i0 = y_2.mul_add(f32x8::splat(simd, 1.402), y_0);
                let i1 = y_2.mul_add(
                    f32x8::splat(simd, -0.71414),
                    y_1.mul_add(f32x8::splat(simd, -0.34413), y_0),
                );
                let i2 = y_1.mul_add(f32x8::splat(simd, 1.772), y_0);

                i0.store(y0);
                i1.store(y1);
                i2.store(y2);
            }
        }
        // Reversible MCT, specified in G.2.
        WaveletTransform::Reversible53 => {
            for ((y0, y1), y2) in s0
                .chunks_exact_mut(8)
                .zip(s1.chunks_exact_mut(8))
                .zip(s2.chunks_exact_mut(8))
            {
                let y_0 = f32x8::from_slice(simd, y0);
                let y_1 = f32x8::from_slice(simd, y1);
                let y_2 = f32x8::from_slice(simd, y2);

                let i1 = y_0 - ((y_2 + y_1) * 0.25).floor();
                let i0 = y_2 + i1;
                let i2 = y_1 + i1;

                i0.store(y0);
                i1.store(y1);
                i2.store(y2);
            }
        }
    }
}
