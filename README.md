# local-restapi

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

The docs follow [Diataxis](https://diataxis.fr/):

- [Tutorial](docs/tutorial.md): start from a clean checkout and make a real request to each service.
- [How-to guides](docs/how-to.md): targeted recipes for common tasks.
- [Reference](docs/reference.md): endpoints, parameters, and response shapes.
- [Explanation](docs/explanation.md): why the services are structured this way and what the limits are.
