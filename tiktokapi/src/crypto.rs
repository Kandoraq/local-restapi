

use std::io::Read;

pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut v).is_ok() {
            return v;
        }
    }

    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
        ^ (std::process::id() as u64).rotate_left(32);
    for b in v.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x >> 24) as u8;
    }
    v
}

pub fn random_array<const N: usize>() -> [u8; N] {
    let mut a = [0u8; N];
    a.copy_from_slice(&random_bytes(N));
    a
}

#[allow(dead_code)]
pub fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < s.len() {
        let hi = s[i].to_digit(16)? as u8;
        let lo = s[i + 1].to_digit(16)? as u8;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    buflen: usize,
    total: u64,
}

impl Sha256 {
    pub fn new() -> Self {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0u8; 64],
            buflen: 0,
            total: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        if self.buflen > 0 {
            let need = 64 - self.buflen;
            let take = need.min(data.len());
            self.buf[self.buflen..self.buflen + take].copy_from_slice(&data[..take]);
            self.buflen += take;
            data = &data[take..];
            if self.buflen == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buflen = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.compress(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buflen = data.len();
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
            self.h[0], self.h[1], self.h[2], self.h[3], self.h[4], self.h[5], self.h[6], self.h[7],
        );
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bitlen = self.total * 8;
        self.update(&[0x80]);
        while self.buflen != 56 {
            self.update(&[0x00]);
        }
        self.update(&bitlen.to_be_bytes());
        let mut out = [0u8; 32];
        for i in 0..8 {
            out[i * 4..i * 4 + 4].copy_from_slice(&self.h[i].to_be_bytes());
        }
        out
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finish()
}

pub fn sha256_many(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finish()
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = sha256_many(&[&ipad, data]);
    sha256_many(&[&opad, &inner])
}

pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let s = if salt.is_empty() { &[0u8; 32][..] } else { salt };
    hmac_sha256(s, ikm)
}

pub fn hkdf_expand(prk: &[u8; 32], info: &[u8], out_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(out_len);
    let mut t: Vec<u8> = Vec::new();
    let mut counter = 1u8;
    while out.len() < out_len {
        let mut input = Vec::new();
        input.extend_from_slice(&t);
        input.extend_from_slice(info);
        input.push(counter);
        t = hmac_sha256(prk, &input).to_vec();
        out.extend_from_slice(&t);
        counter = counter.wrapping_add(1);
    }
    out.truncate(out_len);
    out
}

pub fn tls_expand_label(secret: &[u8; 32], label: &str, context: &[u8], out_len: usize) -> Vec<u8> {
    let mut full_label = Vec::with_capacity(6 + label.len());
    full_label.extend_from_slice(b"tls13 ");
    full_label.extend_from_slice(label.as_bytes());
    let mut info = Vec::new();
    info.extend_from_slice(&(out_len as u16).to_be_bytes());
    info.push(full_label.len() as u8);
    info.extend_from_slice(&full_label);
    info.push(context.len() as u8);
    info.extend_from_slice(context);
    hkdf_expand(secret, &info, out_len)
}

fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[0] = 0x61707865;
    state[1] = 0x3320646e;
    state[2] = 0x79622d32;
    state[3] = 0x6b206574;
    for i in 0..8 {
        state[4 + i] = u32::from_le_bytes([
            key[i * 4],
            key[i * 4 + 1],
            key[i * 4 + 2],
            key[i * 4 + 3],
        ]);
    }
    state[12] = counter;
    for i in 0..3 {
        state[13 + i] = u32::from_le_bytes([
            nonce[i * 4],
            nonce[i * 4 + 1],
            nonce[i * 4 + 2],
            nonce[i * 4 + 3],
        ]);
    }
    let mut x = state;
    macro_rules! qr {
        ($a:expr, $b:expr, $c:expr, $d:expr) => {
            x[$a] = x[$a].wrapping_add(x[$b]);
            x[$d] ^= x[$a];
            x[$d] = x[$d].rotate_left(16);
            x[$c] = x[$c].wrapping_add(x[$d]);
            x[$b] ^= x[$c];
            x[$b] = x[$b].rotate_left(12);
            x[$a] = x[$a].wrapping_add(x[$b]);
            x[$d] ^= x[$a];
            x[$d] = x[$d].rotate_left(8);
            x[$c] = x[$c].wrapping_add(x[$d]);
            x[$b] ^= x[$c];
            x[$b] = x[$b].rotate_left(7);
        };
    }
    for _ in 0..10 {
        qr!(0, 4, 8, 12);
        qr!(1, 5, 9, 13);
        qr!(2, 6, 10, 14);
        qr!(3, 7, 11, 15);
        qr!(0, 5, 10, 15);
        qr!(1, 6, 11, 12);
        qr!(2, 7, 8, 13);
        qr!(3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[i * 4..i * 4 + 4].copy_from_slice(&x[i].wrapping_add(state[i]).to_le_bytes());
    }
    out
}

