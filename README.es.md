# Cliente de PeerWay

[English](README.md)

Cliente de escritorio de PeerWay: transferencia directa de archivos entre
equipos con códigos, cifrado de extremo a extremo, descubrimiento en red local,
NAT traversal y relay como último recurso.

Este repositorio contiene **solo el cliente**. El servidor de coordinación, la
nube y el relay son infraestructura privada de PeerWay y no forman parte de este
código.

## Aviso

Este proyecto fue escrito íntegramente por un modelo de lenguaje (LLM). No es
el resultado de autoría humana. Si quieres hacer una versión escrita por
personas, o inspirarte en este código, puedes hacerlo bajo los términos de la
licencia.

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
