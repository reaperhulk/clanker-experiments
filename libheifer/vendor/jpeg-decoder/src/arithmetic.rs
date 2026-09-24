// libheifer: arithmetic entropy decoding (Recommendation ITU-T T.81 Annex D and
// F.1.4.4 / G.1.3), ported from libjpeg-turbo's jdarith.c and jaricom.c
// (developed 1997-2015 by Guido Vollbeding for the Independent JPEG Group;
// libjpeg-turbo modifications by D. R. Commander). See README.ijg.
use crate::error::Result;
use crate::marker::Marker;
use crate::decoder::UNZIGZAG;
use crate::read_u8;
use std::io::Read;

/// Table D.2 in libjpeg's compact form: Qe << 16 | Next_Index_MPS << 8 |
/// Switch_MPS << 7 | Next_Index_LPS.
const fn v(qe: u32, lps: u32, mps: u32, switch: u32) -> u32 {
    (qe << 16) | (mps << 8) | (switch << 7) | lps
}

static ARITAB: [u32; 114] = [
    v(0x5a1d, 1, 1, 1), v(0x2586, 14, 2, 0), v(0x1114, 16, 3, 0), v(0x080b, 18, 4, 0),
    v(0x03d8, 20, 5, 0), v(0x01da, 23, 6, 0), v(0x00e5, 25, 7, 0), v(0x006f, 28, 8, 0),
    v(0x0036, 30, 9, 0), v(0x001a, 33, 10, 0), v(0x000d, 35, 11, 0), v(0x0006, 9, 12, 0),
    v(0x0003, 10, 13, 0), v(0x0001, 12, 13, 0), v(0x5a7f, 15, 15, 1), v(0x3f25, 36, 16, 0),
    v(0x2cf2, 38, 17, 0), v(0x207c, 39, 18, 0), v(0x17b9, 40, 19, 0), v(0x1182, 42, 20, 0),
    v(0x0cef, 43, 21, 0), v(0x09a1, 45, 22, 0), v(0x072f, 46, 23, 0), v(0x055c, 48, 24, 0),
    v(0x0406, 49, 25, 0), v(0x0303, 51, 26, 0), v(0x0240, 52, 27, 0), v(0x01b1, 54, 28, 0),
    v(0x0144, 56, 29, 0), v(0x00f5, 57, 30, 0), v(0x00b7, 59, 31, 0), v(0x008a, 60, 32, 0),
    v(0x0068, 62, 33, 0), v(0x004e, 63, 34, 0), v(0x003b, 32, 35, 0), v(0x002c, 33, 9, 0),
    v(0x5ae1, 37, 37, 1), v(0x484c, 64, 38, 0), v(0x3a0d, 65, 39, 0), v(0x2ef1, 67, 40, 0),
    v(0x261f, 68, 41, 0), v(0x1f33, 69, 42, 0), v(0x19a8, 70, 43, 0), v(0x1518, 72, 44, 0),
    v(0x1177, 73, 45, 0), v(0x0e74, 74, 46, 0), v(0x0bfb, 75, 47, 0), v(0x09f8, 77, 48, 0),
    v(0x0861, 78, 49, 0), v(0x0706, 79, 50, 0), v(0x05cd, 48, 51, 0), v(0x04de, 50, 52, 0),
    v(0x040f, 50, 53, 0), v(0x0363, 51, 54, 0), v(0x02d4, 52, 55, 0), v(0x025c, 53, 56, 0),
    v(0x01f8, 54, 57, 0), v(0x01a4, 55, 58, 0), v(0x0160, 56, 59, 0), v(0x0125, 57, 60, 0),
    v(0x00f6, 58, 61, 0), v(0x00cb, 59, 62, 0), v(0x00ab, 61, 63, 0), v(0x008f, 61, 32, 0),
    v(0x5b12, 65, 65, 1), v(0x4d04, 80, 66, 0), v(0x412c, 81, 67, 0), v(0x37d8, 82, 68, 0),
    v(0x2fe8, 83, 69, 0), v(0x293c, 84, 70, 0), v(0x2379, 86, 71, 0), v(0x1edf, 87, 72, 0),
    v(0x1aa9, 87, 73, 0), v(0x174e, 72, 74, 0), v(0x1424, 72, 75, 0), v(0x119c, 74, 76, 0),
    v(0x0f6b, 74, 77, 0), v(0x0d51, 75, 78, 0), v(0x0bb6, 77, 79, 0), v(0x0a40, 77, 48, 0),
    v(0x5832, 80, 81, 1), v(0x4d1c, 88, 82, 0), v(0x438e, 89, 83, 0), v(0x3bdd, 90, 84, 0),
    v(0x34ee, 91, 85, 0), v(0x2eae, 92, 86, 0), v(0x299a, 93, 87, 0), v(0x2516, 86, 71, 0),
    v(0x5570, 88, 89, 1), v(0x4ca9, 95, 90, 0), v(0x44d9, 96, 91, 0), v(0x3e22, 97, 92, 0),
    v(0x3824, 99, 93, 0), v(0x32b4, 99, 94, 0), v(0x2e17, 93, 86, 0), v(0x56a8, 95, 96, 1),
    v(0x4f46, 101, 97, 0), v(0x47e5, 102, 98, 0), v(0x41cf, 103, 99, 0), v(0x3c3d, 104, 100, 0),
    v(0x375e, 99, 93, 0), v(0x5231, 105, 102, 0), v(0x4c0f, 106, 103, 0), v(0x4639, 107, 104, 0),
    v(0x415e, 103, 99, 0), v(0x5627, 105, 106, 1), v(0x50e7, 108, 107, 0), v(0x4b85, 109, 103, 0),
    v(0x5597, 110, 109, 0), v(0x504f, 111, 107, 0), v(0x5a10, 110, 111, 1), v(0x5522, 112, 109, 0),
    v(0x59eb, 112, 111, 1), v(0x5a1d, 113, 113, 0),
];

