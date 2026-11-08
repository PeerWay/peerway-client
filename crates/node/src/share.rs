//! Temporary web downloads served from this computer.
//!
//! The link only works while PeerWay is running and the recipient is on the
//! same network; it exists so a phone or a computer without the app can fetch
//! a file from the browser. Nothing is uploaded to the cloud.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

use crate::node::Node;

/// Public port for share links. It is separate from the private UI server,
/// which stays bound to 127.0.0.1 and must never be exposed to the network.
pub const SHARE_PORT: u16 = 8788;

#[derive(Clone)]
pub struct Share {
    pub files: Vec<SharedFile>,
    pub expires_at: i64,
}

#[derive(Clone)]
pub struct SharedFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

pub async fn serve(node: Arc<Node>, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/s/{token}", get(page))
        .route("/s/{token}/{index}", get(download))
        .with_state(node);
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "enlaces web temporales disponibles en la red local");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn page(State(node): State<Arc<Node>>, Path(token): Path<String>) -> Response {
    let Some(share) = take_share(&node, &token) else {
        return (
            StatusCode::GONE,
            Html("<p>Este enlace caducó o ya no existe.</p>"),
        )
            .into_response();
    };
    let rows: String = share
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            format!(
                "<li><a href=\"/s/{token}/{index}\">{}</a><span>{}</span></li>",
                escape(&file.name),
                size_label(file.size)
            )
        })
        .collect();
    let html = format!(
        "<!doctype html><html lang=\"es\"><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>PeerWay</title>\
         <style>body{{background:#0c0d0f;color:#e6ebf5;font-family:system-ui,sans-serif;padding:24px;max-width:640px;margin:auto}}\
         h1{{font-size:18px}}ul{{list-style:none;padding:0;display:grid;gap:8px}}\
         li{{display:flex;justify-content:space-between;gap:12px;padding:12px;border:1px solid #2a2d36;border-radius:10px}}\
         a{{color:#7aa2ff;text-decoration:none}}span{{color:#7483a2;font-size:12px}}\
         p{{color:#7483a2;font-size:12px}}</style>\
         <h1>Archivos compartidos</h1><ul>{rows}</ul>\
         <p>Enlace temporal servido por el equipo que envía. Caduca solo.</p></html>"
    );
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn download(
    State(node): State<Arc<Node>>,
    Path((token, index)): Path<(String, usize)>,
) -> Response {
    let Some(share) = take_share(&node, &token) else {
        return StatusCode::GONE.into_response();
    };
    let Some(file) = share.files.get(index) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(handle) = tokio::fs::File::open(&file.path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let body = Body::from_stream(ReaderStream::new(handle));
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&file.size.to_string()).unwrap_or(HeaderValue::from_static("0")),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        file.name.replace(['"', '\\', '\r', '\n'], "_")
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

fn take_share(node: &Node, token: &str) -> Option<Share> {
    let mut shares = node.shares.lock().unwrap();
    shares.retain(|_, share| share.expires_at > chrono::Utc::now().timestamp());
    shares.get(token).cloned()
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn size_label(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
