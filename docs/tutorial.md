# Tutorial

This tutorial takes you from a clean checkout to one successful request against each service. You need a Rust toolchain and `curl`. The services read and write only local files unless a request asks them to fetch a URL.

Build all five binaries first.

```sh
for d in removewmapi tiktokapi shorturlapi removebgapi youtubeapi; do
  cargo build --release --manifest-path "$d/Cargo.toml"
done
```

## 1. Check that a service is alive

Start removewmapi in one terminal.

```sh
PORT=8081 ./removewmapi/target/release/removewmapi
```

In a second terminal, ask for its health response.

```sh
curl http://127.0.0.1:8081/health
```

You should see `{"status":"ok","service":"..."}`. Stop the service with Ctrl-C when you are done.

The same request works on every service, with ports 8081 through 8085.

## 2. Remove a background

Start removebgapi on port 8084. Send a PNG with a flat background as multipart form data. The field name is `image`.

```sh
curl -X POST http://127.0.0.1:8084/remove-bg \
  -F 'image=@/sdcard/Download/photo.png' \
  -o /sdcard/Download/photo-nobg.png
```

The response is a PNG with transparent pixels where the background was. If the background is not close to white, set `threshold` or pass a hex `color`. See the [how-to guide](how-to.md) for those cases.

## 3. Remove a watermark

Start removewmapi on port 8081. Send a PNG and describe the watermark box with `x`, `y`, `w`, and `h`.

```sh
curl -X POST http://127.0.0.1:8081/remove-watermark \
  -F 'image=@/sdcard/Download/photo.png' \
  -o /sdcard/Download/photo-clean.png
```

Without a box, the service tries to detect the watermark region. Detection is heuristic. For predictable results, pass a box.

## 4. Shorten a URL

Start shorturlapi on port 8083.

```sh
curl -X POST http://127.0.0.1:8083/shorten \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/a/long/path/that/is/at/least/thirty/chars"}'
```

The response contains `code` and `short_url`. Open the `short_url` and you will receive a `302` with `Location` set to the original URL. Mappings are saved to `shorturlapi/shorturls.json`.

## 5. Read TikTok metadata

Start tiktokapi on port 8082.

```sh
curl 'http://127.0.0.1:8082/tiktok?url=https%3A%2F%2Fwww.tiktok.com%2F%40tiktok%2Fvideo%2F7424648048074181931'
```

The response includes `kind`. A video post reports `video`. A photo post reports `slideshow` and includes an `images` array. For a slideshow, the service does not pretend an MP4 exists.

To download the media, call `/download` with `format=mp4` or `format=mp3`.

## 6. Read YouTube metadata

Start youtubeapi on port 8085.

```sh
curl 'http://127.0.0.1:8085/api/info?v=dQw4w9WgXcQ'
```

The `formats` list contains separate video-only and audio-only streams. `format=best` on `/api/stream` selects the highest-resolution video-only stream. The service does not mux video and audio into MP4. That step belongs to ffmpeg, outside this API.

You have now made one real request to each service. The [reference](reference.md) lists every endpoint and parameter.
