# PeerWay client

[Español](README.es.md)

Desktop client for PeerWay: direct peer-to-peer file transfer with codes,
end-to-end encryption, LAN discovery, NAT traversal and relay as a last resort.

This repository contains **only the client**. The coordination server, cloud and
relay are PeerWay's private infrastructure and are not part of this code.

## Disclaimer

This project was written entirely by a large language model (LLM). It is not
the result of human authorship. If you want to build a human-written version,
or use this code as inspiration, you are welcome to do so under the license.

## Structure

- `crates/core`: protocol, codes, identity and cryptography.
- `crates/node`: P2P engine, local API and temporary web links.
- `crates/desktop`: Tauri app for Windows, Linux and macOS.
- `web`: client interface.

## Build

Requirements: Rust 1.85+, Node 22+ and the Tauri dependencies for your system.

```powershell
# Windows
.\scripts\set-version.ps1 -Version 0.1.23
.\scripts\package-desktop.ps1 -Version 0.1.23
```

```bash
# Linux / macOS
cd crates/desktop
npm ci
npm run tauri build
```

Installers end up in `dist/` (Windows) or `target/release/bundle/`
(Linux/macOS).

## License

Apache License 2.0. See `LICENSE` and `NOTICE`.

The PeerWay name, logo and visual identity are PeerWay trademarks and are not
covered by the Apache License. A fork may use the code, but may not present
itself as the official application.

## Links

- Website: https://www.peerway.me
- Downloads: https://www.peerway.me/download