/// DAC conditioning (defaults L = 0, U = 1, K = 5 per table).
#[derive(Clone, Copy)]
pub(crate) struct Conditioning {
    pub dc_l: [u8; 16],
    pub dc_u: [u8; 16],
    pub ac_k: [u8; 16],
}

impl Default for Conditioning {
    fn default() -> Self {
        Conditioning { dc_l: [0; 16], dc_u: [1; 16], ac_k: [5; 16] }
    }
}

/// Where a statistics bin lives.
#[derive(Clone, Copy)]
enum Bin {
    Dc(usize, usize),
    Ac(usize, usize),
    Fixed,
}

pub(crate) struct ArithmeticDecoder {
    c: i64,
    a: i64,
    /// Bit shift counter: -16 before the two initial bytes, 0..7 while
    /// running, -1 after a code error (the rest of the scan decodes nothing).
    ct: i32,
    /// A marker met in the entropy-coded data; zero data is supplied after it.
    pub marker: Option<Marker>,
    dc_stats: [[u8; 64]; 16],
    ac_stats: [[u8; 256]; 16],
    fixed_bin: u8,
    pub last_dc_val: [i32; 4],
    pub dc_context: [usize; 4],
}

impl ArithmeticDecoder {
    pub fn new() -> Self {
        ArithmeticDecoder {
            c: 0,
            a: 0,
            ct: -16,
            marker: None,
            dc_stats: [[0; 64]; 16],
            ac_stats: [[0; 256]; 16],
            fixed_bin: 113,
            last_dc_val: [0; 4],
            dc_context: [0; 4],
        }
    }

    /// `start_pass` / `process_restart`: clear the statistics the scan uses
    /// and the DC predictions, and refill C from two new bytes.
    pub fn reset(&mut self, dc_tables: &[usize], ac_tables: &[usize], dc: bool, ac: bool) {
        for (ci, (&d, &a)) in dc_tables.iter().zip(ac_tables).enumerate() {
            if dc {
                self.dc_stats[d] = [0; 64];
                self.last_dc_val[ci] = 0;
                self.dc_context[ci] = 0;
            }
            if ac {
                self.ac_stats[a] = [0; 256];
            }
        }
        self.c = 0;
        self.a = 0;
        self.ct = -16;
        self.marker = None;
    }

    pub fn failed(&self) -> bool {
        self.ct == -1
    }

    fn fail(&mut self) {
        // JWRN_ARITH_BAD_CODE
        self.ct = -1;
    }

    fn stat(&mut self, bin: Bin) -> &mut u8 {
        match bin {
            Bin::Dc(t, i) => &mut self.dc_stats[t][i],
            Bin::Ac(t, i) => &mut self.ac_stats[t][i],
            Bin::Fixed => &mut self.fixed_bin,
        }
    }

