use crate::code;
use crate::identity::{verify_ed25519, Identity};
use crate::PROTOCOL_VERSION;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Public, signed description of an online peer.
///
/// Records are self-authenticating: they can travel through the coordination
/// server, QR codes or LAN UDP broadcasts and still be verified offline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub v: u32,
    pub code: String,
    pub ed25519_pub: String,
    pub x25519_pub: String,
    pub quic_port_v4: u16,
    pub quic_port_v6: u16,
    pub lan_ips: Vec<String>,
    pub device_name: String,
    pub platform: String,
    pub app_version: String,
    pub ts: i64,
    #[serde(default)]
    pub sig: String,
}

/// Field ordering is part of the wire format. Keep this stable.
#[derive(Serialize)]
struct Unsigned<'a> {
    v: u32,
    code: &'a str,
    ed25519_pub: &'a str,
    x25519_pub: &'a str,
    quic_port_v4: u16,
    quic_port_v6: u16,
    lan_ips: &'a [String],
    device_name: &'a str,
    platform: &'a str,
    app_version: &'a str,
    ts: i64,
}

impl PeerRecord {
    pub fn new(
        identity: &Identity,
        code: String,
        quic_port_v4: u16,
        quic_port_v6: u16,
        lan_ips: Vec<String>,
        device_name: String,
    ) -> Result<Self> {
        let mut record = Self {
            v: PROTOCOL_VERSION,
            code,
            ed25519_pub: identity.ed25519_public_hex(),
            x25519_pub: identity.x25519_public_hex(),
            quic_port_v4,
            quic_port_v6,
            lan_ips,
            device_name,
            platform: std::env::consts::OS.to_string(),
            app_version: crate::APP_VERSION.to_string(),
            ts: chrono::Utc::now().timestamp(),
            sig: String::new(),
        };
        record.resign(identity)?;
        Ok(record)
    }

    pub fn resign(&mut self, identity: &Identity) -> Result<()> {
        self.ts = chrono::Utc::now().timestamp();
        let bytes = self.signing_bytes()?;
        self.sig = identity.sign(&bytes);
        Ok(())
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&Unsigned {
            v: self.v,
            code: &self.code,
            ed25519_pub: &self.ed25519_pub,
            x25519_pub: &self.x25519_pub,
            quic_port_v4: self.quic_port_v4,
            quic_port_v6: self.quic_port_v6,
            lan_ips: &self.lan_ips,
            device_name: &self.device_name,
            platform: &self.platform,
            app_version: &self.app_version,
            ts: self.ts,
        })?)
    }

    /// Verify the self-signature and structural invariants.
    pub fn verify(&self) -> Result<()> {
        if self.v != PROTOCOL_VERSION {
            bail!("unsupported peer record version {}", self.v);
        }
        if !code::is_valid(&self.code) {
            bail!("invalid code in peer record");
        }
        if self.ed25519_pub.len() != 64 || self.x25519_pub.len() != 64 {
            bail!("invalid public key in peer record");
        }
        verify_ed25519(&self.ed25519_pub, &self.signing_bytes()?, &self.sig)
    }

    /// Whether the record was signed recently enough for discovery purposes.
    pub fn is_fresh(&self, max_age_secs: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        let age = now.saturating_sub(self.ts);
        age.abs() <= max_age_secs
    }

    pub fn lan_endpoints(&self) -> Vec<std::net::SocketAddr> {
        let mut out = Vec::new();
        for ip in &self.lan_ips {
            if let Ok(ip) = ip.parse::<std::net::IpAddr>() {
                let port = match ip {
                    std::net::IpAddr::V4(_) => self.quic_port_v4,
                    std::net::IpAddr::V6(_) => self.quic_port_v6,
                };
                if port != 0 {
                    out.push(std::net::SocketAddr::new(ip, port));
                }
            }
        }
        out
    }

    pub fn display_code(&self) -> String {
        code::format(&self.code)
    }
}

/// Payload broadcast over UDP on the local network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanAnnouncement {
    pub record: PeerRecord,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec() -> PeerRecord {
        let id = Identity::generate().unwrap();
        PeerRecord::new(
            &id,
            code::generate(),
            1234,
            0,
            vec!["192.168.1.7".into()],
            "Test PC".into(),
        )
        .unwrap()
    }

    #[test]
    fn record_signature_roundtrip() {
        let record = rec();
        record.verify().unwrap();
        let json = serde_json::to_string(&record).unwrap();
        let back: PeerRecord = serde_json::from_str(&json).unwrap();
        back.verify().unwrap();
    }

    #[test]
    fn rejects_tampering() {
        let mut record = rec();
        record.device_name = "Evil".into();
        assert!(record.verify().is_err());
    }
}
