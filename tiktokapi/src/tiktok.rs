use crate::http;
use crate::util::{extract_json_number, extract_json_string, extract_nested_string, extract_object, extract_string_array, url_encode};

const API: &str = "https://www.tikwm.com/api/";

pub struct Info {
    pub id: String,
    pub title: String,
    pub cover: String,
    pub origin_cover: String,
    pub wmplay: String,
    pub hdplay: String,
    pub play: String,
    pub music: String,
    pub play_count: i64,
    pub digg_count: i64,
    pub comment_count: i64,
    pub share_count: i64,
    pub author: String,
    pub duration: i64,
    pub kind: String,
    pub images: Vec<String>,
}

pub fn fetch(url: &str) -> Result<String, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("invalid url scheme".into());
    }
    let api = format!("{}?url={}&hd=1", API, url_encode(url));
    let text = http::get_text(&api, "10485760", "30")?;
    let code = extract_json_number(&text, "code").unwrap_or(-1);
    if code != 0 {
        let msg = extract_json_string(&text, "msg").unwrap_or_else(|| "unknown error".into());
        return Err(msg);
    }
    Ok(text)
}

pub fn parse_info(raw: &str) -> Result<Info, String> {
    let data = extract_object(raw, "data").unwrap_or_else(|| raw.to_string());
    let id = extract_json_string(&data, "id").unwrap_or_default();
    let title = extract_json_string(&data, "title").unwrap_or_default();
    let cover = extract_json_string(&data, "cover").unwrap_or_default();
    let origin_cover = extract_json_string(&data, "origin_cover").unwrap_or_default();
    let wmplay = extract_json_string(&data, "wmplay").unwrap_or_default();
    let hdplay = extract_json_string(&data, "hdplay").unwrap_or_default();
    let play = extract_json_string(&data, "play").unwrap_or_default();
    let music = extract_json_string(&data, "music").unwrap_or_default();
    let play_count = extract_json_number(&data, "play_count").unwrap_or(0);
    let digg_count = extract_json_number(&data, "digg_count").unwrap_or(0);
    let comment_count = extract_json_number(&data, "comment_count").unwrap_or(0);
    let share_count = extract_json_number(&data, "share_count").unwrap_or(0);
    let author = extract_nested_string(&data, "author", "unique_id").unwrap_or_default();
    let duration = extract_json_number(&data, "duration").unwrap_or(0);
    let images = extract_string_array(&data, "images");
    let kind = if !images.is_empty() { "slideshow".to_string() } else { "video".to_string() };
    Ok(Info { id, title, cover, origin_cover, wmplay, hdplay, play, music, play_count, digg_count, comment_count, share_count, author, duration, kind, images })
}

pub fn media_url(info: &Info, quality: &str, format: &str) -> Result<String, String> {
    if format == "mp3" {
        if info.music.is_empty() { return Err("no audio".into()); }
        return Ok(info.music.clone());
    }
    if info.kind == "slideshow" {
        return Err("this post is a slideshow with images, not a video; use /tiktok to get the image URLs or format=mp3 for audio".into());
    }
    if (quality == "low" || quality == "sd") && !info.play.is_empty() {
        return Ok(info.play.clone());
    }
    if !info.hdplay.is_empty() {
        return Ok(info.hdplay.clone());
    }
    if !info.play.is_empty() {
        return Ok(info.play.clone());
    }
    Err("no video".into())
}

pub fn host_allowed(url: &str) -> bool {
    let host = url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    host.ends_with("tiktokcdn.com")
        || host.ends_with("tiktokcdn-us.com")
        || host.ends_with("tikwm.com")
        || host.ends_with("musicaldown.com")
        || host.ends_with("tiktok.com")
}

pub fn download(media_url: &str, format: &str) -> Result<(Vec<u8>, String, String), String> {
    if !media_url.starts_with("http://") && !media_url.starts_with("https://") {
        return Err("invalid media url".into());
    }
    if !host_allowed(media_url) {
        return Err("media host not allowed".into());
    }
    let mut last = String::from("download failed");
    for _ in 0..3 {
        match http::get(media_url, "524288000", "600") {
            Ok(bytes) => {
                if bytes.is_empty() { last = "empty media".into(); continue; }
                let (ct, ext) = if format == "mp3" {
                    ("audio/mpeg", "mp3")
                } else {
                    ("video/mp4", "mp4")
                };
                return Ok((bytes, ct.to_string(), ext.to_string()));
            }
            Err(e) => { last = e; continue; }
        }
    }
    Err(last)
}
