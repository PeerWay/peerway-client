//! Signaling WebSocket client and NAT punch handling.

use crate::node::Node;
use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use peerway_core::crypto::SecureChannel;
use peerway_core::http_auth::body_sha256;
use peerway_core::protocol::{Control, Frame, SignalMessage};
use peerway_core::record::PeerRecord;
use peerway_core::wire::Channel;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, connect_async_tls_with_config, Connector};
use url::Url;

pub fn spawn(node: Arc<Node>) {
    tokio::spawn(async move {
        let mut backoff = 2u64;
        loop {
            let result = run_once(node.clone()).await;
            if let Err(error) = result {
                tracing::debug!(?error, "señalización desconectada");
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
                _ = node.signal_notify.notified() => {}
            }
            backoff = (backoff * 2).min(20);
        }
    });
}

async fn run_once(node: Arc<Node>) -> Result<()> {
    let code = node.code();
    let ts = chrono::Utc::now().timestamp();
    let signature = node.identity.sign(format!("ws:{code}:{ts}").as_bytes());
    let config = node.config();
    let url = signal_url(&config.server_url, &code, ts, &signature)?;
    let mut request = url.as_str().into_client_request()?;
    node.http.sign_headers(
        "GET",
        "/api/v1/signal",
        &body_sha256(b""),
        request.headers_mut(),
    );
    let (mut ws, response) = if config.insecure_tls {
        let tls = peerway_core::net::insecure_tls_client_config()?;
        connect_async_tls_with_config(request, None, false, Some(Connector::Rustls(tls)))
            .await
            .context("señalización WSS")?
    } else {
        connect_async(request).await.context("señalización WSS")?
    };
    tracing::info!("señalización conectada ({})", response.status());
    node.mark_server_online(true);

    loop {
        tokio::select! {
            _ = node.signal_notify.notified() => {
                let _ = ws.close(None).await;
                return Ok(());
            }
            message = ws.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<SignalMessage>(&text) {
                            Ok(SignalMessage::Punch { from, record, endpoints }) => {
                                if from != record.code {
                                    continue;
                                }
                                let node = node.clone();
                                tokio::spawn(async move {
                                    handle_punch(node, record, endpoints).await;
                                });
                            }
                            Ok(SignalMessage::TunnelIncoming { tunnel_id, token, from, transfer_id }) => {
                                let node = node.clone();
                                tokio::spawn(async move {
                                    if let Err(error) = receive_relay(node.clone(), tunnel_id, token, from, transfer_id).await {
                                        tracing::debug!(?error, "recepción por relay falló");
                                    }
                                });
                            }
                            Ok(SignalMessage::Ping) => {
                                let _ = ws.send(Message::Text("{\"type\":\"ping\"}".into())).await;
                            }
                            Ok(SignalMessage::PeerOffline { code }) => {
                                tracing::debug!(%code, "peer fuera de línea");
                            }
                            Err(error) => tracing::debug!(?error, "mensaje de señalización inválido"),
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = ws.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => return Ok(()),
                    Some(Ok(_)) => {}
                    Some(Err(error)) => bail!("señalización: {error}"),
                }
            }
        }
    }
}

fn signal_url(server_url: &str, code: &str, ts: i64, sig: &str) -> Result<Url> {
    let mut url = Url::parse(server_url).context("URL del servidor inválida")?;
    let scheme = match url.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        other => bail!("esquema no soportado: {other}"),
    };
    url.set_scheme(scheme)
        .map_err(|_| anyhow::anyhow!("esquema inválido"))?;
    url.set_path("/api/v1/signal");
    url.set_query(Some(&format!("code={code}&ts={ts}&sig={sig}")));
    Ok(url)
}

async fn handle_punch(node: Arc<Node>, record: PeerRecord, endpoints: Vec<String>) {
    if record.verify().is_err() || record.code == node.code() {
        return;
    }
    let mut tasks = Vec::new();
    for endpoint in endpoints.into_iter().take(8) {
        let Ok(addr) = endpoint.parse::<std::net::SocketAddr>() else {
            continue;
        };
        let endpoint = if addr.is_ipv4() {
            Some(node.endpoint_v4.clone())
        } else {
            node.endpoint_v6.clone()
        };
        let Some(endpoint) = endpoint else {
            continue;
        };
        tasks.push(tokio::spawn(async move {
            let result = tokio::time::timeout(Duration::from_secs(3), async {
                let connecting = endpoint.connect(addr, "peerway")?;
                Ok::<_, anyhow::Error>(connecting.await?)
            })
            .await;
            if let Ok(Ok(connection)) = result {
                // Keep the connection (and therefore the NAT mapping) alive
                // while the other side dials in.
                tokio::time::sleep(Duration::from_secs(10)).await;
                drop(connection);
            }
        }));
    }
    for task in tasks {
        let _ = task.await;
    }
}

async fn receive_relay(
    node: Arc<Node>,
    tunnel_id: String,
    token: String,
    from: String,
    transfer_id: String,
) -> Result<()> {
    let stream = crate::relay::connect(&node, &tunnel_id, &token).await?;
    let (mut secure, remote_static) = tokio::time::timeout(
        Duration::from_secs(20),
        SecureChannel::responder(
            Channel::Duplex(stream),
            &node.identity.x25519_secret_bytes(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("handshake por relay agotado"))??;

    let frame = tokio::time::timeout(Duration::from_secs(60), secure.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no llegó oferta por relay"))??;
    let Frame::Control(Control::Offer {
        transfer_id: offered_id,
        sender_record,
        files,
        total_bytes,
        comment,
    }) = frame
    else {
        bail!("se esperaba una oferta por relay");
    };
    let offer = peerway_core::protocol::Offer {
        transfer_id: offered_id,
        sender_record: *sender_record,
        files,
        total_bytes,
        created_at: chrono::Utc::now().timestamp(),
        comment,
    };
    node.handle_offer(
        &mut secure,
        &remote_static,
        offer,
        crate::transfer::Method::Relay,
        Some((&transfer_id, &from)),
    )
    .await?;
    secure.close().await;
    Ok(())
}
