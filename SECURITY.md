# Security Policy

## Supported versions

This repository is a small personal project. Only the code on the `main` branch is maintained. Older commits are not patched.

## Reporting a vulnerability

Do not open a public issue for a security bug. Use GitHub's private vulnerability reporting on this repository instead, or contact the maintainer directly through GitHub.

A useful report includes:

- The service and endpoint involved
- The exact request that triggers the problem
- The observed response or behaviour
- The impact you can demonstrate
- A suggested fix, if you have one

Please give the maintainer a reasonable window to respond before disclosing the issue publicly.

## Scope

The services in this repository are intended for local or trusted-network use. They are not hardened for direct exposure to the internet.

Known limitations are documented in the [Security notes](README.md#security-notes) section of the README. These include permissive CORS, a shared static API key on `youtubeapi`, and a TLS client in `youtubeapi` and `tiktokapi` that does not verify certificate chains. Reports that rely on placing these services on an untrusted public network are likely to be closed as working as designed unless they show a flaw beyond those limitations.

## Out of scope

- Findings that require the operator to expose a service publicly against the guidance above
- Missing browser security headers on JSON API responses
- Denial of service through sustained connection flooding on a local service
- Issues in third-party endpoints these services fetch, such as YouTube, TikTok, or tikwm.com
