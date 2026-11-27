## What changed

Describe the change in one or two sentences.

## Why

Link the issue or explain the problem this solves.

## Service affected

- [ ] dashboard
- [ ] removewmapi
- [ ] tiktokapi
- [ ] removebgapi
- [ ] youtubeapi
- [ ] Documentation or repository tooling

## Verification

List the commands you ran and their results.

```sh
cargo build --release --manifest-path <service>/Cargo.toml
cargo test --manifest-path <service>/Cargo.toml
curl ...
```

## Checklist

- [ ] No third-party crates were added
- [ ] No code comments were added
- [ ] `cargo build --release` passes for the affected service
- [ ] `cargo test` passes for the affected service
- [ ] Documentation in `README.md` was updated if behaviour changed
- [ ] No credentials, cookies, or runtime files are committed
