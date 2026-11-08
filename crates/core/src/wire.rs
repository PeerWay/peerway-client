use crate::protocol::MAX_FRAME_BYTES;
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A bidirectional byte pipe: either a QUIC stream or the local half of the
/// HTTPS relay tunnel.
pub enum Channel {
    Quic {
        send: quinn::SendStream,
        recv: quinn::RecvStream,
    },
    Duplex(tokio::io::DuplexStream),
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Channel::Quic { .. } => f.write_str("Channel::Quic"),
            Channel::Duplex(_) => f.write_str("Channel::Duplex"),
        }
    }
}

impl Channel {
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        match self {
            Channel::Quic { send, .. } => send.write_all(buf).await.context("quic write"),
            Channel::Duplex(stream) => stream.write_all(buf).await.context("tunnel write"),
        }
    }

    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        match self {
            Channel::Quic { recv, .. } => recv.read_exact(buf).await.context("quic read"),
            Channel::Duplex(stream) => stream
                .read_exact(buf)
                .await
                .map(|_| ())
                .context("tunnel read"),
        }
    }

    pub async fn close(&mut self) {
        match self {
            Channel::Quic { send, recv } => {
                let _ = send.finish();
                let _ = recv.stop(0u32.into());
            }
            Channel::Duplex(stream) => {
                let _ = stream.shutdown().await;
            }
        }
    }

    /// Write a length-prefixed, plaintext frame. Used only during the Noise
    /// handshake.
    pub async fn write_raw_frame(&mut self, payload: &[u8]) -> Result<()> {
        if payload.len() > u16::MAX as usize {
            bail!("frame too large");
        }
        let len = (payload.len() as u16).to_be_bytes();
        self.write_all(&len).await?;
        self.write_all(payload).await?;
        Ok(())
    }

    pub async fn read_raw_frame(&mut self) -> Result<Vec<u8>> {
        let mut len = [0u8; 2];
        self.read_exact(&mut len).await?;
        let len = u16::from_be_bytes(len) as usize;
        if len == 0 {
            bail!("empty frame");
        }
        let mut payload = vec![0u8; len];
        self.read_exact(&mut payload).await?;
        Ok(payload)
    }

    /// Send a Noise transport message.
    pub async fn write_encrypted_frame(&mut self, ciphertext: &[u8]) -> Result<()> {
        if ciphertext.len() > u16::MAX as usize {
            bail!("encrypted frame too large");
        }
        let len = (ciphertext.len() as u16).to_be_bytes();
        self.write_all(&len).await?;
        self.write_all(ciphertext).await?;
        Ok(())
    }

    pub async fn read_encrypted_frame(&mut self) -> Result<Vec<u8>> {
        let mut len = [0u8; 2];
        self.read_exact(&mut len).await?;
        let len = u16::from_be_bytes(len) as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            bail!("invalid encrypted frame length {len}");
        }
        let mut payload = vec![0u8; len];
        self.read_exact(&mut payload).await?;
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn raw_frames_roundtrip() {
        let (a, b) = tokio::io::duplex(1024);
        let mut a = Channel::Duplex(a);
        let mut b = Channel::Duplex(b);
        let writer = tokio::spawn(async move {
            a.write_raw_frame(b"hello").await.unwrap();
        });
        assert_eq!(b.read_raw_frame().await.unwrap(), b"hello");
        writer.await.unwrap();
    }
}
