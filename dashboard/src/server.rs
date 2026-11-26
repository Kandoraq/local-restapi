use crate::constants::{SERVICES, PORT};
use crate::pages;
use crate::process;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub fn run() -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    println!("dashboard ready at http://0.0.0.0:{}", PORT);
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            thread::spawn(|| handle(s));
        }
    }
    Ok(())
}

struct Req {
    path: String,
    query: std::collections::HashMap<String, String>,
}

fn parse_q(q: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() { continue; }
        let mut kv = pair.splitn(2, '=');
        m.insert(kv.next().unwrap_or("").to_string(), kv.next().unwrap_or("").to_string());
    }
    m
}

fn send(stream: &mut TcpStream, code: u16, reason: &str, ct: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        code, reason, ct, body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

fn json(stream: &mut TcpStream, code: u16, reason: &str, body: String) {
    send(stream, code, reason, "application/json", body.as_bytes());
}

fn html(stream: &mut TcpStream, body: String) {
    send(stream, 200, "OK", "text/html; charset=utf-8", body.as_bytes());
}

fn handle(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let mut reader = BufReader::new(match stream.try_clone() { Ok(s) => s, Err(_) => return });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() { return; }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 { return; }
    let target = parts[1];
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() { return; }
        if h.trim().is_empty() { break; }
    }
    let path = target.split('?').next().unwrap_or(target).to_string();
    let query = parse_q(target.split('?').nth(1).unwrap_or(""));
    let req = Req { path, query };

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

    if req.path.starts_with("/api/") || req.path == "/health" {
        route_api(&mut stream, &req);
        return;
    }

    let body = match req.path.as_str() {
        "/" | "" => pages::dashboard(&host),
        "/download" => pages::download(&host),
        "/watch" => pages::watch(),
        "/images" => pages::images(&host),
        "/about" => pages::about(),
        _ => {
            json(&mut stream, 404, "Not Found", "{\"error\":\"not found\"}".to_string());
            return;
        }
    };
    html(&mut stream, body);
}

fn route_api(stream: &mut TcpStream, req: &Req) {
    match req.path.as_str() {
        "/health" => json(stream, 200, "OK", "{\"ok\":true,\"service\":\"dashboard\"}".to_string()),
        "/api/services" => {
            let mut out = String::from("[");
            for (i, s) in SERVICES.iter().enumerate() {
                if i > 0 { out.push(','); }
                let pid = process::pid(s.name);
                let alive = process::health(s.port);
                let state = if alive { "running" } else if pid.is_some() { "unhealthy" } else { "stopped" };
                let up = process::uptime(s.name).unwrap_or(0);
                out.push_str(&format!(
                    "{{\"name\":\"{}\",\"port\":{},\"pid\":{},\"state\":\"{}\",\"uptime\":{},\"endpoints\":\"{}\"}}",
                    s.name, s.port, pid.map(|p| p.to_string()).unwrap_or_else(|| "null".to_string()), state, up, s.endpoints
                ));
            }
            out.push(']');
            json(stream, 200, "OK", out);
        }
        "/api/start" | "/api/stop" => {
            let name = req.query.get("name").map(|s| s.as_str()).unwrap_or("");
            if process::service(name).is_none() {
                json(stream, 400, "Bad Request", format!("{{\"error\":\"unknown service: {}\"}}", name));
                return;
            }
            let result = if req.path == "/api/start" {
                process::start(name).map(|p| format!("{{\"ok\":true,\"pid\":{}}}", p))
            } else {
                process::stop(name).map(|_| "{\"ok\":true}".to_string())
            };
            match result {
                Ok(body) => json(stream, 200, "OK", body),
                Err(e) => json(stream, 500, "Internal Server Error", format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\""))),
            }
        }
        "/api/logs" => {
            let name = req.query.get("name").map(|s| s.as_str()).unwrap_or("");
            if process::service(name).is_none() {
                json(stream, 400, "Bad Request", format!("{{\"error\":\"unknown service: {}\"}}", name));
                return;
            }
            let tail = process::log_tail(name);
            let escaped = tail.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
            json(stream, 200, "OK", format!("{{\"log\":\"{}\"}}", escaped));
        }
        _ => json(stream, 404, "Not Found", "{\"error\":\"not found\"}".to_string()),
    }
}