fn chacha20_xor(key: &[u8; 32], nonce: &[u8; 12], counter: u32, data: &mut [u8]) {
    let mut ctr = counter;
    for chunk in data.chunks_mut(64) {
        let ks = chacha20_block(key, ctr, nonce);
        for (i, b) in chunk.iter_mut().enumerate() {
            *b ^= ks[i];
        }
        ctr = ctr.wrapping_add(1);
    }
}

struct Poly1305 {
    r: [u32; 5],
    h: [u32; 5],
    pad: [u32; 4],
    buf: [u8; 16],
    buflen: usize,
}

impl Poly1305 {
    fn new(key: &[u8; 32]) -> Self {
        let t0 = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
        let t1 = u32::from_le_bytes([key[4], key[5], key[6], key[7]]);
        let t2 = u32::from_le_bytes([key[8], key[9], key[10], key[11]]);
        let t3 = u32::from_le_bytes([key[12], key[13], key[14], key[15]]);
        let r = [
            t0 & 0x3ffffff,
            ((t0 >> 26) | (t1 << 6)) & 0x3ffff03,
            ((t1 >> 20) | (t2 << 12)) & 0x3ffc0ff,
            ((t2 >> 14) | (t3 << 18)) & 0x3f03fff,
            (t3 >> 8) & 0x00fffff,
        ];
        let pad = [
            u32::from_le_bytes([key[16], key[17], key[18], key[19]]),
            u32::from_le_bytes([key[20], key[21], key[22], key[23]]),
            u32::from_le_bytes([key[24], key[25], key[26], key[27]]),
            u32::from_le_bytes([key[28], key[29], key[30], key[31]]),
        ];
        Poly1305 {
            r,
            h: [0u32; 5],
            pad,
            buf: [0u8; 16],
            buflen: 0,
        }
    }

