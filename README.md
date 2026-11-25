# local-restapi

```
 ██╗     ██╗██╗   ██╗███████╗████████╗███████╗██████╗ ███████╗
 ██║     ██║██║   ██║██╔════╝╚══██╔══╝██╔════╝██╔══██╗██╔════╝
 ██║     ██║██║   ██║█████╗     ██║   █████╗  ██████╔╝█████╗
 ██║     ██║██║   ██║██╔══╝     ██║   ██╔══╝  ██╔══██╗██╔══╝
 ███████╗██║╚██████╔╝███████╗   ██║   ███████╗██║  ██║███████╗
 ╚══════╝╚═╝ ╚═════╝ ╚══════╝   ╚═╝   ╚══════╝╚═╝  ╚═╝╚══════╝
 ██████╗  █████╗ ██████╗ ███████╗███████╗██╗   ██╗███████╗
 ██╔══██╗██╔══██╗██╔══██╗██╔════╝██╔════╝██║   ██║██╔════╝
 ██████╔╝███████║██████╔╝█████╗  ███████╗██║   ██║█████╗
 ██╔══██╗██╔══██╗██╔══██╗██╔══╝  ╚════██║██║   ██║██╔══╝
 ██║  ██║██║  ██║██║  ██║███████╗███████║╚██████╔╝███████╗
 ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝ ╚═════╝ ╚══════╝
```

Five small REST services written in Rust. Each service runs as its own binary on its own port and has no third-party crate dependencies.

| Service | Default port | Purpose |
| --- | --- | --- |
| removewmapi | 8081 | Remove a watermark region from a PNG |
| tiktokapi | 8082 | Read TikTok metadata and download media |
| shorturlapi | 8083 | Shorten URLs and redirect |
| removebgapi | 8084 | Remove a flat-colour background from a PNG |
| youtubeapi | 8085 | Search, read metadata, stream and download YouTube media |

All services listen on `0.0.0.0` by default. Set `PORT` to override the port.

## License

[MIT](LICENSE).

## Documentation

The docs are split by what the reader is trying to do. Each file does one job:

| Reader need | Document |
| --- | --- |
| Learning by doing | [docs/tutorial.md](docs/tutorial.md) |
| Working on a specific task | [docs/how-to.md](docs/how-to.md) |
| Looking up facts while working | [docs/reference.md](docs/reference.md) |
| Understanding why the design is this way | [docs/explanation.md](docs/explanation.md) |

The tutorial gives one fixed path from a clean checkout to a working request. The how-to page assumes competence and answers concrete tasks. The reference lists endpoints and parameters without instructing. The explanation covers trade-offs and limits. Keeping these separate prevents the reference from becoming a tutorial and the tutorial from becoming a feature list.

All documentation is written here from scratch. No documentation framework or external code was copied.
