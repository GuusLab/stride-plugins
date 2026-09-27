//! A small QR code encoder: byte mode, error correction level M, versions 1
//! to 40, with the mask chosen by the standard penalty rules.
//!
//! It follows ISO/IEC 18004 the way Project Nayuki's reference encoder lays it
//! out, and is checked module for module against the `qrcode` npm package in
//! the tests. Level M recovers about 15% damage, which survives a smudged
//! poster or a folded menu without making the code much denser than level L.

/// Error correction codewords per block, level M, indexed by version.
const ECC_PER_BLOCK: [u8; 41] = [
    0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28,
    28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
];

/// Error correction blocks, level M, indexed by version.
const BLOCKS: [u8; 41] = [
    0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23,
    25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
];

/// The largest version `encode` picks.
pub const MAX_VERSION: usize = 13;

/// Level M in the format information.
const FORMAT_M: u32 = 0;

/// A finished code: `size` by `size` modules, `true` is dark.
pub struct QrCode {
    pub size: usize,
    modules: Vec<bool>,
}

impl QrCode {
    pub fn dark(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }

    /// Encode `data` in the smallest version that holds it, or `None` when it
    /// needs more than version 13 (331 bytes). Bigger codes have modules too
    /// small to scan at the sizes shown, and cost too much for a page hook.
    pub fn encode(data: &[u8]) -> Option<QrCode> {
        let version =
            (1..=MAX_VERSION).find(|&v| capacity_bits(v) >= needed_bits(v, data.len()))?;
        Some(Self::encode_version(data, version, None))
    }

    /// Encode in a given version, with a given mask or the best one.
    pub fn encode_version(data: &[u8], version: usize, mask: Option<u8>) -> QrCode {
        let codewords = add_ecc_and_interleave(&data_codewords(data, version), version);
        let mut grid = Grid::new(version);
        grid.draw_function_patterns();
        grid.draw_codewords(&codewords);
        let mask = mask.unwrap_or_else(|| {
            (0..8)
                .min_by_key(|&m| {
                    grid.apply_mask(m);
                    grid.draw_format(m);
                    let score = grid.penalty();
                    grid.apply_mask(m);
                    score
                })
                .unwrap_or(0)
        });
        grid.apply_mask(mask);
        grid.draw_format(mask);
        QrCode {
            size: grid.size,
            modules: grid.modules,
        }
    }
}

fn count_bits(version: usize) -> usize {
    if version < 10 { 8 } else { 16 }
}

fn needed_bits(version: usize, len: usize) -> usize {
    if len >= 1 << count_bits(version) {
        return usize::MAX;
    }
    4 + count_bits(version) + 8 * len
}

fn raw_modules(version: usize) -> usize {
    let mut result = (16 * version + 128) * version + 64;
    if version >= 2 {
        let align = version / 7 + 2;
        result -= (25 * align - 10) * align - 55;
        if version >= 7 {
            result -= 36;
        }
    }
    result
}

fn data_codeword_count(version: usize) -> usize {
    raw_modules(version) / 8 - ECC_PER_BLOCK[version] as usize * BLOCKS[version] as usize
}

fn capacity_bits(version: usize) -> usize {
    data_codeword_count(version) * 8
}

/// Mode, length, bytes, terminator and padding, as codewords.
fn data_codewords(data: &[u8], version: usize) -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::with_capacity(capacity_bits(version));
    let mut push = |value: u32, len: usize, bits: &mut Vec<bool>| {
        for i in (0..len).rev() {
            bits.push((value >> i) & 1 == 1);
        }
    };
    push(0b0100, 4, &mut bits);
    push(data.len() as u32, count_bits(version), &mut bits);
    for &byte in data {
        push(u32::from(byte), 8, &mut bits);
    }
    let capacity = capacity_bits(version);
    let terminator = (capacity - bits.len()).min(4);
    push(0, terminator, &mut bits);
    let pad = (8 - bits.len() % 8) % 8;
    push(0, pad, &mut bits);
    let mut out: Vec<u8> = bits
        .chunks(8)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | u8::from(b)))
        .collect();
    let mut filler = [0xEC, 0x11].into_iter().cycle();
    while out.len() < capacity / 8 {
        out.push(filler.next().unwrap_or(0xEC));
    }
    out
}

fn gf_mul(x: u8, y: u8) -> u8 {
    let (x, y) = (u32::from(x), u32::from(y));
    let mut z: u32 = 0;
    for i in (0..8).rev() {
        z = (z << 1) ^ ((z >> 7) * 0x11D);
        z ^= ((y >> i) & 1) * x;
    }
    z as u8
}

fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    result[degree - 1] = 1;
    let mut root: u8 = 1;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = gf_mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = gf_mul(root, 0x02);
    }
    result
}

fn rs_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for &byte in data {
        let factor = byte ^ result.remove(0);
        result.push(0);
        for (r, &d) in result.iter_mut().zip(divisor) {
            *r ^= gf_mul(d, factor);
        }
    }
    result
}

fn add_ecc_and_interleave(data: &[u8], version: usize) -> Vec<u8> {
    let blocks = BLOCKS[version] as usize;
    let ecc_len = ECC_PER_BLOCK[version] as usize;
    let raw = raw_modules(version) / 8;
    let short_blocks = blocks - raw % blocks;
    let short_len = raw / blocks;
    let divisor = rs_divisor(ecc_len);
    let mut split = Vec::with_capacity(blocks);
    let mut k = 0;
    for i in 0..blocks {
        let len = short_len - ecc_len + usize::from(i >= short_blocks);
        let dat = &data[k..k + len];
        k += len;
        let ecc = rs_remainder(dat, &divisor);
        let mut block = dat.to_vec();
        if i < short_blocks {
            block.push(0);
        }
        block.extend(ecc);
        split.push(block);
    }
    let mut out = Vec::with_capacity(raw);
    for i in 0..split[0].len() {
        for (j, block) in split.iter().enumerate() {
            if i != short_len - ecc_len || j >= short_blocks {
                out.push(block[i]);
            }
        }
    }
    out
}

struct Grid {
    version: usize,
    size: usize,
    modules: Vec<bool>,
    function: Vec<bool>,
}

impl Grid {
    fn new(version: usize) -> Self {
        let size = version * 4 + 17;
        Grid {
            version,
            size,
            modules: vec![false; size * size],
            function: vec![false; size * size],
        }
    }

    fn set(&mut self, x: usize, y: usize, dark: bool) {
        let at = y * self.size + x;
        self.modules[at] = dark;
        self.function[at] = true;
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }

    fn draw_function_patterns(&mut self) {
        let size = self.size;
        for i in 0..size {
            self.set(6, i, i % 2 == 0);
            self.set(i, 6, i % 2 == 0);
        }
        self.draw_finder(3, 3);
        self.draw_finder(size - 4, 3);
        self.draw_finder(3, size - 4);
        let align = alignment_positions(self.version);
        let n = align.len();
        for i in 0..n {
            for j in 0..n {
                let corner = (i == 0 && j == 0) || (i == 0 && j == n - 1) || (i == n - 1 && j == 0);
                if !corner {
                    self.draw_alignment(align[i], align[j]);
                }
            }
        }
        // Reserve the format areas now; the real bits come with the mask.
        self.draw_format(0);
        self.draw_version();
    }