    fn block(&mut self, m: &[u8; 16], hibit: u32) {
        let t0 = u32::from_le_bytes([m[0], m[1], m[2], m[3]]);
        let t1 = u32::from_le_bytes([m[4], m[5], m[6], m[7]]);
        let t2 = u32::from_le_bytes([m[8], m[9], m[10], m[11]]);
        let t3 = u32::from_le_bytes([m[12], m[13], m[14], m[15]]);

        let h0 = self.h[0].wrapping_add(t0 & 0x3ffffff);
        let h1 = self
            .h[1]
            .wrapping_add(((t0 >> 26) | (t1 << 6)) & 0x3ffffff);
        let h2 = self
            .h[2]
            .wrapping_add(((t1 >> 20) | (t2 << 12)) & 0x3ffffff);
        let h3 = self
            .h[3]
            .wrapping_add(((t2 >> 14) | (t3 << 18)) & 0x3ffffff);
        let h4 = self.h[4].wrapping_add((t3 >> 8) | hibit);

        let r0 = self.r[0] as u64;
        let r1 = self.r[1] as u64;
        let r2 = self.r[2] as u64;
        let r3 = self.r[3] as u64;
        let r4 = self.r[4] as u64;
        let s1 = r1 * 5;
        let s2 = r2 * 5;
        let s3 = r3 * 5;
        let s4 = r4 * 5;

        let (h0, h1, h2, h3, h4) = (h0 as u64, h1 as u64, h2 as u64, h3 as u64, h4 as u64);

        let d0 = h0 * r0 + h1 * s4 + h2 * s3 + h3 * s2 + h4 * s1;
        let mut d1 = h0 * r1 + h1 * r0 + h2 * s4 + h3 * s3 + h4 * s2;
        let mut d2 = h0 * r2 + h1 * r1 + h2 * r0 + h3 * s4 + h4 * s3;
        let mut d3 = h0 * r3 + h1 * r2 + h2 * r1 + h3 * r0 + h4 * s4;
        let mut d4 = h0 * r4 + h1 * r3 + h2 * r2 + h3 * r1 + h4 * r0;

        let mut c = d0 >> 26;
        let n0 = (d0 as u32) & 0x3ffffff;
        d1 += c;
        c = d1 >> 26;
        let n1 = (d1 as u32) & 0x3ffffff;
        d2 += c;
        c = d2 >> 26;
        let n2 = (d2 as u32) & 0x3ffffff;
        d3 += c;
        c = d3 >> 26;
        let n3 = (d3 as u32) & 0x3ffffff;
        d4 += c;
        c = d4 >> 26;
        let n4 = (d4 as u32) & 0x3ffffff;
        let n0 = n0.wrapping_add((c as u32) * 5);
        let carry = n0 >> 26;
        let n0 = n0 & 0x3ffffff;

        self.h = [n0, n1.wrapping_add(carry), n2, n3, n4];
    }

    fn update(&mut self, mut data: &[u8]) {
        if self.buflen > 0 {
            let need = 16 - self.buflen;
            let take = need.min(data.len());
            self.buf[self.buflen..self.buflen + take].copy_from_slice(&data[..take]);
            self.buflen += take;
            data = &data[take..];
            if self.buflen == 16 {
                let block = self.buf;
                self.block(&block, 1 << 24);
                self.buflen = 0;
            }
        }
        while data.len() >= 16 {
            let mut block = [0u8; 16];
            block.copy_from_slice(&data[..16]);
            self.block(&block, 1 << 24);
            data = &data[16..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buflen = data.len();
        }
    }

    fn finish(mut self) -> [u8; 16] {
        if self.buflen > 0 {
            let mut block = [0u8; 16];
            block[..self.buflen].copy_from_slice(&self.buf[..self.buflen]);
            block[self.buflen] = 1;
            self.block(&block, 0);
        }

        let mut h = self.h;
        let mut c = h[1] >> 26;
        h[1] &= 0x3ffffff;
        h[2] += c;
        c = h[2] >> 26;
        h[2] &= 0x3ffffff;
        h[3] += c;
        c = h[3] >> 26;
        h[3] &= 0x3ffffff;
        h[4] += c;
        c = h[4] >> 26;
        h[4] &= 0x3ffffff;
        h[0] += c * 5;
        c = h[0] >> 26;
        h[0] &= 0x3ffffff;
        h[1] += c;

        let mut g0 = h[0].wrapping_add(5);
        let mut c = g0 >> 26;
        g0 &= 0x3ffffff;
        let mut g1 = h[1].wrapping_add(c);
        c = g1 >> 26;
        g1 &= 0x3ffffff;
        let mut g2 = h[2].wrapping_add(c);
        c = g2 >> 26;
        g2 &= 0x3ffffff;
        let mut g3 = h[3].wrapping_add(c);
        c = g3 >> 26;
        g3 &= 0x3ffffff;
        let g4 = h[4].wrapping_add(c).wrapping_sub(1 << 26);

        let mask = (g4 >> 31).wrapping_sub(1);
        let nmask = !mask;
        let h0 = (h[0] & nmask) | (g0 & mask);
        let h1 = (h[1] & nmask) | (g1 & mask);
        let h2 = (h[2] & nmask) | (g2 & mask);
        let h3 = (h[3] & nmask) | (g3 & mask);
        let h4 = (h[4] & nmask) | (g4 & mask);

        let f0 = (h0 | (h1 << 26)) as u64;
        let f1 = ((h1 >> 6) | (h2 << 20)) as u64;
        let f2 = ((h2 >> 12) | (h3 << 14)) as u64;
        let f3 = ((h3 >> 18) | (h4 << 8)) as u64;

        let mut f = f0 + self.pad[0] as u64;
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&(f as u32).to_le_bytes());
        f = f1 + self.pad[1] as u64 + (f >> 32);
        out[4..8].copy_from_slice(&(f as u32).to_le_bytes());
        f = f2 + self.pad[2] as u64 + (f >> 32);
        out[8..12].copy_from_slice(&(f as u32).to_le_bytes());
        f = f3 + self.pad[3] as u64 + (f >> 32);
        out[12..16].copy_from_slice(&(f as u32).to_le_bytes());
        out
    }
}

