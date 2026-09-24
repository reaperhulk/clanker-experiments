// SPDX-License-Identifier: LGPL-3.0-or-later
//! Sample adaptive offset (H.266 clause 8.8.4), following vvdec's
//! `SampleAdaptiveOffset::offsetBlock_core`.
use super::pic::{Picture, Plane, SaoParam};
use super::ps::{Pps, PicHeader, Sps};

fn sgn(v: i32) -> i32 {
    v.signum()
}

#[derive(Clone, Copy, Default)]
struct Avail {
    l: bool,
    r: bool,
    a: bool,
    b: bool,
    al: bool,
    ar: bool,
    bl: bool,
    br: bool,
}

struct Vb<'a> {
    ver: &'a [i32],
    hor: &'a [i32],
}

impl Vb<'_> {
    fn off(&self, x: i32, y: i32, use_ver: bool, use_hor: bool) -> bool {
        (use_ver && self.ver.iter().any(|&p| x == p || x == p - 1)) || (use_hor && self.hor.iter().any(|&p| y == p || y == p - 1))
    }
    fn any(&self) -> bool {
        !self.ver.is_empty() || !self.hor.is_empty()
    }
}

/// vvdec's `offsetBlock_core`; `src` is the deblocked picture, `dst` the
/// output, both addressed at the block origin `(bx, by)`.
#[allow(clippy::too_many_arguments)]
fn offset_block(bd: u32, type_idc: u8, offsets: &[i32; 32], src: &Plane, dst: &mut Plane, bx: i32, by: i32, w: i32, h: i32, av: Avail, vb: &Vb) {
    let max = (1i32 << bd) - 1;
    let s = |x: i32, y: i32| i32::from(src.at(bx + x, by + y));
    let mut put = |x: i32, y: i32, v: i32| dst.set(bx + x, by + y, v.clamp(0, max) as i16);
    let crossed = vb.any();
    // edge offsets are indexed by edgeType + 2
    let eo = |e: i32| offsets[(e + 2) as usize];
    let wu = (w + 2) as usize;
    match type_idc {
        1 => {
            let sx = if av.l { 0 } else { 1 };
            let ex = if av.r { w } else { w - 1 };
            for y in 0..h {
                let mut sl = sgn(s(sx, y) - s(sx - 1, y));
                for x in sx..ex {
                    let sr = sgn(s(x, y) - s(x + 1, y));
                    if crossed && vb.off(x, y, true, false) {
                        sl = -sr;
                        continue;
                    }
                    let e = sr + sl;
                    sl = -sr;
                    put(x, y, s(x, y) + eo(e));
                }
            }
        }
        2 => {
            let mut up = vec![0i32; wu];
            let sy = if av.a { 0 } else { 1 };
            let ey = if av.b { h } else { h - 1 };
            for x in 0..w {
                up[x as usize] = sgn(s(x, sy) - s(x, sy - 1));
            }
            for y in sy..ey {
                for x in 0..w {
                    let sd = sgn(s(x, y) - s(x, y + 1));
                    if crossed && vb.off(x, y, false, true) {
                        up[x as usize] = -sd;
                        continue;
                    }
                    let e = sd + up[x as usize];
                    up[x as usize] = -sd;
                    put(x, y, s(x, y) + eo(e));
                }
            }
        }
        3 => {
            // 135 degrees; index lines with a +1 bias so x - 1 stays valid.
            let mut up = vec![0i32; wu + 1];
            let mut down = vec![0i32; wu + 1];
            let ix = |x: i32| (x + 1) as usize;
            let sx = if av.l { 0 } else { 1 };
            let ex = if av.r { w } else { w - 1 };
            for x in sx..ex + 1 {
                up[ix(x)] = sgn(s(x, 1) - s(x - 1, 0));
            }
            let fsx = if av.al { 0 } else { 1 };
            let fex = if av.a { ex } else { 1 };
            for x in fsx..fex {
                if crossed && vb.off(x, 0, true, true) {
                    continue;
                }
                let e = sgn(s(x, 0) - s(x - 1, -1)) - up[ix(x + 1)];
                put(x, 0, s(x, 0) + eo(e));
            }
            for y in 1..h - 1 {
                for x in sx..ex {
                    let sd = sgn(s(x, y) - s(x + 1, y + 1));
                    if crossed && vb.off(x, y, true, true) {
                        down[ix(x + 1)] = -sd;
                        continue;
                    }
                    let e = sd + up[ix(x)];
                    put(x, y, s(x, y) + eo(e));
                    down[ix(x + 1)] = -sd;
                }
                down[ix(sx)] = sgn(s(sx, y + 1) - s(sx - 1, y));
                std::mem::swap(&mut up, &mut down);
            }
            let y = h - 1;
            let lsx = if av.b { sx } else { w - 1 };
            let lex = if av.br { w } else { w - 1 };
            for x in lsx..lex {
                if crossed && vb.off(x, y, true, true) {
                    continue;
                }
                let e = sgn(s(x, y) - s(x + 1, y + 1)) + up[ix(x)];
                put(x, y, s(x, y) + eo(e));
            }
        }
        4 => {
            // 45 degrees; vvdec's sign line starts at index 1 (x - 1 valid).
            let mut up = vec![0i32; wu + 1];
            let ix = |x: i32| (x + 1) as usize;
            let sx = if av.l { 0 } else { 1 };
            let ex = if av.r { w } else { w - 1 };
            for x in sx - 1..ex {
                up[ix(x)] = sgn(s(x, 1) - s(x + 1, 0));
            }
            let fsx = if av.a { sx } else { w - 1 };
            let fex = if av.ar { w } else { w - 1 };
            for x in fsx..fex {
                if crossed && vb.off(x, 0, true, true) {
                    continue;
                }
                let e = sgn(s(x, 0) - s(x + 1, -1)) - up[ix(x - 1)];
                put(x, 0, s(x, 0) + eo(e));
            }
            for y in 1..h - 1 {
                for x in sx..ex {
                    let sd = sgn(s(x, y) - s(x - 1, y + 1));
                    if crossed && vb.off(x, y, true, true) {
                        up[ix(x - 1)] = -sd;
                        continue;
                    }
                    let e = sd + up[ix(x)];
                    put(x, y, s(x, y) + eo(e));
                    up[ix(x - 1)] = -sd;
                }
                up[ix(ex - 1)] = sgn(s(ex - 1, y + 1) - s(ex, y));
            }
            let y = h - 1;
            let lsx = if av.bl { 0 } else { 1 };
            let lex = if av.b { ex } else { 1 };
            for x in lsx..lex {
                if crossed && vb.off(x, y, true, true) {
                    continue;
                }
                let e = sgn(s(x, y) - s(x - 1, y + 1)) + up[ix(x)];
                put(x, y, s(x, y) + eo(e));
            }
        }
        _ => {
            let shift = bd - 5;
            for y in 0..h {
                for x in 0..w {
                    let v = s(x, y);
                    put(x, y, v + offsets[(v >> shift) as usize]);
                }
            }
        }
    }
}

