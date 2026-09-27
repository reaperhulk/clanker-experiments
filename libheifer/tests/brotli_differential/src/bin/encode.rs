use brotli::enc::encode::{BrotliEncoderOperation, BrotliEncoderStateStruct};
use brotli::enc::StandardAlloc;
use std::os::raw::{c_int, c_void};
extern "C" {
    fn BrotliEncoderCreateInstance(a: *const c_void, f: *const c_void, o: *const c_void) -> *mut c_void;
    fn BrotliEncoderDestroyInstance(s: *mut c_void);
    fn BrotliEncoderCompressStream(s: *mut c_void, op: c_int, ai: *mut usize, ni: *mut *const u8, ao: *mut usize, no: *mut *mut u8, t: *mut usize) -> c_int;
    fn BrotliEncoderIsFinished(s: *mut c_void) -> c_int;
}
const BUF: usize = 1 << 18;
// libheif compress_brotli
fn c_enc(d: &[u8]) -> Vec<u8> {
    unsafe {
        let s = BrotliEncoderCreateInstance(std::ptr::null(), std::ptr::null(), std::ptr::null());
        let mut ai = d.len(); let mut ni = d.as_ptr();
        let mut tmp = vec![0u8; BUF]; let mut out = Vec::new();
        let mut ao = BUF; let mut no = tmp.as_mut_ptr();
        loop {
            assert!(BrotliEncoderCompressStream(s, 2, &mut ai, &mut ni, &mut ao, &mut no, std::ptr::null_mut()) == 1);
            let n = BUF - ao;
            if n != 0 { out.extend_from_slice(&tmp[..n]); ao = BUF; no = tmp.as_mut_ptr(); }
            if BrotliEncoderIsFinished(s) == 1 { break }
        }
        BrotliEncoderDestroyInstance(s);
        out
    }
}
fn r_enc(input: &[u8]) -> Vec<u8> {
    let mut state = BrotliEncoderStateStruct::new(StandardAlloc::default());
    let mut out = Vec::new();
    let mut buf = vec![0u8; BUF];
    let (mut avail_in, mut in_off) = (input.len(), 0usize);
    loop {
        let (mut avail_out, mut out_off) = (buf.len(), 0usize);
        let mut total = None;
        assert!(state.compress_stream(BrotliEncoderOperation::BROTLI_OPERATION_FINISH, &mut avail_in, input, &mut in_off,
            &mut avail_out, &mut buf, &mut out_off, &mut total, &mut |_, _, _, _| ()));
        out.extend_from_slice(&buf[..out_off]);
        if state.is_finished() { break; }
    }
    out
}
struct Rng(u64);
impl Rng { fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 } fn below(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize } }
fn gen(r: &mut Rng, corpus: &[Vec<u8>]) -> Vec<u8> {
    let sizes = [0usize, 1, 2, 3, 7, 50, 200, 1000, 4096, 20000, 70000, 200000, 262144, 300000, 700000, 1500000];
    let n = { let s = sizes[r.below(sizes.len())]; if s > 8 { s - r.below(s / 3) } else { s } };
    let mut v = Vec::with_capacity(n);
    match r.below(8) {
        0 => { while v.len() < n { v.push(r.next() as u8) } }
        1 => { let a = r.below(8) as u8 + 1; while v.len() < n { v.push((r.next() % a as u64) as u8) } }
        2 => { // image-like: rows with gradients and noise, 1-4 channels
            let ch = 1 + r.below(4); let w = 16 + r.below(512); let noise = r.below(20) as u64 + 1;
            let mut i = 0usize; while v.len() < n { let x = (i / ch) % w; let y = i / (ch * w); let c = i % ch; v.push(((x * (c + 1) + y * 3) as u64 + r.next() % noise) as u8); i += 1 } }
        3 => { // 16-bit little-endian samples
            let mut i = 0u32; while v.len() < n { let s = (i.wrapping_mul(37) >> 3) as u16 ^ (r.next() as u16 & 0x3f); v.extend_from_slice(&s.to_le_bytes()); i += 1 } v.truncate(n) }
        4 => { // text from corpus with mutations
            let c = &corpus[r.below(corpus.len())]; while v.len() < n { let st = r.below(c.len()); let l = r.below(400) + 1; v.extend_from_slice(&c[st..(st + l).min(c.len())]); if r.below(4) == 0 { v.push(r.next() as u8) } } v.truncate(n) }
        5 => { // repeats with long distances
            let unit: Vec<u8> = (0..1 + r.below(5000)).map(|_| r.next() as u8).collect(); while v.len() < n { v.extend_from_slice(&unit); if r.below(3) == 0 { let k = r.below(64); for _ in 0..k { v.push(r.next() as u8) } } } v.truncate(n) }
        6 => { // UTF-8 multibyte text
            let chars = ['a', 'é', 'ж', '中', '😀', ' ', '\n', 'ß', 'Ω']; let mut s = String::new(); while s.len() < n { s.push(chars[r.below(chars.len())]) } v = s.into_bytes(); v.truncate(n) }
        _ => { // concatenation of mixed segments
            while v.len() < n { let seg = r.below(3000) + 1; let kind = r.below(3); for i in 0..seg { v.push(match kind { 0 => r.next() as u8, 1 => (i % 7) as u8, _ => b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">"[i % 36] }) } } v.truncate(n) }
    }
    v
}
fn main() {
    let iters: usize = std::env::args().nth(1).map_or(200, |s| s.parse().unwrap());
    let seed: u64 = std::env::args().nth(2).map_or(1, |s| s.parse().unwrap());
    let mut r = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mut corpus = vec![];
    for p in ["README.md", "src/context.rs", "docs/RESULTS.md"] { corpus.push(std::fs::read(format!("{}/../../{p}", env!("CARGO_MANIFEST_DIR"))).unwrap()) }
    let mut bad = 0; let mut bytes = 0usize;
    for it in 0..iters {
        let d = gen(&mut r, &corpus); bytes += d.len();
        let (c, rr) = (c_enc(&d), r_enc(&d));
        if c != rr { bad += 1; let f = format!("enc-{seed}-{it}.bin"); std::fs::write(&f, &d).unwrap(); eprintln!("MISMATCH {f} len {} c {} r {}", d.len(), c.len(), rr.len()); }
    }
    println!("seed {seed}: {bad} mismatches of {iters} inputs ({bytes} bytes)");
}
