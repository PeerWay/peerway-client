//! Keeps a QUIC connection open to the coordination server using the same
//! socket that accepts peer connections. The server can therefore report the
//! exact public UDP address (NAT mapping) that other peers should punch.

use crate::api_client::ClientError;
use crate::node::Node;
use anyhow::{bail, Context, Result};
use peerway_core::net::quic_client_config;
use peerway_core::protocol::{RendezvousRequest, RendezvousResponse};
use quinn::{Connection, RecvStream, SendStream};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

pub fn spawn(node: Arc<Node>, family: u8) {
    tokio::spawn(async move {
        let mut backoff = 3u64;
        loop {
            match run_once(node.clone(), family).await {
                Ok(()) => backoff = 3,
                Err(error) => {
                    tracing::debug!(family, ?error, "rendezvous desconectado");
                    backoff = (backoff * 2).min(30);
                }
            }
            tokio::time::sleep(Duration::from_secs(backoff)).await;
        }
    });
}

async fn run_once(node: Arc<Node>, family: u8) -> Result<()> {
    let config = node.config();
    let url = Url::parse(&config.server_url).context("URL del servidor inválida")?;
    let host = url
        .host_str()
        .context("el servidor no tiene host")?
        .to_string();
    let port = config.rendezvous_port;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .context("DNS del servidor")?
        .collect();
    let addr = if family == 4 {
        addresses.iter().find(|a| a.is_ipv4()).copied()
    } else {
        addresses.iter().find(|a| a.is_ipv6()).copied()
    };
    let Some(addr) = addr else {
        bail!("el servidor no tiene dirección para IPv{family}");
    };

    let endpoint = if family == 4 {
        node.endpoint_v4.clone()
    } else {
        node.endpoint_v6
            .clone()
            .context("IPv6 local no disponible")?
    };
    let client = quic_client_config(peerway_core::ALPN_RENDEZVOUS)?;
    let connection = tokio::time::timeout(
        Duration::from_secs(6),
        endpoint.connect_with(client, addr, &host)?,
    )
    .await
    .map_err(|_| anyhow::anyhow!("conexión de rendezvous agotada"))??;
    tracing::info!(%addr, family, "rendezvous conectado");
    node.mark_server_online(true);

    loop {
        let record = node.refresh_record()?;
        let response = tokio::time::timeout(
            Duration::from_secs(12),
            exchange(&connection, &RendezvousRequest { record }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("heartbeat de rendezvous agotado"))??;
        if response.ok {
            if let Some(observed) = response.observed.and_then(|s| s.parse::<SocketAddr>().ok()) {
                let (v4, v6) = node.observed();
                if family == 4 {
                    node.set_observed(Some(observed), v6);
                } else {
                    node.set_observed(v4, Some(observed));
                }
            }
            node.mark_server_online(true);
        } else {
            let kind = response.error_kind.unwrap_or_default();
            if kind == "code_in_use" || kind == "identity_mismatch" {
                let error = ClientError {
                    status: Some(if kind == "code_in_use" { 409 } else { 403 }),
                    message: response
                        .error
                        .unwrap_or_else(|| "conflicto de código".into()),
                };
                node.handle_code_conflict(&error).await;
            } else {
                tracing::debug!(?response.error, "rendezvous rechazado");
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(15)) => {}
            _ = node.announce_notify.notified() => {}
        }
    }
}

async fn exchange(
    connection: &Connection,
    request: &RendezvousRequest,
) -> Result<RendezvousResponse> {
    let (mut send, mut recv) = connection.open_bi().await?;
    write_json(&mut send, request).await?;
    let response = read_json(&mut recv).await?;
    let _ = send.finish();
    Ok(response)
}

async fn read_json<T: serde::de::DeserializeOwned>(recv: &mut RecvStream) -> Result<T> {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len == 0 || len > 256 * 1024 {
        bail!("mensaje de rendezvous inválido");
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf)?)
}

async fn write_json<T: serde::Serialize>(send: &mut SendStream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}
