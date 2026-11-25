# Explanation

This page explains why this repository is shaped the way it is. It does not give instructions.

## Five binaries, not one server

Each service is a separate Rust binary with a separate default port. A change to the URL shortener cannot break the image tools. A long download in youtubeapi does not occupy the TikTok port.

The cost is duplicated HTTP plumbing. Each service parses its own request line and headers. This is deliberate. A shared framework would add a dependency, and the project keeps to the Rust standard library only.

## No third-party crates

None of the five Cargo manifests declare dependencies. The image tools implement PNG decoding, deflate, and inflation locally. youtubeapi implements TLS 1.3, AES-GCM, SHA-256, and X25519 locally. tiktokapi currently calls `curl` for its HTTP transport, which is the one exception to the no-external-binary rule.

Staying on the standard library keeps builds reproducible on a phone with no crate cache. It also means the code is longer than a version built on established libraries.

## Why YouTube media is not MP4

YouTube serves video and audio as separate adaptive streams. There is no single original MP4 file to download. The format named `mp4` in youtubeapi selects a non-audio MP4 container stream, and `audio` selects an audio-only stream. Callers combine them with ffmpeg when they want one file.

Earlier versions of the service exposed a ready-made 360p muxed stream and named it as if it were the original. That was misleading. The muxed path was removed.

Similarly, TikTok photo posts have no video. The API reports `kind: slideshow` and lists `images`. Requesting `format=mp4` for those posts returns an error instead of a mislabelled audio file.

## TLS and trust

youtubeapi completes a TLS 1.3 handshake and encrypts application data with AES-128-GCM. Certificate chain verification is not implemented. A machine-in-the-middle with a trusted network position could present its own certificate and the client would accept it. The service is intended for local use where that risk is understood.

## Why request bodies are read incrementally

An early version allocated a buffer as large as the declared `Content-Length`, capped at 25 MiB. A caller could claim 25 MiB and send one byte, causing a large allocation per open connection. All four affected services now read into a 64 KiB chunk and grow the buffer only as bytes arrive. The cap still applies.

## Why control characters are rejected in short URLs

The redirect response places the original URL into a `Location` header. JSON decoding turns `\r\n` and `\u000d` into real control characters. Without validation, a caller could store a URL that injects extra headers into the redirect response. The service rejects control characters on input and rechecks them when emitting the header, so older stored entries cannot be used for the same trick.

## Documentation structure

The docs follow Diataxis. The tutorial teaches by doing, in one fixed path. The how-to page solves concrete tasks and assumes competence. The reference describes endpoints without instructing. This explanation gives background and trade-offs. Keeping these four needs separate prevents a reference page from turning into a tutorial and a tutorial from turning into a feature list.
