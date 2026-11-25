use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

const MAX_BODY: usize = 25 * 1024 * 1024;

fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8084".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).unwrap();
    println!("listening on {}", addr);
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            thread::spawn(move || handle_conn(s));
        }
    }
}

fn read_body_limited<R: Read>(reader: &mut R, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 65536];
    let mut left = len;
    while left > 0 {
        let want = left.min(chunk.len());
        match reader.read(&mut chunk[..want]) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                left -= n;
            }
            Err(_) => break,
        }
    }
    out
}

fn handle_conn(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() { return; }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 { return; }
    let method = parts[0];
    let path = parts[1];
    let mut headers = std::collections::HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() { return; }
        let t = line.trim_end_matches(['\r','\n']).to_string();
        if t.is_empty() { break; }
        if let Some(i) = t.find(':') {
            let k = t[..i].trim().to_lowercase();
            let v = t[i+1..].trim().to_string();
            headers.insert(k, v);
        }
    }
    let content_length: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let body = read_body_limited(&mut reader, content_length.min(MAX_BODY));
    let path_only = path.split('?').next().unwrap_or(path);
    let query = path.split('?').nth(1).unwrap_or("");
    let params = parse_query(query);

    if method == "GET" && (path_only == "/" || path_only == "/health") {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    if method == "POST" && (path_only == "/remove-bg" || path_only == "/remove-background" || path_only == "/api/remove-bg" || path_only == "/remove") {
        let ct = headers.get("content-type").cloned().unwrap_or_default();
        let img_bytes = extract_image(&ct, &body, &headers);
        match img_bytes {
            None => {
                let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"error\":\"no image found\"}";
                let _ = stream.write_all(resp.as_bytes());
            }
            Some(bytes) => {
                match process_image(&bytes, &params) {
                    Ok(out) => {
                        let header = format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n", out.len());
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&out);
                    }
                    Err(e) => {
                        let resp = format!("HTTP/1.1 422 Unprocessable Entity\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{{\"error\":\"{}\"}}", e);
                        let _ = stream.write_all(resp.as_bytes());
                    }
                }
            }
        }
        return;
    }
    let resp = "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"error\":\"not found\"}";
    let _ = stream.write_all(resp.as_bytes());
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() { continue; }
        let mut kv = pair.splitn(2, '=');
        let k = kv.next().unwrap_or("");
        let v = kv.next().unwrap_or("");
        m.insert(url_decode(k), url_decode(v));
    }
    m
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let h = &s[i+1..i+3];
            if let Ok(val) = u8::from_str_radix(h, 16) {
                out.push(val);
                i += 3;
                continue;
            }
        }
        if b[i] == b'+' { out.push(b' '); } else { out.push(b[i]); }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn extract_image(ct: &str, body: &[u8], _headers: &std::collections::HashMap<String,String>) -> Option<Vec<u8>> {
    if ct.contains("multipart/form-data") {
        let boundary = ct.split("boundary=").nth(1)?.trim().trim_matches('"').to_string();
        let delim = format!("--{}", boundary);
        for part in split_bytes(body, delim.as_bytes()) {
            if part.is_empty() { continue; }
            let sep = b"\r\n\r\n";
            if let Some(pos) = find_subslice(part, sep) {
                let header_bytes = &part[..pos];
                let mut data = &part[pos+4..];
                if data.ends_with(b"\r\n") { data = &data[..data.len()-2]; }
                let headers = String::from_utf8_lossy(header_bytes);
                if headers.to_lowercase().contains("name=\"image\"") || headers.to_lowercase().contains("name='image'") || headers.to_lowercase().contains("filename=") {
                    if headers.to_lowercase().contains("filename=") || headers.to_lowercase().contains("name=\"image\"") {
                        return Some(data.to_vec());
                    }
                }
            }
        }

        for part in split_bytes(body, delim.as_bytes()) {
            let sep = b"\r\n\r\n";
            if let Some(pos) = find_subslice(part, sep) {
                let data = &part[pos+4..];
                let data = if data.ends_with(b"\r\n") { &data[..data.len()-2] } else { data };
                if data.len() > 100 { return Some(data.to_vec()); }
            }
        }
        None
    } else {
        if body.is_empty() { None } else { Some(body.to_vec()) }
    }
}

