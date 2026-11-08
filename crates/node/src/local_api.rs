use axum::body::Body;
use axum::extract::ws::{Message, Utf8Bytes, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use peerway_core::protocol::Plan;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use crate::node::Node;

const INDEX: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const I18N_JS: &str = include_str!("../../../web/i18n.js");
const STYLES: &str = include_str!("../../../web/styles.css");

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    response
}

#[derive(Clone)]
struct Local {
    node: Arc<Node>,
    token: String,
}

fn random_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).expect("os rng");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

pub async fn serve(node: Arc<Node>, requested_port: u16) -> anyhow::Result<u16> {
    let state = Arc::new(Local {
        node: node.clone(),
        token: random_token(),
    });
    let protected = Router::new()
        .route("/api/state", get(state_handler))
        .route("/api/code/rotate", post(rotate_code_handler))
        .route("/api/share", post(share_handler))
        .route("/api/send", post(send_handler))
        .route("/api/pick-files", post(pick_files))
        .route(
            "/api/stage",
            post(stage_files).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/transfers/{id}/accept", post(accept_handler))
        .route("/api/transfers/{id}/decline", post(decline_handler))
        .route("/api/transfers/{id}/cancel", post(cancel_handler))
        .route("/api/transfers/{id}/open", post(open_transfer))
        .route("/api/transfers/{id}/open-file", post(open_transfer_file))
        .route("/api/transfers/{id}/trust", post(transfer_trust_handler))
        .route("/api/open-folder", post(open_folder))
        .route("/api/auth/register", post(register_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/auth/reset", post(reset_handler))
        .route("/api/account/display-name", post(display_name_handler))
        .route("/api/account/email", post(email_handler))
        .route("/api/open-url", post(open_url_handler))
        .route("/api/visibility", post(visibility_handler))
        .route("/api/account/delete", post(delete_account_handler))
        .route("/api/account/sessions", get(sessions_handler))
        .route("/api/account/sessions/revoke", post(revoke_session_handler))
        .route(
            "/api/account/sessions/revoke-others",
            post(revoke_others_handler),
        )
        .route("/api/history/clear", post(clear_history))
        .route("/api/settings", post(settings_handler))
        .route("/api/trust", post(trust_handler))
        .route("/api/speedtest", post(speedtest_handler))
        .route("/api/consent", post(consent_handler))
        .route("/api/plans", get(plans_handler))
        .route("/api/cloud", get(cloud_list))
        .route(
            "/api/cloud/upload",
            post(cloud_upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/cloud/{id}", delete(cloud_delete))
        .route("/api/cloud/{id}/download", get(cloud_download))
        .route("/api/cloud/paypal/order", post(paypal_order))
        .route("/api/cloud/paypal/capture", post(paypal_capture))
        .route("/api/cloud/activate", post(cloud_activate))
        .route("/api/cloud/checkout", post(cloud_checkout))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/i18n.js", get(i18n_js))
        .route("/styles.css", get(styles))
        .route("/api/ping", get(ping))
        .route("/api/events", get(events))
        .route("/api/cloud/{id}/raw", get(cloud_download_public))
        .merge(protected)
        .with_state(state.clone())
        .layer(middleware::from_fn(no_store));

    let mut listener = None;
    let mut port = requested_port;
    for _ in 0..20 {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(bound) => {
                listener = Some(bound);
                break;
            }
            Err(_) => port = port.saturating_add(1),
        }
    }
    let listener =
        listener.ok_or_else(|| anyhow::anyhow!("no hay puerto libre para la interfaz"))?;
    let actual = listener.local_addr()?.port();
    let _ = std::fs::write(&node.paths.ui_port_file, actual.to_string());
    let _ = std::fs::write(&node.paths.local_token, &state.token);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&node.paths.local_token) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&node.paths.local_token, perms);
        }
    }
    tracing::info!("interfaz local en http://127.0.0.1:{actual}");
    axum::serve(listener, app).await?;
    Ok(actual)
}

