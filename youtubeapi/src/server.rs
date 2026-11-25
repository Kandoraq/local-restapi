use crate::httpc;
use crate::json::quote;
use crate::yt::{self, Video};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: u64 = 240;

fn cache() -> &'static Mutex<HashMap<String, (Instant, Video)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Video)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn video_cached(id: &str) -> Result<Video, String> {
    if let Ok(m) = cache().lock() {
        if let Some((t, v)) = m.get(id) {
            if t.elapsed() < Duration::from_secs(CACHE_TTL) {
                return Ok(v.clone());
            }
        }
    }
    let v = yt::extract(id)?;
    if let Ok(mut m) = cache().lock() {
        if m.len() > 200 {
            m.clear();
        }
        m.insert(id.to_string(), (Instant::now(), v.clone()));
    }
    Ok(v)
}

fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "OK",
    }
}

const CORS: &str = "*";

fn base_headers() -> Vec<(String, String)> {
    vec![
        ("Access-Control-Allow-Origin".to_string(), CORS.to_string()),
        (
            "Access-Control-Allow-Headers".to_string(),
            "Range, Content-Type, X-Api-Key".to_string(),
        ),
        (
            "Access-Control-Allow-Methods".to_string(),
            "GET, HEAD, OPTIONS".to_string(),
        ),
        (
            "Access-Control-Expose-Headers".to_string(),
            "Content-Length, Content-Range, Accept-Ranges".to_string(),
        ),
    ]
}

fn send_bytes(
    s: &mut TcpStream,
    status: u16,
    extra: Vec<(String, String)>,
    body: &[u8],
) -> io::Result<()> {
    let mut h = base_headers();
    h.extend(extra);
    h.push(("Content-Length".to_string(), body.len().to_string()));
    let mut out = format!("HTTP/1.1 {} {}\r\n", status, status_text(status));
    for (k, v) in &h {
        out.push_str(k);
        out.push_str(": ");
        out.push_str(v);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    s.write_all(out.as_bytes())?;
    s.write_all(body)?;
    s.flush()
}

fn send_json(s: &mut TcpStream, status: u16, body: String) -> io::Result<()> {
    let mut extra = vec![(
        "Content-Type".to_string(),
        "application/json; charset=utf-8".to_string(),
    )];
    extra.push(("Cache-Control".to_string(), "no-store".to_string()));
    send_bytes(s, status, extra, body.as_bytes())
}

fn error_json(msg: &str) -> String {
    format!("{{\"ok\":false,\"error\":{}}}", quote(msg))
}

fn urldecode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => {
                let h = (b[i + 1] as char).to_digit(16);
                let l = (b[i + 2] as char).to_digit(16);
                match (h, l) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

struct Req {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

impl Req {
    fn q(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    fn h(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn parse_req(raw: &str) -> Option<Req> {
    let mut lines = raw.split("\r\n");
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_ascii_uppercase();
    let target = parts.next()?;
    let (path, qs) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q),
        None => (target.to_string(), ""),
    };
    let mut query = Vec::new();
    for pair in qs.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some(v) => v,
            None => (pair, ""),
        };
        query.push((urldecode(k), urldecode(v)));
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some(i) = line.find(':') {
            headers.push((
                line[..i].trim().to_string(),
                line[i + 1..].trim().to_string(),
            ));
        }
    }
    Some(Req {
        method,
        path,
        query,
        headers,
    })
}

fn extension_for(mime: &str) -> &'static str {
    let m = mime
        .split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase();
    if m.contains("webm") {
        if m.starts_with("audio/") {
            "weba"
        } else {
            "webm"
        }
    } else if m.contains("audio/mp4") || m.contains("m4a") {
        "m4a"
    } else if m.contains("audio/mpeg") {
        "mp3"
    } else if m.contains("mpegurl") {
        "m3u8"
    } else if m.contains("3gpp") {
        "3gp"
    } else if m.starts_with("audio/") {
        "m4a"
    } else {
        "mp4"
    }
}

fn safe_filename(title: &str, ext: &str) -> String {
    let mut t: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else if c.is_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect();
    t = t.trim().to_string();
    if t.is_empty() {
        t = "video".to_string();
    }
    if t.len() > 90 {
        t.truncate(90);
    }
    format!("{}.{}", t, ext)
}

fn formats_json(v: &Video) -> String {
    let mut s = String::new();
    s.push('[');
    for (i, f) in v.formats.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"itag\":{},\"kind\":{},\"quality\":{},\"mime\":{},\"ext\":{},\"codecs\":{},\"bitrate\":{},\"size\":{},\"width\":{},\"height\":{},\"fps\":{},\"duration_ms\":{},\"throttled\":{},\"audio_quality\":{},\"url\":{},\"direct\":{}}}",
            f.itag,
            quote(&f.kind),
            quote(&f.quality),
            quote(&f.mime),
            quote(extension_for(&f.mime)),
            quote(&f.codecs),
            f.bitrate,
            f.size,
            f.width,
            f.height,
            f.fps,
            f.duration_ms,
            f.throttled,
            quote(&f.audio_quality),
            quote(&format!("/api/stream?v={}&itag={}", v.id, f.itag)),
            quote(&f.url)
        ));
    }
    s.push(']');
    s
}

