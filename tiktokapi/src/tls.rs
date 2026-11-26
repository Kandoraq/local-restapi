use crate::crypto::*;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CT_HS: u8 = 22;
const CT_CCS: u8 = 20;
const CT_APP: u8 = 23;
const CT_ALERT: u8 = 21;

const SUITE_CHACHA20: u16 = 0x1303;

pub struct TlsConn {
    tcp: TcpStream,
    read_key: [u8; 32],
    read_iv: [u8; 12],
    read_seq: u64,
    write_key: [u8; 32],
    write_iv: [u8; 12],
    write_seq: u64,
    plain: Vec<u8>,
    ptr: usize,
    closed: bool,
}

fn err(kind: io::ErrorKind, msg: &str) -> io::Error {
    io::Error::new(kind, msg.to_string())
}

fn bad(msg: &str) -> io::Error {
    err(io::ErrorKind::InvalidData, msg)
}

fn u24(b: &[u8]) -> usize {
    ((b[0] as usize) << 16) | ((b[1] as usize) << 8) | (b[2] as usize)
}

fn handshake_msg(t: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 4);
    v.push(t);
    v.push((body.len() >> 16) as u8);
    v.push((body.len() >> 8) as u8);
    v.push(body.len() as u8);
    v.extend_from_slice(body);
    v
}