fn poly1305_auth(key: &[u8; 32], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let mut p = Poly1305::new(key);
    p.update(aad);
    let pad = (16 - (aad.len() % 16)) % 16;
    if pad > 0 {
        p.update(&[0u8; 16][..pad]);
    }
    p.update(ct);
    let pad = (16 - (ct.len() % 16)) % 16;
    if pad > 0 {
        p.update(&[0u8; 16][..pad]);
    }
    p.update(&(aad.len() as u64).to_le_bytes());
    p.update(&(ct.len() as u64).to_le_bytes());
    p.finish()
}

pub fn aead_seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let block0 = chacha20_block(key, 0, nonce);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&block0[..32]);
    let mut ct = plaintext.to_vec();
    chacha20_xor(key, nonce, 1, &mut ct);
    let tag = poly1305_auth(&otk, aad, &ct);
    ct.extend_from_slice(&tag);
    ct
}

pub fn aead_open(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    sealed: &[u8],
) -> Option<Vec<u8>> {
    if sealed.len() < 16 {
        return None;
    }
    let (ct, tag) = sealed.split_at(sealed.len() - 16);
    let block0 = chacha20_block(key, 0, nonce);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&block0[..32]);
    let expect = poly1305_auth(&otk, aad, ct);
    let mut diff = 0u8;
    for i in 0..16 {
        diff |= expect[i] ^ tag[i];
    }
    if diff != 0 {
        return None;
    }
    let mut pt = ct.to_vec();
    chacha20_xor(key, nonce, 1, &mut pt);
    Some(pt)
}

const M51: u64 = (1u64 << 51) - 1;

#[derive(Clone, Copy)]
struct Fe([u64; 5]);

impl Fe {
    fn zero() -> Self {
        Fe([0; 5])
    }
    fn one() -> Self {
        Fe([1, 0, 0, 0, 0])
    }

    fn add(&self, o: &Fe) -> Fe {
        Fe([
            self.0[0] + o.0[0],
            self.0[1] + o.0[1],
            self.0[2] + o.0[2],
            self.0[3] + o.0[3],
            self.0[4] + o.0[4],
        ])
    }

    fn sub(&self, o: &Fe) -> Fe {

        Fe([
            (self.0[0] + 0xfffffffffffda) - o.0[0],
            (self.0[1] + 0xffffffffffffe) - o.0[1],
            (self.0[2] + 0xffffffffffffe) - o.0[2],
            (self.0[3] + 0xffffffffffffe) - o.0[3],
            (self.0[4] + 0xffffffffffffe) - o.0[4],
        ])
    }

