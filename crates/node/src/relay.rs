use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, connect_async_tls_with_config, Connector};
use url::Url;

use crate::node::Node;
use peerway_core::http_auth::body_sha256;

/// Connect to the server's HTTPS fallback tunnel and return a duplex byte
/// stream that speaks the same framed protocol as a direct QUIC stream.
pub async fn connect(node: &Node, tunnel_id: &str, token: &str) -> Result<DuplexStream> {
    let config = node.config();
    let url = tunnel_url(&config.server_url, tunnel_id, token)?;
    let mut request = url
        .as_str()
        .into_client_request()
        .context("websocket request")?;
    node.http.sign_headers(
        "GET",
        &format!("/api/v1/tunnel/{tunnel_id}/ws"),
        &body_sha256(b""),
        request.headers_mut(),
    );
    let (mut ws, _) = if config.insecure_tls {
        let tls = peerway_core::net::insecure_tls_client_config()?;
        connect_async_tls_with_config(request, None, false, Some(Connector::Rustls(tls)))
            .await
            .context("conexión al relay WSS")?
    } else {
        connect_async(request)
            .await
            .context("conexión al relay WSS")?
    };

    // The server sends "ready" once both peers are paired.
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(20), ws.next())
            .await
            .map_err(|_| anyhow::anyhow!("el relay no emparejó la conexión"))?;
        match message {
            Some(Ok(Message::Text(text))) if text.as_str() == "ready" => break,
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) | None => bail!("el relay cerró la conexión"),
            Some(Ok(_)) => continue,
            Some(Err(error)) => bail!("relay: {error}"),
        }
    }

    let (mut sink, mut stream) = ws.split();
    let (app, tunnel) = tokio::io::duplex(256 * 1024);
    let (mut tunnel_read, mut tunnel_write) = tokio::io::split(tunnel);

    // WebSocket -> local byte stream.
    tokio::spawn(async move {
        while let Some(message) = stream.next().await {
            match message {
                Ok(Message::Binary(data)) => {
                    if tunnel_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
        let _ = tunnel_write.shutdown().await;
    });

    // Local byte stream -> WebSocket.
    tokio::spawn(async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match tunnel_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if sink
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sink.close().await;
    });

    Ok(app)
}

pub fn tunnel_url(server_url: &str, tunnel_id: &str, token: &str) -> Result<Url> {
    let mut url = Url::parse(server_url).context("URL del servidor inválida")?;
    let scheme = match url.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        other => bail!("esquema no soportado: {other}"),
    };
    url.set_scheme(scheme)
        .map_err(|_| anyhow::anyhow!("esquema inválido"))?;
    url.set_path(&format!("/api/v1/tunnel/{tunnel_id}/ws"));
    url.set_query(Some(&format!("token={token}")));
    Ok(url)
}
