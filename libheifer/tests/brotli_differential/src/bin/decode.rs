use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};
use std::os::raw::{c_int, c_void};
extern "C" {
    fn BrotliDecoderCreateInstance(a: *const c_void, f: *const c_void, o: *const c_void) -> *mut c_void;
    fn BrotliDecoderDestroyInstance(s: *mut c_void);
    fn BrotliDecoderDecompressStream(s: *mut c_void, ai: *mut usize, ni: *mut *const u8, ao: *mut usize, no: *mut *mut u8, t: *mut usize) -> c_int;
    fn BrotliDecoderGetErrorCode(s: *const c_void) -> c_int;
    fn BrotliEncoderCompress(q: c_int, lgwin: c_int, mode: c_int, n: usize, i: *const u8, en: *mut usize, e: *mut u8) -> c_int;
}
const BUF: usize = 1 << 18;
// (result, code, output, chunk sizes)
fn c_dec(d: &[u8], buf: usize) -> (i32, i32, Vec<u8>, Vec<usize>) {
    unsafe {
        let s = BrotliDecoderCreateInstance(std::ptr::null(), std::ptr::null(), std::ptr::null());
        let mut b = vec![0u8; buf];
        let mut ai = d.len(); let mut ni = d.as_ptr();
        let mut out = Vec::new(); let mut chunks = Vec::new();
        loop {
            let mut ao = buf; let mut no = b.as_mut_ptr();
            let r = BrotliDecoderDecompressStream(s, &mut ai, &mut ni, &mut ao, &mut no, std::ptr::null_mut());
            let n = buf - ao;
            if r == 3 || r == 1 { out.extend_from_slice(&b[..n]); chunks.push(n); if r == 1 { BrotliDecoderDestroyInstance(s); return (r, 0, out, chunks) } continue }
            let code = BrotliDecoderGetErrorCode(s);
            BrotliDecoderDestroyInstance(s);
            return (r, code, out, chunks);
        }
    }
}
fn r_dec(d: &[u8], buf: usize) -> (i32, i32, Vec<u8>, Vec<usize>) {
    let mut st = BrotliState::new_strict(StandardAlloc::default(), StandardAlloc::default(), StandardAlloc::default());
    let mut b = vec![0u8; buf];
    let (mut ai, mut io, mut total) = (d.len(), 0usize, 0usize);
    let mut out = Vec::new(); let mut chunks = Vec::new();
    loop {
        let (mut ao, mut oo) = (buf, 0usize);
        let r = BrotliDecompressStream(&mut ai, &mut io, d, &mut ao, &mut oo, &mut b, &mut total, &mut st);
        match r {
            BrotliResult::NeedsMoreOutput => { out.extend_from_slice(&b[..oo]); chunks.push(oo); }
            BrotliResult::ResultSuccess => { out.extend_from_slice(&b[..oo]); chunks.push(oo); return (1, 0, out, chunks) }
            BrotliResult::NeedsMoreInput => return (2, st.error_code as i32, out, chunks),
            BrotliResult::ResultFailure => return (0, st.error_code as i32, out, chunks),
        }
    }
}
fn comp(d: &[u8], q: i32, w: i32, mode: i32) -> Vec<u8> {
    let mut e = vec![0u8; d.len() * 2 + 1024]; let mut n = e.len();
    unsafe { assert!(BrotliEncoderCompress(q, w, mode, d.len(), d.as_ptr(), &mut n, e.as_mut_ptr()) == 1) }
    e.truncate(n); e
}
struct Rng(u64);
impl Rng { fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 } fn below(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize } }
fn main() {
    let iters: usize = std::env::args().nth(1).map_or(20000, |s| s.parse().unwrap());
    let seed: u64 = std::env::args().nth(2).map_or(1, |s| s.parse().unwrap());
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mut inputs: Vec<Vec<u8>> = vec![vec![], b"x".to_vec(), (0..=255u8).collect(), vec![0; 70000], b"abcde".repeat(6000)];
    let text = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
    inputs.push(text.clone());
    inputs.push((0..300000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect());
    let mut streams = Vec::new();
    for i in &inputs { for q in [0, 1, 2, 4, 5, 9, 10, 11] { for w in [10, 16, 22, 24] { streams.push(comp(i, q, w, 0)); } } }
    let mut bad = std::collections::BTreeMap::<String, usize>::new();
    let mut n = 0;
    for it in 0..iters {
        let mut s = streams[rng.below(streams.len())].clone();
        match rng.below(6) {
            0 => { let k = rng.below(s.len() + 1); s.truncate(k) }
            1 => { for _ in 0..1 + rng.below(3) { if !s.is_empty() { let k = rng.below(s.len()); s[k] ^= 1 << rng.below(8) } } }
            2 => { if !s.is_empty() { let k = rng.below(s.len()); s[k] = rng.next() as u8 } }
            3 => { for _ in 0..1 + rng.below(8) { s.push(rng.next() as u8) } }
            4 => { s = (0..rng.below(64)).map(|_| rng.next() as u8).collect() }
            _ => { let k = rng.below(s.len().min(16) + 1); for j in 0..k { s[j] = rng.next() as u8 } }
        }
        let buf = if it % 5 == 0 { 1 + rng.below(300) } else { BUF };
        let c = c_dec(&s, buf); let r = r_dec(&s, buf);
        if c != r {
            n += 1;
            let key = format!("c=({},{}) r=({},{}) out_eq={} chunks_eq={}", c.0, c.1, r.0, r.1, c.2 == r.2, c.3 == r.3);
            if !bad.contains_key(&key) { std::fs::write(format!("case-{}.br", bad.len()), &s).unwrap(); eprintln!("case-{} buf={buf} {key}", bad.len()); }
            *bad.entry(key).or_default() += 1;
        }
    }
    println!("{n} mismatches of {iters}");
    for (k, v) in bad { println!("{v:6} {k}") }
}