async fn require_token(State(state): State<Arc<Local>>, request: Request, next: Next) -> Response {
    let provided = request
        .headers()
        .get("x-peerway-token")
        .and_then(|v| v.to_str().ok());
    if provided != Some(state.token.as_str()) {
        return error(StatusCode::UNAUTHORIZED, "token local inválido");
    }
    next.run(request).await
}

fn security_headers() -> [(header::HeaderName, HeaderValue); 3] {
    [
        (
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self' https://www.paypal.com https://www.paypalobjects.com; style-src 'self' 'unsafe-inline' https://www.paypal.com; img-src 'self' data: https://www.paypal.com https://www.paypalobjects.com; connect-src 'self' ws://127.0.0.1:* ws://localhost:* https://www.paypal.com https://www.paypalobjects.com https://api-m.paypal.com https://api-m.sandbox.paypal.com; frame-src https://www.paypal.com https://www.sandbox.paypal.com; frame-ancestors 'none'",
            ),
        ),
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ),
    ]
}

async fn index(State(state): State<Arc<Local>>) -> impl IntoResponse {
    let html = INDEX.replace("__PEERWAY_TOKEN__", &state.token);
    (security_headers(), Html(html))
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

async fn i18n_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        I18N_JS,
    )
}

async fn styles() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLES)
}

async fn ping() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "app": "peerway",
        "version": peerway_core::APP_VERSION
    }))
}

#[derive(Deserialize)]
struct TokenQuery {
    token: String,
}

async fn events(
    State(state): State<Arc<Local>>,
    Query(query): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if query.token != state.token {
        return error(StatusCode::UNAUTHORIZED, "token local inválido");
    }
    let mut rx = state.node.subscribe();
    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        let _ = sink
            .send(Message::Text(Utf8Bytes::from_static(
                "{\"type\":\"hello\"}",
            )))
            .await;
        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            let Ok(text) = serde_json::to_string(&event) else { continue };
                            if sink.send(Message::Text(Utf8Bytes::from(text))).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
                message = stream.next() => {
                    match message {
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(_)) => break,
                        _ => {}
                    }
                }
            }
        }
    })
}

async fn rotate_code_handler(State(state): State<Arc<Local>>) -> Response {
    match state.node.rotate_code() {
        Ok(code) => Json(serde_json::json!({ "code": code })).into_response(),
        Err(err) => error(StatusCode::BAD_REQUEST, format!("{err:#}")),
    }
}

#[derive(serde::Deserialize)]
struct ShareRequest {
    paths: Vec<String>,
    #[serde(default = "default_share_hours")]
    hours: u64,
}

fn default_share_hours() -> u64 {
    24
}

async fn share_handler(State(state): State<Arc<Local>>, Json(req): Json<ShareRequest>) -> Response {
    let paths: Vec<PathBuf> = req
        .paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect();
    match state.node.create_share(paths, req.hours) {
        Ok(token) => {
            let ips = crate::node::local_lan_ips();
            let ip = preferred_lan_ip(&ips);
            Json(serde_json::json!({
                "url": format!("http://{ip}:{}/s/{token}", crate::share::SHARE_PORT),
                "token": token,
            }))
            .into_response()
        }
        Err(err) => error(StatusCode::BAD_REQUEST, format!("{err:#}")),
    }
}

/// Prefer a real LAN address over virtual adapters (VPN, WSL, Hyper-V).
fn preferred_lan_ip(ips: &[String]) -> String {
    ips.iter()
        .filter(|ip| !ip.contains(':') && !ip.starts_with("100.") && !ip.starts_with("169.254."))
        .max_by_key(|ip| {
            if ip.starts_with("192.168.") {
                3
            } else if ip.starts_with("10.") {
                2
            } else if ip.starts_with("172.") {
                1
            } else {
                0
            }
        })
        .cloned()
        .or_else(|| ips.first().cloned())
        .unwrap_or_else(|| "127.0.0.1".into())
}

