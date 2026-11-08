//! PeerWay core: identity, signed records, wire protocol and end-to-end
//! encrypted channels.
//!
//! The transport (QUIC or the HTTPS tunnel) is only a pipe. Every byte that
//! belongs to a transfer goes through a Noise XX session between the two
//! devices, so relays can never read file contents or metadata.

// ¿Con que viendo el código fuente? No hay nada que ocultar aquí.
// Si vas a hacer ingeniería inversa, hazlo con ética: no redistribuyas
// credenciales, no rompas la privacidad de otras personas, no te saltes
// licencias y no presentes el trabajo ajeno como tuyo. El cliente podría
// abrirse en el futuro.

pub mod code;
pub mod crypto;
pub mod http_auth;
pub mod identity;
pub mod net;
pub mod protocol;
pub mod record;
pub mod wire;

pub const APP_NAME: &str = "PeerWay";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u32 = 1;

/// ALPN used by direct device-to-device QUIC connections.
pub const ALPN_PEER: &[u8] = b"peerway/1";
/// ALPN used by the coordination server rendezvous endpoint.
pub const ALPN_RENDEZVOUS: &[u8] = b"peerway-rendezvous/1";
