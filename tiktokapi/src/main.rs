mod crypto;
mod http;
mod httpc;
mod json;
mod server;
mod tiktok;
mod tls;
mod util;

fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8082);
    if let Err(e) = server::run(port) {
        eprintln!("server error: {}", e);
        std::process::exit(1);
    }
}
