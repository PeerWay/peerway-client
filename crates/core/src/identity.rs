use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(s).context("invalid hex key")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("key must be 32 bytes"))
}

/// A device identity. Ed25519 is used to sign peer records; X25519 is the
/// static key of the Noise XX handshake.
#[derive(Serialize, Deserialize, Clone)]
pub struct IdentityFile {
    pub ed25519_secret: String,
    pub x25519_secret: String,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct Identity {
    ed25519_secret: [u8; 32],
    x25519_secret: [u8; 32],
}

impl Identity {
    pub fn generate() -> Result<Self> {
        let mut ed = [0u8; 32];
        let mut x = [0u8; 32];
        getrandom::fill(&mut ed).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
        getrandom::fill(&mut x).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
        Ok(Self {
            ed25519_secret: ed,
            x25519_secret: x,
        })
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let raw = std::fs::read_to_string(path)?;
            let file: IdentityFile = serde_json::from_str(&raw)?;
            Ok(Self {
                ed25519_secret: hex32(&file.ed25519_secret)?,
                x25519_secret: hex32(&file.x25519_secret)?,
            })
        } else {
            let identity = Self::generate()?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = IdentityFile {
                ed25519_secret: hex::encode(identity.ed25519_secret),
                x25519_secret: hex::encode(identity.x25519_secret),
                created_at: chrono::Utc::now().timestamp(),
            };
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(&file)?)?;
            restrict_permissions(&tmp)?;
            std::fs::rename(&tmp, path)?;
            Ok(identity)
        }
    }

    pub fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.ed25519_secret)
    }

    pub fn ed25519_public(&self) -> [u8; 32] {
        self.signing_key().verifying_key().to_bytes()
    }

    pub fn ed25519_public_hex(&self) -> String {
        hex::encode(self.ed25519_public())
    }

    pub fn x25519_public(&self) -> [u8; 32] {
        x25519_dalek::PublicKey::from(&self.x25519_secret()).to_bytes()
    }

    pub fn x25519_public_hex(&self) -> String {
        hex::encode(self.x25519_public())
    }

    pub fn x25519_secret(&self) -> x25519_dalek::StaticSecret {
        x25519_dalek::StaticSecret::from(self.x25519_secret)
    }

    pub fn x25519_secret_bytes(&self) -> [u8; 32] {
        self.x25519_secret
    }

    pub fn sign(&self, message: &[u8]) -> String {
        let sig = self.signing_key().sign(message);
        hex::encode(sig.to_bytes())
    }
}

pub fn verify_ed25519(public_hex: &str, message: &[u8], sig_hex: &str) -> Result<()> {
    let key = hex32(public_hex)?;
    let sig = hex::decode(sig_hex).context("invalid signature hex")?;
    let sig = Signature::from_slice(&sig).map_err(|e| anyhow::anyhow!("invalid signature: {e}"))?;
    let verifying =
        VerifyingKey::from_bytes(&key).map_err(|e| anyhow::anyhow!("invalid key: {e}"))?;
    verifying
        .verify(message, &sig)
        .map_err(|_| anyhow::anyhow!("signature verification failed"))
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let id = Identity::generate().unwrap();
        let sig = id.sign(b"hello");
        verify_ed25519(&id.ed25519_public_hex(), b"hello", &sig).unwrap();
        assert!(verify_ed25519(&id.ed25519_public_hex(), b"tampered", &sig).is_err());
    }

    #[test]
    fn persists_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        let first = Identity::load_or_create(&path).unwrap();
        let second = Identity::load_or_create(&path).unwrap();
        assert_eq!(first.ed25519_public_hex(), second.ed25519_public_hex());
        assert_eq!(first.x25519_public_hex(), second.x25519_public_hex());
    }
}