fn video_json(v: &Video) -> String {
    format!(
        "{{\"id\":{},\"title\":{},\"author\":{},\"duration\":{},\"duration_ms\":{},\"duration_ms_approx\":{},\"duration_source\":{},\"views\":{},\"live\":{},\"thumbnail\":{},\"client\":{},\"description\":{},\"formats\":{}}}",
        quote(&v.id),
        quote(&v.title),
        quote(&v.author),
        v.duration,
        v.duration_ms,
        v.duration_ms_approx,
        quote(&v.duration_source),
        v.view_count,
        v.live,
        quote(&v.thumbnail),
        quote(&v.client),
        quote(&v.description),
        formats_json(v)
    )
}

fn require_key(req: &Req) -> bool {
    let key = match std::env::var("YT_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return true,
    };
    if req.q("key") == Some(key.as_str()) {
        return true;
    }
    if let Some(h) = req.h("x-api-key") {
        if h == key {
            return true;
        }
    }
    false
}

fn allowed_target(url: &str) -> bool {
    match httpc::Url::parse(url) {
        Ok(u) => {
            if u.scheme != "https" {
                return false;
            }
            let h = u.host.to_ascii_lowercase();
            [
                "googlevideo.com",
                "youtube.com",
                "ytimg.com",
                "googleusercontent.com",
                "ggpht.com",
            ]
            .iter()
            .any(|d| h == *d || h.ends_with(&format!(".{}", d)))
        }
        Err(_) => false,
    }
}

fn resolve_video(req: &Req) -> Result<Video, String> {
    let id = match req.q("v").or_else(|| req.q("id")).or_else(|| req.q("url")) {
        Some(x) => yt::video_id(x),
        None => return Err("parameter v is required".to_string()),
    };
    if id.is_empty() || id.len() > 64 {
        return Err("invalid video id".to_string());
    }
    video_cached(&id)
}

fn pick_spec(req: &Req) -> String {
    if let Some(it) = req.q("itag") {
        return it.to_string();
    }
    if let Some(f) = req.q("format") {
        return f.to_string();
    }
    if let Some(f) = req.q("quality") {
        return f.to_string();
    }
    "best".to_string()
}

fn parse_content_range(v: &str) -> Option<(u64, u64, Option<u64>)> {
    let v = v.trim();
    let rest = v.strip_prefix("bytes")?.trim();
    let (range, total) = match rest.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let (s, e) = range.split_once('-')?;
    let start: u64 = s.trim().parse().ok()?;
    let end: u64 = e.trim().parse().ok()?;
    if e.trim().is_empty() || end < start {
        return None;
    }
    let total = total.and_then(|t| {
        let t = t.trim();
        if t == "*" {
            None
        } else {
            t.parse::<u64>().ok()
        }
    });
    Some((start, end, total))
}

