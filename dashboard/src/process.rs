use crate::constants::{Service, LOG_TAIL_LINES, SERVICES};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub struct Entry {
    pub child: Option<Child>,
    pub started: Option<Instant>,
}

static STATE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();

fn state() -> &'static Mutex<HashMap<String, Entry>> {
    STATE.get_or_init(|| {
        let mut m = HashMap::new();
        for s in SERVICES {
            m.insert(s.name.to_string(), Entry { child: None, started: None });
        }
        Mutex::new(m)
    })
}

pub fn service(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.name == name)
}

fn log_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from("logs");
    p.push(format!("{}.log", name));
    p
}

pub fn start(name: &str) -> Result<u32, String> {
    let s = service(name).ok_or_else(|| format!("unknown service: {}", name))?;
    std::fs::create_dir_all("logs").map_err(|e| e.to_string())?;
    let log = File::create(log_path(name)).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let bin = root.join(s.bin).join("target").join("release").join(s.bin);
    let child = Command::new(&bin)
        .current_dir(root.join(s.bin))
        .env("PORT", s.port.to_string())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("cannot start {}: {}", name, e))?;
    let pid = child.id();
    let mut map = state().lock().unwrap();
    if let Some(mut old) = map.get_mut(name).and_then(|e| e.child.take()) {
        let _ = old.kill();
    }
    map.insert(name.to_string(), Entry { child: Some(child), started: Some(Instant::now()) });
    Ok(pid)
}

pub fn stop(name: &str) -> Result<(), String> {
    service(name).ok_or_else(|| format!("unknown service: {}", name))?;
    let mut map = state().lock().unwrap();
    let entry = map.get_mut(name).ok_or_else(|| "no state".to_string())?;
    if let Some(mut child) = entry.child.take() {
        child.kill().map_err(|e| e.to_string())?;
    }
    entry.started = None;
    Ok(())
}

pub fn pid(name: &str) -> Option<u32> {
    state().lock().unwrap().get(name).and_then(|e| e.child.as_ref().map(|c| c.id()))
}

pub fn uptime(name: &str) -> Option<u64> {
    state().lock().unwrap().get(name).and_then(|e| e.started.map(|t| t.elapsed().as_secs()))
}

pub fn log_tail(name: &str) -> String {
    service(name).ok_or(()).map_err(|_| ()).ok();
    let file = match File::open(log_path(name)) {
        Ok(f) => f,
        Err(_) => return "no log".to_string(),
    };
    let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    let from = lines.len().saturating_sub(LOG_TAIL_LINES);
    lines[from..].join("\n")
}

pub fn health(port: u16) -> bool {
    use std::net::TcpStream;
    let mut stream = match TcpStream::connect(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let req = format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 64];
    match stream.read(&mut buf) {
        Ok(n) => n > 0 && buf[..n].starts_with(b"HTTP/1.1 200"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::service;

    #[test]
    fn unknown_service_is_none() {
        assert!(service("bogus").is_none());
    }

    #[test]
    fn known_services_present() {
        assert!(service("tiktokapi").is_some());
        assert!(service("youtubeapi").is_some());
    }
}
