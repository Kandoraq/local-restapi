use crate::constants::{MAX_BODY, READ_CHUNK, READ_TIMEOUT_SECS};
use crate::tiktok;
use crate::util::{extract_json_string, json_escape, parse_query};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn bind(port: u16) -> std::io::Result<TcpListener> {
    let addr = format!("0.0.0.0:{}", port);
    TcpListener::bind(&addr)
}

pub fn run(port: u16) -> std::io::Result<()> {
    let listener = bind(port)?;
    println!("listening on 0.0.0.0:{}", port);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| handle(stream));
            }
            Err(e) => eprintln!("connection error: {}", e),
        }
    }
    Ok(())
}

fn read_body_limited<R: Read>(reader: &mut R, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; READ_CHUNK];
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

fn handle(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(READ_TIMEOUT_SECS)));
    let mut reader = BufReader::new(match stream.try_clone() { Ok(s) => s, Err(_) => return });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() { return; }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 { return; }
    let method = parts[0];
    let target = parts[1];

    let mut headers = std::collections::HashMap::new();
    loop {
        let mut l = String::new();
        if reader.read_line(&mut l).is_err() { return; }
        let t = l.trim_end_matches(['\r', '\n']);
        if t.is_empty() { break; }
        if let Some(i) = t.find(':') {
            headers.insert(t[..i].trim().to_lowercase(), t[i+1..].trim().to_string());
        }
    }

    let content_length: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let body = read_body_limited(&mut reader, content_length.min(MAX_BODY));

    let path = target.split('?').next().unwrap_or(target);
    let query = target.split('?').nth(1).unwrap_or("");
    let params = parse_query(query);

    if method == "OPTIONS" {
        write(&mut stream, 204, "No Content", "application/json", b"", None, true);
        return;
    }
    if method == "GET" && (path == "/" || path == "/health") {
        write(&mut stream, 200, "OK", "application/json", br#"{"status":"ok","service":"tiktok-api"}"#, None, true);
        return;
    }

    let is_api = path == "/tiktok" || path == "/api/tiktok";
    let is_media = path == "/download" || path == "/media" || path == "/file";
    if !is_api && !is_media {
        write(&mut stream, 404, "Not Found", "application/json", br#"{"error":"not found"}"#, None, true);
        return;
    }

    let url = if method == "POST" {
        let ct = headers.get("content-type").cloned().unwrap_or_default();
        let text = String::from_utf8_lossy(&body);
        if ct.contains("application/json") {
            extract_json_string(&text, "url")
        } else if ct.contains("application/x-www-form-urlencoded") {
            parse_query(&text).get("url").cloned()
        } else {
            params.get("url").cloned()
        }
    } else {
        params.get("url").cloned()
    };

    let url = match url {
        Some(u) if !u.is_empty() => u,
        _ => {
            write(&mut stream, 400, "Bad Request", "application/json", br#"{"error":"missing url param"}"#, None, true);
            return;
        }
    };

    if is_media {
        let fmt = match params.get("format").map(|s| s.as_str()).unwrap_or("mp4") {
            "mp3" => "mp3",
            _ => "mp4",
        };
        let quality = params.get("quality").map(|s| s.as_str()).unwrap_or("hd");
        match tiktok::fetch(&url).and_then(|raw| tiktok::parse_info(&raw)) {
            Ok(info) => match tiktok::media_url(&info, quality, fmt)
                .and_then(|m| tiktok::download(&m, fmt)) {
                Ok((bytes, ct, ext)) => {
                    let name = if info.id.is_empty() { format!("tiktok.{}", ext) } else { format!("tiktok_{}.{}", info.id, ext) };
                    write(&mut stream, 200, "OK", &ct, &bytes, Some(&name), false);
                }
                Err(e) => error(&mut stream, &e),
            },
            Err(e) => error(&mut stream, &e),
        }
    } else {
        match tiktok::fetch(&url) {
            Ok(raw) => {
                let body = match tiktok::parse_info(&raw) {
                    Ok(info) => {
                        format!(
                            "{{\"status\":\"ok\",\"id\":\"{}\",\"title\":\"{}\",\"kind\":\"{}\",\"images\":{},\"cover\":\"{}\",\"origin_cover\":\"{}\",\"wmplay\":\"{}\",\"hdplay\":\"{}\",\"play\":\"{}\",\"music\":\"{}\",\"play_count\":{},\"digg_count\":{},\"comment_count\":{},\"share_count\":{},\"author\":\"{}\",\"duration\":{},\"raw\":{}}}",
                            json_escape(&info.id),
                            json_escape(&info.title),
                            json_escape(&info.kind),
                            {
                                let mut a = String::from("[");
                                for (i, u) in info.images.iter().enumerate() {
                                    if i > 0 { a.push(','); }
                                    a.push_str(&format!("\"{}\"", json_escape(u)));
                                }
                                a.push(']');
                                a
                            },
                            json_escape(&info.cover),
                            json_escape(&info.origin_cover),
                            json_escape(&info.wmplay),
                            json_escape(&info.hdplay),
                            json_escape(&info.play),
                            json_escape(&info.music),
                            info.play_count,
                            info.digg_count,
                            info.comment_count,
                            info.share_count,
                            json_escape(&info.author),
                            info.duration,
                            raw,
                        )
                    }
                    Err(e) => format!("{{\"status\":\"ok\",\"raw\":{},\"parse_error\":\"{}\"}}", raw, json_escape(&e)),
                };
                write(&mut stream, 200, "OK", "application/json", body.as_bytes(), None, false);
            }
            Err(e) => error(&mut stream, &e),
        }
    }
}

fn error(stream: &mut TcpStream, msg: &str) {
    let body = format!("{{\"error\":\"{}\"}}", json_escape(msg));
    write(stream, 502, "Bad Gateway", "application/json", body.as_bytes(), None, true);
}

fn write(stream: &mut TcpStream, code: u16, reason: &str, ct: &str, body: &[u8], name: Option<&str>, json_len: bool) {
    let mut resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n",
        code, reason, ct, body.len()
    );
    if let Some(n) = name {
        resp.push_str(&format!("Content-Disposition: attachment; filename=\"{}\"\r\n", n));
    }
    let _ = json_len;
    resp.push_str("\r\n");
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(body);
}
