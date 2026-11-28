
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut acc: u32 = 0;
    let mut bits: u8 = 0;
    for c in s.chars() {
        if c == '=' { break; }
        let v = match c {
            'A'..='Z' => c as u8 - b'A',
            'a'..='z' => c as u8 - b'a' + 26,
            '0'..='9' => c as u8 - b'0' + 52,
            '+' => 62,
            '/' => 63,
            '\n' | '\r' | ' ' | '\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            buf.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(buf)
}

pub fn pem_decode(pem: &str) -> Option<Vec<u8>> {
    let mut b64 = String::new();
    let mut in_block = false;
    for line in pem.lines() {
        let l = line.trim();
        if l.starts_with("-----BEGIN") { in_block = true; continue; }
        if l.starts_with("-----END") { in_block = false; continue; }
        if in_block { b64.push_str(l); }
    }
    base64_decode(&b64)
}

pub fn read_der_len(data: &[u8]) -> Option<(usize, usize)> {
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

pub struct CertInfo {
    pub subject: Vec<u8>,
    pub issuer: Vec<u8>,
    pub spki_der: Vec<u8>,
    pub signature: Vec<u8>,
    pub sig_alg_oid: Vec<u8>,
    pub tbs_der: Vec<u8>,
}

pub fn parse_cert_minimal(der: &[u8]) -> Option<CertInfo> {
    if der.is_empty() || der[0] != 0x30 { return None; }
    let (cert_len, cert_hdr) = read_der_len(&der[1..])?;
    let cert_content = &der[1+cert_hdr..1+cert_hdr+cert_len];
    if cert_content.is_empty() || cert_content[0] != 0x30 { return None; }
    let (tbs_len, tbs_hdr) = read_der_len(&cert_content[1..])?;
    let tbs_der = cert_content[..1+tbs_hdr+tbs_len].to_vec();
    let mut pos = 1 + tbs_hdr + tbs_len;
    if pos >= cert_content.len() || cert_content[pos] != 0x30 { return None; }
    let (alg_len, alg_hdr) = read_der_len(&cert_content[pos+1..])?;
    let alg_end = pos + 1 + alg_hdr + alg_len;
    let alg_content = &cert_content[pos+1+alg_hdr..alg_end];
    let mut oid = Vec::new();
    if !alg_content.is_empty() && alg_content[0] == 0x06 {
        let (olen, ohdr) = read_der_len(&alg_content[1..])?;
        oid.extend_from_slice(&alg_content[1+ohdr..1+ohdr+olen]);
    }
    pos = alg_end;
    if pos >= cert_content.len() || cert_content[pos] != 0x03 { return None; }
    let (sig_len, sig_hdr) = read_der_len(&cert_content[pos+1..])?;
    let sig_content = &cert_content[pos+1+sig_hdr..pos+1+sig_hdr+sig_len];
    let signature = if sig_content.is_empty() { Vec::new() } else { sig_content[1..].to_vec() };

    let tbs = &cert_content[..1+tbs_hdr+tbs_len];
    let (_, th) = read_der_len(&tbs[1..])?;
    let mut tp = 1 + th;
    if tp < tbs.len() && tbs[tp] == 0xa0 {
        let (vlen, vhdr) = read_der_len(&tbs[tp+1..])?;
        tp += 1 + vhdr + vlen;
    }
    if tp >= tbs.len() || tbs[tp] != 0x02 { return None; }
    let (slen, shdr) = read_der_len(&tbs[tp+1..])?;
    tp += 1 + shdr + slen;
    if tp >= tbs.len() || tbs[tp] != 0x30 { return None; }
    let (alen, ahdr) = read_der_len(&tbs[tp+1..])?;
    tp += 1 + ahdr + alen;
    if tp >= tbs.len() || tbs[tp] != 0x30 { return None; }
    let (ilen, ihdr) = read_der_len(&tbs[tp+1..])?;
    let issuer = tbs[tp..tp+1+ihdr+ilen].to_vec();
    tp += 1 + ihdr + ilen;
    if tp >= tbs.len() || tbs[tp] != 0x30 { return None; }
    let (vlen, vhdr) = read_der_len(&tbs[tp+1..])?;
    tp += 1 + vhdr + vlen;
    if tp >= tbs.len() || tbs[tp] != 0x30 { return None; }
    let (sulen, suhdr) = read_der_len(&tbs[tp+1..])?;
    let subject = tbs[tp..tp+1+suhdr+sulen].to_vec();
    tp += 1 + suhdr + sulen;
    if tp >= tbs.len() || tbs[tp] != 0x30 { return None; }
    let (plen, phdr) = read_der_len(&tbs[tp+1..])?;
    let spki_der = tbs[tp..tp+1+phdr+plen].to_vec();

    Some(CertInfo { subject, issuer, spki_der, signature, sig_alg_oid: oid, tbs_der })
}

pub fn verify_cert(cert_der: &[u8], ca_spki_der: &[u8]) -> bool {
    let cert = match parse_cert_minimal(cert_der) { Some(c) => c, None => return false };
    let rsa_oid: [u8; 9] = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
    if cert.sig_alg_oid == rsa_oid {
        crate::rsa::rsa_verify_pkcs1v15_sha256(ca_spki_der, &cert.tbs_der, &cert.signature)
    } else {
        false
    }
}
