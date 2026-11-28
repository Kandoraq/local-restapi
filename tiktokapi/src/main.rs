mod constants;
mod http;
mod server;
mod tiktok;
mod util;

fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(constants::DEFAULT_PORT);
    if let Err(e) = server::run(port) {
        eprintln!("server error: {}", e);
        std::process::exit(1);
    }
}
