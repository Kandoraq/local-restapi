# How-to guides

Each guide solves one task. They assume you already built the binaries as shown in the [tutorial](tutorial.md).

## How to run a service on a different port

Set `PORT` before starting the binary.

```sh
PORT=9091 ./removewmapi/target/release/removewmapi
```

Every service reads `PORT`. youtubeapi also reads `HOST`, which defaults to `127.0.0.1`. The other services bind to `0.0.0.0`.

## How to remove a background that is not white

removebgapi defaults to `threshold=32` and samples the border colour. For a known solid colour, pass it as hex.

```sh
curl -X POST http://127.0.0.1:8084/remove-bg \
  -F 'image=@photo.png' \
  -F 'color=00ff00'
```

Raise `threshold` when the background has slight colour variation.

## How to remove a known watermark box

Pass a bounding box instead of relying on detection.

```sh
curl -X POST http://127.0.0.1:8081/remove-watermark \
  -F 'image=@photo.png' \
  -F 'x=10' -F 'y=10' -F 'w=120' -F 'h=40'
```

`x` and `y` are pixels from the top-left corner. `w` and `h` are the box size.

## How to download TikTok audio only

```sh
curl 'http://127.0.0.1:8082/download?url=<encoded-tiktok-url>&format=mp3'
```

The response is `audio/mpeg`. If the post is a slideshow, this returns its backsound track. If the post has no audio, the service returns an error.

## How to tell a TikTok video from a photo post

Call `/tiktok` and read the `kind` field:

- `video`: `hdplay` and `play` point to video streams.
- `slideshow`: `images` lists the photos and `hdplay` points to audio only.

Do not request `format=mp4` for a `slideshow`. It returns an error by design.

## How to download YouTube video and audio separately

```sh
curl 'http://127.0.0.1:8085/api/download?v=VIDEO_ID&format=best'
curl 'http://127.0.0.1:8085/api/download?v=VIDEO_ID&format=audio'
```

The first response is a video-only `.webm`. The second is audio-only. Combine them with ffmpeg if you need a single MP4:

```sh
ffmpeg -i video.webm -i audio.weba -c copy out.mp4
```

Copying streams avoids re-encoding. Some codec combinations need `-c:v copy -c:a aac` instead.

## How to stream a byte range from YouTube

The `/api/stream` endpoint forwards `Range` requests.

```sh
curl -r 0-99 'http://127.0.0.1:8085/api/stream?v=VIDEO_ID&format=best' -o first-100.bin
```

The upstream decides whether to answer `206`. YouTube video hosts usually do.

## How to disable unauthenticated access to youtubeapi

Set `YT_KEY`. Callers must then pass `?key=...` on every `/api/` request.

```sh
YT_KEY='a-long-random-string' ./youtubeapi/target/release/youtubeapi
curl 'http://127.0.0.1:8085/api/search?q=rust&key=a-long-random-string'
```

`YT_KEY` is a shared static string, not a per-user token. It keeps casual callers out. It is not an authentication system.
