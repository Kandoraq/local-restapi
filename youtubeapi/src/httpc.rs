use crate::tls::TlsConn;
use std::io;
use std::sync::OnceLock;
use std::time::Duration;

pub type HttpResult<T> = Result<T, String>;

pub const USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36";

pub struct Url {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    pub fn parse(s: &str) -> Result<Url, String> {
        let (scheme, rest) = match s.split_once("://") {
            Some(v) => v,
            None => return Err(format!("invalid URL: {}", s)),
        };
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "https" && scheme != "http" {
            return Err(format!("unsupported scheme: {}", scheme));
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = if authority.starts_with('[') {
            match authority.find(']') {
                Some(i) => {
                    let h = &authority[..=i];
                    let p = if authority[i + 1..].starts_with(':') {
                        authority[i + 2..].parse::<u16>().map_err(|_| "invalid port".to_string())?
                    } else if scheme == "https" {
                        443
                    } else {
                        80
                    };
                    (h.to_string(), p)
                }
                None => return Err("invalid IPv6 host".to_string()),
            }
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) => (
                    h.to_string(),
                    p.parse::<u16>().map_err(|_| "invalid port".to_string())?,
                ),
                None => (
                    authority.to_string(),
                    if scheme == "https" { 443 } else { 80 },
                ),
            }
        };
        if host.is_empty() {
            return Err("empty host".to_string());
        }
        Ok(Url {
            scheme,
            host,
            port,
            path: path.to_string(),
        })
    }

}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    None,
    Length,
    Chunked,
    Eof,
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub final_url: String,
    conn: TlsConn,
    mode: Mode,
    remaining: u64,
    chunk_left: u64,
    finished: bool,
}

fn ioerr(e: io::Error) -> String {
    e.to_string()
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        header_value(&self.headers, name)
    }

    pub fn content_type(&self) -> String {
        self.header("content-type")
            .unwrap_or("application/octet-stream")
            .to_string()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.header("content-length").and_then(|v| v.trim().parse().ok())
    }

    pub fn read_body(&mut self, out: &mut [u8]) -> HttpResult<usize> {
        if out.is_empty() || self.finished {
            return Ok(0);
        }
        match self.mode {
            Mode::None => {
                self.finished = true;
                Ok(0)
            }
            Mode::Length => {
                if self.remaining == 0 {
                    self.finished = true;
                    return Ok(0);
                }
                let want = out.len().min(self.remaining as usize);
                let n = self.conn.read_some(&mut out[..want]).map_err(ioerr)?;
                if n == 0 {
                    self.finished = true;
                    return Ok(0);
                }
                self.remaining -= n as u64;
                Ok(n)
            }
            Mode::Eof => {
                let n = self.conn.read_some(out).map_err(ioerr)?;
                if n == 0 {
                    self.finished = true;
                }
                Ok(n)
            }
            Mode::Chunked => {
                if self.chunk_left == 0 {
                    let line = self.conn.read_until(b"\r\n", 256).map_err(ioerr)?;
                    let s = String::from_utf8_lossy(&line);
                    let sz = s.split(';').next().unwrap_or("").trim();
                    let size = u64::from_str_radix(sz, 16)
                        .map_err(|_| format!("invalid chunk: {:?}", sz))?;
                    if size == 0 {
                        loop {
                            let t = self.conn.read_until(b"\r\n", 8192).map_err(ioerr)?;
                            if t.is_empty() {
                                break;
                            }
                        }
                        self.finished = true;
                        return Ok(0);
                    }
                    self.chunk_left = size;
                }
                let want = out.len().min(self.chunk_left as usize);
                let n = self.conn.read_some(&mut out[..want]).map_err(ioerr)?;
                if n == 0 {
                    self.finished = true;
                    return Ok(0);
                }
                self.chunk_left -= n as u64;
                if self.chunk_left == 0 {
                    let _ = self.conn.read_until(b"\r\n", 2);
                }
                Ok(n)
            }
        }
    }

    pub fn read_to_end(&mut self, max: usize) -> HttpResult<Vec<u8>> {
        let mut out = Vec::new();
        let mut tmp = [0u8; 16384];
        loop {
            let n = self.read_body(&mut tmp)?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&tmp[..n]);
            if out.len() > max {
                return Err("response exceeds size limit".to_string());
            }
        }
        Ok(out)
    }

    pub fn set_timeout(&self, secs: u64) {
        self.conn.set_timeout(secs);
    }
}

fn default_pin() -> Option<[u8; 32]> {
    static P: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    *P.get_or_init(|| {
        std::env::var("TLS_PIN_SHA256").ok().and_then(|s| {
            let v = crate::crypto::hex_decode(&s)?;
            if v.len() == 32 {
                let mut a = [0u8; 32];
                a.copy_from_slice(&v);
                Some(a)
            } else {
                None
            }
        })
    })
}

