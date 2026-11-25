# Reference

This page lists facts about each service. It does not give instructions. For instructions, see the [how-to guides](how-to.md).

All responses include `Access-Control-Allow-Origin: *`. Every service closes the connection after one response.

## removewmapi

Default port: `8081`. Environment: `PORT`.

### `GET /` and `GET /health`

Returns a JSON status object.

### `POST /remove-watermark`

Aliases: `/remove`, `/api/remove`.

Request body: `multipart/form-data` with field `image` containing a PNG.

Query parameters:

| Name | Type | Default | Meaning |
| --- | --- | --- | --- |
| `x` | u32 | detected | Left edge of the watermark box in pixels |
| `y` | u32 | detected | Top edge of the watermark box in pixels |
| `w` | u32 | detected | Box width |
| `h` | u32 | detected | Box height |
| `threshold` | u8 | 200 | Brightness cutoff used during detection |
| `radius` | i32 | 3 | Inpainting radius |
| `iters` | usize | 4 | Inpainting iterations |

Response `200` with `Content-Type: image/png` and the processed PNG as body. Response `400` when no image is present. Response `422` when the PNG cannot be decoded.

The request body limit is 25 MiB.

## removebgapi

Default port: `8084`. Environment: `PORT`.

### `GET /` and `GET /health`

Returns a JSON status object.

### `POST /remove-bg`

Aliases: `/remove`, `/remove-background`, `/api/remove-bg`.

Request body: `multipart/form-data` with field `image` containing a PNG.

Query parameters:

| Name | Type | Default | Meaning |
| --- | --- | --- | --- |
| `threshold` | u32 | 32 | Colour distance from the border colour |
| `color` | hex string | border sample | Explicit background colour as `RRGGBB` |

Response `200` with `Content-Type: image/png`. Transparent pixels replace the background. Response `400` when no image is present. Response `422` on decode failure.

The request body limit is 25 MiB.

## shorturlapi

Default port: `8083`. Environment: `PORT`, `BASE_URL`.

State file: `shorturls.json` in the service working directory.

### `GET /` and `GET /health`

Returns a JSON status object.

### `POST /shorten`

Alias: `/api/shorten`.

Body may be JSON or form-encoded.

| Field | Meaning |
| --- | --- |
| `url` | Original URL. Must start with `http://` or `https://` and be at least 30 characters |
| `base_url` | Optional prefix used in `short_url` |

Response `200` with `code`, `short_url`, and `original_url`. Response `400` for a missing, short, already-short, or malformed URL.

URLs that contain control characters are rejected. This prevents header injection when the short URL is followed.

### `GET /list`

Returns `{"urls":{code:original_url,...}}`.

### `GET /{code}`

Response `302` with `Location` set to the original URL. Response `404` for an unknown code.

The request body limit is 64 KiB.

## tiktokapi

Default port: `8082`. Environment: `PORT`.

### `GET /` and `GET /health`

Returns a JSON status object.

### `GET /tiktok` and `POST /tiktok`

Alias: `/api/tiktok`.

| Parameter | Meaning |
| --- | --- |
| `url` | A TikTok URL with `http://` or `https://` |

For POST, `url` may be a JSON field, a form field, or a query parameter.

Response `200` is a JSON object. Fields include `id`, `title`, `kind`, `images`, `cover`, `origin_cover`, `wmplay`, `hdplay`, `play`, `music`, `play_count`, `digg_count`, `comment_count`, `share_count`, `author`, and `duration`. The `raw` field contains the upstream response.

`kind` is `video` or `slideshow`. A `slideshow` has a non-empty `images` array.

### `GET /download`, `/media`, `/file`

| Parameter | Default | Meaning |
| --- | --- | --- |
| `url` | required | TikTok URL |
| `format` | `mp4` | `mp4` or `mp3` |
| `quality` | `hd` | `hd`, `low`, or `sd` |

Response `200` with the media bytes and a `Content-Disposition` filename. Response `502` when the upstream fails or when a slideshow is requested as `mp4`.

Media hosts are restricted to known TikTok CDN domains.

The request body limit is 1 MiB.

## youtubeapi

Default port: `8085`. Environment: `PORT`, `HOST`, `YT_KEY`.

### Endpoints

| Path | Parameters | Result |
| --- | --- | --- |
| `/` | none | Service description and endpoint list |
| `/health` | none | Status object |
| `/api/search` | `q` | Search results |
| `/api/info` | `v`, `id`, or `url` | Video metadata and formats |
| `/api/formats` | `v`, `id`, or `url` | Format list |
| `/api/stream` | `v`, `format`, `key` | Proxied media stream, supports `Range` |
| `/api/download` | `v`, `format`, `key` | Same stream with `Content-Disposition` |
| `/api/proxy` | `url`, `key` | Proxied upstream media URL |

When `YT_KEY` is set, every `/api/` request must include `key`.

Format selectors: `best`, `worst`, `audio`, `audioonly`, `video`, `videoonly`, `m4a`, `webm`, `mp4`, `opus`, `aac`, `1080`, `720`, `480`, `360`, or a numeric itag.

`best` selects the highest-resolution video-only format. The service does not return muxed video and audio.

`/api/proxy` accepts only HTTPS URLs on `googlevideo.com`, `youtube.com`, `ytimg.com`, `googleusercontent.com`, and `ggpht.com`, including their subdomains. Everything else is rejected with `403`.

The service uses its own TLS 1.3 implementation. It does not verify certificate chains.
