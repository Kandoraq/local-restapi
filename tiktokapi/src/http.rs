use crate::httpc;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct Cookie {
    value: String,
    expires: i64,
}

impl Cookie {
    fn alive(&self) -> bool {
        if self.expires <= 0 {
            return true;
        }
        now() < self.expires
    }
}

struct Jar {
    cookies: BTreeMap<String, Cookie>,
}

static JAR: OnceLock<Mutex<Jar>> = OnceLock::new();

fn jar() -> &'static Mutex<Jar> {
    JAR.get_or_init(|| Mutex::new(Jar { cookies: BTreeMap::new() }))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_set_cookie(raw: &str) -> Option<(String, Cookie)> {
    let first = raw.split(';').next()?;
    let mut kv = first.splitn(2, '=');
    let name = kv.next()?.trim().to_string();
    let value = kv.next()?.trim().to_string();
    if name.is_empty() || value.is_empty() {
        return None;
    }
    let mut expires = 0i64;
    for part in raw.split(';').skip(1) {
        let p = part.trim();
        if let Some(v) = p.strip_prefix("Expires=") {
            expires = parse_http_date(v);
        } else if let Some(v) = p.strip_prefix("Max-Age=") {
            if let Ok(secs) = v.parse::<i64>() {
                expires = now() + secs;
            }
        }
    }
    Some((name, Cookie { value, expires }))
}

fn parse_http_date(s: &str) -> i64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return 0;
    }
    let day = parts[1].parse::<i64>().unwrap_or(0);
    let months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
    let month = months.iter().position(|m| *m == parts[2]).map(|i| i as i64 + 1).unwrap_or(0);
    let year = parts[3].parse::<i64>().unwrap_or(0);
    let clock: Vec<&str> = parts[4].split(':').collect();
    let h = clock.first().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let mi = clock.get(1).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let se = clock.get(2).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    days_from_civil(year, month, day) * 86400 + h * 3600 + mi * 60 + se
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn cookie_header() -> String {
    let mut j = jar().lock().unwrap();
    j.cookies.retain(|_, c| c.alive());
    let mut parts = Vec::new();
    for (name, c) in j.cookies.iter() {
        parts.push(format!("{}={}", name, c.value));
    }
    parts.join("; ")
}

fn absorb(headers: &[(String, String)]) {
    let mut j = jar().lock().unwrap();
    for (k, v) in headers {
        if !k.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        if let Some((name, c)) = parse_set_cookie(v) {
            j.cookies.insert(name, c);
        }
    }
}

fn request(url: &str, extra: &[(String, String)]) -> Result<httpc::Response, String> {
    let mut headers = vec![
        ("User-Agent".to_string(), httpc::USER_AGENT.to_string()),
        ("Accept".to_string(), "application/json, text/html;q=0.9, */*;q=0.8".to_string()),
        ("Cookie".to_string(), cookie_header()),
    ];
    for (k, v) in extra {
        headers.push((k.clone(), v.clone()));
    }
    let r = httpc::get(url, &headers, 30)?;
    absorb(&r.headers);
    Ok(r)
}

pub fn get_text(url: &str, _max_size: &str, _timeout: &str) -> Result<String, String> {
    let mut r = request(url, &[])?;
    let bytes = r.read_to_end(10 * 1024 * 1024)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

pub fn get(url: &str, _max_size: &str, _timeout: &str) -> Result<Vec<u8>, String> {
    let mut r = request(url, &[])?;
    r.read_to_end(512 * 1024 * 1024)
}

#[cfg(test)]
mod cookie_tests {
    use super::parse_set_cookie;

    #[test]
    fn parses_cookie_pair() {
        let (name, c) = parse_set_cookie("sid=abc123; Path=/; HttpOnly").unwrap();
        assert_eq!(name, "sid");
        assert_eq!(c.value, "abc123");
        assert!(c.alive());
    }

    #[test]
    fn parses_max_age() {
        let (_, c) = parse_set_cookie("sid=abc; Max-Age=3600").unwrap();
        assert!(c.alive());
        assert!(c.expires > 0);
    }

    #[test]
    fn expired_cookie_is_dead() {
        let (_, c) = parse_set_cookie("sid=abc; Max-Age=-1").unwrap();
        assert!(!c.alive());
    }
}
