use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BODY: usize = 64 * 1024;
const STORE_FILE: &str = "shorturls.json";
const MIN_URL_LEN: usize = 30;

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn main() {
    let store = load_store();
    let state = Arc::new(Mutex::new(store));
    let port = std::env::var("PORT").unwrap_or_else(|_| "8083".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).unwrap();
    println!("listening on {}", addr);
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            let st = state.clone();
            thread::spawn(move || handle_conn(s, st));
        }
    }
}

fn load_store() -> HashMap<String, String> {
    if let Ok(txt) = fs::read_to_string(STORE_FILE) {
        parse_store(&txt)
    } else {
        HashMap::new()
    }
}

fn parse_store(txt: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let t = txt.trim();
    if !t.starts_with('{') || !t.ends_with('}') { return map; }
    let inner = &t[1..t.len()-1];
    for entry in split_top_level(inner) {
        let entry = entry.trim();
        if entry.is_empty() { continue; }
        let mut parts = entry.splitn(2, ':');
        let k = parts.next().unwrap_or("").trim().trim_matches('"');
        let v = parts.next().unwrap_or("").trim().trim_matches('"');
        if !k.is_empty() && !v.is_empty() {
            map.insert(unescape(k), unescape(v));
        }
    }
    map
}

fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if esc { cur.push(c); esc = false; continue; }
        if c == '\\' { esc = true; cur.push(c); continue; }
        if c == '"' { in_str = !in_str; cur.push(c); continue; }
        if !in_str {
            if c == '{' || c == '[' { depth += 1; }
            if c == '}' || c == ']' { depth -= 1; }
            if c == ',' && depth == 0 {
                out.push(cur.clone());
                cur.clear();
                continue;
            }
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() { out.push(cur); }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some(c) => out.push(c),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn save_store(store: &HashMap<String, String>) {
    let mut s = String::from("{");
    let mut first = true;
    for (k, v) in store {
        if !first { s.push(','); }
        first = false;
        s.push('"');
        s.push_str(&escape(k));
        s.push_str("\":\"");
        s.push_str(&escape(v));
        s.push('"');
    }
    s.push('}');
    let _ = fs::write(STORE_FILE, s);
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

fn handle_conn(mut stream: TcpStream, state: Arc<Mutex<HashMap<String, String>>>) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() { return; }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 { return; }
    let method = parts[0];
    let path = parts[1];
    let mut headers = HashMap::new();
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
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"status\":\"ok\",\"service\":\"shorturl\"}";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    if method == "POST" && (path_only == "/shorten" || path_only == "/api/shorten") {
        let ct = headers.get("content-type").cloned().unwrap_or_default();
        let url = if ct.contains("application/json") {
            extract_json_string(&String::from_utf8_lossy(&body), "url")
        } else if ct.contains("application/x-www-form-urlencoded") {
            parse_query(&String::from_utf8_lossy(&body)).get("url").cloned()
        } else {
            params.get("url").cloned()
        };
        let base_override = if ct.contains("application/json") {
            extract_json_string(&String::from_utf8_lossy(&body), "base_url")
        } else {
            params.get("base_url").cloned()
        };
        match url {
            None => {
                let resp = "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"error\":\"missing url\"}";
                let _ = stream.write_all(resp.as_bytes());
            }
            Some(u) => {
                match shorten_url(&u, &state) {
                    Ok(code) => {
                        let host = if let Some(b) = base_override.clone() {
                            b
                        } else if let Ok(b) = std::env::var("BASE_URL") {
                            b
                        } else if let Some(h) = headers.get("host") {
                            let proto = headers.get("x-forwarded-proto").cloned().unwrap_or_else(|| "http".to_string());
                            format!("{}://{}", proto, h)
                        } else {
                            format!("http://localhost:{}", std::env::var("PORT").unwrap_or_else(|_| "8083".to_string()))
                        };
                        let short = format!("{}/{}", host.trim_end_matches('/'), code);
                        let json = format!("{{\"status\":\"ok\",\"code\":\"{}\",\"short_url\":\"{}\",\"original_url\":\"{}\"}}", escape(&code), escape(&short), escape(&u));
                        let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n", json.len());
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(json.as_bytes());
                    }
                    Err(e) => {
                        let resp = format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{{\"error\":\"{}\"}}", json_escape(&e));
                        let _ = stream.write_all(resp.as_bytes());
                    }
                }
            }
        }
        return;
    }
    if method == "GET" && path_only == "/list" {
        let store = state.lock().unwrap();
        let mut json = String::from("{\"urls\":{");
        let mut first = true;
        for (k, v) in store.iter() {
            if !first { json.push(','); }
            first = false;
            json.push('"');
            json.push_str(&escape(k));
            json.push_str("\":\"");
            json.push_str(&escape(v));
            json.push('"');
        }
        json.push_str("}}");
        let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n", json.len());
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(json.as_bytes());
        return;
    }
    if method == "GET" {
        let code = path_only.trim_start_matches('/');
        if !code.is_empty() && !code.contains('/') {
            let store = state.lock().unwrap();
            if let Some(url) = store.get(code) {
                let url = match header_safe(url) {
                    Some(u) => u,
                    None => {
                        drop(store);
                        let resp = "HTTP/1.1 400 Bad Request\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                        return;
                    }
                };
                drop(store);
                let resp = format!("HTTP/1.1 302 Found\r\nLocation: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n", url);
                let _ = stream.write_all(resp.as_bytes());
                return;
            }
        }
    }
    let resp = "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{\"error\":\"not found\"}";
    let _ = stream.write_all(resp.as_bytes());
}

fn header_safe(s: &str) -> Option<String> {
    if s.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(s.to_string())
}

fn shorten_url(url: &str, state: &Arc<Mutex<HashMap<String, String>>>) -> Result<String, String> {
    let u = url.trim();
    if !u.starts_with("http://") && !u.starts_with("https://") {
        return Err("invalid url scheme".into());
    }
    if header_safe(u).is_none() {
        return Err("url contains control characters".into());
    }
    if u.len() < MIN_URL_LEN {
        return Err("url too short".into());
    }
    let host = extract_host(u);
    if is_shortener_host(&host) {
        return Err("url already short".into());
    }
    let mut store = state.lock().unwrap();
    for (_, v) in store.iter() {
        if v == u {
            return Err("url already exists".into());
        }
    }
    let mut code = gen_code();
    while store.contains_key(&code) {
        code = gen_code();
    }
    store.insert(code.clone(), u.to_string());
    save_store(&store);
    Ok(code)
}

fn extract_host(url: &str) -> String {
    let s = url.split("://").nth(1).unwrap_or("");
    s.split('/').next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase()
}

fn is_shortener_host(host: &str) -> bool {
    let shorteners = [
        "bit.ly", "t.co", "goo.gl", "tinyurl.com", "ow.ly", "is.gd", "buff.ly",
        "adf.ly", "bit.do", "mcaf.ee", "su.pr", "tiny.cc", "lnkd.in", "db.tt",
        "qr.ae", "adfoc.us", "ity.im", "q.gs", "po.st", "bc.vc", "twitthis.com",
        "u.to", "j.mp", "buzurl.com", "cutt.us", "u.bb", "yourls.org", "x.co",
        "prettylinkpro.com", "scrnch.me", "filoops.info", "vzturl.com", "qr.net",
        "1url.com", "tweez.me", "v.gd", "tr.im", "link.zip.net",
    ];
    shorteners.iter().any(|s| host == *s || host.ends_with(&format!(".{}", s)))
}

fn gen_code() -> String {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let mut x = t ^ (n.wrapping_mul(0x9E3779B97F4A7C15));
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut out = String::new();
    for _ in 0..7 {
        out.push(alphabet[(x % 62) as usize] as char);
        x /= 62;
    }
    out
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
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

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let idx = json.find(&needle)?;
    let rest = &json[idx + needle.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon+1..].trim_start();
    if !rest.starts_with('"') { return None; }
    let rest = &rest[1..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'u' => {
                        let mut hex = String::new();
                        for _ in 0..4 { if let Some(h) = chars.next() { hex.push(h); } }
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(code) { out.push(ch); }
                        }
                    }
                    _ => out.push(next),
                }
            }
        } else if c == '"' {
            break;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod header_safety_tests {
    use super::header_safe;

    #[test]
    fn rejects_crlf_in_redirect_target() {
        let bad = "https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\nX-Injected: yes";
        assert_eq!(header_safe(bad), None);
    }

    #[test]
    fn rejects_control_chars() {
        assert_eq!(header_safe("https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\u{0000}"), None);
        assert_eq!(header_safe("https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\u{0007}"), None);
    }

    #[test]
    fn accepts_normal_url() {
        let ok = "https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/path?q=1";
        assert_eq!(header_safe(ok), Some(ok.to_string()));
    }
}