fn push_ext(out: &mut Vec<u8>, t: u16, data: &[u8]) {
    out.extend_from_slice(&t.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
}

fn nonce(iv: &[u8; 12], seq: u64) -> [u8; 12] {
    let mut n = *iv;
    let s = seq.to_be_bytes();
    for i in 0..8 {
        n[4 + i] ^= s[i];
    }
    n
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

fn build_client_hello(host: &str, random: &[u8; 32], sid: &[u8; 32], pubkey: &[u8; 32]) -> Vec<u8> {
    let mut exts = Vec::new();

    let mut sni = Vec::new();
    sni.extend_from_slice(&((host.len() + 3) as u16).to_be_bytes());
    sni.push(0);
    sni.extend_from_slice(&(host.len() as u16).to_be_bytes());
    sni.extend_from_slice(host.as_bytes());
    push_ext(&mut exts, 0, &sni);

    push_ext(&mut exts, 10, &[0, 2, 0, 0x1d]);

    let sigs: [u16; 9] = [0x0403, 0x0503, 0x0603, 0x0804, 0x0805, 0x0806, 0x0401, 0x0501, 0x0601];
    let mut sa = Vec::new();
    sa.extend_from_slice(&((sigs.len() * 2) as u16).to_be_bytes());
    for s in sigs.iter() {
        sa.extend_from_slice(&s.to_be_bytes());
    }
    push_ext(&mut exts, 13, &sa);

    push_ext(&mut exts, 43, &[2, 0x03, 0x04]);

    let mut ks = Vec::new();
    ks.extend_from_slice(&36u16.to_be_bytes());
    ks.extend_from_slice(&0x001du16.to_be_bytes());
    ks.extend_from_slice(&32u16.to_be_bytes());
    ks.extend_from_slice(pubkey);
    push_ext(&mut exts, 51, &ks);

    let mut body = Vec::new();
    body.extend_from_slice(&[0x03, 0x03]);
    body.extend_from_slice(random);
    body.push(32);
    body.extend_from_slice(sid);
    body.extend_from_slice(&[0x00, 0x02, 0x13, 0x03]);
    body.extend_from_slice(&[0x01, 0x00]);
    body.extend_from_slice(&(exts.len() as u16).to_be_bytes());
    body.extend_from_slice(&exts);
    handshake_msg(1, &body)
}

fn parse_server_hello(body: &[u8]) -> io::Result<([u8; 32], u16)> {
    if body.len() < 38 {
        return Err(bad("server hello pendek"));
    }
    let sidlen = body[34] as usize;
    let mut p = 35 + sidlen;
    if body.len() < p + 3 {
        return Err(bad("server hello rusak"));
    }
    let suite = u16::from_be_bytes([body[p], body[p + 1]]);
    p += 2;
    p += 1;
    if body.len() < p + 2 {
        return Err(bad("server hello without extensions"));
    }
    let extlen = u16::from_be_bytes([body[p], body[p + 1]]) as usize;
    p += 2;
    let end = (p + extlen).min(body.len());
    let mut key: Option<[u8; 32]> = None;
    while p + 4 <= end {
        let t = u16::from_be_bytes([body[p], body[p + 1]]);
        let l = u16::from_be_bytes([body[p + 2], body[p + 3]]) as usize;
        p += 4;
        if p + l > end {
            break;
        }
        if t == 51 && l >= 36 {
            let group = u16::from_be_bytes([body[p], body[p + 1]]);
            let klen = u16::from_be_bytes([body[p + 2], body[p + 3]]) as usize;
            if group == 0x001d && klen == 32 {
                let mut k = [0u8; 32];
                k.copy_from_slice(&body[p + 4..p + 36]);
                key = Some(k);
            }
        }
        p += l;
    }
    match key {
        Some(k) => Ok((k, suite)),
        None => Err(bad("server did not send x25519 key_share")),
    }
}

fn leaf_cert_from_cert_msg(body: &[u8]) -> Option<Vec<u8>> {
    if body.is_empty() {
        return None;
    }
    let ctx = body[0] as usize;
    let mut p = 1 + ctx;
    if body.len() < p + 3 {
        return None;
    }
    let listlen = u24(&body[p..p + 3]);
    p += 3;
    if body.len() < p + 3 {
        return None;
    }
    let clen = u24(&body[p..p + 3]);
    p += 3;
    if listlen == 0 || clen == 0 || body.len() < p + clen {
        return None;
    }
    Some(body[p..p + clen].to_vec())
}

fn alert_text(payload: &[u8]) -> String {
    if payload.len() >= 2 {
        format!("TLS alert {} {}", payload[0], payload[1])
    } else {
        "TLS alert".to_string()
    }
}

fn resolve(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    type Cache = HashMap<String, (Instant, Vec<SocketAddr>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = format!("{}:{}", host, port);
    if let Ok(m) = cache.lock() {
        if let Some((t, v)) = m.get(&key) {
            if t.elapsed() < Duration::from_secs(300) && !v.is_empty() {
                return Ok(v.clone());
            }
        }
    }
    let mut last: Option<io::Error> = None;
    for attempt in 0..5 {
        match (host, port).to_socket_addrs() {
            Ok(it) => {
                let v: Vec<SocketAddr> = it.collect();
                if !v.is_empty() {
                    if let Ok(mut m) = cache.lock() {
                        m.insert(key.clone(), (Instant::now(), v.clone()));
                    }
                    return Ok(v);
                }
                last = Some(err(io::ErrorKind::NotFound, "empty DNS response"));
            }
            Err(e) => last = Some(e),
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_millis(150 * (attempt + 1) as u64));
        }
    }
    Err(err(
        io::ErrorKind::NotFound,
        &format!("DNS {}: {}", host, last.map(|e| e.to_string()).unwrap_or_default()),
    ))
}

impl TlsConn {
    pub fn connect(host: &str, port: u16, pin: Option<[u8; 32]>, timeout: u64) -> io::Result<TlsConn> {
        let mut tcp = None;
        let mut last = None;
        for _round in 0..2 {
            let addrs = match resolve(host, port) {
                Ok(v) => v,
                Err(e) => {
                    last = Some(e);
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            };
            for a in addrs {
                match TcpStream::connect_timeout(&a, Duration::from_secs(timeout.min(20))) {
                    Ok(s) => {
                        tcp = Some(s);
                        break;
                    }
                    Err(e) => last = Some(e),
                }
            }
            if tcp.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        let tcp = match tcp {
            Some(t) => t,
            None => return Err(last.unwrap_or_else(|| err(io::ErrorKind::Other, "connection failed"))),
        };
        tcp.set_nodelay(true).ok();
        tcp.set_read_timeout(Some(Duration::from_secs(timeout))).ok();
        tcp.set_write_timeout(Some(Duration::from_secs(timeout))).ok();

        let mut c = TlsConn {
            tcp,
            read_key: [0; 32],
            read_iv: [0; 12],
            read_seq: 0,
            write_key: [0; 32],
            write_iv: [0; 12],
            write_seq: 0,
            plain: Vec::new(),
            ptr: 0,
            closed: false,
        };
        c.handshake(host, pin)?;
        Ok(c)
    }


    fn set_read_secret(&mut self, secret: &[u8; 32]) {
        let k = tls_expand_label(secret, "key", &[], 32);
        let iv = tls_expand_label(secret, "iv", &[], 12);
        self.read_key.copy_from_slice(&k);
        self.read_iv.copy_from_slice(&iv);
        self.read_seq = 0;
    }

    fn set_write_secret(&mut self, secret: &[u8; 32]) {
        let k = tls_expand_label(secret, "key", &[], 32);
        let iv = tls_expand_label(secret, "iv", &[], 12);
        self.write_key.copy_from_slice(&k);
        self.write_iv.copy_from_slice(&iv);
        self.write_seq = 0;
    }

    fn handshake(&mut self, host: &str, pin: Option<[u8; 32]>) -> io::Result<()> {
        let random = random_array::<32>();
        let sid = random_array::<32>();
        let priv_key = random_array::<32>();
        let pub_key = x25519(&priv_key, &X25519_BASE);
        let ch = build_client_hello(host, &random, &sid, &pub_key);
        self.write_plain(CT_HS, &ch)?;

        let mut transcript = ch.clone();
        let mut hs: Vec<u8> = Vec::new();
        let mut hs_secret: Option<[u8; 32]> = None;
        let mut c_hs: Option<[u8; 32]> = None;
        let mut s_hs: Option<[u8; 32]> = None;

        loop {
            let (rtype, payload) = self.read_record()?;
            let data = match rtype {
                CT_HS => payload,
                CT_CCS => continue,
                CT_APP => {
                    let (ct, inner) = self.decrypt_record(&payload)?;
                    if ct == CT_ALERT {
                        return Err(err(io::ErrorKind::ConnectionAborted, &alert_text(&inner)));
                    }
                    if ct != CT_HS {
                        continue;
                    }
                    inner
                }
                CT_ALERT => {
                    return Err(err(io::ErrorKind::ConnectionAborted, &alert_text(&payload)));
                }
                _ => continue,
            };
            hs.extend_from_slice(&data);
            loop {
                if hs.len() < 4 {
                    break;
                }
                let mlen = u24(&hs[1..4]);
                if hs.len() < 4 + mlen {
                    break;
                }
                let msg: Vec<u8> = hs.drain(..4 + mlen).collect();
                let mtype = msg[0];
                let body = &msg[4..];
                match mtype {
                    2 => {
                        let (server_pub, suite) = parse_server_hello(body)?;
                        if suite != SUITE_CHACHA20 {
                            return Err(bad("server selected an unsupported cipher"));
                        }
                        transcript.extend_from_slice(&msg);
                        let shared = x25519(&priv_key, &server_pub);
                        let early = hkdf_extract(&[0u8; 32], &[0u8; 32]);
                        let derived = tls_expand_label(&early, "derived", &sha256(&[]), 32);
                        let hssec = hkdf_extract(&derived, &shared);
                        let th = sha256(&transcript);
                        let chs = tls_expand_label(&hssec, "c hs traffic", &th, 32);
                        let shs = tls_expand_label(&hssec, "s hs traffic", &th, 32);
                        let mut a = [0u8; 32];
                        a.copy_from_slice(&chs);
                        c_hs = Some(a);
                        let mut b = [0u8; 32];
                        b.copy_from_slice(&shs);
                        s_hs = Some(b);
                        self.set_read_secret(&b);
                        self.set_write_secret(&a);
                        self.write_seq = 0;
                        hs_secret = Some(hssec);
                    }
                    11 => {
                        transcript.extend_from_slice(&msg);
                        if let Some(expected) = pin {
                            let leaf = leaf_cert_from_cert_msg(body)
                                .ok_or_else(|| bad("unreadable certificate"))?;
                            if sha256(&leaf) != expected {
                                return Err(bad("TLS certificate pin mismatch"));
                            }
                        }
                    }
                    20 => {
                        let shs = s_hs.ok_or_else(|| bad("invalid handshake order"))?;
                        let fk = tls_expand_label(&shs, "finished", &[], 32);
                        let mut k = [0u8; 32];
                        k.copy_from_slice(&fk);
                        let th = sha256(&transcript);
                        let expect = hmac_sha256(&k, &th);
                        if body.len() != 32 || expect != body[..32] {
                            return Err(bad("server Finished verification failed"));
                        }
                        transcript.extend_from_slice(&msg);
                        let th2 = sha256(&transcript);

                        let hssec = hs_secret.ok_or_else(|| bad("no handshake secret"))?;
                        let derived2 = tls_expand_label(&hssec, "derived", &sha256(&[]), 32);
                        let master = hkdf_extract(&derived2, &[0u8; 32]);
                        let c_ap = tls_expand_label(&master, "c ap traffic", &th2, 32);
                        let s_ap = tls_expand_label(&master, "s ap traffic", &th2, 32);

                        let chs = c_hs.ok_or_else(|| bad("no client handshake secret"))?;
                        let cfk = tls_expand_label(&chs, "finished", &[], 32);
                        let mut ck = [0u8; 32];
                        ck.copy_from_slice(&cfk);
                        let cverify = hmac_sha256(&ck, &th2);
                        let cfin = handshake_msg(20, &cverify);
                        self.write_encrypted(CT_HS, &cfin)?;

                        let mut ca = [0u8; 32];
                        ca.copy_from_slice(&c_ap);
                        let mut sa = [0u8; 32];
                        sa.copy_from_slice(&s_ap);
                        self.set_write_secret(&ca);
                        self.set_read_secret(&sa);
                        return Ok(());
                    }
                    _ => {
                        transcript.extend_from_slice(&msg);
                    }
                }
            }
        }
    }

    fn read_exact_tcp(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.tcp.read_exact(buf)
    }

    fn read_record(&mut self) -> io::Result<(u8, Vec<u8>)> {
        let mut hdr = [0u8; 5];
        self.read_exact_tcp(&mut hdr)?;
        let len = ((hdr[3] as usize) << 8) | (hdr[4] as usize);
        if len > 18432 {
            return Err(bad("record TLS terlalu besar"));
        }
        let mut payload = vec![0u8; len];
        self.read_exact_tcp(&mut payload)?;
        Ok((hdr[0], payload))
    }

    fn write_plain(&mut self, ct: u8, data: &[u8]) -> io::Result<()> {
        let mut hdr = [ct, 0x03, 0x01, 0, 0];
        hdr[3] = (data.len() >> 8) as u8;
        hdr[4] = data.len() as u8;
        self.tcp.write_all(&hdr)?;
        self.tcp.write_all(data)?;
        self.tcp.flush()
    }

    fn write_encrypted(&mut self, ct: u8, data: &[u8]) -> io::Result<()> {
        for chunk in data.chunks(16384) {
            let mut inner = Vec::with_capacity(chunk.len() + 1);
            inner.extend_from_slice(chunk);
            inner.push(ct);
            let ct_len = inner.len() + 16;
            let hdr = [CT_APP, 0x03, 0x03, (ct_len >> 8) as u8, ct_len as u8];
            let n = nonce(&self.write_iv, self.write_seq);
            let sealed = aead_seal(&self.write_key, &n, &hdr, &inner);
            self.tcp.write_all(&hdr)?;
            self.tcp.write_all(&sealed)?;
            self.write_seq += 1;
        }
        self.tcp.flush()
    }

    fn decrypt_record(&mut self, payload: &[u8]) -> io::Result<(u8, Vec<u8>)> {
        let n = nonce(&self.read_iv, self.read_seq);
        let aad = [CT_APP, 0x03, 0x03, (payload.len() >> 8) as u8, payload.len() as u8];
        let inner = match aead_open(&self.read_key, &n, &aad, payload) {
            Some(v) => v,
            None => return Err(bad("TLS record decryption failed")),
        };
        self.read_seq += 1;
        let mut end = inner.len();
        while end > 0 && inner[end - 1] == 0 {
            end -= 1;
        }
        if end == 0 {
            return Err(bad("TLS record without type"));
        }
        let ct = inner[end - 1];
        Ok((ct, inner[..end - 1].to_vec()))
    }

    fn post_handshake(&mut self, data: &[u8]) {
        let mut p = 0;
        while p + 4 <= data.len() {
            let mlen = u24(&data[p + 1..p + 4]);
            if p + 4 + mlen > data.len() {
                break;
            }
            let mtype = data[p];
            if mtype == 24 && mlen == 1 && data[p + 4] == 1 {
                    let k = tls_expand_label(&self.read_key, "traffic upd", &[], 32);
                    let mut s = [0u8; 32];
                    s.copy_from_slice(&k);
                    self.set_read_secret(&s);
                    let k2 = tls_expand_label(&self.write_key, "traffic upd", &[], 32);
                    let mut s2 = [0u8; 32];
                    s2.copy_from_slice(&k2);
                    self.set_write_secret(&s2);
            }
            p += 4 + mlen;
        }
    }

    fn fill(&mut self) -> io::Result<bool> {
        if self.closed {
            return Ok(false);
        }
        loop {
            if self.ptr > 0 && self.ptr >= self.plain.len() {
                self.plain.clear();
                self.ptr = 0;
            }
            let (rtype, payload) = match self.read_record() {
                Ok(v) => v,
                Err(e) => {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        self.closed = true;
                        return Ok(false);
                    }
                    return Err(e);
                }
            };
            match rtype {
                CT_APP => {
                    let (ct, data) = self.decrypt_record(&payload)?;
                    match ct {
                        CT_APP => {
                            self.plain.extend_from_slice(&data);
                            return Ok(true);
                        }
                        CT_HS => {
                            self.post_handshake(&data);
                            continue;
                        }
                        CT_ALERT => {
                            self.closed = true;
                            return Ok(false);
                        }
                        _ => continue,
                    }
                }
                CT_ALERT => {
                    self.closed = true;
                    return Ok(false);
                }
                _ => continue,
            }
        }
    }

    pub fn read_some(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.ptr >= self.plain.len() && !self.fill()? {
            return Ok(0);
        }
        let avail = self.plain.len() - self.ptr;
        let n = avail.min(out.len());
        out[..n].copy_from_slice(&self.plain[self.ptr..self.ptr + n]);
        self.ptr += n;
        Ok(n)
    }

    #[allow(dead_code)]
    pub fn read_exact_n(&mut self, out: &mut [u8]) -> io::Result<()> {
        let mut off = 0;
        while off < out.len() {
            let n = self.read_some(&mut out[off..])?;
            if n == 0 {
                return Err(err(io::ErrorKind::UnexpectedEof, "TLS connection closed"));
            }
            off += n;
        }
        Ok(())
    }

    pub fn read_until(&mut self, delim: &[u8], limit: usize) -> io::Result<Vec<u8>> {
        loop {
            if self.ptr < self.plain.len() {
                if let Some(pos) = find_sub(&self.plain[self.ptr..], delim) {
                    let line = self.plain[self.ptr..self.ptr + pos].to_vec();
                    self.ptr += pos + delim.len();
                    return Ok(line);
                }
            }
            if self.plain.len() - self.ptr > limit {
                return Err(bad("baris terlalu panjang"));
            }
            if !self.fill()? {
                return Err(err(io::ErrorKind::UnexpectedEof, "TLS connection closed"));
            }
        }
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_encrypted(CT_APP, data)
    }

    #[allow(dead_code)]
    pub fn shutdown(&mut self) {
        let _ = self.tcp.shutdown(std::net::Shutdown::Both);
    }
}
