use crate::httpc;

fn be32(b: &[u8], o: usize) -> Option<u32> {
    if o + 4 > b.len() {
        return None;
    }
    Some(u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]))
}

fn be64(b: &[u8], o: usize) -> Option<u64> {
    if o + 8 > b.len() {
        return None;
    }
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    Some(u64::from_be_bytes(a))
}

fn box_at(b: &[u8], off: usize) -> Option<(usize, usize, [u8; 4])> {
    if off + 8 > b.len() {
        return None;
    }
    let mut size = be32(b, off)? as u64;
    let mut name = [0u8; 4];
    name.copy_from_slice(&b[off + 4..off + 8]);
    let mut hdr = 8usize;
    if size == 1 {
        size = be64(b, off + 8)?;
        hdr = 16;
    } else if size == 0 {
        size = b.len() as u64 - off as u64;
    }
    if size < hdr as u64 {
        return None;
    }
    Some((hdr, size as usize, name))
}

fn mvhd_ms(payload: &[u8]) -> Option<i64> {
    if payload.len() < 20 {
        return None;
    }
    let ver = payload[0];
    let (ts, dur) = if ver == 1 {
        (be32(payload, 20)? as u64, be64(payload, 24)?)
    } else {
        (be32(payload, 12)? as u64, be32(payload, 16)? as u64)
    };
    if ts == 0 {
        return None;
    }
    if dur == 0 || dur == u32::MAX as u64 || dur == u64::MAX {
        return None;
    }
    Some((dur as u128 * 1000 / ts as u128) as i64)
}

fn moov_duration(payload: &[u8]) -> Option<i64> {
    let mut best: Option<i64> = None;
    let mut off = 0usize;
    while let Some((hdr, size, name)) = box_at(payload, off) {
        if &name == b"mvhd" || &name == b"mdhd" {
            if let Some(ms) = mvhd_ms(&payload[off + hdr..]) {
                if best.map_or(true, |b| ms > b) {
                    best = Some(ms);
                }
            }
        } else if &name == b"trak" || &name == b"mdia" {
            if let Some(ms) = moov_duration(&payload[off + hdr..(off + size).min(payload.len())]) {
                if best.map_or(true, |b| ms > b) {
                    best = Some(ms);
                }
            }
        }
        off += size;
    }
    best
}

pub fn parse(buf: &[u8]) -> Option<i64> {
    let mut off = 0usize;
    while let Some((hdr, size, name)) = box_at(buf, off) {
        if &name == b"moov" {
            return moov_duration(&buf[off + hdr..(off + size).min(buf.len())]);
        }
        if &name == b"moof" {
            return None;
        }
        off += size;
    }
    None
}

fn parse_tail(buf: &[u8]) -> Option<i64> {
    let mut i = 4usize;
    while i + 4 <= buf.len() {
        if &buf[i..i + 4] == b"moov" {
            let start = i - 4;
            let mut size = be32(buf, start)? as u64;
            let mut hdr = 8usize;
            if size == 1 {
                size = be64(buf, start + 8)?;
                hdr = 16;
            }
            if size >= hdr as u64 && start + size as usize <= buf.len() {
                let p = &buf[start + hdr..start + size as usize];
                if let Some(ms) = moov_duration(p) {
                    return Some(ms);
                }
            }
        }
        i += 1;
    }
    None
}

fn range_headers(range: &str) -> Vec<(String, String)> {
    vec![
        ("User-Agent".to_string(), httpc::USER_AGENT.to_string()),
        ("Accept-Encoding".to_string(), "identity".to_string()),
        ("Referer".to_string(), "https://www.youtube.com/".to_string()),
        ("Range".to_string(), range.to_string()),
    ]
}

pub fn probe(url: &str) -> Option<i64> {
    let h = range_headers("bytes=0-65535");
    if let Ok(mut r) = httpc::fetch(url, "GET", &h, None, 30, None) {
        if r.status == 200 || r.status == 206 {
            if let Ok(body) = r.read_to_end(200_000) {
                if let Some(ms) = parse(&body) {
                    return Some(ms);
                }
            }
        }
    }
    let h = range_headers("bytes=-262144");
    if let Ok(mut r) = httpc::fetch(url, "GET", &h, None, 30, None) {
        if r.status == 200 || r.status == 206 {
            if let Ok(body) = r.read_to_end(600_000) {
                return parse_tail(&body);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        v.extend_from_slice(name);
        v.extend_from_slice(payload);
        v
    }

    fn mvhd(timescale: u32, duration: u32) -> Vec<u8> {
        let mut p = vec![0u8; 4];
        p.extend_from_slice(&[0u8; 8]);
        p.extend_from_slice(&timescale.to_be_bytes());
        p.extend_from_slice(&duration.to_be_bytes());
        p.extend_from_slice(&[0u8; 80]);
        boxed(b"mvhd", &p)
    }

    #[test]
    fn mvhd_v0_duration() {
        let mut moov = mvhd(1000, 18947);
        moov = boxed(b"moov", &moov);
        let mut ftyp = vec![0u8; 16];
        ftyp[0..4].copy_from_slice(&16u32.to_be_bytes());
        ftyp[4..8].copy_from_slice(b"ftyp");
        let mut file = ftyp;
        file.extend_from_slice(&moov);
        assert_eq!(parse(&file), Some(18947));
    }

    #[test]
    fn mvhd_v1_duration() {
        let mut p = vec![1u8, 0, 0, 0];
        p.extend_from_slice(&[0u8; 16]);
        p.extend_from_slice(&44100u32.to_be_bytes());
        p.extend_from_slice(&835584u64.to_be_bytes());
        p.extend_from_slice(&[0u8; 80]);
        let m = boxed(b"mvhd", &p);
        let moov = boxed(b"moov", &m);
        assert_eq!(parse(&moov), Some(18947));
    }

    #[test]
    fn tail_moov() {
        let m = mvhd(15360, 291033);
        let moov = boxed(b"moov", &m);
        let mut file = vec![0u8; 100];
        file.extend_from_slice(&moov);
        assert_eq!(parse_tail(&file), Some(18947));
    }
}
