// SPDX-License-Identifier: Zlib
// Rust adaptation of zlib 1.3's default-level deflate.c and trees.c algorithms.
// This is an altered implementation, not the original zlib source.
// Copyright (C) 1995-2022 Jean-loup Gailly and Mark Adler.
// The complete upstream notice is retained in licenses/zlib.txt.
//! Reproduce the pinned metadata encoder's match choices, tree tie breaking,
//! block boundaries, and byte stream. General streaming/other levels are not
//! exposed: libheif uses level 6, a 32 KiB window and memory level 8 here.

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_BITS: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_BITS: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    pending: u32,
    count: u8,
}
impl Bits {
    fn write(&mut self, value: usize, bits: u8) {
        self.pending |= (value as u32) << self.count;
        self.count += bits;
        while self.count >= 8 {
            self.bytes.push(self.pending as u8);
            self.pending >>= 8;
            self.count -= 8;
        }
    }
    fn align(&mut self) {
        if self.count != 0 {
            self.bytes.push(self.pending as u8);
            self.pending = 0;
            self.count = 0;
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Node {
    frequency: u32,
    depth: u16,
    parent: usize,
    length: u8,
}
struct Tree {
    lengths: Vec<u8>,
    codes: Vec<u16>,
}
impl Tree {
    fn lengths(lengths: Vec<u8>) -> Self {
        let mut counts = [0u16; 16];
        for &n in &lengths {
            if n != 0 {
                counts[n as usize] += 1;
            }
        }
        let mut next = [0u16; 16];
        let mut code = 0;
        for bits in 1..16 {
            code = (code + counts[bits - 1]) << 1;
            next[bits] = code;
        }
        let codes = lengths
            .iter()
            .map(|&n| {
                if n == 0 {
                    return 0;
                }
                let code = next[n as usize];
                next[n as usize] += 1;
                code.reverse_bits() >> (16 - n)
            })
            .collect();
        Self { lengths, codes }
    }
    fn new(frequency: &[u32], maximum: u8) -> Self {
        let count = frequency.len();
        let mut nodes = vec![Node::default(); 2 * count + 1];
        let mut heap = vec![0usize; 2 * count + 1];
        let mut size = 0;
        let mut highest = -1isize;
        for (i, &freq) in frequency.iter().enumerate() {
            nodes[i].frequency = freq;
            if freq != 0 {
                size += 1;
                heap[size] = i;
                highest = i as isize;
            }
        }
        while size < 2 {
            let index = if highest < 2 {
                highest += 1;
                highest as usize
            } else {
                0
            };
            size += 1;
            heap[size] = index;
            nodes[index].frequency = 1;
        }
        for i in (1..=size / 2).rev() {
            down(&mut heap, size, i, &nodes);
        }
        let mut sorted = heap.len();
        let mut parent = count;
        loop {
            let first = heap[1];
            heap[1] = heap[size];
            size -= 1;
            down(&mut heap, size, 1, &nodes);
            let second = heap[1];
            sorted -= 1;
            heap[sorted] = first;
            sorted -= 1;
            heap[sorted] = second;
            nodes[parent].frequency = nodes[first].frequency + nodes[second].frequency;
            nodes[parent].depth = nodes[first].depth.max(nodes[second].depth) + 1;
            nodes[first].parent = parent;
            nodes[second].parent = parent;
            heap[1] = parent;
            parent += 1;
            down(&mut heap, size, 1, &nodes);
            if size < 2 {
                break;
            }
        }
        sorted -= 1;
        heap[sorted] = heap[1];
        let mut histogram = [0usize; 16];
        let mut overflow = 0i32;
        for &index in &heap[sorted + 1..] {
            let mut length = nodes[nodes[index].parent].length + 1;
            if length > maximum {
                length = maximum;
                overflow += 1;
            }
            nodes[index].length = length;
            if index <= highest as usize {
                histogram[length as usize] += 1;
            }
        }
        if overflow != 0 {
            while overflow > 0 {
                let mut bits = maximum as usize - 1;
                while histogram[bits] == 0 {
                    bits -= 1;
                }
                histogram[bits] -= 1;
                histogram[bits + 1] += 2;
                histogram[maximum as usize] -= 1;
                overflow -= 2;
            }
            let mut at = heap.len();
            for bits in (1..=maximum).rev() {
                for _ in 0..histogram[bits as usize] {
                    loop {
                        at -= 1;
                        let index = heap[at];
                        if index > highest as usize {
                            continue;
                        }
                        nodes[index].length = bits;
                        break;
                    }
                }
            }
        }
        Self::lengths(
            nodes[..=highest as usize]
                .iter()
                .map(|n| n.length)
                .collect(),
        )
    }
    fn send(&self, bits: &mut Bits, symbol: usize) {
        bits.write(self.codes[symbol] as usize, self.lengths[symbol]);
    }
}
fn down(heap: &mut [usize], size: usize, mut at: usize, nodes: &[Node]) {
    let value = heap[at];
    let smaller = |a: usize, b: usize| {
        nodes[a].frequency < nodes[b].frequency
            || (nodes[a].frequency == nodes[b].frequency && nodes[a].depth <= nodes[b].depth)
    };
    let mut child = at * 2;
    while child <= size {
        if child < size && smaller(heap[child + 1], heap[child]) {
            child += 1;
        }
        if smaller(value, heap[child]) {
            break;
        }
        heap[at] = heap[child];
        at = child;
        child *= 2;
    }
    heap[at] = value;
}

// Encoded code-length run: symbol, additional value, additional bit count.
fn runs(lengths: &[u8]) -> Vec<(usize, usize, u8)> {
    let mut result = Vec::new();
    let mut previous = 255;
    let (mut maximum, mut minimum) = if lengths[0] == 0 { (138, 3) } else { (7, 4) };
    let mut count = 0;
    for (index, &length) in lengths.iter().enumerate() {
        let next = lengths.get(index + 1).copied().unwrap_or(255);
        count += 1;
        if count < maximum && length == next {
            continue;
        }
        if count < minimum {
            for _ in 0..count {
                result.push((length as usize, 0, 0));
            }
        } else if length != 0 {
            if length != previous {
                result.push((length as usize, 0, 0));
                count -= 1;
            }
            result.push((16, count - 3, 2));
        } else if count <= 10 {
            result.push((17, count - 3, 3));
        } else {
            result.push((18, count - 11, 7));
        }
        count = 0;
        previous = length;
        (maximum, minimum) = if next == 0 {
            (138, 3)
        } else if length == next {
            (6, 3)
        } else {
            (7, 4)
        };
    }
    result
}

enum Token {
    Literal(u8),
    Match { length: usize, distance: usize },
}
impl Token {
    fn symbols(&self) -> (usize, Option<usize>) {
        match self {
            Self::Literal(value) => (*value as usize, None),
            Self::Match { length, distance } => (
                257 + LENGTH_BASE.iter().rposition(|v| v <= length).unwrap(),
                Some(DISTANCE_BASE.iter().rposition(|v| v <= distance).unwrap()),
            ),
        }
    }
}
fn fixed() -> (Tree, Tree) {
    (
        Tree::lengths(
            (0..288)
                .map(|n| match n {
                    0..=143 => 8,
                    144..=255 => 9,
                    256..=279 => 7,
                    _ => 8,
                })
                .collect(),
        ),
        Tree::lengths(vec![5; 32]),
    )
}
fn token_cost(tokens: &[Token], literal: &Tree, distance: &Tree) -> usize {
    literal.lengths[256] as usize
        + tokens
            .iter()
            .map(|t| {
                let (l, d) = t.symbols();
                literal.lengths[l] as usize
                    + d.map_or(0, |d| {
                        LENGTH_BITS[l - 257] as usize
                            + distance.lengths[d] as usize
                            + DISTANCE_BITS[d] as usize
                    })
            })
            .sum::<usize>()
}
fn block(bits: &mut Bits, tokens: &[Token], original: &[u8], last: bool, can_store: bool) {
    let mut lf = [0; 286];
    let mut df = [0; 30];
    lf[256] = 1;
    for token in tokens {
        let (l, d) = token.symbols();
        lf[l] += 1;
        if let Some(d) = d {
            df[d] += 1;
        }
    }
    let literal = Tree::new(&lf, 15);
    let distance = Tree::new(&df, 15);
    let mut rle = runs(&literal.lengths);
    rle.extend(runs(&distance.lengths));
    let mut frequencies = [0; 19];
    for &(symbol, _, _) in &rle {
        frequencies[symbol] += 1;
    }
    let code_lengths = Tree::new(&frequencies, 7);
    let count = ORDER
        .iter()
        .rposition(|n| code_lengths.lengths.get(*n).is_some_and(|len| *len != 0))
        .unwrap_or(3)
        .max(3)
        + 1;
    let dynamic_cost = token_cost(tokens, &literal, &distance)
        + 14
        + 3 * count
        + rle
            .iter()
            .map(|(symbol, _, extra)| code_lengths.lengths[*symbol] as usize + *extra as usize)
            .sum::<usize>();
    let (fixed_literal, fixed_distance) = fixed();
    let fixed_bytes = (token_cost(tokens, &fixed_literal, &fixed_distance) + 10) / 8;
    let dynamic_bytes = (dynamic_cost + 10) / 8;
    if can_store && original.len() <= 65535 && original.len() + 4 <= fixed_bytes.min(dynamic_bytes)
    {
        bits.write(usize::from(last), 3);
        bits.align();
        let len = original.len() as u16;
        bits.bytes.extend_from_slice(&len.to_le_bytes());
        bits.bytes.extend_from_slice(&(!len).to_le_bytes());
        bits.bytes.extend_from_slice(original);
        return;
    }
    let (literal, distance) = if fixed_bytes <= dynamic_bytes {
        bits.write(2 + usize::from(last), 3);
        (&fixed_literal, &fixed_distance)
    } else {
        bits.write(4 + usize::from(last), 3);
        bits.write(literal.lengths.len() - 257, 5);
        bits.write(distance.lengths.len() - 1, 5);
        bits.write(count - 4, 4);
        for &symbol in &ORDER[..count] {
            bits.write(
                code_lengths.lengths.get(symbol).copied().unwrap_or(0) as usize,
                3,
            );
        }
        for (symbol, value, extra) in rle {
            code_lengths.send(bits, symbol);
            bits.write(value, extra);
        }
        (&literal, &distance)
    };
    for token in tokens {
        let (l, d) = token.symbols();
        literal.send(bits, l);
        if let (
            Some(d),
            Token::Match {
                length,
                distance: dist,
            },
        ) = (d, token)
        {
            bits.write(length - LENGTH_BASE[l - 257], LENGTH_BITS[l - 257]);
            distance.send(bits, d);
            bits.write(dist - DISTANCE_BASE[d], DISTANCE_BITS[d]);
        }
    }
    literal.send(bits, 256);
}

struct Dictionary {
    head: Vec<usize>,
    previous: Vec<usize>,
}
impl Dictionary {
    fn new() -> Self {
        Self {
            head: vec![usize::MAX; 32768],
            previous: vec![usize::MAX; 32768],
        }
    }
    fn insert(&mut self, data: &[u8], at: usize) -> usize {
        let key = ((usize::from(data[at]) << 10)
            ^ (usize::from(data[at + 1]) << 5)
            ^ usize::from(data[at + 2]))
            & 32767;
        let prev = self.head[key];
        self.previous[at & 32767] = prev;
        self.head[key] = at;
        prev
    }
    fn longest(
        &self,
        data: &[u8],
        at: usize,
        mut candidate: usize,
        base: usize,
        previous: usize,
        match_at: &mut usize,
    ) -> usize {
        let limit = at.saturating_sub(32506).max(base);
        let mut chain = if previous >= 8 { 32 } else { 128 };
        let mut best = previous;
        let nice = 128.min(data.len() - at);
        let byte = |index| data.get(index).copied().unwrap_or(0);
        loop {
            if byte(candidate + best) == byte(at + best)
                && byte(candidate + best - 1) == byte(at + best - 1)
                && byte(candidate) == byte(at)
                && byte(candidate + 1) == byte(at + 1)
            {
                let mut len = 2;
                while len < 258 && byte(candidate + len) == byte(at + len) {
                    len += 1;
                }
                if len > best {
                    *match_at = candidate;
                    best = len;
                    if len >= nice {
                        break;
                    }
                }
            }
            candidate = self.previous[candidate & 32767];
            chain -= 1;
            if candidate == usize::MAX || candidate <= limit || chain == 0 {
                break;
            }
        }
        best.min(data.len() - at)
    }
}

pub(crate) fn compress(data: &[u8], zlib: bool) -> Vec<u8> {
    let mut bits = Bits::default();
    if zlib {
        bits.bytes.extend_from_slice(&[0x78, 0x9c]);
    }
    let mut dictionary = Dictionary::new();
    let mut tokens = Vec::with_capacity(16383);
    let mut at = 0;
    let mut base = 0;
    let mut loaded = data.len().min(65536);
    let mut block_start = 0;
    let mut length = 2;
    let mut match_at = 0;
    let mut pending = false;
    while at < data.len() {
        if loaded - at < 262 {
            if at - base >= 65274 {
                base += 32768;
            }
            loaded = data.len().min(base + 65536);
        }
        let head = if data.len() - at >= 3 {
            dictionary.insert(data, at)
        } else {
            usize::MAX
        };
        let previous_length = length;
        let previous_match = match_at;
        length = 2;
        if head != usize::MAX && head > base && previous_length < 16 && at - head <= 32506 {
            length = dictionary.longest(data, at, head, base, previous_length, &mut match_at);
            if length == 3 && at - match_at > 4096 {
                length = 2;
            }
        }
        if previous_length >= 3 && length <= previous_length {
            tokens.push(Token::Match {
                length: previous_length,
                distance: at - 1 - previous_match,
            });
            let next = at + previous_length - 1;
            at += 1;
            while at < next {
                if at + 3 <= data.len() {
                    dictionary.insert(data, at);
                }
                at += 1;
            }
            pending = false;
            length = 2;
            if tokens.len() == 16383 {
                block(
                    &mut bits,
                    &tokens,
                    &data[block_start..at],
                    false,
                    block_start >= base,
                );
                tokens.clear();
                block_start = at;
            }
        } else if pending {
            tokens.push(Token::Literal(data[at - 1]));
            if tokens.len() == 16383 {
                block(
                    &mut bits,
                    &tokens,
                    &data[block_start..at],
                    false,
                    block_start >= base,
                );
                tokens.clear();
                block_start = at;
            }
            at += 1;
        } else {
            pending = true;
            at += 1;
        }
    }
    if pending {
        tokens.push(Token::Literal(data[at - 1]));
    }
    block(
        &mut bits,
        &tokens,
        &data[block_start..at],
        true,
        block_start >= base,
    );
    bits.align();
    if zlib {
        let (mut a, mut b) = (1u32, 0u32);
        for chunk in data.chunks(5552) {
            for byte in chunk {
                a += u32::from(*byte);
                b += a;
            }
            a %= 65521;
            b %= 65521;
        }
        bits.bytes.extend_from_slice(&((b << 16) | a).to_be_bytes());
    }
    bits.bytes
}