    fn mul(&self, o: &Fe) -> Fe {
        let a0 = self.0[0] as u128;
        let a1 = self.0[1] as u128;
        let a2 = self.0[2] as u128;
        let a3 = self.0[3] as u128;
        let a4 = self.0[4] as u128;
        let b0 = o.0[0] as u128;
        let b1 = o.0[1] as u128;
        let b2 = o.0[2] as u128;
        let b3 = o.0[3] as u128;
        let b4 = o.0[4] as u128;
        let b1_19 = b1 * 19;
        let b2_19 = b2 * 19;
        let b3_19 = b3 * 19;
        let b4_19 = b4 * 19;

        let mut c0 = a0 * b0 + a1 * b4_19 + a2 * b3_19 + a3 * b2_19 + a4 * b1_19;
        let mut c1 = a0 * b1 + a1 * b0 + a2 * b4_19 + a3 * b3_19 + a4 * b2_19;
        let mut c2 = a0 * b2 + a1 * b1 + a2 * b0 + a3 * b4_19 + a4 * b3_19;
        let mut c3 = a0 * b3 + a1 * b2 + a2 * b1 + a3 * b0 + a4 * b4_19;
        let mut c4 = a0 * b4 + a1 * b3 + a2 * b2 + a3 * b1 + a4 * b0;

        let mask = M51 as u128;
        c1 += c0 >> 51;
        c0 &= mask;
        c2 += c1 >> 51;
        c1 &= mask;
        c3 += c2 >> 51;
        c2 &= mask;
        c4 += c3 >> 51;
        c3 &= mask;
        c0 += 19 * (c4 >> 51);
        c4 &= mask;
        c1 += c0 >> 51;
        c0 &= mask;

        Fe([c0 as u64, c1 as u64, c2 as u64, c3 as u64, c4 as u64])
    }

    fn mul_small(&self, n: u64) -> Fe {
        let n = n as u128;
        let mut c = [
            self.0[0] as u128 * n,
            self.0[1] as u128 * n,
            self.0[2] as u128 * n,
            self.0[3] as u128 * n,
            self.0[4] as u128 * n,
        ];
        let mask = M51 as u128;
        let mut carry = c[0] >> 51;
        c[0] &= mask;
        c[1] += carry;
        carry = c[1] >> 51;
        c[1] &= mask;
        c[2] += carry;
        carry = c[2] >> 51;
        c[2] &= mask;
        c[3] += carry;
        carry = c[3] >> 51;
        c[3] &= mask;
        c[4] += carry;
        carry = c[4] >> 51;
        c[4] &= mask;
        c[0] += 19 * carry;
        Fe([c[0] as u64, c[1] as u64, c[2] as u64, c[3] as u64, c[4] as u64])
    }

    fn from_bytes(b: &[u8; 32]) -> Fe {
        let load = |i: usize| -> u64 {
            let mut v = [0u8; 8];
            v.copy_from_slice(&b[i..i + 8]);
            u64::from_le_bytes(v)
        };
        Fe([
            load(0) & M51,
            (load(6) >> 3) & M51,
            (load(12) >> 6) & M51,
            (load(19) >> 1) & M51,
            (load(24) >> 12) & M51,
        ])
    }

    fn to_bytes(self) -> [u8; 32] {
        let mut t = self.0;
        let mut c;
        c = t[0] >> 51;
        t[0] &= M51;
        t[1] += c;
        c = t[1] >> 51;
        t[1] &= M51;
        t[2] += c;
        c = t[2] >> 51;
        t[2] &= M51;
        t[3] += c;
        c = t[3] >> 51;
        t[3] &= M51;
        t[4] += c;
        c = t[4] >> 51;
        t[4] &= M51;
        t[0] += c * 19;
        c = t[0] >> 51;
        t[0] &= M51;
        t[1] += c;

        let mut q = (t[0] + 19) >> 51;
        q = (t[1] + q) >> 51;
        q = (t[2] + q) >> 51;
        q = (t[3] + q) >> 51;
        q = (t[4] + q) >> 51;
        t[0] += 19 * q;
        c = t[0] >> 51;
        t[0] &= M51;
        t[1] += c;

        let mut out = [0u8; 32];
        let mut acc: u128 = 0;
        let mut bits = 0usize;
        let mut oi = 0usize;
        for limb in t.iter() {
            acc |= (*limb as u128) << bits;
            bits += 51;
            while bits >= 8 {
                out[oi] = acc as u8;
                oi += 1;
                acc >>= 8;
                bits -= 8;
            }
        }
        if oi < 32 {
            out[oi] = acc as u8;
        }
        out
    }

    fn invert(&self) -> Fe {

        let mut exp = [0xffu8; 32];
        exp[0] = 0xeb;
        exp[31] = 0x7f;
        let mut r = Fe::one();
        for i in (0..256).rev() {
            r = r.mul(&r);
            if (exp[i / 8] >> (i % 8)) & 1 == 1 {
                r = r.mul(self);
            }
        }
        r
    }
}

