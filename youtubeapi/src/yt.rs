use crate::httpc;
use crate::json::{self, quote, Json};
use std::sync::OnceLock;

const PLAYER_URL: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const PLAYER_URL_KEY: &str = "https://www.youtube.com/youtubei/v1/player?key=REDACTED_YOUTUBE_API_KEY&prettyPrint=false";
const SEARCH_URL: &str = "https://www.youtube.com/youtubei/v1/search?key=REDACTED_YOUTUBE_API_KEY&prettyPrint=false";
const HOME_URL: &str = "https://www.youtube.com/";

struct Profile {
    name: &'static str,
    ver: &'static str,
    num: &'static str,
    ua: &'static str,
    extra: &'static str,
    referer: &'static str,
    keyed: bool,
}

const PROFILES: &[Profile] = &[
    Profile {
        name: "ANDROID",
        ver: "21.44.38",
        num: "3",
        ua: "com.google.android.youtube/21.44.38 (Linux; U; Android 14) gzip",
        extra: "\"androidSdkVersion\":34,\"osName\":\"Android\",\"osVersion\":\"14\",\"deviceMake\":\"Google\",\"deviceModel\":\"Pixel 7\",\"userAgent\":\"com.google.android.youtube/21.44.38 (Linux; U; Android 14) gzip\"",
        referer: "https://www.youtube.com/",
        keyed: false,
    },
    Profile {
        name: "IOS",
        ver: "21.44.3",
        num: "5",
        ua: "com.google.ios.youtube/21.44.3 (iPhone17,2; U; CPU iOS 18_5_0 like Mac OS X;)",
        extra: "\"deviceMake\":\"Apple\",\"deviceModel\":\"iPhone17,2\",\"osName\":\"iPhone\",\"osVersion\":\"18.5.0.22F76\",\"userAgent\":\"com.google.ios.youtube/21.44.3 (iPhone17,2; U; CPU iOS 18_5_0 like Mac OS X;)\"",
        referer: "https://www.youtube.com/",
        keyed: false,
    },
    Profile {
        name: "ANDROID_VR",
        ver: "1.62.27",
        num: "28",
        ua: "com.google.android.apps.youtube.vr.oculus/1.62.27 (Linux; U; Android 12; GB) gzip",
        extra: "\"deviceMake\":\"Oculus\",\"deviceModel\":\"Quest 3\",\"androidSdkVersion\":32,\"osName\":\"Android\",\"osVersion\":\"12\",\"userAgent\":\"com.google.android.apps.youtube.vr.oculus/1.62.27 (Linux; U; Android 12; GB) gzip\"",
        referer: "https://www.youtube.com/",
        keyed: false,
    },
    Profile {
        name: "WEB",
        ver: "2.20261123.08.00",
        num: "1",
        ua: httpc::USER_AGENT,
        extra: "",
        referer: "https://www.youtube.com/",
        keyed: true,
    },
    Profile {
        name: "MWEB",
        ver: "2.20261123.08.00",
        num: "2",
        ua: "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36",
        extra: "",
        referer: "https://www.youtube.com/",
        keyed: true,
    },
    Profile {
        name: "TVHTML5_SIMPLY_EMBEDDED_PLAYER",
        ver: "2.0",
        num: "85",
        ua: "Mozilla/5.0 (PlayStation; PlayStation 4/12.00) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Safari/605.1.15",
        extra: "\"clientScreen\":\"EMBED\",\"thirdParty\":{\"embedUrl\":\"https://www.youtube.com/\"}",
        referer: "https://www.youtube.com/",
        keyed: false,
    },
    Profile {
        name: "WEB_EMBEDDED_PLAYER",
        ver: "1.20261123.01.00",
        num: "56",
        ua: httpc::USER_AGENT,
        extra: "\"clientScreen\":\"EMBED\",\"thirdParty\":{\"embedUrl\":\"https://www.youtube.com/\"}",
        referer: "https://www.youtube.com/",
        keyed: false,
    },
];

#[derive(Clone, Debug)]
pub struct Format {
    pub itag: i64,
    pub url: String,
    pub mime: String,
    pub kind: String,
    pub quality: String,
    pub bitrate: i64,
    pub size: i64,
    pub width: i64,
    pub height: i64,
    pub fps: i64,
    pub codecs: String,
    pub audio_quality: String,
    pub duration_ms: i64,
    pub throttled: bool,
}

#[derive(Clone, Debug)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub author: String,
    pub duration: i64,
    pub duration_ms: i64,
    pub duration_ms_approx: i64,
    pub duration_source: String,
    pub view_count: i64,
    pub live: bool,
    pub thumbnail: String,
    pub description: String,
    pub client: String,
    pub formats: Vec<Format>,
}