/// Applies SAO to the whole (deblocked) picture.
pub fn sao(pic: &mut Picture, sps: &Sps, pps: &Pps, ph: &PicHeader) {
    let n = pic.ctus.len();
    let wc = pic.width_ctus as usize;
    // reconstruct merged parameters in raster order
    let mut params: Vec<[SaoParam; 3]> = pic.ctus.iter().map(|c| c.sao).collect();
    let size = 1i32 << pic.ctu_log2;
    for addr in 0..n {
        let (cx, cy) = (addr % wc, addr / wc);
        for comp in 0..pic.fmt.num_comp() {
            let p = params[addr][comp];
            if p.mode == 2 {
                let src = if p.type_idc == 0 { addr.checked_sub(1).filter(|_| cx > 0) } else { addr.checked_sub(wc).filter(|_| cy > 0) };
                params[addr][comp] = src.map_or(SaoParam::default(), |s| params[s][comp]);
            }
        }
    }
    if params.iter().all(|p| p.iter().all(|c| c.mode == 0)) {
        return;
    }
    let src = pic.planes.clone();
    let step_log2 = pic.bit_depth.saturating_sub(10);
    let ctu_slice = |a: usize| pic.ctus[a].slice.unwrap_or(0);
    let ctu_tile = |a: usize| pic.ctus[a].tile;
    let subpic = |a: usize| -> usize {
        let (x, y) = ((a % wc) as u32, (a / wc) as u32);
        (0..sps.num_subpics as usize)
            .find(|&i| x >= sps.subpic_x[i] && x < sps.subpic_x[i] + sps.subpic_w[i] && y >= sps.subpic_y[i] && y < sps.subpic_y[i] + sps.subpic_h[i])
            .unwrap_or(0)
    };
    let hc = pic.height_ctus as usize;
    for addr in 0..n {
        if params[addr].iter().all(|c| c.mode == 0) {
            continue;
        }
        let (cx, cy) = (addr % wc, addr / wc);
        let nb = |dx: i32, dy: i32| -> Option<usize> {
            let (x, y) = (cx as i32 + dx, cy as i32 + dy);
            if x < 0 || y < 0 || x >= wc as i32 || y >= hc as i32 { None } else { Some(y as usize * wc + x as usize) }
        };
        let (l, r, a, b) = (nb(-1, 0), nb(1, 0), nb(0, -1), nb(0, 1));
        let al = if l.is_some() && a.is_some() { nb(-1, -1) } else { None };
        let ar = if r.is_some() && a.is_some() { nb(1, -1) } else { None };
        let bl = if l.is_some() && b.is_some() { nb(-1, 1) } else { None };
        let br = if r.is_some() && b.is_some() { nb(1, 1) } else { None };
        let ok = |o: Option<usize>| -> bool {
            let Some(o) = o else { return false };
            (pps.loop_filter_across_slices || ctu_slice(o) == ctu_slice(addr))
                && (pps.loop_filter_across_tiles || ctu_tile(o) == ctu_tile(addr))
                && (!sps.subpic_info_present || sps.loop_filter_across_subpic[subpic(addr)] || subpic(o) == subpic(addr))
        };
        let av = Avail { l: ok(l), r: ok(r), a: ok(a), b: ok(b), al: ok(al), ar: ok(ar), bl: ok(bl), br: ok(br) };
        let (x0, y0) = (cx as i32 * size, cy as i32 * size);
        let (w, h) = (size.min(pic.width - x0), size.min(pic.height - y0));
        let mut vb_ver = Vec::new();
        let mut vb_hor = Vec::new();
        if ph.vb_present {
            for &p in &ph.vb_pos_y {
                let p = p as i32;
                if y0 <= p && p <= y0 + h {
                    vb_hor.push(p);
                }
            }
            for &p in &ph.vb_pos_x {
                let p = p as i32;
                if x0 <= p && p <= x0 + w {
                    vb_ver.push(p);
                }
            }
        }
        for comp in 0..pic.fmt.num_comp() {
            let p = params[addr][comp];
            if p.mode == 0 {
                continue;
            }
            let (sx, sy) = pic.fmt.scale(comp);
            let (bx, by, bw, bh) = (x0 >> sx, y0 >> sy, w >> sx, h >> sy);
            let ver: Vec<i32> = vb_ver.iter().map(|&v| (v >> sx) - bx).collect();
            let hor: Vec<i32> = vb_hor.iter().map(|&v| (v >> sy) - by).collect();
            let mut offs = [0i32; 32];
            if p.type_idc == 0 {
                for i in 0..4 {
                    offs[(p.band_pos as usize + i) % 32] = p.offset[i] << step_log2;
                }
            } else {
                for i in 0..5 {
                    offs[i] = p.offset[i] << step_log2;
                }
            }
            offset_block(pic.bit_depth, p.type_idc, &offs, &src[comp], &mut pic.planes[comp], bx, by, bw, bh, av, &Vb { ver: &ver, hor: &hor });
        }
    }
}