fn split_bytes<'a>(haystack: &'a [u8], needle: &'a [u8]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    if needle.is_empty() { out.push(haystack); return out; }
    let mut start = 0;
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i+needle.len()] == needle {
            out.push(&haystack[start..i]);
            start = i + needle.len();
            i = start;
        } else {
            i += 1;
        }
    }
    out.push(&haystack[start..]);
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() { return Some(0); }
    if haystack.len() < needle.len() { return None; }
    for i in 0..=haystack.len()-needle.len() {
        if &haystack[i..i+needle.len()] == needle { return Some(i); }
    }
    None
}

fn process_image(bytes: &[u8], params: &std::collections::HashMap<String,String>) -> Result<Vec<u8>, String> {
    let (w,h,mut rgba) = decode_png(bytes).map_err(|e| e.to_string())?;
    remove_background(&mut rgba, w, h, params);
    encode_png(w, h, &rgba)
}

fn remove_background(rgba: &mut [u8], w: u32, h: u32, params: &std::collections::HashMap<String,String>) {
    let threshold: u32 = params.get("threshold").and_then(|v| v.parse().ok()).unwrap_or(32);
    let (br, bg, bb) = if let Some(hex) = params.get("color") {
        parse_color(hex).unwrap_or_else(|| corner_color(rgba, w, h))
    } else {
        corner_color(rgba, w, h)
    };
    let mut removed = vec![false; (w*h) as usize];
    let mut stack: Vec<usize> = Vec::new();
    let idx = |x: u32, y: u32| -> usize { (y*w + x) as usize };
    for x in 0..w {
        for &y in &[0u32, h-1] {
            let i = idx(x,y);
            if (rgba[i*4+3] == 0 || color_dist(rgba, i, br, bg, bb) <= threshold) && !removed[i] {
                removed[i]=true; stack.push(i);
            }
        }
    }
    for y in 0..h {
        for &x in &[0u32, w-1] {
            let i = idx(x,y);
            if (rgba[i*4+3] == 0 || color_dist(rgba, i, br, bg, bb) <= threshold) && !removed[i] {
                removed[i]=true; stack.push(i);
            }
        }
    }
    while let Some(i) = stack.pop() {
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        for (dx,dy) in [(-1i32,0i32),(1,0),(0,-1),(0,1)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 { continue; }
            let ni = idx(nx as u32, ny as u32);
            if removed[ni] { continue; }
            if rgba[ni*4+3] == 0 || color_dist(rgba, ni, br, bg, bb) <= threshold {
                removed[ni]=true;
                stack.push(ni);
            }
        }
    }
    for i in 0..(w*h) as usize {
        if removed[i] {
            rgba[i*4+3] = 0;
        }
    }
}