async fn state_handler(State(state): State<Arc<Local>>) -> Json<serde_json::Value> {
    let node = &state.node;
    let config = node.config();
    let session = node.session();
    let lan: Vec<serde_json::Value> = node
        .lan_peers()
        .into_iter()
        .map(|peer| {
            serde_json::json!({
                "code": peer.record.display_code(),
                "device_name": peer.record.device_name,
                "platform": peer.record.platform,
                "addr": peer.addr.to_string(),
            })
        })
        .collect();
    let plan = session.as_ref().map(|s| s.plan).unwrap_or(Plan::Free);
    Json(serde_json::json!({
        "app": "PeerWay",
        "version": peerway_core::APP_VERSION,
        "platform": std::env::consts::OS,
        "code": peerway_core::code::format(&node.code()),
        "device_name": config.device_name,
        "download_dir": config.download_dir,
        "server_online": node.server_online(),
        "last_error": node.last_error(),
        "logged_in": session.is_some(),
        "username": session.as_ref().map(|s| s.username.clone()),
        "display_name": session.as_ref().and_then(|s| s.display_name.clone()),
        "email": session.as_ref().and_then(|s| s.email.clone()),
        "email_verified": session.as_ref().map(|s| s.email_verified).unwrap_or(false),
        "plan": plan,
        "plan_label": plan.label(),
        "plan_expires": session.as_ref().and_then(|s| s.plan_expires),
        "quota_bytes": session.as_ref().map(|s| s.quota_bytes).unwrap_or(0),
        "used_bytes": session.as_ref().map(|s| s.used_bytes).unwrap_or(0),
        "trusted_codes": config.trusted_codes.iter().map(|c| peerway_core::code::format(c)).collect::<Vec<_>>(),
        "auto_accept_trusted": config.auto_accept_trusted,
        "auto_update": config.auto_update,
        "autostart_enabled": config.autostart_enabled,
        "language": config.language,
        "telemetry_enabled": config.telemetry_enabled,
        "telemetry_choice_made": config.telemetry_choice_made,
        "notifications_enabled": config.notifications_enabled,
        "notify_updates": config.notify_updates,
        "notify_transfers": config.notify_transfers,
        "upload_speed": node.upload_speed(),
        "lan_discovery": config.lan_discovery,
        "lan_peers": lan,
        "transfers": node.transfers(),
        "truncated": false
    }))
}

#[derive(Deserialize)]
struct SendRequest {
    code: String,
    paths: Vec<String>,
    #[serde(default)]
    comment: String,
}

