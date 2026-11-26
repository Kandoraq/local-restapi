# local-restapi

![local-restapi banner](docs/banner.png)

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org)
[![Dependencies](https://img.shields.io/badge/dependencies-none-green.svg)](Cargo.toml)

A collection of small REST services written in Rust. Every service is its own binary, listens on its own port, and builds with no third-party crates.

| Service | Port | Purpose |
| --- | --- | --- |
| `dashboard` | 8080 | Monitor and control the services, plus browser tools |
| `removewmapi` | 8081 | Remove a watermark from a PNG |
| `tiktokapi` | 8082 | Read TikTok metadata and download its media |
| `removebgapi` | 8084 | Remove a flat-colour background from a PNG |
| `youtubeapi` | 8085 | Search, inspect, stream, and download YouTube media |

## Contents

- [Quick start](#quick-start)
- [Architecture](#architecture)
- [Service reference](#service-reference)
- [Common tasks](#common-tasks)
- [Configuration](#configuration)
- [Security notes](#security-notes)
- [Design decisions](#design-decisions)
- [Security policy](SECURITY.md)
- [License](#license)

## Quick start

Requirements: a Rust toolchain and `curl`.

Build all four binaries:

```sh
for d in removewmapi tiktokapi removebgapi youtubeapi; do
  cargo build --release --manifest-path "$d/Cargo.toml"
done
```

Start one service:

```sh
PORT=8081 ./removewmapi/target/release/removewmapi
```

Check that it is alive:

```sh
curl http://127.0.0.1:8081/health
```

Every service answers `/health` with a JSON status object and closes the connection after one response.

A first real request for each service:

```sh
# Remove a background from a PNG
curl -X POST http://127.0.0.1:8084/remove-bg \
  -F 'image=@photo.png' -o photo-nobg.png

# Remove a watermark from a PNG
curl -X POST http://127.0.0.1:8081/remove-watermark \
  -F 'image=@photo.png' -o photo-clean.png

# Read TikTok metadata
curl 'http://127.0.0.1:8082/tiktok?url=https%3A%2F%2Fwww.tiktok.com%2F%40tiktok%2Fvideo%2F7424648048074181931'

# Read YouTube metadata
curl 'http://127.0.0.1:8085/api/info?v=dQw4w9WgXcQ'
```

## Architecture

Each service owns one port and one process. A change to one service cannot take down another. A long video download in `youtubeapi` does not block image processing in `removebgapi`.

The cost is duplicated HTTP plumbing: every service parses its own request line and headers. This is deliberate. A shared web framework would introduce a dependency, and these services stay on the Rust standard library only.

Networking differs by service:

- `removewmapi` and `removebgapi` only read the request body. They never make outbound requests.
- `tiktokapi` and `youtubeapi` make outbound HTTPS requests with a local TLS 1.3 implementation. Both keep a cookie jar that absorbs `Set-Cookie` and drops expired entries before the next request.
- `youtubeapi` also implements AES-GCM, SHA-256, and X25519 locally. Its TLS client does not verify certificate chains. See [Security notes](#security-notes).

Request bodies are read in 64 KiB chunks. The declared `Content-Length` is capped, and memory grows only as bytes arrive, so a caller cannot force a large allocation with a single header.

## Service reference

### removewmapi (8081)

| Method | Path | Result |
| --- | --- | --- |
| GET | `/`, `/health` | JSON status |
| POST | `/remove-watermark`, `/remove`, `/api/remove` | PNG with the watermark region inpainted |

Request body: `multipart/form-data`, field `image`, containing a PNG.

| Query | Type | Default | Meaning |
| --- | --- | --- | --- |
| `x` | u32 | detected | Left edge of the watermark box |
| `y` | u32 | detected | Top edge of the watermark box |
| `w` | u32 | detected | Box width |
| `h` | u32 | detected | Box height |
| `threshold` | u8 | 200 | Brightness cutoff for detection |
| `radius` | i32 | 3 | Inpainting radius |
| `iters` | usize | 4 | Inpainting iterations |

Responses: `200` with `image/png`, `400` when no image is present, `422` on decode failure. Body limit: 25 MiB.

### removebgapi (8084)

| Method | Path | Result |
| --- | --- | --- |
| GET | `/`, `/health` | JSON status |
| POST | `/remove-bg`, `/remove-background`, `/api/remove-bg`, `/remove` | PNG with a transparent background |

Request body: `multipart/form-data`, field `image`, containing a PNG.

| Query | Type | Default | Meaning |
| --- | --- | --- | --- |
| `threshold` | u32 | 32 | Colour distance from the sampled border colour |
| `color` | hex | border sample | Explicit background colour as `RRGGBB` |

Responses: `200` with `image/png`, `400` when no image is present, `422` on decode failure. Body limit: 25 MiB.

### tiktokapi (8082)

| Method | Path | Result |
| --- | --- | --- |
| GET | `/`, `/health` | JSON status |
| GET, POST | `/tiktok`, `/api/tiktok` | Metadata, media URLs, and `kind` |
| GET | `/download`, `/media`, `/file` | Media bytes with `Content-Disposition` |

| Query | Default | Meaning |
| --- | --- | --- |
| `url` | required | TikTok URL with `http://` or `https://` |
| `format` | `mp4` | `mp4` or `mp3` |
| `quality` | `hd` | `hd`, `low`, or `sd` |

For POST, `url` may be a JSON field, a form field, or a query parameter.

The metadata response includes `id`, `title`, `kind`, `images`, `cover`, `origin_cover`, `wmplay`, `hdplay`, `play`, `music`, `play_count`, `digg_count`, `comment_count`, `share_count`, `author`, and `duration`, plus the upstream JSON in `raw`.

`kind` is either `video` or `slideshow`. A slideshow has a non-empty `images` array and no video stream. Requesting `format=mp4` for a slideshow returns an error instead of a mislabelled audio file.

Media hosts are limited to known TikTok CDN domains. Body limit: 1 MiB.

### youtubeapi (8085)

| Path | Parameters | Result |
| --- | --- | --- |
| `/` | none | Service description and endpoint list |
| `/health` | none | JSON status |
| `/api/search` | `q` | Search results |
| `/api/info` | `v`, `id`, or `url` | Video metadata and format list |
| `/api/formats` | `v`, `id`, or `url` | Format list |
| `/api/stream` | `v`, `format`, `key` | Proxied media stream with `Range` support |
| `/api/download` | `v`, `format`, `key` | Same stream with `Content-Disposition` |
| `/api/proxy` | `url`, `key` | Proxied upstream media URL |

Format selectors: `best`, `worst`, `audio`, `audioonly`, `video`, `videoonly`, `m4a`, `webm`, `mp4`, `opus`, `aac`, `1080`, `720`, `480`, `360`, or a numeric itag.

`best` selects the highest-resolution video-only format. The service does not mux video and audio. `/api/proxy` accepts only HTTPS URLs on `googlevideo.com`, `youtube.com`, `ytimg.com`, `googleusercontent.com`, and `ggpht.com`, including their subdomains. Everything else returns `403`.

## Common tasks

### Remove a background that is not white

```sh
curl -X POST http://127.0.0.1:8084/remove-bg \
  -F 'image=@photo.png' -F 'color=00ff00'
```

### Remove a watermark in a known position

```sh
curl -X POST http://127.0.0.1:8081/remove-watermark \
  -F 'image=@photo.png' -F 'x=10' -F 'y=10' -F 'w=120' -F 'h=40'
```

`x` and `y` are measured from the top-left corner. `w` and `h` are the box size in pixels.

### Download TikTok audio only

```sh
curl 'http://127.0.0.1:8082/download?url=<encoded-url>&format=mp3'
```

The response is `audio/mpeg`. A slideshow returns its backsound track. A post without audio returns an error.

### Download YouTube video and audio separately

```sh
curl 'http://127.0.0.1:8085/api/download?v=VIDEO_ID&format=best'
curl 'http://127.0.0.1:8085/api/download?v=VIDEO_ID&format=audio'
```

Combine them into one file outside this API:

```sh
ffmpeg -i video.webm -i audio.weba -c copy out.mp4
```

Some codec combinations need `-c:v copy -c:a aac` instead.

### Stream a byte range

```sh
curl -r 0-99 'http://127.0.0.1:8085/api/stream?v=VIDEO_ID&format=best' -o first-100.bin
```

The upstream decides whether to answer `206`.

### Restrict youtubeapi access

```sh
YT_KEY='a-long-random-string' ./youtubeapi/target/release/youtubeapi
curl 'http://127.0.0.1:8085/api/search?q=rust&key=a-long-random-string'
```

`YT_KEY` is a shared static string, not a per-user token. It keeps casual callers out. It is not an authentication system.

YouTube often gates one client profile with a bot check. The API tries Android, iOS, and Android VR first, uses the first response with both video and audio formats, and stops early so a gated client does not block playback. It also accepts progressive formats when adaptive formats are missing. The web and embedded profiles are kept only as a last resort because YouTube frequently rejects them.

## Dashboard

Run the dashboard from the repository root:

```sh
cargo build --release --manifest-path dashboard/Cargo.toml
./dashboard/target/release/dashboard
```

Open `http://<this-device-ip>:8080` in a browser. The dashboard has five pages:

- `/` shows one card per service with status, PID, uptime, endpoints, recent logs, and Start/Stop buttons.
- `/download` fetches TikTok or YouTube media directly from the APIs and saves it in the browser.
- `/watch` searches YouTube through `/api/search`, plays a selected result with `<video controls>` using `/api/stream`, and has a container and resolution selector. The default is 720p from YouTube's original stream. `/api/stream` opens the upstream without a Range header and serves byte ranges from that single response, so the browser receives the exact `Content-Length` it expects instead of a truncated file.
- `/images` uploads a PNG and applies remove-bg or remove-watermark, then shows the result.
- `/about` shows the project identity, the developer GitHub profile, repository and license links, and a live table of services with ports, status, PID, and endpoints.

The interface is built with React 18. React, React DOM, and htm are vendored under `dashboard/assets/` and embedded into the Rust binary with `include_str!`. There is no Node.js build step and no Node.js runtime dependency. Rust serves the HTML shell and the assets; the browser runs React as plain UMD scripts.

The dashboard has no authentication. Use it only on a trusted local network. It writes service output to `logs/<service>.log`.

## Configuration

| Variable | Service | Default | Meaning |
| --- | --- | --- | --- |
| `PORT` | all | see table above | Listening port |
| `HOST` | `youtubeapi` | `127.0.0.1` | Bind address |
| `YT_KEY` | `youtubeapi` | unset | Required `?key=` value when set |
| `YT_COOKIES` | `youtubeapi` | `cookies.txt` | Cookie file path |

The other services bind to `0.0.0.0`.

## Security notes

- All responses set `Access-Control-Allow-Origin: *`. Do not expose these services to untrusted networks without a reverse proxy that enforces your own policy.
- `youtubeapi` completes a TLS 1.3 handshake but does not verify certificate chains. A machine in a trusted network position could present its own certificate. Use it on networks you control.
- `YT_KEY` is a shared string. It is not a substitute for per-user authentication.
- Cookie files and credentials are gitignored. See [.gitignore](.gitignore).
- `tiktokapi` and `youtubeapi` only fetch hosts on an allowlist. `removewmapi` and `removebgapi` make no outbound requests.
- Request bodies are capped and read incrementally to avoid large allocations from a single header.

## Design decisions

**Four binaries, not one server.** Isolation is cheaper than a plugin system here, and a crash in one service leaves the others running.

**No third-party crates.** The image tools implement PNG decoding, deflate, and inflation locally. The network tools implement TLS, AES-GCM, SHA-256, and X25519 locally. Builds work without a crate cache.

**Original files and metadata.** Both APIs return the bytes, content type, and filename from the upstream source. `tiktokapi` reads the CDN `Content-Type` and `Content-Disposition`, falling back to the last segment of the CDN URL. `youtubeapi` forwards the upstream `Content-Disposition`, falling back to the last segment of the googlevideo URL. Neither API invents a filename or rewrites the media container.

**No muxed YouTube stream.** YouTube serves video and audio as separate adaptive streams. There is no single original MP4 to download. `youtubeapi` exposes the separate streams and leaves combining them to ffmpeg.

**No fake TikTok video.** A photo post is reported as `slideshow` with an `images` array. It is never presented as an MP4.

**Incremental body reads.** An earlier version allocated the full declared `Content-Length`. A caller could claim 25 MiB and send one byte. Every service now grows its buffer as bytes arrive.

## Tests

```sh
for d in removewmapi tiktokapi removebgapi youtubeapi; do
  cargo test --manifest-path "$d/Cargo.toml"
done
```

`tiktokapi` passes `cargo clippy` with no warnings. The other services still report style warnings.

## License

[MIT](LICENSE).