fn corner_color(rgba: &[u8], w: u32, h: u32) -> (u8,u8,u8) {
    let mut hist: std::collections::HashMap<(u8,u8,u8), u32> = std::collections::HashMap::new();
    for x in 0..w {
        for &y in &[0u32, h.saturating_sub(1)] {
            let i = (y*w+x) as usize;
            let key = (rgba[i*4] & 0xF0, rgba[i*4+1] & 0xF0, rgba[i*4+2] & 0xF0);
            *hist.entry(key).or_insert(0) += 1;
        }
    }
    for y in 0..h {
        for &x in &[0u32, w.saturating_sub(1)] {
            let i = (y*w+x) as usize;
            let key = (rgba[i*4] & 0xF0, rgba[i*4+1] & 0xF0, rgba[i*4+2] & 0xF0);
            *hist.entry(key).or_insert(0) += 1;
        }
    }
    let mut best = (0u8,0u8,0u8);
    let mut bestc = 0u32;
    for (k,v) in &hist {
        if *v > bestc {
            bestc = *v;
            best = *k;
        }
    }
    let mut sr: u32=0; let mut sg: u32=0; let mut sb: u32=0; let mut cnt: u32=0;
    for x in 0..w {
        for &y in &[0u32, h.saturating_sub(1)] {
            let i = (y*w+x) as usize;
            let key = (rgba[i*4] & 0xF0, rgba[i*4+1] & 0xF0, rgba[i*4+2] & 0xF0);
            if key == best {
                sr += rgba[i*4] as u32; sg += rgba[i*4+1] as u32; sb += rgba[i*4+2] as u32; cnt += 1;
            }
        }
    }
    for y in 0..h {
        for &x in &[0u32, w.saturating_sub(1)] {
            let i = (y*w+x) as usize;
            let key = (rgba[i*4] & 0xF0, rgba[i*4+1] & 0xF0, rgba[i*4+2] & 0xF0);
            if key == best {
                sr += rgba[i*4] as u32; sg += rgba[i*4+1] as u32; sb += rgba[i*4+2] as u32; cnt += 1;
            }
        }
    }
    if cnt == 0 {
        let corners = [(0u32,0u32),(w-1,0),(0,h-1),(w-1,h-1)];
        let mut sr: u32=0; let mut sg: u32=0; let mut sb: u32=0;
        for (x,y) in corners {
            let i = (y*w+x) as usize;
            sr += rgba[i*4] as u32; sg += rgba[i*4+1] as u32; sb += rgba[i*4+2] as u32;
        }
        return ((sr/4) as u8,(sg/4) as u8,(sb/4) as u8);
    }
    ((sr/cnt) as u8,(sg/cnt) as u8,(sb/cnt) as u8)
}

fn parse_color(hex: &str) -> Option<(u8,u8,u8)> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 { return None; }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r,g,b))
}

fn color_dist(rgba: &[u8], i: usize, r: u8, g: u8, b: u8) -> u32 {
    let dr = rgba[i*4] as i32 - r as i32;
    let dg = rgba[i*4+1] as i32 - g as i32;
    let db = rgba[i*4+2] as i32 - b as i32;
    (dr*dr + dg*dg + db*db) as u32
}