async fn send_handler(State(state): State<Arc<Local>>, Json(req): Json<SendRequest>) -> Response {
    let paths: Vec<PathBuf> = req
        .paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    if paths.is_empty() {
        return error(StatusCode::BAD_REQUEST, "no hay archivos válidos");
    }
    match state.node.send_files(&req.code, paths, req.comment).await {
        Ok(id) => Json(serde_json::json!({ "id": id })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn pick_files() -> Response {
    match open_file_dialog().await {
        Ok(paths) => {
            let files: Vec<serde_json::Value> = paths
                .into_iter()
                .filter_map(|path| {
                    let metadata = std::fs::metadata(&path).ok()?;
                    if !metadata.is_file() {
                        return None;
                    }
                    let name = PathBuf::from(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "archivo".into());
                    Some(serde_json::json!({
                        "path": path,
                        "name": name,
                        "size": metadata.len(),
                    }))
                })
                .collect();
            Json(serde_json::json!({ "files": files })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct SettingsRequest {
    device_name: String,
    download_dir: String,
    auto_accept_trusted: bool,
    lan_discovery: bool,
    auto_update: bool,
    #[serde(default = "default_true")]
    autostart_enabled: bool,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    telemetry_enabled: Option<bool>,
    #[serde(default)]
    telemetry_choice_made: Option<bool>,
    #[serde(default)]
    notifications_enabled: Option<bool>,
    #[serde(default)]
    notify_updates: Option<bool>,
    #[serde(default)]
    notify_transfers: Option<bool>,
}

async fn stage_files(State(state): State<Arc<Local>>, mut multipart: Multipart) -> Response {
    let stage_dir = state
        .node
        .paths
        .home
        .join("staging")
        .join(uuid::Uuid::new_v4().to_string());
    if let Err(e) = tokio::fs::create_dir_all(&stage_dir).await {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let mut files = Vec::new();
    while let Some(mut field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(e) => return error(StatusCode::BAD_REQUEST, format!("multipart: {e}")),
    } {
        let name = crate::transfer::sanitize_file_name(field.file_name().unwrap_or("archivo"));
        let path = stage_dir.join(&name);
        let mut file = match tokio::fs::File::create(&path).await {
            Ok(file) => file,
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        let mut size = 0u64;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size += chunk.len() as u64;
                    if let Err(e) = file.write_all(&chunk).await {
                        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                    }
                }
                Ok(None) => break,
                Err(e) => return error(StatusCode::BAD_REQUEST, format!("subida: {e}")),
            }
        }
        let _ = file.flush().await;
        files.push(serde_json::json!({
            "path": path.to_string_lossy(),
            "name": name,
            "size": size,
        }));
    }
    Json(serde_json::json!({ "files": files })).into_response()
}

async fn accept_handler(
    State(state): State<Arc<Local>>,
    Path(id): Path<String>,
    Query(query): Query<AcceptQuery>,
) -> Response {
    match state.node.accept_transfer(&id, query.remember) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[derive(Deserialize)]
struct AcceptQuery {
    #[serde(default)]
    remember: bool,
}

#[derive(Deserialize)]
struct VisibilityBody {
    visible: bool,
}

/// The UI reports when it is hidden so discovery stops broadcasting on battery.
async fn visibility_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<VisibilityBody>,
) -> Response {
    state.node.set_ui_visible(req.visible);
    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn decline_handler(State(state): State<Arc<Local>>, Path(id): Path<String>) -> Response {
    match state.node.decline_transfer(&id) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

async fn cancel_handler(State(state): State<Arc<Local>>, Path(id): Path<String>) -> Response {
    match state.node.cancel_transfer(&id) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

async fn open_transfer(State(state): State<Arc<Local>>, Path(id): Path<String>) -> Response {
    let Some(transfer) = state.node.transfer(&id) else {
        return error(StatusCode::NOT_FOUND, "transferencia no encontrada");
    };
    match transfer
        .folder
        .map(PathBuf::from)
        .or_else(|| Some(state.node.download_dir()))
    {
        Some(path) => match state.node.open_path(&path) {
            Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
            Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
        },
        None => error(StatusCode::BAD_REQUEST, "sin carpeta disponible"),
    }
}

async fn open_transfer_file(State(state): State<Arc<Local>>, Path(id): Path<String>) -> Response {
    let Some(transfer) = state.node.transfer(&id) else {
        return error(StatusCode::NOT_FOUND, "transferencia no encontrada");
    };
    if transfer.direction != "receive" || transfer.status != "completed" {
        return error(StatusCode::BAD_REQUEST, "la transferencia no está completa");
    }
    let Some(folder) = transfer.folder.map(PathBuf::from) else {
        return error(StatusCode::BAD_REQUEST, "sin carpeta disponible");
    };
    let Some(file) = transfer.files.first() else {
        return error(
            StatusCode::BAD_REQUEST,
            "la transferencia no tiene archivos",
        );
    };
    let Some(name) = std::path::Path::new(&file.name).file_name() else {
        return error(StatusCode::BAD_REQUEST, "nombre de archivo inválido");
    };
    let path = folder.join(name);
    match state.node.open_path(&path) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[derive(Deserialize)]
struct TransferTrustBody {
    #[serde(default = "default_true")]
    trusted: bool,
}

/// Trust the device behind a transfer without making the user copy its code.
/// This is what the "add to trusted devices" action uses after a download or
/// upload has appeared in the activity list.
async fn transfer_trust_handler(
    State(state): State<Arc<Local>>,
    Path(id): Path<String>,
    Json(req): Json<TransferTrustBody>,
) -> Response {
    let Some(transfer) = state.node.transfer(&id) else {
        return error(StatusCode::NOT_FOUND, "transferencia no encontrada");
    };
    match state.node.trust_code(&transfer.peer_code, req.trusted) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn open_folder(State(state): State<Arc<Local>>) -> Response {
    match state.node.open_download_dir() {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[derive(Deserialize)]
struct EmailBody {
    email: String,
}

#[derive(Deserialize)]
struct OpenUrlBody {
    url: String,
}

/// Open an allow-listed PeerWay page in the system browser.
async fn open_url_handler(Json(req): Json<OpenUrlBody>) -> Response {
    let url = req.url.trim();
    const HOSTS: [&str; 4] = [
        "https://www.peerway.me/",
        "https://peerway.me/",
        "https://www.peerway.io/",
        "https://peerway.io/",
    ];
    if !HOSTS.iter().any(|prefix| url.starts_with(prefix)) {
        return error(StatusCode::BAD_REQUEST, "enlace no permitido");
    }
    match open::that(url) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("no se pudo abrir: {e}")),
    }
}

async fn email_handler(State(state): State<Arc<Local>>, Json(req): Json<EmailBody>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.update_email(&token, req.email.trim()).await {
        Ok(me) => {
            state.node.apply_me(me);
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

#[derive(Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
    #[serde(default)]
    email: String,
}

async fn register_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<AuthRequest>,
) -> Response {
    match state
        .node
        .register(&req.username, &req.password, &req.email)
        .await
    {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn login_handler(State(state): State<Arc<Local>>, Json(req): Json<AuthRequest>) -> Response {
    match state.node.login(&req.username, &req.password).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn logout_handler(State(state): State<Arc<Local>>) -> Response {
    match state.node.logout().await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

#[derive(Deserialize)]
struct ResetRequest {
    email: String,
}

async fn reset_handler(State(state): State<Arc<Local>>, Json(req): Json<ResetRequest>) -> Response {
    match state
        .node
        .http
        .reset_password_request(req.email.trim())
        .await
    {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

#[derive(Deserialize)]
struct DeleteAccountBody {
    password: String,
}

async fn delete_account_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<DeleteAccountBody>,
) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.delete_account(&token, &req.password).await {
        Ok(()) => {
            let _ = state.node.logout().await;
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn sessions_handler(State(state): State<Arc<Local>>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.list_sessions(&token).await {
        Ok(sessions) => Json(sessions).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

#[derive(Deserialize)]
struct RevokeSessionBody {
    id: String,
}

async fn revoke_session_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<RevokeSessionBody>,
) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.revoke_session(&token, req.id.trim()).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn revoke_others_handler(State(state): State<Arc<Local>>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.revoke_other_sessions(&token).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn settings_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<SettingsRequest>,
) -> Response {
    if req.device_name.trim().is_empty() {
        return error(
            StatusCode::BAD_REQUEST,
            "el nombre del equipo no puede estar vacío",
        );
    }
    if req.download_dir.trim().is_empty() {
        return error(
            StatusCode::BAD_REQUEST,
            "la carpeta de descargas no puede estar vacía",
        );
    }
    let mut config = state.node.config();
    config.device_name = req.device_name.trim().to_string();
    config.download_dir = req.download_dir.trim().to_string();
    config.auto_accept_trusted = req.auto_accept_trusted;
    config.lan_discovery = req.lan_discovery;
    config.auto_update = req.auto_update;
    config.autostart_enabled = req.autostart_enabled;
    if let Some(language) = req.language {
        config.language = language;
    }
    if let Some(telemetry_enabled) = req.telemetry_enabled {
        config.telemetry_enabled = telemetry_enabled;
    }
    if let Some(choice_made) = req.telemetry_choice_made {
        config.telemetry_choice_made = choice_made;
    }
    if let Some(enabled) = req.notifications_enabled {
        config.notifications_enabled = enabled;
    }
    if let Some(enabled) = req.notify_updates {
        config.notify_updates = enabled;
    }
    if let Some(enabled) = req.notify_transfers {
        config.notify_transfers = enabled;
    }
    let autostart_enabled = config.autostart_enabled;
    match state.node.update_config(config) {
        Ok(()) => {
            if let Err(e) = crate::autostart::apply(autostart_enabled) {
                return error(
                    StatusCode::BAD_REQUEST,
                    format!("no se pudo actualizar el inicio automático: {e:#}"),
                );
            }
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

#[derive(Deserialize)]
struct TrustRequest {
    code: String,
    trusted: bool,
}

async fn trust_handler(State(state): State<Arc<Local>>, Json(req): Json<TrustRequest>) -> Response {
    match state.node.trust_code(&req.code, req.trusted) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

#[allow(clippy::result_large_err)]
fn session_token(node: &Node) -> Result<String, Response> {
    node.session()
        .map(|s| s.token)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "inicia sesión para usar la nube"))
}

async fn display_name_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<serde_json::Value>,
) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    let display_name = req
        .get("display_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    match state
        .node
        .http
        .update_display_name(&token, &display_name)
        .await
    {
        Ok(me) => {
            state.node.apply_me(me);
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn clear_history(State(state): State<Arc<Local>>) -> Response {
    state.node.clear_history();
    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn cloud_list(State(state): State<Arc<Local>>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    state.node.refresh_session().await;
    match state.node.http.cloud_list(&token).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn speedtest_handler(State(state): State<Arc<Local>>) -> Response {
    match state.node.measure_upload().await {
        Ok(speed) => Json(speed).into_response(),
        Err(err) => error(StatusCode::BAD_GATEWAY, err.message),
    }
}

#[derive(Deserialize)]
struct ConsentRequest {
    accepted: bool,
}

async fn consent_handler(
    State(state): State<Arc<Local>>,
    Json(req): Json<ConsentRequest>,
) -> Response {
    let mut config = state.node.config();
    config.telemetry_enabled = req.accepted;
    config.telemetry_choice_made = true;
    match state.node.update_config(config) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn plans_handler(State(state): State<Arc<Local>>) -> Response {
    match state.node.http.plans().await {
        Ok(plans) => Json(plans).into_response(),
        Err(e) => error(StatusCode::BAD_GATEWAY, e.message),
    }
}

async fn cloud_upload(State(state): State<Arc<Local>>, mut multipart: Multipart) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    if let Some(mut field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(e) => return error(StatusCode::BAD_REQUEST, format!("multipart: {e}")),
    } {
        let filename = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "archivo".into());
        let temp = std::env::temp_dir().join(format!("peerway-upload-{}", uuid::Uuid::new_v4()));
        let mut file = match tokio::fs::File::create(&temp).await {
            Ok(file) => file,
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        let _ = tokio::fs::remove_file(&temp).await;
                        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&temp).await;
                    return error(StatusCode::BAD_REQUEST, format!("error de subida: {e}"));
                }
            }
        }
        let _ = file.flush().await;
        drop(file);
        // Report real progress: the browser part is local and finishes fast,
        // this is the slow upload from the node to the server.
        let total = tokio::fs::metadata(&temp)
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
        let progress = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let watcher = {
            let node = state.node.clone();
            let progress = progress.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    node.emit(crate::state::UiEvent::CloudUpload {
                        loaded: progress.load(std::sync::atomic::Ordering::Relaxed),
                        total,
                    });
                }
            })
        };
        let result = state
            .node
            .http
            .cloud_upload(&token, &temp, &filename, Some(progress))
            .await;
        watcher.abort();
        let _ = tokio::fs::remove_file(&temp).await;
        return match result {
            Ok(file) => Json(file).into_response(),
            Err(e) => error(StatusCode::BAD_REQUEST, e.message),
        };
    }
    error(StatusCode::BAD_REQUEST, "no se recibió ningún archivo")
}

async fn cloud_delete(State(state): State<Arc<Local>>, Path(id): Path<String>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.cloud_delete(&token, &id).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn cloud_download(State(state): State<Arc<Local>>, Path(id): Path<String>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    proxy_cloud_download(&state, &token, &id).await
}

async fn cloud_download_public(
    State(state): State<Arc<Local>>,
    Path(id): Path<String>,
    Query(query): Query<TokenQuery>,
) -> Response {
    if query.token != state.token {
        return error(StatusCode::UNAUTHORIZED, "token local inválido");
    }
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    proxy_cloud_download(&state, &token, &id).await
}

async fn proxy_cloud_download(state: &Local, token: &str, id: &str) -> Response {
    let name = match state.node.http.cloud_list(token).await {
        Ok(list) => list
            .files
            .into_iter()
            .find(|f| f.id == id)
            .map(|f| f.name)
            .unwrap_or_else(|| "archivo".into()),
        Err(e) => return error(StatusCode::BAD_REQUEST, e.message),
    };
    match state.node.http.cloud_download(token, id).await {
        Ok(response) => {
            let mut builder = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", name.replace('"', "")),
                );
            if let Some(length) = response.content_length() {
                builder = builder.header(header::CONTENT_LENGTH, length);
            }
            builder
                .body(Body::from_stream(response.bytes_stream()))
                .unwrap_or_else(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

#[derive(Deserialize)]
struct ActivateBody {
    license_key: String,
}

async fn cloud_activate(
    State(state): State<Arc<Local>>,
    Json(req): Json<ActivateBody>,
) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.activate(&token, &req.license_key).await {
        Ok(me) => {
            state.node.apply_me(me);
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn cloud_checkout(State(state): State<Arc<Local>>) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.checkout(&token).await {
        Ok(url) => Json(serde_json::json!({ "url": url })).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

#[derive(serde::Deserialize)]
struct PayPalPlanBody {
    plan: String,
}

async fn paypal_order(
    State(state): State<Arc<Local>>,
    Json(req): Json<PayPalPlanBody>,
) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.paypal_order(&token, &req.plan).await {
        Ok(order) => Json(order).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

#[derive(serde::Deserialize)]
struct PayPalCaptureBody {
    order_id: String,
}

async fn paypal_capture(
    State(state): State<Arc<Local>>,
    Json(req): Json<PayPalCaptureBody>,
) -> Response {
    let token = match session_token(&state.node) {
        Ok(token) => token,
        Err(response) => return response,
    };
    match state.node.http.paypal_capture(&token, &req.order_id).await {
        Ok(me) => {
            state.node.apply_me(me);
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e.message),
    }
}

async fn open_file_dialog() -> anyhow::Result<Vec<String>> {
    #[cfg(target_os = "windows")]
    {
        let script = r#"Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Multiselect=$true; $d.Title='Seleccionar archivos para enviar con PeerWay'; if($d.ShowDialog() -eq 'OK'){ @($d.FileNames) | ConvertTo-Json -Compress } else { '[]' }"#;
        let mut command = tokio::process::Command::new("powershell");
        command.args(["-NoProfile", "-STA", "-Command", script]);
        // The picker runs hidden: without this flag Windows opens a console
        // window for PowerShell every time the user chooses files.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
        let output = tokio::time::timeout(std::time::Duration::from_secs(600), command.output())
            .await
            .map_err(|_| anyhow::anyhow!("el selector de archivos tardó demasiado"))??;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let mut paths = Vec::new();
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    if let Some(path) = item.as_str() {
                        paths.push(path.to_string());
                    }
                }
            }
            serde_json::Value::String(path) => paths.push(path),
            _ => {}
        }
        return Ok(paths);
    }
    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("zenity")
            .args([
                "--file-selection",
                "--multiple",
                "--separator=\n",
                "--title=Seleccionar archivos para enviar con PeerWay",
            ])
            .output()
            .await?;
        if !output.status.success() {
            return Ok(Vec::new());
        }
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        return Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect());
    }
    #[cfg(target_os = "macos")]
    {
        let script = r#"set theFiles to choose file with multiple selections allowed
set out to ""
repeat with f in theFiles
set out to out & POSIX path of f & linefeed
end repeat
return out"#;
        let output = tokio::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .await?;
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        return Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect());
    }
    #[allow(unreachable_code)]
    Ok(Vec::new())
}
