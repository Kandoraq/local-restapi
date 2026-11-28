use crate::crypto::sha256;

fn parse_rsa_spki(der: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut i = 0;
    while i < der.len() {
        if der[i] == 0x03 {
            let (len, hdr) = read_der_len(&der[i+1..])?;
            let content_start = i + 1 + hdr;
            let content = &der[content_start..content_start+len];
            if content.is_empty() { return None; }
            let inner = &content[1..];
            let mut pos = 0;
            if inner.is_empty() || inner[pos] != 0x30 { return None; }
            let (_, h) = read_der_len(&inner[pos+1..])?;
            pos = 1 + h;
            if pos >= inner.len() || inner[pos] != 0x02 { return None; }
            let (nlen, nh) = read_der_len(&inner[pos+1..])?;
            let nstart = pos + 1 + nh;
            let mut n = inner[nstart..nstart+nlen].to_vec();
            if n.len() > 1 && n[0] == 0 { n.remove(0); }
            pos = nstart + nlen;
            if pos >= inner.len() || inner[pos] != 0x02 { return None; }
            let (elen, eh) = read_der_len(&inner[pos+1..])?;
            let estart = pos + 1 + eh;
            let mut e = inner[estart..estart+elen].to_vec();
            if e.len() > 1 && e[0] == 0 { e.remove(0); }
            return Some((n, e));
        }
        i += 1;
    }
    None
}

fn read_der_len(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() { return None; }
    if data[0] < 0x80 {
        Some((data[0] as usize, 1))
    } else {
        let num = (data[0] & 0x7f) as usize;
        if num == 0 || num > 4 || data.len() < 1 + num { return None; }
        let mut len = 0usize;
        for i in 0..num {
            len = (len << 8) | data[1 + i] as usize;
        }
        Some((len, 1 + num))
    }
}

pub fn rsa_verify_pkcs1v15_sha256(spki_der: &[u8], message: &[u8], signature: &[u8]) -> bool {
    let (n_bytes, e_bytes) = match parse_rsa_spki(spki_der) { Some(v) => v, None => return false };
    let mut n = n_bytes.clone();
    while n.len() > 1 && n[0] == 0 { n.remove(0); }
    let mut e = e_bytes.clone();
    while e.len() > 1 && e[0] == 0 { e.remove(0); }
    let mut sig = signature.to_vec();
    while sig.len() > 1 && sig[0] == 0 { sig.remove(0); }
    if sig.len() < n.len() {
        let mut padded = vec![0u8; n.len() - sig.len()];
        padded.extend_from_slice(&sig);
        sig = padded;
    }
    if sig.len() != n.len() { return false; }

    let k = n.len().div_ceil(8);
    // use BigUint from crate::crypto? We removed BigUint. Implement with u64 limbs via montgomery.
    let n_le = be_to_le_limbs(&n, k);
    let e_le = be_to_le_limbs(&e, e.len().div_ceil(8));
    let sig_le = be_to_le_limbs(&sig, k);

    let np = inv_mod_2pow64(n_le[0]).wrapping_neg();
    let mut r2 = vec![0u64; k];
    r2[0] = 1;
    for _ in 0..(2 * k * 64) {
        let mut carry = 0u64;
        for r in r2.iter_mut() {
            let v = *r;
            *r = (v << 1) | carry;
            carry = v >> 63;
        }
        if carry != 0 || ge_limbs(&r2, &n_le) {
            r2 = sub_limbs(&r2, &n_le);
        }
    }
    let sig_bar = mont_mul(&sig_le, &r2, &n_le, np);
    let one = { let mut v = vec![0u64; k]; v[0] = 1; v };
    let mut res_bar = mont_mul(&one, &r2, &n_le, np);
    let mut bits: Vec<u8> = Vec::new();
    for &limb in e_le.iter().rev() {
        for i in (0..64).rev() {
            bits.push(((limb >> i) & 1) as u8);
        }
    }
    let mut started = false;
    for &bit in &bits {
        if !started {
            if bit == 0 { continue; }
            started = true;
            res_bar = sig_bar.clone();
            continue;
        }
        res_bar = mont_mul(&res_bar, &res_bar, &n_le, np);
        if bit == 1 {
            res_bar = mont_mul(&res_bar, &sig_bar, &n_le, np);
        }
    }
    if !started {
        res_bar = mont_mul(&one, &r2, &n_le, np);
    }
    let result_le = mont_mul(&res_bar, &one, &n_le, np);
    let mut em = Vec::with_capacity(k * 8);
    for i in (0..k).rev() {
        let limb = if i < result_le.len() { result_le[i] } else { 0 };
        em.extend_from_slice(&limb.to_be_bytes());
    }
    let kbytes = n.len();
    if em.len() != kbytes { return false; }
    let em_padded = em;

    let digest = sha256(message);
    let digest_info: [u8; 51] = {
        let mut v = [0u8; 51];
        let prefix: [u8; 19] = [
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01,
            0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20,
        ];
        v[..19].copy_from_slice(&prefix);
        v[19..].copy_from_slice(&digest);
        v
    };
    let mut expected = vec![0u8; kbytes];
    expected[0] = 0x00;
    expected[1] = 0x01;
    let ps_len = kbytes - 3 - digest_info.len();
    if ps_len < 8 { return false; }
    for v in expected[2..2+ps_len].iter_mut() { *v = 0xFF; }
    expected[2+ps_len] = 0x00;
    expected[3+ps_len..].copy_from_slice(&digest_info);
    em_padded == expected
}