#[derive(Clone, Debug)]
pub struct SearchItem {
    pub id: String,
    pub title: String,
    pub author: String,
    pub duration: String,
    pub duration_seconds: i64,
    pub views: String,
    pub views_count: i64,
    pub published: String,
    pub verified: bool,
    pub live: bool,
    pub thumbnail: String,
}

#[derive(Clone, Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub expires: i64,
    pub domain: String,
}

#[derive(Clone, Debug, Default)]
pub struct CookieJar {
    cookies: std::collections::BTreeMap<String, Cookie>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.get(name)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn set(&mut self, name: &str, value: &str, expires: i64) {
        self.cookies.insert(
            name.to_string(),
            Cookie {
                name: name.to_string(),
                value: value.to_string(),
                expires,
                domain: ".youtube.com".to_string(),
            },
        );
    }

    pub fn merge(&mut self, items: &[(String, String, i64)]) {
        for (name, value, expires) in items {
            self.set(name, value, *expires);
        }
    }

    pub fn header(&self) -> String {
        let now = now_unix();
        self.cookies
            .values()
            .filter(|c| c.expires == 0 || c.expires > now)
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_cookie_file(txt: &str) -> CookieJar {
    let mut jar = CookieJar::new();
    for line in txt.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        let domain = f[0];
        if !(domain.contains("youtube.com") || domain.contains("google.com")) {
            continue;
        }
        let expires = f[4].parse::<i64>().unwrap_or(0);
        if expires != 0 && expires <= now_unix() {
            continue;
        }
        jar.set(f[5], f[6], expires);
    }
    jar
}

fn parse_http_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }
    let day = parts[1].parse::<i64>().ok()?;
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mon = months.iter().position(|m| *m == parts[2])? as i64 + 1;
    let year = parts[3].parse::<i64>().ok()?;
    let clock: Vec<&str> = parts[4].split(':').collect();
    if clock.len() != 3 {
        return None;
    }
    let h = clock[0].parse::<i64>().ok()?;
    let mi = clock[1].parse::<i64>().ok()?;
    let se = clock[2].parse::<i64>().ok()?;

    let (year, mon) = if mon == 12 && day > 31 {
        (year + 1, 1)
    } else {
        (year, mon)
    };
    let days = days_from_civil(year, mon, day);
    Some(days * 86400 + h * 3600 + mi * 60 + se)
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

fn parse_set_cookie(raw: &str) -> Option<(String, String, i64)> {
    let mut it = raw.split(';');
    let pair = it.next()?.trim();
    let (name, value) = pair.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut expires = 0i64;
    for part in it {
        let part = part.trim();
        let (k, v) = match part.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        if k.eq_ignore_ascii_case("expires") {
            expires = parse_http_date(v).unwrap_or(0);
        } else if k.eq_ignore_ascii_case("max-age") {
            let secs = v.trim().parse::<i64>().unwrap_or(0);
            expires = if secs > 0 {
                now_unix() + secs
            } else {
                0
            };
        }
    }
    Some((name.to_string(), value.to_string(), expires))
}

struct Session {
    jar: std::sync::Mutex<CookieJar>,
    visitor: std::sync::Mutex<Option<String>>,
    path: String,
    last_refresh: std::sync::Mutex<i64>,
    file_stamp: std::sync::Mutex<(u64, u64)>,
}

fn file_stamp(path: &str) -> (u64, u64) {
    match std::fs::metadata(path) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (modified, m.len())
        }
        Err(_) => (0, 0),
    }
}

static SESSION: OnceLock<Session> = OnceLock::new();

fn session() -> &'static Session {
    SESSION.get_or_init(|| Session {
        jar: std::sync::Mutex::new(CookieJar::new()),
        visitor: std::sync::Mutex::new(None),
        path: std::env::var("YT_COOKIES").unwrap_or_else(|_| "cookies.txt".to_string()),
        last_refresh: std::sync::Mutex::new(0),
        file_stamp: std::sync::Mutex::new((0, 0)),
    })
}

const REFRESH_TTL: i64 = 300;

fn refresh_cookies() {
    let s = session();
    let now = now_unix();
    let stamp = file_stamp(&s.path);
    let changed = match s.file_stamp.lock() {
        Ok(mut prev) => {
            if *prev != stamp {
                *prev = stamp;
                true
            } else {
                false
            }
        }
        Err(_) => false,
    };
    let expired_ttl = match s.last_refresh.lock() {
        Ok(t) => now - *t >= REFRESH_TTL,
        Err(_) => false,
    };
    if !changed && !expired_ttl {
        return;
    }
    if let Ok(txt) = std::fs::read_to_string(&s.path) {
        let fresh = parse_cookie_file(&txt);
        if let Ok(mut jar) = s.jar.lock() {
            for (name, cookie) in fresh.cookies {
                jar.cookies.insert(name, cookie);
            }
        }
    }
    if let Ok(mut t) = s.last_refresh.lock() {
        *t = now;
    }
}