fn decode_png(data: &[u8]) -> Result<(u32,u32,Vec<u8>), String> {
    if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" { return Err("not png".into()); }
    let mut i = 8;
    let mut width: u32 = 0; let mut height: u32 = 0;
    let mut bit_depth: u8 = 0; let mut color_type: u8 = 0;
    let mut idat: Vec<u8> = Vec::new();
    let mut palette: Vec<[u8;3]> = Vec::new();
    while i < data.len() {
        if i + 12 > data.len() { break; }
        let len = u32::from_be_bytes([data[i],data[i+1],data[i+2],data[i+3]]) as usize;
        if i + 12 + len > data.len() { break; }
        let ctype = &data[i+4..i+8];
        let chunk = &data[i+8..i+8+len];
        if ctype == b"IHDR" {
            if chunk.len() < 13 { return Err("bad ihdr".into()); }
            width = u32::from_be_bytes([chunk[0],chunk[1],chunk[2],chunk[3]]);
            height = u32::from_be_bytes([chunk[4],chunk[5],chunk[6],chunk[7]]);
            bit_depth = chunk[8];
            color_type = chunk[9];
        } else if ctype == b"PLTE" {
            for c in chunk.chunks(3) {
                if c.len()==3 { palette.push([c[0],c[1],c[2]]); }
            }
        } else if ctype == b"IDAT" {
            idat.extend_from_slice(chunk);
        } else if ctype == b"IEND" {
            break;
        }
        i += 12 + len;
    }
    if width==0 || height==0 { return Err("bad ihdr".into()); }
    if bit_depth != 8 { return Err("only 8-bit supported".into()); }
    let raw = inflate_zlib(&idat).map_err(|e| e.to_string())?;
    let channels = match color_type {
        0 => 1, 2 => 3, 4 => 2, 6 => 4, 3 => 1, _ => return Err("unsupported color type".into()),
    };
    let bpp = channels;
    let stride = (width as usize)*bpp;
    let mut out = vec![0u8; (width*height) as usize * 4];
    let mut prev = vec![0u8; stride];
    let mut row = vec![0u8; stride];
    let mut pos = 0;
    for y in 0..height as usize {
        if pos >= raw.len() { return Err("truncated".into()); }
        let filter = raw[pos]; pos += 1;
        for x in 0..stride {
            if pos >= raw.len() { return Err("truncated".into()); }
            let val = raw[pos]; pos += 1;
            let left = if x >= bpp { row[x-bpp] } else { 0 };
            let up = prev[x];
            let upleft = if x >= bpp { prev[x-bpp] } else { 0 };
            let recon = match filter {
                0 => val,
                1 => val.wrapping_add(left),
                2 => val.wrapping_add(up),
                3 => val.wrapping_add(((left as u16 + up as u16)/2) as u8),
                4 => val.wrapping_add(paeth(left, up, upleft)),
                _ => return Err("bad filter".into()),
            };
            row[x] = recon;
        }
        for x in 0..width as usize {
            let base = (y*width as usize + x)*4;
            match color_type {
                0 => { let g = row[x]; out[base]=g; out[base+1]=g; out[base+2]=g; out[base+3]=255; }
                2 => { out[base]=row[x*3]; out[base+1]=row[x*3+1]; out[base+2]=row[x*3+2]; out[base+3]=255; }
                4 => { let g=row[x*2]; let a=row[x*2+1]; out[base]=g;out[base+1]=g;out[base+2]=g;out[base+3]=a; }
                6 => { out[base]=row[x*4];out[base+1]=row[x*4+1];out[base+2]=row[x*4+2];out[base+3]=row[x*4+3]; }
                3 => {
                    let pi = row[x] as usize;
                    if pi < palette.len() {
                        out[base]=palette[pi][0]; out[base+1]=palette[pi][1]; out[base+2]=palette[pi][2];
                    } else { out[base]=0;out[base+1]=0;out[base+2]=0; }
                    out[base+3]=255;
                }
                _ => {}
            }
        }
        prev.copy_from_slice(&row);
    }
    Ok((width,height,out))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32; let b = b as i32; let c = c as i32;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc { a as u8 }
    else if pb <= pc { b as u8 }
    else { c as u8 }
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.push(8); ihdr.push(6); ihdr.push(0); ihdr.push(0); ihdr.push(0);
    write_chunk(&mut out, b"IHDR", &ihdr);
    let stride = (w as usize)*4;
    let mut raw = Vec::with_capacity((stride+1)*(h as usize));
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y*stride..(y+1)*stride]);
    }
    let z = zlib_store(&raw);
    write_chunk(&mut out, b"IDAT", &z);
    write_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, typ: &[u8;4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(typ);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4+data.len());
    crc_input.extend_from_slice(typ);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in data {
        let mut c = crc ^ (b as u32);
        for _ in 0..8 {
            if c & 1 == 1 { c = (c >> 1) ^ 0xEDB88320; } else { c >>= 1; }
        }
        crc = c;
    }
    crc ^ 0xFFFFFFFF
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0x78); out.push(0x01);
    let mut pos = 0;
    let chunk_size = 65535usize;
    while pos < data.len() || data.is_empty() {
        if data.is_empty() && pos==0 {
            out.push(1);
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            break;
        }
        let remaining = data.len() - pos;
        let n = remaining.min(chunk_size);
        let final_block = pos + n >= data.len();
        out.push(if final_block { 1 } else { 0 });
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[pos..pos+n]);
        pos += n;
        if n == 0 { break; }
        if data.is_empty() { break; }
    }
    if data.is_empty() {
    } else {
    }
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1; let mut b: u32 = 0;
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 6 { return Err("short".into()); }
    let cmf = data[0]; let flg = data[1];
    if cmf & 0x0F != 8 { return Err("bad cmf".into()); }
    if ((cmf as u32)*256 + flg as u32) % 31 != 0 { return Err("bad flg".into()); }
    let pos = 2;

    if flg & 0x20 != 0 { return Err("preset dict not supported".into()); }
    let out = inflate_deflate(&data[pos..data.len()-4])?;
    Ok(out)
}

