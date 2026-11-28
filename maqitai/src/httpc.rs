use crate::tls::{TlsConn, Verify};
use std::io;
use std::time::Duration;

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

static COOKIE_JAR: LazyLock<Mutex<HashMap<String, Vec<String>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn cookie_header_for(host: &str) -> Option<String> {
    let jar = COOKIE_JAR.lock().unwrap_or_else(|e| e.into_inner());
    let cookies = jar.get(host)?;
    if cookies.is_empty() { None } else { Some(cookies.join("; ")) }
}

fn store_cookies(host: &str, headers: &[(String, String)]) {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("set-cookie") {
            if let Some(nv) = v.split(';').next() {
                let nv = nv.trim();
                if !nv.is_empty() {
                    let name = nv.split('=').next().unwrap_or("").trim().to_string();
                    let mut jar = COOKIE_JAR.lock().unwrap_or_else(|e| e.into_inner());
                    let entry = jar.entry(host.to_string()).or_default();
                    if !name.is_empty() {
                        entry.retain(|c| !c.starts_with(&format!("{}=", name)));
                    }
                    entry.push(nv.to_string());
                }
            }
        }
    }
}

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

    pub fn read_body_bytes(&mut self) -> HttpResult<Vec<u8>> {
        let raw = self.read_to_end(256 * 1024 * 1024)?;
        let enc = self.header("content-encoding").unwrap_or("").to_ascii_lowercase();
        if enc.contains("gzip") {
            crate::inflate::inflate_gzip(&raw).map_err(|e| format!("gzip: {}", e))
        } else if enc.contains("deflate") {
            crate::inflate::inflate_zlib(&raw)
                .or_else(|_| crate::inflate::inflate_deflate(&raw))
                .map_err(|e| format!("deflate: {}", e))
        } else {
            Ok(raw)
        }
    }

    pub fn set_timeout(&self, secs: u64) {
        self.conn.set_timeout(secs);
    }
}

pub const ROOT_PEM: &str = r#"
-----BEGIN CERTIFICATE-----
MIIFCzCCAvOgAwIBAgIQf/AFoHxM3tEArZ1mpRB7mDANBgkqhkiG9w0BAQsFADBH
MQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZpY2VzIExM
QzEUMBIGA1UEAxMLR1RTIFJvb3QgUjEwHhcNMjMxMjEzMDkwMDAwWhcNMjkwMjIw
MTQwMDAwWjA7MQswCQYDVQQGEwJVUzEeMBwGA1UEChMVR29vZ2xlIFRydXN0IFNl
cnZpY2VzMQwwCgYDVQQDEwNXUjIwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEK
AoIBAQCp/5x/RR5wqFOfytnlDd5GV1d9vI+aWqxG8YSau5HbyfsvAfuSCQAWXqAc
+MGr+XgvSszYhaLYWTwO0xj7sfUkDSbutltkdnwUxy96zqhMt/TZCPzfhyM1IKji
aeKMTj+xWfpgoh6zySBTGYLKNlNtYE3pAJH8do1cCA8Kwtzxc2vFE24KT3rC8gIc
LrRjg9ox9i11MLL7q8Ju26nADrn5Z9TDJVd06wW06Y613ijNzHoU5HEDy01hLmFX
xRmpC5iEGuh5KdmyjS//V2pm4M6rlagplmNwEmceOuHbsCFx13ye/aoXbv4r+zgX
FNFmp6+atXDMyGOBOozAKql2N87jAgMBAAGjgf4wgfswDgYDVR0PAQH/BAQDAgGG
MB0GA1UdJQQWMBQGCCsGAQUFBwMBBggrBgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/
AgEAMB0GA1UdDgQWBBTeGx7teRXUPjckwyG77DQ5bUKyMDAfBgNVHSMEGDAWgBTk
rysmcRorSCeFL1JmLO/wiRNxPjA0BggrBgEFBQcBAQQoMCYwJAYIKwYBBQUHMAKG
GGh0dHA6Ly9pLnBraS5nb29nL3IxLmNydDArBgNVHR8EJDAiMCCgHqAchhpodHRw
Oi8vYy5wa2kuZ29vZy9yL3IxLmNybDATBgNVHSAEDDAKMAgGBmeBDAECATANBgkq
hkiG9w0BAQsFAAOCAgEARXWL5R87RBOWGqtY8TXJbz3S0DNKhjO6V1FP7sQ02hYS
TL8Tnw3UVOlIecAwPJQl8hr0ujKUtjNyC4XuCRElNJThb0Lbgpt7fyqaqf9/qdLe
SiDLs/sDA7j4BwXaWZIvGEaYzq9yviQmsR4ATb0IrZNBRAq7x9UBhb+TV+PfdBJT
DhEl05vc3ssnbrPCuTNiOcLgNeFbpwkuGcuRKnZc8d/KI4RApW//mkHgte8y0YWu
ryUJ8GLFbsLIbjL9uNrizkqRSvOFVU6xddZIMy9vhNkSXJ/UcZhjJY1pXAprffJB
vei7j+Qi151lRehMCofa6WBmiA4fx+FOVsV2/7R6V2nyAiIJJkEd2nSi5SnzxJrl
Xdaqev3htytmOPvoKWa676ATL/hzfvDaQBEcXd2Ppvy+275W+DKcH0FBbX62xevG
iza3F4ydzxl6NJ8hk8R+dDXSqv1MbRT1ybB5W0k8878XSOjvmiYTDIfyc9acxVJr
Y/cykHipa+te1pOhv7wYPYtZ9orGBV5SGOJm4NrB3K1aJar0RfzxC3ikr7Dyc6Qw
qDTBU39CluVIQeuQRgwG3MuSxl7zRERDRilGoKb8uY45JzmxWuKxrfwT/478JuHU
/oTxUFqOl2stKnn7QGTq8z29W+GgBLCXSBxC9epaHM0myFH/FJlniXJfHeytWt0=
-----END CERTIFICATE-----
"#;