fn cookie_header() -> String {
    refresh_cookies();
    match session().jar.lock() {
        Ok(jar) => jar.header(),
        Err(_) => String::new(),
    }
}

fn absorb_cookies(headers: &[(String, String)]) {
    let mut updates = Vec::new();
    for (k, v) in headers {
        if !k.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        if let Some((name, value, expires)) = parse_set_cookie(v) {
            updates.push((name, value, expires));
        }
    }
    if updates.is_empty() {
        return;
    }
    if let Ok(mut jar) = session().jar.lock() {
        jar.merge(&updates);
        let _ = save_cookies(&jar);
    }
}

fn save_cookies(jar: &CookieJar) -> std::io::Result<()> {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    let now = now_unix();
    for c in jar.cookies.values() {
        if c.expires != 0 && c.expires <= now {
            continue;
        }
        out.push_str(&format!(
            "{}\tTRUE\t/\tFALSE\t{}\t{}\t{}\n",
            c.domain, c.expires, c.name, c.value
        ));
    }
    std::fs::write(&session().path, out)
}

fn visitor_data() -> Option<String> {
    if let Ok(v) = std::env::var("YT_VISITOR") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    if let Ok(v) = session().visitor.lock() {
        if let Some(v) = v.as_ref() {
            return Some(v.clone());
        }
    }
    let headers = vec![
        ("User-Agent".to_string(), httpc::USER_AGENT.to_string()),
        ("Accept-Language".to_string(), "en-US,en;q=0.9".to_string()),
        ("Cookie".to_string(), cookie_header()),
    ];
    let mut r = httpc::get(HOME_URL, &headers, 30).ok()?;
    absorb_cookies(&r.headers);
    let body = r.read_to_end(3_000_000).ok()?;
    let txt = String::from_utf8_lossy(&body);
    let v = extract_json_string(&txt, "visitorData")?;
    if let Ok(mut slot) = session().visitor.lock() {
        *slot = Some(v.clone());
    }
    Some(v)
}

fn extract_json_string(hay: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", key);
    let i = hay.find(&pat)? + pat.len();
    let rest = &hay[i..];
    let end = rest.find('"')?;
    let v = &rest[..end];
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

fn profile_headers(p: &Profile) -> Vec<(String, String)> {
    let mut h = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), p.ua.to_string()),
        ("X-YouTube-Client-Name".to_string(), p.num.to_string()),
        ("X-YouTube-Client-Version".to_string(), p.ver.to_string()),
        ("Accept-Language".to_string(), "en-US,en;q=0.9".to_string()),
        ("Origin".to_string(), "https://www.youtube.com".to_string()),
        ("Referer".to_string(), p.referer.to_string()),
    ];
    let ck = cookie_header();
    if !ck.is_empty() {
        h.push(("Cookie".to_string(), ck));
    }
    if let Some(v) = visitor_data() {
        h.push(("X-Goog-Visitor-Id".to_string(), v));
    }
    h
}

fn player_body(p: &Profile, id: &str) -> String {
    let mut s = String::new();
    s.push_str("{\"context\":{\"client\":{\"clientName\":");
    s.push_str(&quote(p.name));
    s.push_str(",\"clientVersion\":");
    s.push_str(&quote(p.ver));
    s.push_str(",\"hl\":\"en\",\"gl\":\"US\"");
    if !p.extra.is_empty() {
        s.push(',');
        s.push_str(p.extra);
    }
    s.push_str("}},\"videoId\":");
    s.push_str(&quote(id));
    s.push_str(",\"contentCheckOk\":true,\"racyCheckOk\":true}");
    s
}

fn player_url(p: &Profile) -> &'static str {
    if p.keyed {
        PLAYER_URL_KEY
    } else {
        PLAYER_URL
    }
}