fn inflate_deflate(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut br = BitReader { data: input, bitpos: 0 };
    let mut out: Vec<u8> = Vec::new();
    let mut window: Vec<u8> = Vec::new();
    loop {
        let final_block = br.read_bit()?;
        let btype = br.read_bits(2)?;
        if btype == 0 {
            br.align();
            let len = br.read_u16()?;
            let nlen = br.read_u16()?;
            if len != !nlen { return Err("bad stored len".into()); }
            for _ in 0..len {
                let b = br.read_byte()?;
                out.push(b); window.push(b);
            }
        } else {
            let (lit_tree, dist_tree) = if btype == 1 {
                (fixed_lit_tree(), fixed_dist_tree())
            } else if btype == 2 {
                dynamic_trees(&mut br)?
            } else {
                return Err("reserved btype".into());
            };
            loop {
                let sym = decode_huffman(&lit_tree, &mut br)?;
                if sym < 256 {
                    let b = sym as u8;
                    out.push(b); window.push(b);
                } else if sym == 256 {
                    break;
                } else {
                    let len = decode_length(sym, &mut br)?;
                    let dist_sym = decode_huffman(&dist_tree, &mut br)?;
                    let dist = decode_distance(dist_sym, &mut br)?;
                    for _ in 0..len {
                        let idx = window.len() - dist;
                        let b = window[idx];
                        out.push(b); window.push(b);
                    }
                }
            }
        }
        if final_block == 1 { break; }
    }
    Ok(out)
}

struct BitReader<'a> { data: &'a [u8], bitpos: usize }

impl<'a> BitReader<'a> {
    fn read_bit(&mut self) -> Result<u8, String> {
        if self.bitpos / 8 >= self.data.len() { return Err("eof".into()); }
        let b = (self.data[self.bitpos/8] >> (self.bitpos % 8)) & 1;
        self.bitpos += 1;
        Ok(b)
    }
    fn read_bits(&mut self, n: usize) -> Result<usize, String> {
        let mut v = 0usize;
        for i in 0..n {
            let b = self.read_bit()?;
            v |= (b as usize) << i;
        }
        Ok(v)
    }
    fn align(&mut self) {
        if self.bitpos % 8 != 0 { self.bitpos += 8 - (self.bitpos % 8); }
    }
    fn read_u16(&mut self) -> Result<u16, String> {
        if self.bitpos/8 + 2 > self.data.len() { return Err("eof".into()); }
        let i = self.bitpos/8;
        let v = u16::from_le_bytes([self.data[i], self.data[i+1]]);
        self.bitpos += 16;
        Ok(v)
    }
    fn read_byte(&mut self) -> Result<u8, String> {
        if self.bitpos/8 >= self.data.len() { return Err("eof".into()); }
        let b = self.data[self.bitpos/8];
        self.bitpos += 8;
        Ok(b)
    }
}

enum Node { Leaf(u16), Branch(Box<Node>, Box<Node>) }

fn decode_huffman(tree: &Node, br: &mut BitReader) -> Result<u16, String> {
    let mut node = tree;
    loop {
        match node {
            Node::Leaf(s) => return Ok(*s),
            Node::Branch(l,r) => {
                let b = br.read_bit()?;
                node = if b == 0 { l } else { r };
            }
        }
    }
}