pub const X25519_BASE: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub fn x25519(scalar: &[u8; 32], u: &[u8; 32]) -> [u8; 32] {
    let mut k = *scalar;
    k[0] &= 248;
    k[31] &= 127;
    k[31] |= 64;

    let x1 = Fe::from_bytes(u);
    let mut x2 = Fe::one();
    let mut z2 = Fe::zero();
    let mut x3 = x1;
    let mut z3 = Fe::one();
    let mut swap = 0u8;

    for t in (0..255).rev() {
        let kt = (k[t / 8] >> (t % 8)) & 1;
        swap ^= kt;
        if swap == 1 {
            std::mem::swap(&mut x2, &mut x3);
            std::mem::swap(&mut z2, &mut z3);
        }
        swap = kt;

        let a = x2.add(&z2);
        let aa = a.mul(&a);
        let b = x2.sub(&z2);
        let bb = b.mul(&b);
        let e = aa.sub(&bb);
        let c = x3.add(&z3);
        let d = x3.sub(&z3);
        let da = d.mul(&a);
        let cb = c.mul(&b);
        let da_cb = da.add(&cb);
        let da_cb2 = da.sub(&cb);
        x3 = da_cb.mul(&da_cb);
        z3 = x1.mul(&da_cb2.mul(&da_cb2));
        x2 = aa.mul(&bb);
        let t = aa.add(&e.mul_small(121665));
        z2 = e.mul(&t);
    }

    if swap == 1 {
        std::mem::swap(&mut x2, &mut x3);
        std::mem::swap(&mut z2, &mut z3);
    }

    x2.mul(&z2.invert()).to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        hex_decode(s).unwrap()
    }

    #[test]
    fn sha256_kat() {
        assert_eq!(
            hex_encode(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_encode(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let long = vec![b'a'; 1_000_000];
        assert_eq!(
            hex_encode(&sha256(&long)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn hmac_kat() {
        let key = [0x0bu8; 20];
        assert_eq!(
            hex_encode(&hmac_sha256(&key, b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hkdf_kat() {
        let ikm = hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = hex("000102030405060708090a0b0c");
        let info = hex("f0f1f2f3f4f5f6f7f8f9");
        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            hex_encode(&prk),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );
        let okm = hkdf_expand(&prk, &info, 42);
        assert_eq!(
            hex_encode(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    #[test]
    fn chacha20poly1305_kat() {
        let key: [u8; 32] =
            hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .try_into()
                .unwrap();
        let nonce: [u8; 12] = hex("070000004041424344454647").try_into().unwrap();
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let pt = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let sealed = aead_seal(&key, &nonce, &aad, pt);
        let expect_ct = "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116";
        let expect_tag = "1ae10b594f09e26a7e902ecbd0600691";
        assert_eq!(hex_encode(&sealed[..pt.len()]), expect_ct);
        assert_eq!(hex_encode(&sealed[pt.len()..]), expect_tag);
        assert_eq!(aead_open(&key, &nonce, &aad, &sealed).unwrap(), pt);

        let mut bad = sealed.clone();
        bad[0] ^= 1;
        assert!(aead_open(&key, &nonce, &aad, &bad).is_none());
    }

    #[test]
    fn x25519_kat() {
        let s: [u8; 32] =
            hex("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4")
                .try_into()
                .unwrap();
        let u: [u8; 32] =
            hex("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c")
                .try_into()
                .unwrap();
        assert_eq!(
            hex_encode(&x25519(&s, &u)),
            "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552"
        );

        let s: [u8; 32] =
            hex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
                .try_into()
                .unwrap();
        assert_eq!(
            hex_encode(&x25519(&s, &X25519_BASE)),
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
        );

        let a = random_array::<32>();
        let b = random_array::<32>();
        let pa = x25519(&a, &X25519_BASE);
        let pb = x25519(&b, &X25519_BASE);
        assert_eq!(x25519(&a, &pb), x25519(&b, &pa));
    }
}