fn be_to_le_limbs(bytes: &[u8], nlimbs: usize) -> Vec<u64> {
    let mut out = vec![0u64; nlimbs];
    let mut idx = bytes.len();
    for out_limb in out.iter_mut() {
        let mut val = 0u64;
        for j in 0..8 {
            if idx == 0 { break; }
            idx -= 1;
            val |= (bytes[idx] as u64) << (j * 8);
        }
        *out_limb = val;
    }
    out
}

pub fn inv_mod_2pow64(a: u64) -> u64 {
    let mut x = 1u64;
    for _ in 0..6 {
        x = x.wrapping_mul(2u64.wrapping_sub(a.wrapping_mul(x)));
    }
    x
}

pub fn ge_limbs(a: &[u64], b: &[u64]) -> bool {
    let n = a.len().max(b.len());
    for i in (0..n).rev() {
        let av = if i < a.len() { a[i] } else { 0 };
        let bv = if i < b.len() { b[i] } else { 0 };
        if av != bv { return av > bv; }
    }
    true
}

pub fn sub_limbs(a: &[u64], b: &[u64]) -> Vec<u64> {
    let n = a.len();
    let mut res = vec![0u64; n];
    let mut borrow = 0u64;
    for i in 0..n {
        let av = a[i];
        let bv = if i < b.len() { b[i] } else { 0 };
        let (r, ov1) = av.overflowing_sub(bv);
        let (r2, ov2) = r.overflowing_sub(borrow);
        res[i] = r2;
        borrow = if ov1 || ov2 { 1 } else { 0 };
    }
    res
}

pub fn mont_mul(a: &[u64], b: &[u64], n: &[u64], np: u64) -> Vec<u64> {
    let k = n.len();
    let mut t = vec![0u64; 2 * k];
    for i in 0..k {
        let mut carry = 0u64;
        for j in 0..k {
            let prod = (a[i] as u128) * (b[j] as u128) + (t[i + j] as u128) + (carry as u128);
            t[i + j] = prod as u64;
            carry = (prod >> 64) as u64;
        }
        // carry propagates into high half
        let mut c = carry;
        let mut j = i + k;
        while c != 0 && j < 2 * k {
            let (s, ov) = t[j].overflowing_add(c);
            t[j] = s;
            c = if ov { 1 } else { 0 };
            j += 1;
        }
    }
    for i in 0..k {
        let m = t[i].wrapping_mul(np);
        let mut carry = 0u64;
        for j in 0..k {
            let prod = (m as u128) * (n[j] as u128) + (t[i + j] as u128) + (carry as u128);
            t[i + j] = prod as u64;
            carry = (prod >> 64) as u64;
        }
        let mut c = carry;
        for tj in t[(i + k)..(2 * k)].iter_mut() {
            let (s, ov) = tj.overflowing_add(c);
            *tj = s;
            c = if ov { 1 } else { 0 };
            if c == 0 { break; }
        }
    }
    let mut res = t[k..2 * k].to_vec();
    if ge_limbs(&res, n) {
        res = sub_limbs(&res, n);
    }
    res
}


#[cfg(test)]
mod ntest {
    use super::*;
    use crate::crypto::hex_decode;
    #[test]
    fn mont_identity_realn() {
        let n_bytes = hex_decode("D72B64BADFB0E53536E208789BD7BB8C45289F065D90AB35E824BBD2F94E8AB10C453BFD4236C90CD7190D2ED8A03EEF5AC46BA0C28E3A1AF4974E61B4B1F01D").unwrap();
        let k = n_bytes.len().div_ceil(8);
        let n_le = be_to_le_limbs(&n_bytes, k);
        let np = inv_mod_2pow64(n_le[0]).wrapping_neg();
        // R2
        let mut r2v = vec![0u64; k];
        r2v[0] = 1;
        for _ in 0..(k*64*2) {
            let mut carry = 0u64;
            for r in r2v.iter_mut() { let v = *r; *r = (v << 1) | carry; carry = v >> 63; }
            if carry != 0 || ge_limbs(&r2v, &n_le) { r2v = sub_limbs(&r2v, &n_le); }
        }
        let one = { let mut v = vec![0u64; k]; v[0] = 1; v };
        let one_bar = mont_mul(&one, &r2v, &n_le, np);
        let back = mont_mul(&one_bar, &one, &n_le, np);
        assert_eq!(back, one, "round-trip failed");
    }
}