fn build_tree(lengths: &[u8]) -> Node {
    let max = *lengths.iter().max().unwrap_or(&0) as usize;
    if max == 0 { return Node::Leaf(256); }
    let mut counts = vec![0usize; max+1];
    for &l in lengths { if l > 0 { counts[l as usize] += 1; } }
    let mut next_code = vec![0usize; max+2];
    let mut code = 0usize;
    for bits in 1..=max {
        code = (code + counts[bits-1]) << 1;
        next_code[bits] = code;
    }
    let mut codes = vec![0usize; lengths.len()];
    for (i,&l) in lengths.iter().enumerate() {
        if l > 0 {
            let bits = l as usize;
            codes[i] = next_code[bits];
            next_code[bits] += 1;
        }
    }
    let mut root = Node::Branch(Box::new(Node::Leaf(0)), Box::new(Node::Leaf(0)));
    for (i,&l) in lengths.iter().enumerate() {
        if l == 0 { continue; }
        let bits = l as usize;
        insert_node(&mut root, codes[i], bits, i as u16);
    }
    root
}

fn insert_node(node: &mut Node, code: usize, bits: usize, symbol: u16) {
    if bits == 0 {
        *node = Node::Leaf(symbol);
        return;
    }
    if let Node::Leaf(_) = node {
        *node = Node::Branch(Box::new(Node::Leaf(0)), Box::new(Node::Leaf(0)));
    }
    if let Node::Branch(l, r) = node {
        let bit = (code >> (bits - 1)) & 1;
        if bit == 0 {
            insert_node(l, code, bits - 1, symbol);
        } else {
            insert_node(r, code, bits - 1, symbol);
        }
    }
}

fn fixed_lit_tree() -> Node {
    let mut lengths = vec![0u8; 288];
    for i in 0..144 { lengths[i] = 8; }
    for i in 144..256 { lengths[i] = 9; }
    for i in 256..280 { lengths[i] = 7; }
    for i in 280..288 { lengths[i] = 8; }
    build_tree(&lengths)
}

fn fixed_dist_tree() -> Node {
    let lengths = vec![5u8; 32];
    build_tree(&lengths)
}

fn dynamic_trees(br: &mut BitReader) -> Result<(Node,Node), String> {
    let hlit = br.read_bits(5)? + 257;
    let hdist = br.read_bits(5)? + 1;
    let hclen = br.read_bits(4)? + 4;
    let order = [16usize,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];
    let mut cl_lengths = vec![0u8; 19];
    for i in 0..hclen {
        cl_lengths[order[i]] = br.read_bits(3)? as u8;
    }
    let cl_tree = build_tree(&cl_lengths);
    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = decode_huffman(&cl_tree, br)?;
        match sym {
            0..=15 => { lengths[i] = sym as u8; i += 1; }
            16 => {
                let rep = br.read_bits(2)? + 3;
                let prev = if i > 0 { lengths[i-1] } else { 0 };
                for _ in 0..rep { if i < total { lengths[i] = prev; i += 1; } }
            }
            17 => {
                let rep = br.read_bits(3)? + 3;
                for _ in 0..rep { if i < total { lengths[i] = 0; i += 1; } }
            }
            18 => {
                let rep = br.read_bits(7)? + 11;
                for _ in 0..rep { if i < total { lengths[i] = 0; i += 1; } }
            }
            _ => return Err("bad cl sym".into()),
        }
    }
    let lit_lengths = &lengths[..hlit];
    let dist_lengths = &lengths[hlit..];
    Ok((build_tree(lit_lengths), build_tree(dist_lengths)))
}

fn decode_length(sym: u16, br: &mut BitReader) -> Result<usize, String> {
    let base = [3usize,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
    let extra = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
    let idx = (sym as usize) - 257;
    if idx >= base.len() { return Err("bad length sym".into()); }
    let mut v = base[idx];
    if extra[idx] > 0 { v += br.read_bits(extra[idx])?; }
    Ok(v)
}

fn decode_distance(sym: u16, br: &mut BitReader) -> Result<usize, String> {
    let base = [1usize,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
    let extra = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13];
    let idx = sym as usize;
    if idx >= base.len() { return Err("bad dist sym".into()); }
    let mut v = base[idx];
    if extra[idx] > 0 { v += br.read_bits(extra[idx])?; }
    Ok(v)
}
