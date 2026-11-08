use crate::protocol::{Frame, MAX_CHUNK_BYTES};
use crate::wire::Channel;
use anyhow::{bail, Context, Result};
use snow::{Builder, TransportState};

/// Noise XX: mutual authentication, forward secrecy and no dependence on TLS
/// certificate authorities. The transport (QUIC or WSS relay) does not learn
/// the static public keys or the payload.
const PARAMS: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
const MAX_NOISE_MESSAGE: usize = 65_535;

pub struct SecureChannel {
    channel: Channel,
    transport: TransportState,
}

impl SecureChannel {
    /// Initiator side of the handshake. Returns the channel and the peer's
    /// static X25519 public key (to be matched against a signed record).
    pub async fn initiator(
        mut channel: Channel,
        local_static: &[u8; 32],
    ) -> Result<(Self, [u8; 32])> {
        let params = PARAMS
            .parse()
            .map_err(|e| anyhow::anyhow!("noise params: {e}"))?;
        let mut noise = Builder::new(params)
            .local_private_key(local_static)
            .context("noise local key")?
            .build_initiator()
            .context("noise initiator")?;

        let mut buf = vec![0u8; MAX_NOISE_MESSAGE];
        let len = noise.write_message(&[], &mut buf).context("noise msg1")?;
        channel.write_raw_frame(&buf[..len]).await?;

        let msg = channel.read_raw_frame().await?;
        noise.read_message(&msg, &mut buf).context("noise msg2")?;

        let len = noise.write_message(&[], &mut buf).context("noise msg3")?;
        channel.write_raw_frame(&buf[..len]).await?;

        finish(noise, channel)
    }

    /// Responder side of the handshake.
    pub async fn responder(
        mut channel: Channel,
        local_static: &[u8; 32],
    ) -> Result<(Self, [u8; 32])> {
        let params = PARAMS
            .parse()
            .map_err(|e| anyhow::anyhow!("noise params: {e}"))?;
        let mut noise = Builder::new(params)
            .local_private_key(local_static)
            .context("noise local key")?
            .build_responder()
            .context("noise responder")?;

        let mut buf = vec![0u8; MAX_NOISE_MESSAGE];
        let msg = channel.read_raw_frame().await?;
        noise.read_message(&msg, &mut buf).context("noise msg1")?;

        let len = noise.write_message(&[], &mut buf).context("noise msg2")?;
        channel.write_raw_frame(&buf[..len]).await?;

        let msg = channel.read_raw_frame().await?;
        noise.read_message(&msg, &mut buf).context("noise msg3")?;

        finish(noise, channel)
    }

    pub async fn send(&mut self, frame: &Frame) -> Result<()> {
        let plaintext = frame.encode()?;
        if plaintext.len() > MAX_NOISE_MESSAGE - 16 {
            bail!("frame exceeds noise limit");
        }
        let mut out = vec![0u8; MAX_NOISE_MESSAGE];
        let len = self
            .transport
            .write_message(&plaintext, &mut out)
            .context("noise encrypt")?;
        self.channel.write_encrypted_frame(&out[..len]).await
    }

    pub async fn recv(&mut self) -> Result<Frame> {
        let ciphertext = self.channel.read_encrypted_frame().await?;
        let mut out = vec![0u8; MAX_NOISE_MESSAGE];
        let len = self
            .transport
            .read_message(&ciphertext, &mut out)
            .context("noise decrypt")?;
        Frame::decode(&out[..len])
    }

    pub async fn send_data(&mut self, file_index: u32, data: &[u8]) -> Result<()> {
        if data.len() > MAX_CHUNK_BYTES {
            bail!("chunk too large");
        }
        self.send(&Frame::Data {
            file_index,
            data: data.to_vec(),
        })
        .await
    }

    pub async fn close(&mut self) {
        self.channel.close().await;
    }
}

fn finish(noise: snow::HandshakeState, channel: Channel) -> Result<(SecureChannel, [u8; 32])> {
    let remote = noise
        .get_remote_static()
        .context("noise peer static key missing")?;
    if remote.len() != 32 {
        bail!("unexpected peer static key length");
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(remote);
    let transport = noise.into_transport_mode().context("noise transport")?;
    Ok((SecureChannel { channel, transport }, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Control;

    #[tokio::test]
    async fn handshake_and_frame_exchange() {
        let (a, b) = tokio::io::duplex(128 * 1024);
        let ka = [7u8; 32];
        let kb = [9u8; 32];
        let responder = tokio::spawn(async move {
            let (mut secure, remote) = SecureChannel::responder(Channel::Duplex(b), &kb)
                .await
                .unwrap();
            assert_eq!(
                remote,
                x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(ka)).to_bytes()
            );
            let frame = secure.recv().await.unwrap();
            match frame {
                Frame::Control(Control::Ping) => {}
                _ => panic!("unexpected frame"),
            }
            secure
                .send(&Frame::Control(Control::Accept {
                    transfer_id: "t1".into(),
                }))
                .await
                .unwrap();
        });

        let (mut secure, remote) = SecureChannel::initiator(Channel::Duplex(a), &ka)
            .await
            .unwrap();
        assert_eq!(
            remote,
            x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(kb)).to_bytes()
        );
        secure.send(&Frame::Control(Control::Ping)).await.unwrap();
        match secure.recv().await.unwrap() {
            Frame::Control(Control::Accept { transfer_id }) => assert_eq!(transfer_id, "t1"),
            _ => panic!("unexpected frame"),
        }
        responder.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_tampered_ciphertext() {
        let (a, b) = tokio::io::duplex(128 * 1024);
        let ka = [1u8; 32];
        let kb = [2u8; 32];
        let responder = tokio::spawn(async move {
            let (mut secure, _) = SecureChannel::responder(Channel::Duplex(b), &kb)
                .await
                .unwrap();
            // Read raw ciphertext then replace it with garbage inside the channel.
            let mut raw = secure.channel.read_encrypted_frame().await.unwrap();
            raw[0] ^= 0xff;
            secure.channel.write_encrypted_frame(&raw).await.unwrap();
        });
        let (mut secure, _) = SecureChannel::initiator(Channel::Duplex(a), &ka)
            .await
            .unwrap();
        secure.send(&Frame::Control(Control::Ping)).await.unwrap();
        assert!(secure.recv().await.is_err());
        responder.await.unwrap();
    }
}
