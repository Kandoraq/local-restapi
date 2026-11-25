pub fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() { continue; }
        let mut kv = pair.splitn(2, '=');
        let k = kv.next().unwrap_or("");
        let v = kv.next().unwrap_or("");
        map.insert(url_decode(k), url_decode(v));
    }
    map
}

pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 < b.len() {
                let h = hexval(b[i+1]);
                let l = hexval(b[i+2]);
                if let (Some(h), Some(l)) = (h, l) {
                    out.push((h << 4) | l);
                    i += 3;
                    continue;
                }
            }
            out.push(b'%');
            i += 1;
        } else if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(b[i]);
            i += 1;
        }
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

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn json_escape(s: &str) -> String {
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

pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)? + needle.len();
    let rest = &json[pos..];
    let colon = rest.find(':')?;
    let rest = rest[colon+1..].trim_start();
    if !rest.starts_with('"') { return None; }
    let mut out = String::new();
    let mut esc = false;
    for c in rest[1..].chars() {
        if esc {
            match c {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'u' => out.push('u'),
                c => out.push(c),
            }
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

pub fn extract_json_number(json: &str, key: &str) -> Option<i64> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)? + needle.len();
    let rest = &json[pos..];
    let colon = rest.find(':')?;
    let rest = rest[colon+1..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

pub fn extract_object(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)? + needle.len();
    let rest = &json[pos..];
    let colon = rest.find(':')?;
    let rest = rest[colon+1..].trim_start();
    if !rest.starts_with('{') { return None; }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in rest.char_indices() {
        if in_str {
            if esc { esc = false; }
            else if c == '\\' { esc = true; }
            else if c == '"' { in_str = false; }
        } else {
            match c {
                '"' => in_str = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 { return Some(rest[..=i].to_string()); }
                }
                _ => {}
            }
        }
    }
    None
}

pub fn extract_nested_string(json: &str, obj: &str, key: &str) -> Option<String> {
    let inner = extract_object(json, obj)?;
    extract_json_string(&inner, key)
}

pub fn extract_string_array(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{}\"", key);
    let pos = match json.find(&needle) { Some(p) => p + needle.len(), None => return vec![] };
    let rest = &json[pos..];
    let colon = match rest.find(':') { Some(c) => c, None => return vec![] };
    let rest = rest[colon+1..].trim_start();
    if !rest.starts_with('[') { return vec![]; }
    let mut out = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    let mut cur = String::new();
    for c in rest[1..].chars() {
        if in_str {
            if esc {
                match c {
                    'n' => cur.push('\n'),
                    'r' => cur.push('\r'),
                    't' => cur.push('\t'),
                    c => cur.push(c),
                }
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                out.push(cur.clone());
                cur.clear();
                in_str = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_str = true;
        } else if c == ']' {
            break;
        }
    }
    out
}