fn open_upstream(target: &str, range: Option<&str>) -> Result<httpc::Response, String> {
    let mut headers = vec![
        ("User-Agent".to_string(), httpc::USER_AGENT.to_string()),
        ("Accept-Encoding".to_string(), "identity".to_string()),
        ("Referer".to_string(), "https://www.youtube.com/".to_string()),
    ];
    if let Some(r) = range {
        headers.push(("Range".to_string(), r.to_string()));
    }
    httpc::fetch(target, "GET", &headers, None, 60, None)
}

fn proxy_stream(
    s: &mut TcpStream,
    req: &Req,
    target: &str,
    content_type: Option<String>,
    disposition: Option<String>,
) -> io::Result<()> {
    let client_range = req.h("range").map(|r| r.to_string());
    let mut up = match open_upstream(target, client_range.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            let _ = send_json(s, 502, error_json(&format!("upstream: {}", e)));
            return Ok(());
        }
    };
    if up.status != 200 && up.status != 206 {
        let _ = send_json(
            s,
            502,
            error_json(&format!("upstream status {}", up.status)),
        );
        return Ok(());
    }
    up.set_timeout(180);

    let (start, mut expected) = if up.status == 206 {
        match up.header("content-range").and_then(parse_content_range) {
            Some((st, en, _total)) => (st, Some(en - st + 1)),
            None => (0, up.content_length()),
        }
    } else {
        (0, up.content_length())
    };

    if expected.is_none() && up.status == 200 {
        if let Ok(pr) = open_upstream(target, Some("bytes=0-0")) {
            if pr.status == 206 {
                if let Some((_, _, Some(total))) =
                    pr.header("content-range").and_then(parse_content_range)
                {
                    expected = Some(total);
                }
            }
        }
    }

    let mut h = base_headers();
    h.push((
        "Content-Type".to_string(),
        content_type.unwrap_or_else(|| up.content_type()),
    ));
    h.push(("Accept-Ranges".to_string(), "bytes".to_string()));
    if let Some(cr) = up.header("content-range") {
        h.push(("Content-Range".to_string(), cr.to_string()));
    }
    if let Some(d) = disposition {
        h.push(("Content-Disposition".to_string(), d));
    }

    let chunked = expected.is_none();
    let mut head = format!("HTTP/1.1 {} {}\r\n", up.status, status_text(up.status));
    for (k, v) in &h {
        head.push_str(k);
        head.push_str(": ");
        head.push_str(v);
        head.push_str("\r\n");
    }
    if let Some(n) = expected {
        head.push_str(&format!("Content-Length: {}\r\n", n));
    } else {
        head.push_str("Transfer-Encoding: chunked\r\n");
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes())?;
    s.flush()?;

    let mut buf = vec![0u8; 65536];
    let mut written: u64 = 0;
    let mut retries: u32 = 0;
    loop {
        if let Some(total) = expected {
            if written >= total {
                break;
            }
        }
        let n = match up.read_body(&mut buf) {
            Ok(n) => n,
            Err(_) => 0,
        };
        if n == 0 {
            let total = match expected {
                Some(t) => t,
                None => break,
            };
            if written >= total {
                break;
            }
            retries += 1;
            if retries > 10 {
                eprintln!("[proxy] incomplete stream: {}/{} bytes", written, total);
                break;
            }
            let from = start + written;
            let to = start + total - 1;
            let rng = format!("bytes={}-{}", from, to);
            std::thread::sleep(Duration::from_millis(200 * retries.min(8) as u64));
            match open_upstream(target, Some(&rng)) {
                Ok(r) if r.status == 206 => {
                    r.set_timeout(180);
                    up = r;
                }
                Ok(r) => {
                    eprintln!("[proxy] resume rejected (HTTP {})", r.status);
                    break;
                }
                Err(e) => {
                    eprintln!("[proxy] resume failed: {}", e);
                }
            }
            continue;
        }
        let mut k = n;
        if let Some(total) = expected {
            let remain = total - written;
            if k as u64 > remain {
                k = remain as usize;
            }
        }
        let data = &buf[..k];
        let ok = if chunked {
            let mut frame = format!("{:x}\r\n", k).into_bytes();
            frame.extend_from_slice(data);
            frame.extend_from_slice(b"\r\n");
            s.write_all(&frame)
        } else {
            s.write_all(data)
        };
        if ok.is_err() {
            return Ok(());
        }
        written += k as u64;
        if s.flush().is_err() {
            return Ok(());
        }
    }
    if chunked {
        let _ = s.write_all(b"0\r\n\r\n");
    }
    let _ = s.flush();
    Ok(())
}

