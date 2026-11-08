# PeerWay client

Desktop client for PeerWay: direct peer-to-peer file transfer with codes,
end-to-end encryption, LAN discovery, NAT traversal and relay as a last resort.

This repository contains **only the client**. The coordination server, cloud and
relay are PeerWay's private infrastructure and are not part of this code.

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

---

# Cliente de PeerWay

Cliente de escritorio de PeerWay: transferencia directa de archivos entre
equipos con códigos, cifrado de extremo a extremo, descubrimiento en red local,
NAT traversal y relay como último recurso.

Este repositorio contiene **solo el cliente**. El servidor de coordinación, la
nube y el relay son infraestructura privada de PeerWay y no forman parte de este
código.

## Estructura

- `crates/core`: protocolo, códigos, identidad y criptografía.
- `crates/node`: motor P2P, API local y enlaces web temporales.
- `crates/desktop`: aplicación Tauri para Windows, Linux y macOS.
- `web`: interfaz del cliente.

## Compilar

Requisitos: Rust 1.85+, Node 22+ y las dependencias de Tauri para tu sistema.

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

Los instaladores quedan en `dist/` (Windows) o en
`target/release/bundle/` (Linux/macOS).

## Licencia

Apache License 2.0. Consulta `LICENSE` y `NOTICE`.

El nombre PeerWay, su logotipo y su identidad visual son marcas de PeerWay y no
están cubiertos por la licencia Apache. Un fork puede usar el código, pero no
presentarse como la aplicación oficial.

## Enlaces

- Web: https://www.peerway.me
- Descargas: https://www.peerway.me/download