fn send_once(
    url: &Url,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: u64,
    pin: Option<[u8; 32]>,
) -> HttpResult<Response> {
    let pin = pin.or_else(default_pin);
    let mut conn = TlsConn::connect(&url.host, url.port, pin, timeout).map_err(ioerr)?;

    let mut req = String::new();
    req.push_str(method);
    req.push(' ');
    req.push_str(&url.path);
    req.push_str(" HTTP/1.1\r\n");
    let default_host = if (url.scheme == "https" && url.port == 443) || (url.scheme == "http" && url.port == 80) {
        url.host.clone()
    } else {
        format!("{}:{}", url.host, url.port)
    };
    let has = |n: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    if !has("host") {
        req.push_str("Host: ");
        req.push_str(&default_host);
        req.push_str("\r\n");
    }
    if !has("user-agent") {
        req.push_str("User-Agent: ");
        req.push_str(USER_AGENT);
        req.push_str("\r\n");
    }
    if !has("accept-encoding") {
        req.push_str("Accept-Encoding: identity\r\n");
    }
    if !has("connection") {
        req.push_str("Connection: close\r\n");
    }
    if let Some(b) = body {
        if !has("content-length") {
            req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        }
    }
    for (k, v) in headers {
        req.push_str(k);
        req.push_str(": ");
        req.push_str(v);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");

    conn.write_all(req.as_bytes()).map_err(ioerr)?;
    if let Some(b) = body {
        conn.write_all(b).map_err(ioerr)?;
    }

    let line = conn.read_until(b"\r\n", 8192).map_err(ioerr)?;
    let s = String::from_utf8_lossy(&line).to_string();
    let mut parts = s.split_whitespace();
    let _ver = parts.next();
    let status: u16 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("invalid HTTP status: {:?}", s))?;

    let mut hdrs: Vec<(String, String)> = Vec::new();
    loop {
        let l = conn.read_until(b"\r\n", 32768).map_err(ioerr)?;
        if l.is_empty() {
            break;
        }
        let line = String::from_utf8_lossy(&l).to_string();
        if let Some(i) = line.find(':') {
            hdrs.push((
                line[..i].trim().to_ascii_lowercase(),
                line[i + 1..].trim().to_string(),
            ));
        }
        if hdrs.len() > 200 {
            return Err("terlalu banyak header".to_string());
        }
    }

    let no_body = method.eq_ignore_ascii_case("HEAD") || status == 204 || status == 304 || (100..200).contains(&status);
    let te_chunked = header_value(&hdrs, "transfer-encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    let cl = header_value(&hdrs, "content-length").and_then(|v| v.trim().parse::<u64>().ok());

    let (mode, remaining) = if no_body {
        (Mode::None, 0)
    } else if te_chunked {
        (Mode::Chunked, 0)
    } else if let Some(n) = cl {
        (Mode::Length, n)
    } else {
        (Mode::Eof, 0)
    };

    Ok(Response {
        status,
        headers: hdrs,
        final_url: format!("{}://{}{}", url.scheme, default_host, url.path),
        conn,
        mode,
        remaining,
        chunk_left: 0,
        finished: false,
    })
}

fn resolve_location(base: &Url, loc: &str) -> String {
    if loc.starts_with("http://") || loc.starts_with("https://") {
        loc.to_string()
    } else if loc.starts_with('/') {
        format!("{}://{}{}", base.scheme, base.host, loc)
    } else {
        format!("{}://{}/{}", base.scheme, base.host, loc)
    }
}

pub fn fetch(
    url: &str,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: u64,
    pin: Option<[u8; 32]>,
) -> HttpResult<Response> {
    let mut current = Url::parse(url)?;
    let mut method = method.to_string();
    let mut body = body.map(|b| b.to_vec());
    for _ in 0..6 {
        let resp = match send_once(&current, &method, headers, body.as_deref(), timeout, pin) {
            Ok(r) => r,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(120));
                send_once(&current, &method, headers, body.as_deref(), timeout, pin)?
            }
        };
        if resp.status >= 300 && resp.status < 400 {
            if let Some(loc) = resp.header("location").map(|s| s.to_string()) {
                let next = resolve_location(&current, &loc);
                current = Url::parse(&next)?;
                if resp.status == 303 {
                    method = "GET".to_string();
                    body = None;
                }
                drop(resp);
                continue;
            }
        }
        return Ok(resp);
    }
    Err("terlalu banyak redirect".to_string())
}

pub fn get(url: &str, headers: &[(String, String)], timeout: u64) -> HttpResult<Response> {
    fetch(url, "GET", headers, None, timeout, None)
}

pub fn post_json(url: &str, json: &str, headers: &[(String, String)], timeout: u64) -> HttpResult<Response> {
    let mut h = vec![("Content-Type".to_string(), "application/json".to_string())];
    h.extend_from_slice(headers);
    fetch(url, "POST", &h, Some(json.as_bytes()), timeout, None)
}