fn route(s: &mut TcpStream, req: &Req) -> io::Result<()> {
    if req.method == "OPTIONS" {
        return send_bytes(s, 204, vec![], b"");
    }
    if req.path == "/" {
        return send_json(
            s,
            200,
            "{\"ok\":true,\"service\":\"youtubeapi\",\"note\":\"YouTube provides separate video/audio streams only. Combine them with ffmpeg outside this API.\",\"endpoints\":[{\"path\":\"/health\",\"method\":\"GET\"},{\"path\":\"/api/search\",\"method\":\"GET\",\"params\":[\"q\"]},{\"path\":\"/api/info\",\"method\":\"GET\",\"params\":[\"v\"]},{\"path\":\"/api/formats\",\"method\":\"GET\",\"params\":[\"v\"]},{\"path\":\"/api/stream\",\"method\":\"GET\",\"params\":[\"v\",\"format\"],\"range\":true},{\"path\":\"/api/download\",\"method\":\"GET\",\"params\":[\"v\",\"format\"]},{\"path\":\"/api/proxy\",\"method\":\"GET\",\"params\":[\"url\"]}]}".to_string(),
        );
    }
    if req.path == "/health" {
        return send_json(s, 200, "{\"ok\":true,\"service\":\"youtubeapi\",\"note\":\"YouTube provides separate video/audio streams only. Combine them with ffmpeg outside this API.\"}".to_string());
    }
    if !req.path.starts_with("/api/") {
        return send_json(s, 404, error_json("endpoint not found"));
    }
    if !require_key(req) {
        return send_json(s, 401, error_json("API key missing or invalid"));
    }

    match req.path.as_str() {
        "/api/search" => {
            let q = match req.q("q") {
                Some(x) if !x.trim().is_empty() => x,
                _ => return send_json(s, 400, error_json("parameter q is required")),
            };
            match yt::search(q) {
                Ok(items) => {
                    let mut arr = String::from("[");
                    for (i, it) in items.iter().enumerate() {
                        if i > 0 {
                            arr.push(',');
                        }
                        arr.push_str(&format!(
                            "{{\"id\":{},\"title\":{},\"author\":{},\"duration\":{},\"duration_seconds\":{},\"views\":{},\"views_count\":{},\"published\":{},\"verified\":{},\"live\":{},\"thumbnail\":{},\"url\":{}}}",
                            quote(&it.id),
                            quote(&it.title),
                            quote(&it.author),
                            quote(&it.duration),
                            it.duration_seconds,
                            quote(&it.views),
                            it.views_count,
                            quote(&it.published),
                            it.verified,
                            it.live,
                            quote(&it.thumbnail),
                            quote(&format!("https://www.youtube.com/watch?v={}", it.id))
                        ));
                    }
                    arr.push(']');
                    send_json(s, 200, format!("{{\"ok\":true,\"results\":{}}}", arr))
                }
                Err(e) => send_json(s, 502, error_json(&e)),
            }
        }
        "/api/info" | "/api/formats" => match resolve_video(req) {
            Ok(v) => {
                if req.path == "/api/formats" {
                    send_json(s, 200, format!("{{\"ok\":true,\"formats\":{}}}", formats_json(&v)))
                } else {
                    send_json(s, 200, format!("{{\"ok\":true,\"video\":{}}}", video_json(&v)))
                }
            }
            Err(e) => send_json(s, 502, error_json(&e)),
        },
        "/api/stream" | "/api/download" => {
            let v = match resolve_video(req) {
                Ok(v) => v,
                Err(e) => return send_json(s, 502, error_json(&e)),
            };
            let spec = pick_spec(req);
            let low = spec.trim().to_ascii_lowercase();
            if low == "mp3" || low == "audio/mpeg" {
                return send_json(
                    s,
                    400,
                    error_json("YouTube does not provide MP3. Native YouTube audio is m4a (AAC) or weba (Opus). Converting to MP3 requires ffmpeg outside this API."),
                );
            }
            let f = match yt::select(&v, &spec) {
                Some(f) => f.clone(),
                None => {
                    let avail = v
                        .formats
                        .iter()
                        .map(|x| format!("{}={} ({})", x.itag, x.kind, x.quality))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return send_json(
                        s,
                        404,
                        error_json(&format!(
                            "format '{}' is not available. Original formats provided by YouTube for this video: {}",
                            spec, avail
                        )),
                    );
                }
            };
            if f.url.is_empty() {
                return send_json(s, 502, error_json("format has no direct URL"));
            }
            let ct = f.mime.split(';').next().unwrap_or("video/mp4").trim().to_string();
            let disp = if req.path == "/api/download" {
                let ext = extension_for(&f.mime);
                let quality = if f.quality.is_empty() {
                    if f.height > 0 { format!("{}p", f.height) } else { "best".to_string() }
                } else {
                    f.quality.clone()
                };
                let label = format!("{} [youtube {} {}]", v.title, quality, f.kind);
                let name = safe_filename(&label, ext);
                Some(format!(
                    "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                    name.replace('"', ""),
                    urlish(&name)
                ))
            } else {
                None
            };
            proxy_stream(s, req, &f.url, Some(ct), disp)
        }
        "/api/proxy" => {
            let target = match req.q("url") {
                Some(u) => u,
                None => return send_json(s, 400, error_json("parameter url is required")),
            };
            if !allowed_target(target) {
                return send_json(s, 403, error_json("host not allowed"));
            }
            proxy_stream(s, req, target, None, None)
        }
        _ => send_json(s, 404, error_json("endpoint not found")),
    }
}