    fn draw_finder(&mut self, x: usize, y: usize) {
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let dist = dx.abs().max(dy.abs());
                let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                if (0..self.size as i32).contains(&xx) && (0..self.size as i32).contains(&yy) {
                    self.set(xx as usize, yy as usize, dist != 2 && dist != 4);
                }
            }
        }
    }

    fn draw_alignment(&mut self, x: usize, y: usize) {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let dist = dx.abs().max(dy.abs());
                self.set(
                    (x as i32 + dx) as usize,
                    (y as i32 + dy) as usize,
                    dist != 1,
                );
            }
        }
    }

    fn draw_format(&mut self, mask: u8) {
        let data = (FORMAT_M << 3) | u32::from(mask);
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = ((data << 10) | rem) ^ 0x5412;
        let bit = |i: usize| (bits >> i) & 1 == 1;
        for i in 0..=5 {
            self.set(8, i, bit(i));
        }
        self.set(8, 7, bit(6));
        self.set(8, 8, bit(7));
        self.set(7, 8, bit(8));
        for i in 9..15 {
            self.set(14 - i, 8, bit(i));
        }
        let size = self.size;
        for i in 0..8 {
            self.set(size - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set(8, size - 15 + i, bit(i));
        }
        self.set(8, size - 8, true);
    }

    fn draw_version(&mut self) {
        if self.version < 7 {
            return;
        }
        let v = self.version as u32;
        let mut rem = v;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let bits = (v << 12) | rem;
        for i in 0..18 {
            let dark = (bits >> i) & 1 == 1;
            let a = self.size - 11 + i % 3;
            let b = i / 3;
            self.set(a, b, dark);
            self.set(b, a, dark);
        }
    }

    fn draw_codewords(&mut self, data: &[u8]) {
        let size = self.size;
        let total = data.len() * 8;
        let mut i = 0;
        let mut right = size as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..size {
                for j in 0..2 {
                    let x = (right - j) as usize;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { size - 1 - vert } else { vert };
                    let at = y * size + x;
                    if !self.function[at] && i < total {
                        self.modules[at] = (data[i >> 3] >> (7 - (i & 7))) & 1 == 1;
                        i += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let invert = match mask {
                    0 => (x + y) % 2 == 0,
                    1 => y % 2 == 0,
                    2 => x % 3 == 0,
                    3 => (x + y) % 3 == 0,
                    4 => (x / 3 + y / 2) % 2 == 0,
                    5 => x * y % 2 + x * y % 3 == 0,
                    6 => (x * y % 2 + x * y % 3) % 2 == 0,
                    _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
                };
                let at = y * self.size + x;
                if invert && !self.function[at] {
                    self.modules[at] = !self.modules[at];
                }
            }
        }
    }

    /// The four penalty rules. Only the ranking matters: any mask decodes.
    fn penalty(&self) -> usize {
        let size = self.size;
        let mut score = 0;
        const FINDER: [bool; 11] = [
            true, false, true, true, true, false, true, false, false, false, false,
        ];
        for horizontal in [true, false] {
            for a in 0..size {
                let at = |b: usize| {
                    if horizontal {
                        self.get(b, a)
                    } else {
                        self.get(a, b)
                    }
                };
                let mut run = 1;
                for b in 1..size {
                    if at(b) == at(b - 1) {
                        run += 1;
                    } else {
                        if run >= 5 {
                            score += run - 2;
                        }
                        run = 1;
                    }
                }
                if run >= 5 {
                    score += run - 2;
                }
                // A finder-like 1:1:3:1:1 run with four light modules (or
                // the edge) on one side.
                for b in 0..size.saturating_sub(6) {
                    let matches = |reverse: bool| {
                        (0..11).all(|k| {
                            let expect = if reverse { FINDER[10 - k] } else { FINDER[k] };
                            let pos = b as i32 + k as i32 - if reverse { 4 } else { 0 };
                            if pos < 0 || pos >= size as i32 {
                                !expect
                            } else {
                                at(pos as usize) == expect
                            }
                        })
                    };
                    if matches(false) || matches(true) {
                        score += 40;
                    }
                }
            }
        }
        let mut dark = 0;
        for y in 0..size {
            for x in 0..size {
                let c = self.get(x, y);
                dark += usize::from(c);
                if x + 1 < size
                    && y + 1 < size
                    && c == self.get(x + 1, y)
                    && c == self.get(x, y + 1)
                    && c == self.get(x + 1, y + 1)
                {
                    score += 3;
                }
            }
        }
        let total = size * size;
        let k = ((dark * 20).abs_diff(total * 10) + total - 1) / total - 1;
        score + k * 10
    }
}

fn alignment_positions(version: usize) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let n = version / 7 + 2;
    let step = if version == 32 {
        26
    } else {
        (version * 4 + n * 2 + 1) / (n * 2 - 2) * 2
    };
    let mut result = vec![6];
    let mut pos = version * 4 + 17 - 7;
    for _ in 0..n - 1 {
        result.insert(1, pos);
        pos -= step;
    }
    result
}

/// The dark modules as one SVG path in module units, offset by `quiet`.
pub fn svg_path(code: &QrCode, quiet: usize) -> String {
    let mut d = String::with_capacity(code.size * code.size * 4);
    for y in 0..code.size {
        let mut x = 0;
        while x < code.size {
            if code.dark(x, y) {
                let start = x;
                while x < code.size && code.dark(x, y) {
                    x += 1;
                }
                d.push_str(&format!(
                    "M{} {}h{}v1H{}z",
                    start + quiet,
                    y + quiet,
                    x - start,
                    start + quiet
                ));
            } else {
                x += 1;
            }
        }
    }
    d
}

#[cfg(test)]
pub fn to_bits(code: &QrCode) -> String {
    let mut out = String::new();
    for y in 0..code.size {
        for x in 0..code.size {
            out.push(if code.dark(x, y) { '1' } else { '0' });
        }
    }
    out
}
