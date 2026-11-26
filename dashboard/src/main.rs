mod constants;
mod pages;
mod process;
mod server;

fn main() {
    if let Err(e) = server::run() {
        eprintln!("dashboard error: {}", e);
        std::process::exit(1);
    }
}
