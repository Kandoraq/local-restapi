#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn at(&self, path: &[&str]) -> Option<&Json> {
        let mut cur = self;
        for k in path {
            cur = cur.get(k)?;
        }
        Some(cur)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Str(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|n| n as i64)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_obj(&self) -> Option<&Vec<(String, Json)>> {
        match self {
            Json::Obj(o) => Some(o),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Parser {
            b: s.as_bytes(),
            i: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\n' || c == b'\r' || c == b'\t' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("invalid JSON character: {}", c as char)),
            None => Err("empty JSON".to_string()),
        }
    }

    fn literal(&mut self, lit: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at position {}", self.i))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.i += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| "invalid number")?;
        s.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("invalid number: {}", s))
    }

    fn string(&mut self) -> Result<String, String> {
        if self.bump() != Some(b'"') {
            return Err("string harus diawali kutip".to_string());
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.bump().ok_or_else(|| "unterminated string".to_string())?;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.bump().ok_or_else(|| "escape rusak".to_string())?;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) {
                                if self.peek() == Some(b'\\') && self.b.get(self.i + 1) == Some(&b'u') {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    let c = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                    push_char(&mut out, c);
                                } else {
                                    push_char(&mut out, 0xFFFD);
                                }
                            } else if (0xDC00..0xE000).contains(&cp) {
                                push_char(&mut out, 0xFFFD);
                            } else {
                                push_char(&mut out, cp);
                            }
                        }
                        _ => return Err(format!("unknown escape: {}", e as char)),
                    }
                }
                c if c < 0x20 => out.push(b' '),
                c => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| "string bukan UTF-8".to_string())
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.b.len() {
            return Err("unicode escape pendek".to_string());
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + 4]).map_err(|_| "unicode rusak")?;
        let v = u32::from_str_radix(s, 16).map_err(|_| "invalid unicode".to_string())?;
        self.i += 4;
        Ok(v)
    }

    fn array(&mut self) -> Result<Json, String> {
        self.bump();
        let mut v = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.bump();
            return Ok(Json::Arr(v));
        }
        loop {
            v.push(self.value()?);
            self.skip_ws();
            match self.bump() {
                Some(b',') => continue,
                Some(b']') => break,
                _ => return Err("invalid array".to_string()),
            }
        }
        Ok(Json::Arr(v))
    }

    fn object(&mut self) -> Result<Json, String> {
        self.bump();
        let mut v = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.bump();
            return Ok(Json::Obj(v));
        }
        loop {
            self.skip_ws();
            let k = self.string()?;
            self.skip_ws();
            if self.bump() != Some(b':') {
                return Err("object without colon".to_string());
            }
            let val = self.value()?;
            v.push((k, val));
            self.skip_ws();
            match self.bump() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err("invalid object".to_string()),
            }
        }
        Ok(Json::Obj(v))
    }
}

fn push_char(out: &mut Vec<u8>, cp: u32) {
    if let Some(c) = char::from_u32(cp) {
        let mut buf = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
}

pub fn parse(s: &str) -> Result<Json, String> {
    let mut p = Parser::new(s);
    let v = p.value()?;
    p.skip_ws();
    Ok(v)
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
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

pub fn quote(s: &str) -> String {
    format!("\"{}\"", escape(s))
}

pub fn collect_with_key<'a>(j: &'a Json, key: &str, out: &mut Vec<&'a Json>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if k == key {
                    out.push(v);
                }
                collect_with_key(v, key, out);
            }
        }
        Json::Arr(a) => {
            for v in a {
                collect_with_key(v, key, out);
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
pub fn find_first_with_key<'a>(j: &'a Json, key: &str) -> Option<&'a Json> {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if k == key {
                    return Some(v);
                }
            }
            for (_, v) in m {
                if let Some(x) = find_first_with_key(v, key) {
                    return Some(x);
                }
            }
            None
        }
        Json::Arr(a) => {
            for v in a {
                if let Some(x) = find_first_with_key(v, key) {
                    return Some(x);
                }
            }
            None
        }
        _ => None,
    }
}