    /// `arith_decode`: one binary decision with adaptive probability.
    fn decode<R: Read>(&mut self, reader: &mut R, bin: Bin) -> Result<u8> {
        while self.a < 0x8000 {
            self.ct -= 1;
            if self.ct < 0 {
                let data = if self.marker.is_some() {
                    0
                } else {
                    let mut data = read_u8(reader)?;
                    if data == 0xFF {
                        loop {
                            data = read_u8(reader)?;
                            if data != 0xFF {
                                break;
                            }
                        }
                        if data == 0 {
                            data = 0xFF;
                        } else {
                            // A marker is legal here: supply zero data from now on.
                            self.marker = Marker::from_u8(data);
                            data = 0;
                        }
                    }
                    data
                };
                self.c = (self.c << 8) | i64::from(data);
                self.ct += 8;
                if self.ct < 0 {
                    self.ct += 1;
                    if self.ct == 0 {
                        // Two initial bytes read: re-initialize A.
                        self.a = 0x8000;
                    }
                }
            }
            self.a <<= 1;
        }
        let st = self.stat(bin);
        let sv = *st;
        let mut qe = ARITAB[usize::from(sv & 0x7F)];
        let nl = (qe & 0xFF) as u8;
        qe >>= 8;
        let nm = (qe & 0xFF) as u8;
        qe >>= 8;
        let qe = i64::from(qe);
        let mut sv = sv;
        let mut temp = self.a - qe;
        self.a = temp;
        temp <<= self.ct;
        let next;
        if self.c >= temp {
            self.c -= temp;
            if self.a < qe {
                self.a = qe;
                next = (sv & 0x80) ^ nm;
            } else {
                self.a = qe;
                next = (sv & 0x80) ^ nl;
                sv ^= 0x80;
            }
        } else if self.a < 0x8000 {
            if self.a < qe {
                next = (sv & 0x80) ^ nl;
                sv ^= 0x80;
            } else {
                next = (sv & 0x80) ^ nm;
            }
        } else {
            return Ok(sv >> 7);
        }
        *self.stat(bin) = next;
        Ok(sv >> 7)
    }

    /// Figures F.19 and F.21-F.24: one DC difference; updates the prediction
    /// and conditioning. Returns false after a code error.
    fn dc_diff<R: Read>(
        &mut self,
        reader: &mut R,
        ci: usize,
        tbl: usize,
        cond: &Conditioning,
    ) -> Result<bool> {
        let s0 = self.dc_context[ci];
        if self.decode(reader, Bin::Dc(tbl, s0))? == 0 {
            self.dc_context[ci] = 0;
            return Ok(true);
        }
        let sign = usize::from(self.decode(reader, Bin::Dc(tbl, s0 + 1))?);
        let mut st = s0 + 2 + sign;
        let mut m: i32 = i32::from(self.decode(reader, Bin::Dc(tbl, st))?);
        if m != 0 {
            st = 20;
            while self.decode(reader, Bin::Dc(tbl, st))? != 0 {
                m <<= 1;
                if m == 0x8000 {
                    self.fail();
                    return Ok(false);
                }
                st += 1;
            }
        }
        let l = cond.dc_l[tbl];
        let u = cond.dc_u[tbl];
        self.dc_context[ci] = if m < ((1i64 << l) >> 1) as i32 {
            0
        } else if m > ((1i64 << u) >> 1) as i32 {
            12 + sign * 4
        } else {
            4 + sign * 4
        };
        let mut value = m;
        st += 14;
        while {
            m >>= 1;
            m != 0
        } {
            if self.decode(reader, Bin::Dc(tbl, st))? != 0 {
                value |= m;
            }
        }
        value += 1;
        if sign != 0 {
            value = -value;
        }
        self.last_dc_val[ci] = (self.last_dc_val[ci] + value) & 0xffff;
        Ok(true)
    }

    /// Figure F.20: AC coefficients `start..=end` of one block, each stored
    /// shifted by `al`. Returns false after a code error.
    #[allow(clippy::too_many_arguments)]
    fn ac_first<R: Read>(
        &mut self,
        reader: &mut R,
        block: &mut [i16; 64],
        tbl: usize,
        start: usize,
        end: usize,
        al: u8,
        cond: &Conditioning,
    ) -> Result<bool> {
        let mut k = start;
        while k <= end {
            let mut st = 3 * (k - 1);
            if self.decode(reader, Bin::Ac(tbl, st))? != 0 {
                break; // EOB
            }
            while self.decode(reader, Bin::Ac(tbl, st + 1))? == 0 {
                st += 3;
                k += 1;
                if k > end {
                    self.fail();
                    return Ok(false);
                }
            }
            let sign = self.decode(reader, Bin::Fixed)?;
            st += 2;
            let mut m: i32 = i32::from(self.decode(reader, Bin::Ac(tbl, st))?);
            if m != 0 && self.decode(reader, Bin::Ac(tbl, st))? != 0 {
                m <<= 1;
                st = if k <= usize::from(cond.ac_k[tbl]) { 189 } else { 217 };
                while self.decode(reader, Bin::Ac(tbl, st))? != 0 {
                    m <<= 1;
                    if m == 0x8000 {
                        self.fail();
                        return Ok(false);
                    }
                    st += 1;
                }
            }
            let mut value = m;
            st += 14;
            while {
                m >>= 1;
                m != 0
            } {
                if self.decode(reader, Bin::Ac(tbl, st))? != 0 {
                    value |= m;
                }
            }
            value += 1;
            if sign != 0 {
                value = -value;
            }
            block[usize::from(UNZIGZAG[k])] = ((value as u32) << al) as i16;
            k += 1;
        }
        Ok(true)
    }

