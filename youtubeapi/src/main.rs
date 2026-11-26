mod constants;
mod crypto;
mod httpc;
mod json;
mod mp4;
mod server;
mod tls;
mod yt;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--search" {
        let t0 = std::time::Instant::now();
        match yt::search(&args[2]) {
            Ok(v) => {
                println!("results={} in {:?}", v.len(), t0.elapsed());
                for it in v.iter().take(5) {
                    println!("{} | {} | {}", it.id, it.title, it.author);
                }
            }
            Err(e) => println!("ERR {} ({:?})", e, t0.elapsed()),
        }
        return;
    }
    if args.len() >= 3 && args[1] == "--get" {
        let url = &args[2];
        let mut headers = vec![("User-Agent".to_string(), httpc::USER_AGENT.to_string())];
        if let Some(r) = args.get(3) {
            headers.push(("Range".to_string(), r.clone()));
        }
        match httpc::fetch(url, "GET", &headers, None, 20, None) {
            Ok(mut r) => {
                println!(
                    "status={} ct={} cl={:?} cr={:?}",
                    r.status,
                    r.content_type(),
                    r.content_length(),
                    r.header("content-range")
                );
                let mut buf = [0u8; 65536];
                match r.read_body(&mut buf) {
                    Ok(n) => println!("first_read={} head={:?}", n, &buf[..n.min(16)]),
                    Err(e) => println!("read_err={}", e),
                }
            }
            Err(e) => println!("ERR {}", e),
        }
        return;
    }
    if args.len() >= 3 && args[1] == "--tls-test" {
        let host = &args[2];
        let url = format!("https://{}/", host);
        match httpc::get(&url, &[], 60) {
            Ok(mut r) => {
                println!("status={} url={} ct={}", r.status, r.final_url, r.content_type());
                match r.read_to_end(200_000) {
                    Ok(b) => {
                        let head = String::from_utf8_lossy(&b[..b.len().min(150)]);
                        println!("len={} head={:?}", b.len(), head);
                    }
                    Err(e) => println!("body err: {}", e),
                }
            }
            Err(e) => println!("ERR {}", e),
        }
        return;
    }
    if args.len() >= 3 && args[1] == "--probe" {
        let id = yt::video_id(&args[2]);
        print!("{}", yt::probe(&id));
        return;
    }
    if args.len() >= 3 && args[1] == "--url" {
        let id = yt::video_id(&args[2]);
        let spec = args.get(3).map(|s| s.as_str()).unwrap_or("best");
        match yt::extract(&id) {
            Ok(v) => match yt::select(&v, spec) {
                Some(f) => println!("{}", f.url),
                None => println!("ERR format unavailable"),
            },
            Err(e) => println!("ERR {}", e),
        }
        return;
    }
    if args.len() >= 3 && args[1] == "--info" {
        let id = yt::video_id(&args[2]);
        match yt::extract(&id) {
            Ok(v) => {
                println!(
                    "title={}\nauthor={}\ndur={}s dur_ms={} (approx={}, {}) views={} client={} live={}",
                    v.title,
                    v.author,
                    v.duration,
                    v.duration_ms,
                    v.duration_ms_approx,
                    v.duration_source,
                    v.view_count,
                    v.client,
                    v.live
                );
                println!("formats={}", v.formats.len());
                for f in v.formats.iter().take(40) {
                    println!(
                        "itag={} kind={} q={} bitrate={} size={} codecs={} url={}",
                        f.itag,
                        f.kind,
                        f.quality,
                        f.bitrate,
                        f.size,
                        f.codecs,
                        &f.url[..f.url.len().min(90)]
                    );
                }
            }
            Err(e) => println!("ERR {}", e),
        }
        return;
    }

    let host = std::env::var("HOST").unwrap_or_else(|_| constants::DEFAULT_HOST.to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(constants::DEFAULT_PORT);

    println!("youtubeapi ready at http://{}:{}", host, port);
    println!("  GET /api/search?q=kata");
    println!("  GET /api/info?v=VIDEO_ID");
    println!("  GET /api/formats?v=VIDEO_ID");
    println!("  GET /api/stream?v=VIDEO_ID&format=best");
    println!("  GET /api/download?v=VIDEO_ID&format=best");
    println!("  GET /api/proxy?url=URL_GOOGLEVIDEO");
    if let Ok(k) = std::env::var("YT_KEY") {
        if !k.is_empty() {
            println!("  API key aktif (?key=...)");
        }
    }

    if let Err(e) = server::serve(&host, port) {
        eprintln!("server failed: {}", e);
        std::process::exit(1);
    }
}