fn urlish(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        let c = *b as char;
        if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn handle(mut s: TcpStream) -> io::Result<()> {
    s.set_nodelay(true).ok();
    s.set_read_timeout(Some(Duration::from_secs(20))).ok();
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 65536 {
            return Ok(());
        }
    }
    let end = match buf.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(i) => i + 4,
        None => return Ok(()),
    };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let req = match parse_req(&head) {
        Some(r) => r,
        None => {
            let _ = send_json(&mut s, 400, error_json("invalid request"));
            return Ok(());
        }
    };
    let res = route(&mut s, &req);
    let _ = s.flush();
    let _ = s.shutdown(std::net::Shutdown::Both);
    res
}

pub fn serve(host: &str, port: u16) -> io::Result<()> {
    let listener = TcpListener::bind((host, port))?;
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                std::thread::spawn(move || {
                    let _ = handle(stream);
                });
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_parsing() {
        assert_eq!(parse_content_range("bytes 0-99/1000"), Some((0, 99, Some(1000))));
        assert_eq!(
            parse_content_range("bytes 500-999/1000"),
            Some((500, 999, Some(1000)))
        );
        assert_eq!(parse_content_range("bytes 0-99/*"), Some((0, 99, None)));
        assert_eq!(parse_content_range("bytes */1000"), None);
        assert_eq!(parse_content_range("bogus"), None);
    }
}

#[cfg(test)]
mod ext_tests {
    use super::extension_for;

    #[test]
    fn extension_matches_mime() {
        assert_eq!(extension_for("video/mp4; codecs=\"avc1\""), "mp4");
        assert_eq!(extension_for("audio/mp4; codecs=\"mp4a.40.2\""), "m4a");
        assert_eq!(extension_for("video/webm; codecs=\"vp9\""), "webm");
        assert_eq!(extension_for("audio/webm; codecs=\"opus\""), "weba");
        assert_eq!(extension_for("audio/mpeg"), "mp3");
        assert_eq!(extension_for("application/x-mpegURL"), "m3u8");
    }
}