fn playability(v: &Json) -> (String, String) {
    let st = v
        .at(&["playabilityStatus", "status"])
        .and_then(|x| x.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();
    let reason = v
        .at(&["playabilityStatus", "reason"])
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    (st, reason)
}

fn url_of(j: &Json) -> Option<String> {
    if let Some(u) = j.get("url").and_then(|x| x.as_str()) {
        return Some(u.to_string());
    }
    if let Some(sc) = j.get("signatureCipher").and_then(|x| x.as_str()) {
        for pair in sc.split('&') {
            if let Some(rest) = pair.strip_prefix("url=") {
                return Some(percent_decode(rest));
            }
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hexval(b[i + 1]), hexval(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hexval(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn query_has(url: &str, key: &str) -> bool {
    match url.split_once('?') {
        Some((_, q)) => q.split('&').any(|p| p.split('=').next() == Some(key)),
        None => false,
    }
}

fn parse_format(j: &Json, kind: &str) -> Option<Format> {
    let url = url_of(j)?;
    let mime = j
        .get("mimeType")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let codecs = mime
        .split("codecs=\"")
        .nth(1)
        .map(|s| s.trim_end_matches('"').to_string())
        .unwrap_or_default();
    let throttled = query_has(&url, "n");
    Some(Format {
        itag: j.get("itag").and_then(|x| x.as_i64()).unwrap_or(0),
        url,
        mime,
        kind: kind.to_string(),
        quality: j
            .get("qualityLabel")
            .and_then(|x| x.as_str())
            .or_else(|| j.get("quality").and_then(|x| x.as_str()))
            .unwrap_or("")
            .to_string(),
        bitrate: j.get("bitrate").and_then(|x| x.as_i64()).unwrap_or(0),
        size: j.get("contentLength").and_then(|x| x.as_i64()).unwrap_or(0),
        width: j.get("width").and_then(|x| x.as_i64()).unwrap_or(0),
        height: j.get("height").and_then(|x| x.as_i64()).unwrap_or(0),
        fps: j.get("fps").and_then(|x| x.as_i64()).unwrap_or(0),
        codecs,
        audio_quality: j
            .get("audioQuality")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        duration_ms: j
            .get("approxDurationMs")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        throttled,
    })
}

fn parse_video(v: &Json, client: &str) -> Option<Video> {
    let sd = v.get("streamingData")?;
    let mut formats: Vec<Format> = Vec::new();
    for key in ["adaptiveFormats", "formats"] {
        if let Some(arr) = sd.get(key).and_then(|x| x.as_arr()) {
            for f in arr {
                let mime = f.get("mimeType").and_then(|x| x.as_str()).unwrap_or("");
                let kind = if mime.starts_with("audio/") {
                    "audio"
                } else if mime.starts_with("video/") {
                    "video"
                } else {
                    continue;
                };
                if let Some(x) = parse_format(f, kind) {
                    if !formats.iter().any(|e| e.itag == x.itag) {
                        formats.push(x);
                    }
                }
            }
        }
        if !formats.is_empty() {
            break;
        }
    }
    if formats.is_empty() {
        return None;
    }
    let det = v.get("videoDetails");
    let g = |k: &str| {
        det.and_then(|d| d.get(k))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let length_s = det
        .and_then(|d| d.get("lengthSeconds"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let micro_s = v
        .at(&["microformat", "playerMicroformatRenderer", "lengthSeconds"])
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let fmt_ms = formats.iter().map(|f| f.duration_ms).max().unwrap_or(0);
    let duration_ms = if fmt_ms > 0 {
        fmt_ms
    } else if length_s > 0 {
        length_s * 1000
    } else if micro_s > 0 {
        micro_s * 1000
    } else {
        0
    };
    let duration = if duration_ms > 0 {
        (duration_ms + 500) / 1000
    } else {
        length_s.max(micro_s)
    };
    let thumb = det
        .and_then(|d| d.at(&["thumbnail", "thumbnails"]))
        .and_then(|x| x.as_arr())
        .and_then(|a| a.last())
        .and_then(|t| t.get("url"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Some(Video {
        id: g("videoId"),
        title: g("title"),
        author: g("author"),
        duration,
        duration_ms,
        duration_ms_approx: duration_ms,
        duration_source: "approx".to_string(),
        view_count: det
            .and_then(|d| d.get("viewCount"))
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        live: det
            .and_then(|d| d.get("isLiveContent"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        thumbnail: thumb,
        description: g("shortDescription"),
        client: client.to_string(),
        formats,
    })
}

fn html_player(id: &str) -> Option<Json> {
    let url = format!(
        "https://www.youtube.com/watch?v={}&bpctr=9999999999&has_verified=1",
        id
    );
    let headers = vec![
        ("User-Agent".to_string(), httpc::USER_AGENT.to_string()),
        ("Accept-Language".to_string(), "en-US,en;q=0.9".to_string()),
        ("Cookie".to_string(), cookie_header().to_string()),
    ];
    let mut r = httpc::get(&url, &headers, 30).ok()?;
    absorb_cookies(&r.headers);
    let body = r.read_to_end(6_000_000).ok()?;
    let txt = String::from_utf8_lossy(&body);
    let obj = extract_balanced_object(&txt, "ytInitialPlayerResponse")?;
    json::parse(&obj).ok()
}

fn extract_balanced_object(hay: &str, key: &str) -> Option<String> {
    let k = hay.find(&format!("\"{}\"", key))?;
    let after = &hay[k + key.len() + 2..];
    let brace = after.find('{')?;
    let bytes = after.as_bytes();
    let start = brace;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(after[start..=i].to_string());
            }
        }
        i += 1;
    }
    None
}

fn try_player(p: &Profile, id: &str) -> Result<Video, String> {
    let body = player_body(p, id);
    let h = profile_headers(p);
    let mut r = httpc::post_json(player_url(p), &body, &h, 60)?;
    absorb_cookies(&r.headers);
    if r.status != 200 {
        return Err(format!("http {}", r.status));
    }
    let raw = r.read_to_end(6_000_000)?;
    let txt = String::from_utf8_lossy(&raw);
    let j = json::parse(&txt)?;
    let (st, reason) = playability(&j);
    match parse_video(&j, p.name) {
        Some(v) => Ok(v),
        None => Err(format!("{} {}", st, reason)),
    }
}

pub fn probe(id: &str) -> String {
    let mut out = String::new();
    for p in PROFILES {
        match try_player(p, id) {
            Ok(v) => {
                let with_n = v.formats.iter().filter(|f| f.throttled).count();
                out.push_str(&format!(
                    "{} OK total={} with_n={} dur_ms={}\n",
                    p.name,
                    v.formats.len(),
                    with_n,
                    v.duration_ms
                ));
            }
            Err(e) => out.push_str(&format!("{} FAIL {}\n", p.name, e)),
        }
    }
    match html_player(id) {
        Some(j) => {
            let (st, reason) = playability(&j);
            match parse_video(&j, "HTML") {
                Some(v) => out.push_str(&format!(
                    "HTML OK total={} dur_ms={}\n",
                    v.formats.len(),
                    v.duration_ms
                )),
                None => out.push_str(&format!("HTML FAIL {} {}\n", st, reason)),
            }
        }
        None => out.push_str("HTML FAIL no player response\n"),
    }
    out
}

pub fn extract(id: &str) -> Result<Video, String> {
    let mut errs = Vec::new();
    let mut best: Option<Video> = None;
    for p in PROFILES {
        match try_player(p, id) {
            Ok(v) => {
                if best.as_ref().is_none_or(|b| v.formats.len() > b.formats.len()) {
                    best = Some(v);
                }
                let enough = best.as_ref().is_some_and(|b| {
                    b.formats.len() >= 4
                        && b.formats.iter().any(|f| f.kind == "video")
                        && b.formats.iter().any(|f| f.kind == "audio")
                });
                if enough {
                    break;
                }
            }
            Err(e) => errs.push(format!("{}: {}", p.name, e)),
        }
    }
    if let Some(v) = best {
        return Ok(v);
    }
    if let Some(j) = html_player(id) {
        if let Some(v) = parse_video(&j, "HTML") {
            return Ok(v);
        }
        let (st, reason) = playability(&j);
        errs.push(format!("HTML: {} {}", st, reason));
    } else {
        errs.push("HTML: no player response".to_string());
    }
    Err(errs.join(" | "))
}

pub fn select<'a>(v: &'a Video, spec: &str) -> Option<&'a Format> {
    let s = spec.trim().to_ascii_lowercase();
    if let Ok(itag) = s.parse::<i64>() {
        if let Some(f) = v.formats.iter().find(|f| f.itag == itag) {
            return Some(f);
        }
    }
    let audio = || v.formats.iter().filter(|f| f.kind == "audio");
    let video = || v.formats.iter().filter(|f| f.kind == "video");
    match s.as_str() {
        "" | "best" => video()
            .max_by_key(|f| (f.height, f.bitrate))
            .or_else(|| audio().max_by_key(|f| f.bitrate)),
        "worst" => v.formats.iter().min_by_key(|f| f.bitrate),
        "audio" | "audioonly" => audio().max_by_key(|f| f.bitrate),
        "video" | "videoonly" => video().max_by_key(|f| (f.height, f.bitrate)),
        "m4a" => audio()
            .filter(|f| f.mime.contains("mp4"))
            .max_by_key(|f| f.bitrate),
        "webm" => v
            .formats
            .iter()
            .filter(|f| f.mime.contains("webm"))
            .max_by_key(|f| f.bitrate),
        "mp4" => v
            .formats
            .iter()
            .filter(|f| f.mime.contains("mp4") && f.kind != "audio")
            .max_by_key(|f| f.bitrate),
        "opus" => audio()
            .filter(|f| f.codecs.to_ascii_lowercase().contains("opus"))
            .max_by_key(|f| f.bitrate),
        "aac" => audio()
            .filter(|f| f.codecs.to_ascii_lowercase().contains("mp4a"))
            .max_by_key(|f| f.bitrate),
        "1080" | "1080p" => video().filter(|f| f.height == 1080).max_by_key(|f| f.bitrate),
        "720" | "720p" => video().filter(|f| f.height == 720).max_by_key(|f| f.bitrate),
        "480" | "480p" => video().filter(|f| f.height == 480).max_by_key(|f| f.bitrate),
        "360" | "360p" => video().filter(|f| f.height == 360).max_by_key(|f| f.bitrate),
        _ => None,
    }
}

fn parse_clock(s: &str) -> i64 {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return 0;
    }
    let mut values = Vec::with_capacity(parts.len());
    for part in &parts {
        let p = part.trim();
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return 0;
        }
        let v: i64 = match p.parse() {
            Ok(v) => v,
            Err(_) => return 0,
        };
        values.push(v);
    }
    if parts.len() > 1 {
        for v in values.iter().skip(1) {
            if *v >= 60 {
                return 0;
            }
        }
    }
    let mut total = 0i64;
    for v in values {
        total = total * 60 + v;
    }
    total
}

fn parse_views(s: &str) -> i64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }
    let first = s.split_whitespace().next().unwrap_or("");
    let lower = first.to_ascii_lowercase();
    let multiplier = if lower.ends_with('m') {
        1_000_000.0
    } else if lower.ends_with('k') {
        1_000.0
    } else {
        1.0
    };
    let digits: String = first
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if digits.is_empty() {
        return 0;
    }
    let n: f64 = match digits.parse() {
        Ok(n) => n,
        Err(_) => return 0,
    };
    (n * multiplier) as i64
}

pub fn parse_search_item(vr: &Json) -> SearchItem {
    let id = vr
        .get("videoId")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let runs_text = |path: &[&str]| -> String {
        let runs = match vr.at(path).and_then(|x| x.as_arr()) {
            Some(r) => r,
            None => return String::new(),
        };
        runs.iter()
            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
            .collect()
    };
    let simple = |path: &[&str]| -> String {
        vr.at(path)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let title_runs = runs_text(&["title", "runs"]);
    let title = if title_runs.is_empty() {
        simple(&["title", "simpleText"])
    } else {
        title_runs
    };
    let duration = simple(&["lengthText", "simpleText"]);
    let views = {
        let v = simple(&["viewCountText", "simpleText"]);
        if v.is_empty() {
            simple(&["shortViewCountText", "simpleText"])
        } else {
            v
        }
    };
    let badge_style = |needle: &str| {
        vr.get("badges")
            .and_then(|x| x.as_arr())
            .map(|a| {
                a.iter().any(|b| {
                    b.at(&["metadataBadgeRenderer", "style"])
                        .and_then(|x| x.as_str())
                        == Some(needle)
                })
            })
            .unwrap_or(false)
    };
    let verified = vr
        .get("ownerBadges")
        .and_then(|x| x.as_arr())
        .map(|a| {
            a.iter().any(|b| {
                b.at(&["metadataBadgeRenderer", "style"])
                    .and_then(|x| x.as_str())
                    == Some("BADGE_STYLE_TYPE_VERIFIED")
            })
        })
        .unwrap_or(false);
    let thumbnail = vr
        .at(&["thumbnail", "thumbnails"])
        .and_then(|x| x.as_arr())
        .and_then(|a| a.last())
        .and_then(|t| t.get("url"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let author = {
        let a = runs_text(&["ownerText", "runs"]);
        if !a.is_empty() {
            a
        } else {
            let b = runs_text(&["longBylineText", "runs"]);
            if !b.is_empty() {
                b
            } else {
                runs_text(&["shortBylineText", "runs"])
            }
        }
    };
    let live = badge_style("BADGE_STYLE_TYPE_LIVE_NOW") || duration.is_empty();
    SearchItem {
        id,
        title,
        author,
        duration_seconds: if live { 0 } else { parse_clock(&duration) },
        duration,
        views_count: parse_views(&views),
        views,
        published: simple(&["publishedTimeText", "simpleText"]),
        verified,
        live,
        thumbnail,
    }
}

pub fn dedup_search(items: Vec<SearchItem>) -> Vec<SearchItem> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|it| !it.id.is_empty() && seen.insert(it.id.clone()))
        .collect()
}

fn search_client_version() -> &'static str {
    PROFILES
        .iter()
        .find(|p| p.name == "WEB")
        .map(|p| p.ver)
        .unwrap_or("2.20261123.08.00")
}

pub fn search(q: &str) -> Result<Vec<SearchItem>, String> {
    let mut body = String::new();
    body.push_str("{\"context\":{\"client\":{\"clientName\":\"WEB\",\"clientVersion\":");
    body.push_str(&quote(search_client_version()));
    body.push_str(",\"hl\":\"en\",\"gl\":\"US\"}},\"query\":");
    body.push_str(&quote(q));
    body.push('}');
    let headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), httpc::USER_AGENT.to_string()),
        ("Accept-Language".to_string(), "en-US,en;q=0.9".to_string()),
        ("Origin".to_string(), "https://www.youtube.com".to_string()),
        ("Referer".to_string(), "https://www.youtube.com/".to_string()),
        ("Cookie".to_string(), cookie_header().to_string()),
    ];
    let mut r = httpc::post_json(SEARCH_URL, &body, &headers, 60)?;
    absorb_cookies(&r.headers);
    if r.status != 200 {
        return Err(format!("search HTTP {}", r.status));
    }
    let raw = r.read_to_end(8_000_000)?;
    let txt = String::from_utf8_lossy(&raw);
    let j = json::parse(&txt)?;
    let mut found = Vec::new();
    json::collect_with_key(&j, "videoRenderer", &mut found);
    let items: Vec<SearchItem> = found.into_iter().map(parse_search_item).collect();
    let items = dedup_search(items);
    Ok(items.into_iter().take(25).collect())
}

pub fn video_id(input: &str) -> String {
    let s = input.trim();
    if s.len() == 11
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return s.to_string();
    }
    if let Some(rest) = s.split("youtu.be/").nth(1) {
        return rest.split(['?', '&', '/']).next().unwrap_or("").to_string();
    }
    if let Some(rest) = s.split("v=").nth(1) {
        return rest.split('&').next().unwrap_or("").to_string();
    }
    if let Some(rest) = s.split("/shorts/").nth(1) {
        return rest.split(['?', '&', '/']).next().unwrap_or("").to_string();
    }
    if let Some(rest) = s.split("/embed/").nth(1) {
        return rest.split(['?', '&', '/']).next().unwrap_or("").to_string();
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(itag: i64, kind: &str, mime: &str, codecs: &str, height: i64, bitrate: i64) -> Format {
        Format {
            itag,
            url: format!("https://example.com/{}", itag),
            mime: mime.to_string(),
            kind: kind.to_string(),
            quality: if height > 0 {
                format!("{}p", height)
            } else {
                "audio".to_string()
            },
            bitrate,
            size: 0,
            width: 0,
            height,
            fps: 0,
            codecs: codecs.to_string(),
            audio_quality: String::new(),
            duration_ms: 1000,
            throttled: false,
        }
    }

    fn video(formats: Vec<Format>) -> Video {
        Video {
            id: "x".to_string(),
            title: "t".to_string(),
            author: "a".to_string(),
            duration: 1,
            duration_ms: 1000,
            duration_ms_approx: 1000,
            duration_source: "container".to_string(),
            view_count: 0,
            live: false,
            thumbnail: String::new(),
            description: String::new(),
            client: "TEST".to_string(),
            formats,
        }
    }

    #[test]
    fn select_original_formats_only() {
        let v = video(vec![
            fmt(140, "audio", "audio/mp4; codecs=\"mp4a.40.2\"", "mp4a.40.2", 0, 128),
            fmt(251, "audio", "audio/webm; codecs=\"opus\"", "opus", 0, 160),
            fmt(137, "video", "video/mp4; codecs=\"avc1.640028\"", "avc1.640028", 1080, 4000),
            fmt(248, "video", "video/webm; codecs=\"vp9\"", "vp9", 1080, 3000),
        ]);
        assert_eq!(select(&v, "best").map(|f| f.itag), Some(137));
        assert_eq!(select(&v, "audio").map(|f| f.itag), Some(251));
        assert_eq!(select(&v, "m4a").map(|f| f.itag), Some(140));
        assert_eq!(select(&v, "webm").map(|f| f.itag), Some(248));
        assert_eq!(select(&v, "opus").map(|f| f.itag), Some(251));
        assert_eq!(select(&v, "1080").map(|f| f.itag), Some(137));
        assert!(select(&v, "mp3").is_none());
        assert!(select(&v, "720").is_none());
    }

    #[test]
    fn parse_clock_validation() {
        assert_eq!(parse_clock("1:44:10"), 6250);
        assert_eq!(parse_clock("2:29"), 149);
        assert_eq!(parse_clock("12:30"), 750);
        assert_eq!(parse_clock("1:2:3:4"), 0);
        assert_eq!(parse_clock("1:99:00"), 0);
        assert_eq!(parse_clock("bad"), 0);
    }

    #[test]
    fn parse_views_counts() {
        assert_eq!(parse_views("337,793 views"), 337793);
        assert_eq!(parse_views("2.6M views"), 2600000);
        assert_eq!(parse_views("1.3K views"), 1300);
        assert_eq!(parse_views("No views"), 0);
        assert_eq!(parse_views(""), 0);
    }

    #[test]
    fn parse_search_item_fields() {
        let j = json::parse(r#"{"videoId":"abc12345678","title":{"runs":[{"text":"Hello "},{"text":"World"}]},"ownerText":{"runs":[{"text":"Chan"}]},"lengthText":{"simpleText":"2:29"},"viewCountText":{"simpleText":"2,697,240 views"},"publishedTimeText":{"simpleText":"5 years ago"},"ownerBadges":[{"metadataBadgeRenderer":{"style":"BADGE_STYLE_TYPE_VERIFIED"}}],"thumbnail":{"thumbnails":[{"url":"small"},{"url":"large"}]}}"#).unwrap();
        let it = parse_search_item(&j);
        assert_eq!(it.id, "abc12345678");
        assert_eq!(it.title, "Hello World");
        assert_eq!(it.author, "Chan");
        assert_eq!(it.duration_seconds, 149);
        assert_eq!(it.views_count, 2697240);
        assert_eq!(it.published, "5 years ago");
        assert!(it.verified);
        assert!(!it.live);
        assert_eq!(it.thumbnail, "large");
    }

    #[test]
    fn dedup_search_results() {
        let a = json::parse(r#"{"videoId":"same0000001","title":{"simpleText":"A"}}"#).unwrap();
        let b = json::parse(r#"{"videoId":"same0000001","title":{"simpleText":"A"}}"#).unwrap();
        let c = json::parse(r#"{"videoId":"other000002","title":{"simpleText":"B"}}"#).unwrap();
        let items = dedup_search(vec![parse_search_item(&a), parse_search_item(&b), parse_search_item(&c)]);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "same0000001");
        assert_eq!(items[1].id, "other000002");
    }
}

#[cfg(test)]
mod cookie_tests {
    use super::*;

    fn netscape(domain: &str, name: &str, value: &str, expires: i64) -> String {
        format!("# Netscape HTTP Cookie File\n{}\tTRUE\t/\tFALSE\t{}\t{}\t{}\n", domain, expires, name, value)
    }

    #[test]
    fn parse_netscape_filters_domains() {
        let txt = format!("{}{}{}",
            netscape(".youtube.com", "SID", "abc", 9999999999),
            netscape(".evil.com", "TRACK", "zzz", 9999999999),
            netscape(".google.com", "HSID", "def", 9999999999));
        let c = parse_cookie_file(&txt);
        assert_eq!(c.get("SID").map(|x| x.value.as_str()), Some("abc"));
        assert_eq!(c.get("HSID").map(|x| x.value.as_str()), Some("def"));
        assert!(c.get("TRACK").is_none());
    }

    #[test]
    fn parse_netscape_drops_expired() {
        let txt = netscape(".youtube.com", "OLD", "stale", 1);
        let c = parse_cookie_file(&txt);
        assert!(c.is_empty());
    }

    #[test]
    fn cookie_header_sorted_and_deduped() {
        let mut c = CookieJar::new();
        c.set("b", "2", 9999999999);
        c.set("a", "1", 9999999999);
        c.set("a", "3", 9999999999);
        assert_eq!(c.header(), "a=3; b=2");
    }

    #[test]
    fn set_cookie_parses_attributes() {
        let c = parse_set_cookie("SID=xyz; Path=/; Domain=.youtube.com; Expires=Wed, 21 Oct 2037 07:28:00 GMT; HttpOnly; Secure").unwrap();
        assert_eq!(c.0, "SID");
        assert_eq!(c.1, "xyz");
        assert!(c.2 > 0);
    }

    #[test]
    fn set_cookie_session_has_zero_expiry() {
        let c = parse_set_cookie("VISITOR_INFO1_LIVE=abc; Path=/; Secure").unwrap();
        assert_eq!(c.2, 0);
    }

    #[test]
    fn merge_refreshes_existing_cookie() {
        let mut c = CookieJar::new();
        c.set("SID", "old", 9999999999);
        c.merge(&[("SID".to_string(), "new".to_string(), 9999999999)]);
        assert_eq!(c.get("SID").unwrap().value, "new");
    }

    #[test]
    fn author_fallback_byline() {
        let j = json::parse(r#"{"videoId":"abc12345678","title":{"simpleText":"T"},"longBylineText":{"runs":[{"text":"Fallback Chan"}]}}"#).unwrap();
        assert_eq!(parse_search_item(&j).author, "Fallback Chan");
    }

    #[test]
    fn live_badge_detected() {
        let j = json::parse(r#"{"videoId":"abc12345678","title":{"simpleText":"T"},"lengthText":{"simpleText":"LIVE"},"badges":[{"metadataBadgeRenderer":{"style":"BADGE_STYLE_TYPE_LIVE_NOW"}}]}"#).unwrap();
        assert!(parse_search_item(&j).live);
        assert_eq!(parse_search_item(&j).duration_seconds, 0);
    }

    #[test]
    fn non_live_with_length_not_marked_live() {
        let j = json::parse(r#"{"videoId":"abc12345678","title":{"simpleText":"T"},"lengthText":{"simpleText":"2:29"}}"#).unwrap();
        assert!(!parse_search_item(&j).live);
    }

    #[test]
    fn views_short_text_fallback() {
        let j = json::parse(r#"{"videoId":"abc12345678","title":{"simpleText":"T"},"shortViewCountText":{"simpleText":"2.6M views"}}"#).unwrap();
        assert_eq!(parse_search_item(&j).views_count, 2600000);
    }
}