fn default_pin(host: &str) -> Result<Verify, String> {
    if let Ok(v) = std::env::var("TLS_PIN_SHA256") {
        let bytes = crate::crypto::hex_decode(&v)
            .ok_or_else(|| "TLS_PIN_SHA256 is not valid hex".to_string())?;
        if bytes.len() != 32 {
            return Err("TLS_PIN_SHA256 must be 32 bytes".to_string());
        }
        let mut a = [0u8; 32];
        a.copy_from_slice(&bytes);
        return Ok(Verify::Pin(a));
    }
    let google = "9757a339075f3299e0bb4083a9a0f39bfbeaba31f4b4418031423367d9f2e0b3";
    let tikwm = "555dd530c18a2ef14d9249cd6968947d25841ba69400c7e57070765e4443a745";
    let builtin = if host.ends_with(".googlevideo.com") {
        ""
    } else if host.ends_with(".youtube.com") || host == "youtube.com"
        || host.ends_with(".ytimg.com") || host == "ytimg.com"
        || host.ends_with(".googleusercontent.com")
    {
        google
    } else if host.ends_with(".tikwm.com") || host == "tikwm.com" {
        tikwm
    } else {
        ""
    };
    if !builtin.is_empty() {
        let bytes = crate::crypto::hex_decode(builtin).ok_or_else(|| "bad builtin pin".to_string())?;
        let mut a = [0u8; 32];
        a.copy_from_slice(&bytes);
        return Ok(Verify::Pin(a));
    }
    if host.ends_with(".googlevideo.com") || host == "googlevideo.com" {
        return Ok(Verify::CaPem(ROOT_PEM));
    }
    if std::env::var("TLS_ALLOW_UNPINNED").map(|v| v == "1").unwrap_or(false) {
        return Ok(Verify::None);
    }
    Err(format!(
        "no TLS pin for {}. Set TLS_PIN_SHA256 to the SHA-256 of the server SPKI, or TLS_ALLOW_UNPINNED=1 to allow unverified TLS",
        host
    ))
}

fn send_once(
    url: &Url,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: u64,
    verify: Option<Verify>,
) -> HttpResult<Response> {
    let verify = match verify {
        Some(v) => v,
        None => default_pin(&url.host)?,
    };
    let mut conn = TlsConn::connect(&url.host, url.port, verify, timeout).map_err(ioerr)?;

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
    let mut all_headers: Vec<(String, String)> = headers.to_vec();
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("cookie")) {
        if let Some(c) = cookie_header_for(&url.host) {
            all_headers.push(("Cookie".to_string(), c));
        }
    }
    let has = |n: &str| all_headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
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
        req.push_str("Accept-Encoding: gzip, deflate\r\n");
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

    store_cookies(&url.host, &hdrs);

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
    verify: Option<Verify>,
) -> HttpResult<Response> {
    let mut current = Url::parse(url)?;
    let mut method = method.to_string();
    let mut body = body.map(|b| b.to_vec());
    for _ in 0..6 {
        let resp = match send_once(&current, &method, headers, body.as_deref(), timeout, verify) {
            Ok(r) => r,
            Err(_) => {
                std::thread::sleep(Duration::from_millis(120));
                send_once(&current, &method, headers, body.as_deref(), timeout, verify)?
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