    /// `decode_mcu`, one block of a sequential scan.
    #[allow(clippy::too_many_arguments)]
    pub fn sequential<R: Read>(
        &mut self,
        reader: &mut R,
        block: &mut [i16; 64],
        ci: usize,
        dc_tbl: usize,
        ac_tbl: usize,
        cond: &Conditioning,
    ) -> Result<()> {
        if self.failed() || !self.dc_diff(reader, ci, dc_tbl, cond)? {
            return Ok(());
        }
        block[0] = self.last_dc_val[ci] as i16;
        self.ac_first(reader, block, ac_tbl, 1, 63, 0, cond)?;
        Ok(())
    }

    /// `decode_mcu_DC_first`, one block.
    pub fn dc_first<R: Read>(
        &mut self,
        reader: &mut R,
        block: &mut [i16; 64],
        ci: usize,
        tbl: usize,
        al: u8,
        cond: &Conditioning,
    ) -> Result<()> {
        if self.failed() || !self.dc_diff(reader, ci, tbl, cond)? {
            return Ok(());
        }
        block[0] = (self.last_dc_val[ci] << al) as i16;
        Ok(())
    }

    /// `decode_mcu_AC_first`.
    #[allow(clippy::too_many_arguments)]
    pub fn ac_first_block<R: Read>(
        &mut self,
        reader: &mut R,
        block: &mut [i16; 64],
        tbl: usize,
        start: u8,
        end: u8,
        al: u8,
        cond: &Conditioning,
    ) -> Result<()> {
        if self.failed() {
            return Ok(());
        }
        self.ac_first(reader, block, tbl, usize::from(start), usize::from(end), al, cond)?;
        Ok(())
    }

    /// `decode_mcu_DC_refine`, one block (no error check, as in libjpeg).
    pub fn dc_refine<R: Read>(&mut self, reader: &mut R, block: &mut [i16; 64], al: u8) -> Result<()> {
        if self.decode(reader, Bin::Fixed)? != 0 {
            block[0] |= (1i32 << al) as i16;
        }
        Ok(())
    }

    /// `decode_mcu_AC_refine`.
    pub fn ac_refine<R: Read>(
        &mut self,
        reader: &mut R,
        block: &mut [i16; 64],
        tbl: usize,
        start: u8,
        end: u8,
        al: u8,
    ) -> Result<()> {
        if self.failed() {
            return Ok(());
        }
        let (start, end) = (usize::from(start), usize::from(end));
        let p1 = (1i32 << al) as i16;
        let m1 = ((-1i32 as u32) << al) as i16;
        let mut kex = end;
        while kex > 0 && block[usize::from(UNZIGZAG[kex])] == 0 {
            kex -= 1;
        }
        let mut k = start;
        while k <= end {
            let mut st = 3 * (k - 1);
            if k > kex && self.decode(reader, Bin::Ac(tbl, st))? != 0 {
                break; // EOB
            }
            loop {
                let at = usize::from(UNZIGZAG[k]);
                if block[at] != 0 {
                    if self.decode(reader, Bin::Ac(tbl, st + 2))? != 0 {
                        block[at] = if block[at] < 0 {
                            block[at].wrapping_add(m1)
                        } else {
                            block[at].wrapping_add(p1)
                        };
                    }
                    break;
                }
                if self.decode(reader, Bin::Ac(tbl, st + 1))? != 0 {
                    block[at] = if self.decode(reader, Bin::Fixed)? != 0 { m1 } else { p1 };
                    break;
                }
                st += 3;
                k += 1;
                if k > end {
                    self.fail();
                    return Ok(());
                }
            }
            k += 1;
        }
        Ok(())
    }
}
