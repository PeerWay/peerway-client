//! Signed HTTP requests between the desktop client and the coordination server.
//!
//! The client keeps a per-installation Ed25519 key and signs a canonical
//! description of every request. The server stores only the public key, so a
//! database leak cannot be used to impersonate clients. Signatures prove
//! possession of the installation key, integrity of the request metadata and
//! the declared application version; they are not a replacement for platform
//! attestation or code signing.

use crate::identity::verify_ed25519;
use sha2::{Digest, Sha256};

pub const SIGNING_DOMAIN: &str = "peerway-http-v1";
/// Body hash used for streaming/multipart requests. The transport (TLS)
/// protects the body; the signature still binds the method, path and metadata.
pub const STREAMED_BODY_HASH: &str = "-";

pub const HEADER_APP_ID: &str = "x-peerway-app";
pub const HEADER_VERSION: &str = "x-peerway-version";
pub const HEADER_PLATFORM: &str = "x-peerway-platform";
pub const HEADER_TIMESTAMP: &str = "x-peerway-timestamp";
pub const HEADER_NONCE: &str = "x-peerway-nonce";
pub const HEADER_BODY_SHA256: &str = "x-peerway-body-sha256";
pub const HEADER_PUBLIC_KEY: &str = "x-peerway-public-key";
pub const HEADER_SIGNATURE: &str = "x-peerway-signature";

/// Canonical request description. Field order is part of the wire format.
#[derive(Debug, Clone, Copy)]
pub struct RequestSignature<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body_sha256: &'a str,
    pub app_id: &'a str,
    pub version: &'a str,
    pub platform: &'a str,
    pub timestamp: i64,
    pub nonce: &'a str,
}

impl RequestSignature<'_> {
    pub fn canonical(&self) -> String {
        format!(
            "{SIGNING_DOMAIN}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.method.to_ascii_uppercase(),
            self.path,
            self.body_sha256,
            self.app_id,
            self.version,
            self.platform,
            self.timestamp,
            self.nonce
        )
    }

    pub fn verify(&self, public_key_hex: &str, signature_hex: &str) -> anyhow::Result<()> {
        verify_ed25519(
            public_key_hex,
            self.canonical().as_bytes(),
            signature_hex,
        )
    }
}

pub fn body_sha256(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

pub fn new_nonce() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("operating system RNG is available");
    hex::encode(bytes)
}

/// Compare dotted numeric versions. Non-numeric suffixes are ignored, so
/// `0.1.4-beta` compares as `0.1.4`. Invalid versions are treated as `0`.
pub fn version_is_older(candidate: &str, minimum: &str) -> bool {
    parse_version(candidate) < parse_version(minimum)
}

fn parse_version(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map_while(|part| part.parse::<u64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::identity::Identity;

    use super::*;

    fn signature(app_id: &str) -> RequestSignature<'_> {
        RequestSignature {
            method: "post",
            path: "/api/v1/peers/announce",
            body_sha256: "abc123",
            app_id,
            version: "0.1.4",
            platform: "windows",
            timestamp: 1_793_400_000,
            nonce: "00112233445566778899aabbccddeeff",
        }
    }

    #[test]
    fn canonical_format_is_stable() {
        let value = signature("install-1").canonical();
        assert_eq!(
            value,
            "peerway-http-v1\nPOST\n/api/v1/peers/announce\nabc123\ninstall-1\n0.1.4\nwindows\n1793400000\n00112233445566778899aabbccddeeff"
        );
    }

    #[test]
    fn signature_roundtrip_and_tampering() {
        let identity = Identity::generate().unwrap();
        let request = signature("install-1");
        let signature_hex = identity.sign(request.canonical().as_bytes());
        request
            .verify(&identity.ed25519_public_hex(), &signature_hex)
            .unwrap();

        let mut tampered = signature("install-1");
        tampered.path = "/api/v1/cloud/files";
        assert!(tampered
            .verify(&identity.ed25519_public_hex(), &signature_hex)
            .is_err());
    }

    #[test]
    fn nonce_is_random_hex() {
        let first = new_nonce();
        let second = new_nonce();
        assert_eq!(first.len(), 32);
        assert_ne!(first, second);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn version_ordering() {
        assert!(version_is_older("0.1.3", "0.1.4"));
        assert!(!version_is_older("0.1.4", "0.1.4"));
        assert!(!version_is_older("0.2.0", "0.1.9"));
        assert!(!version_is_older("v0.1.4-beta", "0.1.4"));
        assert!(version_is_older("invalid", "0.1.4"));
    }
}
