pub const PORT: u16 = 8080;
pub const LOG_TAIL_LINES: usize = 12;

pub struct Service {
    pub name: &'static str,
    pub port: u16,
    pub bin: &'static str,
    pub endpoints: &'static str,
}

pub const SERVICES: &[Service] = &[
    Service { name: "removewmapi", port: 8081, bin: "removewmapi", endpoints: "GET /health, POST /remove-watermark" },
    Service { name: "tiktokapi", port: 8082, bin: "tiktokapi", endpoints: "GET /health, GET /tiktok, GET /download" },
    Service { name: "removebgapi", port: 8084, bin: "removebgapi", endpoints: "GET /health, POST /remove-bg" },
    Service { name: "youtubeapi", port: 8085, bin: "youtubeapi", endpoints: "GET /health, GET /api/search, GET /api/stream" },
];
