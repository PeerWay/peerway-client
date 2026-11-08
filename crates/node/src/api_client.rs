use anyhow::{Context, Result};
use futures_util::StreamExt;
use peerway_core::http_auth::{
    body_sha256, new_nonce, RequestSignature, HEADER_APP_ID, HEADER_BODY_SHA256, HEADER_NONCE,
    HEADER_PLATFORM, HEADER_PUBLIC_KEY, HEADER_SIGNATURE, HEADER_TIMESTAMP, HEADER_VERSION,
    STREAMED_BODY_HASH,
};
use peerway_core::identity::Identity;
use peerway_core::protocol::*;
use peerway_core::record::PeerRecord;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio_util::io::ReaderStream;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ClientError {
    pub status: Option<u16>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UploadSpeed {
    pub mbps: f64,
    pub bytes: u64,
    pub seconds: f64,
    pub measured_at: i64,
}

pub struct ApiClient {
    base: String,
    http: reqwest::Client,
    identity: Identity,
    app_id: String,
}

impl ApiClient {
    pub fn new(
        base: &str,
        insecure_tls: bool,
        identity: Identity,
        app_id: impl Into<String>,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(insecure_tls)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("http client")?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            http,
            identity,
            app_id: app_id.into(),
        })
    }

    fn signature_values(
        &self,
        method: &str,
        path: &str,
        body_sha256: &str,
    ) -> Vec<(&'static str, String)> {
        let timestamp = chrono::Utc::now().timestamp();
        let nonce = new_nonce();
        let canonical = RequestSignature {
            method,
            path,
            body_sha256,
            app_id: &self.app_id,
            version: peerway_core::APP_VERSION,
            platform: std::env::consts::OS,
            timestamp,
            nonce: &nonce,
        }
        .canonical();
        let signature = self.identity.sign(canonical.as_bytes());
        vec![
            (HEADER_APP_ID, self.app_id.clone()),
            (HEADER_VERSION, peerway_core::APP_VERSION.to_string()),
            (HEADER_PLATFORM, std::env::consts::OS.to_string()),
            (HEADER_TIMESTAMP, timestamp.to_string()),
            (HEADER_NONCE, nonce),
            (HEADER_BODY_SHA256, body_sha256.to_string()),
            (HEADER_PUBLIC_KEY, self.identity.ed25519_public_hex()),
            (HEADER_SIGNATURE, signature),
        ]
    }

    fn signed(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        request: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        self.signed_hash(method, path, &body_sha256(body), request)
    }

    fn signed_hash(
        &self,
        method: &str,
        path: &str,
        body_sha256: &str,
        mut request: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        for (name, value) in self.signature_values(method, path, body_sha256) {
            request = request.header(name, value);
        }
        request
    }

    /// Add the same signature headers to a WebSocket upgrade request.
    pub(crate) fn sign_headers(
        &self,
        method: &str,
        path: &str,
        body_sha256: &str,
        headers: &mut reqwest::header::HeaderMap,
    ) {
        for (name, value) in self.signature_values(method, path, body_sha256) {
            if let (Ok(name), Ok(value)) = (
                reqwest::header::HeaderName::from_bytes(name.as_bytes()),
                reqwest::header::HeaderValue::from_str(&value),
            ) {
                headers.insert(name, value);
            }
        }
    }

    fn get(&self, path: &str, token: Option<&str>) -> reqwest::RequestBuilder {
        let mut request = self.http.get(format!("{}{}", self.base, path));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        self.signed("GET", path, b"", request)
    }

    fn delete(&self, path: &str, token: &str) -> reqwest::RequestBuilder {
        self.signed(
            "DELETE",
            path,
            b"",
            self.http
                .delete(format!("{}{}", self.base, path))
                .bearer_auth(token),
        )
    }

    fn post_empty(&self, path: &str, token: Option<&str>) -> reqwest::RequestBuilder {
        let mut request = self.http.post(format!("{}{}", self.base, path));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        self.signed("POST", path, b"", request)
    }

    fn post_json<T: Serialize>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &T,
    ) -> Result<reqwest::RequestBuilder, ClientError> {
        let bytes = serde_json::to_vec(body).map_err(|error| ClientError {
            status: None,
            message: format!("no se pudo preparar la petición: {error}"),
        })?;
        let mut request = self
            .http
            .post(format!("{}{}", self.base, path))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes.clone());
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        Ok(self.signed("POST", path, &bytes, request))
    }

    async fn parse<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ClientError> {
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ApiError>(&body)
                .map(|e| e.error)
                .unwrap_or_else(|_| format!("error del servidor ({status})"));
            return Err(ClientError {
                status: Some(status.as_u16()),
                message,
            });
        }
        response.json::<T>().await.map_err(|e| ClientError {
            status: None,
            message: format!("respuesta inválida: {e}"),
        })
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ClientError> {
        let response = request.send().await.map_err(|e| ClientError {
            status: None,
            message: format!("sin conexión con el servidor: {e}"),
        })?;
        self.parse(response).await
    }

    async fn send_empty(&self, request: reqwest::RequestBuilder) -> Result<(), ClientError> {
        let response = request.send().await.map_err(|e| ClientError {
            status: None,
            message: format!("sin conexión con el servidor: {e}"),
        })?;
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ApiError>(&body)
                .map(|e| e.error)
                .unwrap_or_else(|_| format!("error del servidor ({status})"));
            Err(ClientError {
                status: Some(status.as_u16()),
                message,
            })
        }
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        device_name: &str,
        ed25519_pub: &str,
        x25519_pub: &str,
        email: &str,
    ) -> Result<AuthResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/register",
            None,
            &RegisterRequest {
                username: username.to_string(),
                password: password.to_string(),
                device_name: device_name.to_string(),
                ed25519_pub: ed25519_pub.to_string(),
                x25519_pub: x25519_pub.to_string(),
                email: email.to_string(),
            },
        )?;
        self.send_json(request).await
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_name: &str,
        ed25519_pub: &str,
        x25519_pub: &str,
    ) -> Result<AuthResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/login",
            None,
            &LoginRequest {
                username: username.to_string(),
                password: password.to_string(),
                device_name: device_name.to_string(),
                ed25519_pub: ed25519_pub.to_string(),
                x25519_pub: x25519_pub.to_string(),
            },
        )?;
        self.send_json(request).await
    }

    pub async fn logout(&self, token: &str) -> Result<(), ClientError> {
        let request = self.post_empty("/api/v1/logout", Some(token));
        let response = request.send().await.map_err(|e| ClientError {
            status: None,
            message: e.to_string(),
        })?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::UNAUTHORIZED
        {
            Ok(())
        } else {
            Err(ClientError {
                status: Some(response.status().as_u16()),
                message: "no se pudo cerrar la sesión".into(),
            })
        }
    }

    pub async fn me(&self, token: &str) -> Result<MeResponse, ClientError> {
        self.send_json(self.get("/api/v1/me", Some(token))).await
    }

    pub async fn update_display_name(
        &self,
        token: &str,
        display_name: &str,
    ) -> Result<MeResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/me/display-name",
            Some(token),
            &serde_json::json!({ "display_name": display_name }),
        )?;
        self.send_json(request).await
    }

    pub async fn update_email(&self, token: &str, email: &str) -> Result<MeResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/me/email",
            Some(token),
            &serde_json::json!({ "email": email }),
        )?;
        self.send_json(request).await
    }

    pub async fn reset_password_request(&self, email: &str) -> Result<(), ClientError> {
        let request = self.post_json(
            "/api/v1/password/reset/request",
            None,
            &PasswordResetRequest {
                email: email.to_string(),
            },
        )?;
        self.send_empty(request).await
    }

    pub async fn reset_password_confirm(
        &self,
        token: &str,
        password: &str,
    ) -> Result<(), ClientError> {
        let request = self.post_json(
            "/api/v1/password/reset/confirm",
            None,
            &PasswordResetConfirmRequest {
                token: token.to_string(),
                password: password.to_string(),
            },
        )?;
        self.send_empty(request).await
    }

    pub async fn delete_account(&self, token: &str, password: &str) -> Result<(), ClientError> {
        let request = self.post_json(
            "/api/v1/account/delete",
            Some(token),
            &DeleteAccountRequest {
                password: password.to_string(),
            },
        )?;
        self.send_empty(request).await
    }

    pub async fn list_sessions(&self, token: &str) -> Result<SessionsResponse, ClientError> {
        self.send_json(self.get("/api/v1/me/sessions", Some(token)))
            .await
    }

    pub async fn revoke_session(&self, token: &str, id: &str) -> Result<(), ClientError> {
        let request = self.post_json(
            "/api/v1/me/sessions/revoke",
            Some(token),
            &RevokeSessionRequest { id: id.to_string() },
        )?;
        self.send_empty(request).await
    }

    pub async fn revoke_other_sessions(&self, token: &str) -> Result<(), ClientError> {
        self.send_empty(self.post_empty("/api/v1/me/sessions/revoke-others", Some(token)))
            .await
    }

    pub async fn plans(&self) -> Result<PlansResponse, ClientError> {
        self.send_json(self.get("/api/v1/plans", None)).await
    }

    pub async fn announce(&self, record: &PeerRecord) -> Result<AnnounceResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/peers/announce",
            None,
            &AnnounceRequest {
                record: record.clone(),
            },
        )?;
        self.send_json(request).await
    }

    pub async fn peer(&self, code: &str) -> Result<Option<PeerLookup>, ClientError> {
        let response = self
            .get(&format!("/api/v1/peers/{code}"), None)
            .send()
            .await
            .map_err(|e| ClientError {
                status: None,
                message: format!("sin conexión con el servidor: {e}"),
            })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        self.parse(response).await.map(Some)
    }

    pub async fn punch(&self, target_code: &str, record: &PeerRecord) -> Result<(), ClientError> {
        let request = self.post_json(
            "/api/v1/punch",
            None,
            &PunchRequest {
                target_code: target_code.to_string(),
                from_record: record.clone(),
            },
        )?;
        self.send_empty(request).await
    }

    pub async fn create_tunnel(
        &self,
        target_code: &str,
        transfer_id: &str,
        record: &PeerRecord,
    ) -> Result<TunnelCreated, ClientError> {
        let request = self.post_json(
            "/api/v1/tunnels",
            None,
            &TunnelCreateRequest {
                target_code: target_code.to_string(),
                transfer_id: transfer_id.to_string(),
                from_record: record.clone(),
            },
        )?;
        self.send_json(request).await
    }

    pub async fn cloud_list(&self, token: &str) -> Result<CloudListResponse, ClientError> {
        self.send_json(self.get("/api/v1/cloud/files", Some(token)))
            .await
    }

    pub async fn cloud_upload(
        &self,
        token: &str,
        path: &Path,
        name: &str,
        progress: Option<Arc<AtomicU64>>,
    ) -> Result<CloudFileDto, ClientError> {
        let metadata = tokio::fs::metadata(path).await.map_err(|e| ClientError {
            status: None,
            message: format!("no se pudo leer el archivo: {e}"),
        })?;
        let file = tokio::fs::File::open(path).await.map_err(|e| ClientError {
            status: None,
            message: format!("no se pudo abrir el archivo: {e}"),
        })?;
        let stream = ReaderStream::new(file).map(move |chunk| {
            if let (Ok(bytes), Some(counter)) = (&chunk, &progress) {
                counter.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            }
            chunk
        });
        let part = reqwest::multipart::Part::stream_with_length(
            reqwest::Body::wrap_stream(stream),
            metadata.len(),
        )
        .file_name(name.to_string());
        let form = reqwest::multipart::Form::new().part("file", part);
        let request = self.signed_hash(
            "POST",
            "/api/v1/cloud/files",
            STREAMED_BODY_HASH,
            self.http
                .post(format!("{}/api/v1/cloud/files", self.base))
                .bearer_auth(token)
                .multipart(form),
        );
        let response = request.send().await.map_err(|e| ClientError {
            status: None,
            message: format!("sin conexión con el servidor: {e}"),
        })?;
        self.parse(response).await
    }

    pub async fn cloud_delete(&self, token: &str, id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v1/cloud/files/{id}");
        self.send_empty(self.delete(&path, token)).await
    }

    pub async fn cloud_download(
        &self,
        token: &str,
        id: &str,
    ) -> Result<reqwest::Response, ClientError> {
        let response = self
            .get(&format!("/api/v1/cloud/files/{id}/download"), Some(token))
            .send()
            .await
            .map_err(|e| ClientError {
                status: None,
                message: format!("sin conexión con el servidor: {e}"),
            })?;
        if !response.status().is_success() {
            return Err(ClientError {
                status: Some(response.status().as_u16()),
                message: "no se pudo descargar el archivo".into(),
            });
        }
        Ok(response)
    }

    pub async fn activate(
        &self,
        token: &str,
        license_key: &str,
    ) -> Result<MeResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/billing/activate",
            Some(token),
            &ActivateRequest {
                license_key: license_key.to_string(),
            },
        )?;
        self.send_json(request).await
    }

    pub async fn checkout(&self, token: &str) -> Result<String, ClientError> {
        let value: serde_json::Value = self
            .send_json(self.post_empty("/api/v1/billing/checkout", Some(token)))
            .await?;
        Ok(value
            .get("url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string())
    }

    pub async fn paypal_order(
        &self,
        token: &str,
        plan: &str,
    ) -> Result<PayPalOrderResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/billing/paypal/order",
            Some(token),
            &PayPalOrderRequest {
                plan: Plan::from_str(plan),
            },
        )?;
        self.send_json(request).await
    }

    pub async fn paypal_capture(
        &self,
        token: &str,
        order_id: &str,
    ) -> Result<MeResponse, ClientError> {
        let request = self.post_json(
            "/api/v1/billing/paypal/capture",
            Some(token),
            &PayPalCaptureRequest {
                order_id: order_id.to_string(),
            },
        )?;
        self.send_json(request).await
    }

    async fn speedtest_probe(&self, bytes: usize) -> Result<(u64, f64), ClientError> {
        let mut payload = vec![0u8; bytes];
        getrandom::fill(&mut payload).map_err(|error| ClientError {
            status: None,
            message: format!("no se pudo preparar la prueba: {error}"),
        })?;
        let path = "/api/v1/speedtest";
        let hash = body_sha256(&payload);
        let request = self
            .http
            .post(format!("{}{}", self.base, path))
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(payload);
        let request = self.signed_hash("POST", path, &hash, request);
        let started = Instant::now();
        let response = request.send().await.map_err(|error| ClientError {
            status: None,
            message: format!("sin conexión con el servidor: {error}"),
        })?;
        let seconds = started.elapsed().as_secs_f64();
        if !response.status().is_success() {
            return Err(ClientError {
                status: Some(response.status().as_u16()),
                message: "no se pudo medir la velocidad de subida".into(),
            });
        }
        Ok((bytes as u64, seconds))
    }

    /// Measure upload throughput with one large, steady-state probe. Small
    /// requests mostly measure latency, which made fast connections look slow.
    pub async fn measure_upload(&self) -> Result<UploadSpeed, ClientError> {
        // Warm-up: TLS/TCP setup is not part of the result.
        let _ = self.speedtest_probe(256 * 1024).await?;

        // A 1 MiB sample chooses the payload size. The final sample is the
        // only one used for the reported speed.
        let (rough_bytes, rough_seconds) = self.speedtest_probe(1024 * 1024).await?;
        let rough_mbps = if rough_seconds > 0.0 {
            (rough_bytes as f64 * 8.0) / rough_seconds / 1_000_000.0
        } else {
            0.0
        };
        let size = speedtest_target(rough_mbps);
        let (bytes, seconds) = self.speedtest_probe(size).await?;
        if seconds <= 0.0 {
            return Err(ClientError {
                status: None,
                message: "la prueba de velocidad no devolvió un tiempo válido".into(),
            });
        }
        let mbps = (bytes as f64 * 8.0) / seconds / 1_000_000.0;
        Ok(UploadSpeed {
            mbps,
            bytes,
            seconds,
            measured_at: chrono::Utc::now().timestamp(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn telemetry_heartbeat(
        &self,
        version: &str,
        platform: &str,
        plan: &str,
        cloud_enabled: bool,
        transfers_completed: u64,
        transfers_failed: u64,
        relay_used: u64,
        transfer_methods: &std::collections::HashMap<String, u64>,
        port_mapping: &str,
    ) -> Result<(), ClientError> {
        let request = self.post_json(
            "/api/v1/telemetry/heartbeat",
            None,
            &serde_json::json!({
                "install_id": self.app_id,
                "version": version,
                "platform": platform,
                "plan": plan,
                "cloud_enabled": cloud_enabled,
                "transfers_completed": transfers_completed,
                "transfers_failed": transfers_failed,
                "relay_used": relay_used,
                "transfer_methods": transfer_methods,
                "port_mapping": port_mapping,
            }),
        )?;
        self.send_empty(request).await
    }
}

fn speedtest_target(rough_mbps: f64) -> usize {
    if rough_mbps >= 40.0 {
        24 * 1024 * 1024
    } else if rough_mbps >= 10.0 {
        8 * 1024 * 1024
    } else if rough_mbps >= 3.0 {
        4 * 1024 * 1024
    } else {
        1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peerway_core::http_auth::{
        RequestSignature, HEADER_APP_ID, HEADER_BODY_SHA256, HEADER_NONCE, HEADER_SIGNATURE,
        HEADER_TIMESTAMP,
    };
    use std::collections::HashMap;

    #[test]
    fn signed_headers_verify() {
        let identity = Identity::generate().unwrap();
        let public_key = identity.ed25519_public_hex();
        let client =
            ApiClient::new("https://example.invalid", false, identity, "install-test").unwrap();
        let body = br#"{"hello":"world"}"#;
        let values = client.signature_values("POST", "/api/v1/test", &body_sha256(body));
        let headers: HashMap<_, _> = values.into_iter().collect();
        assert_eq!(headers[HEADER_APP_ID], "install-test");
        assert_eq!(headers[HEADER_BODY_SHA256], body_sha256(body));
        RequestSignature {
            method: "POST",
            path: "/api/v1/test",
            body_sha256: &headers[HEADER_BODY_SHA256],
            app_id: &headers[HEADER_APP_ID],
            version: peerway_core::APP_VERSION,
            platform: std::env::consts::OS,
            timestamp: headers[HEADER_TIMESTAMP].parse().unwrap(),
            nonce: &headers[HEADER_NONCE],
        }
        .verify(&public_key, &headers[HEADER_SIGNATURE])
        .unwrap();
    }

    #[test]
    fn speedtest_size_scales_with_rough_speed() {
        assert_eq!(speedtest_target(0.5), 1024 * 1024);
        assert_eq!(speedtest_target(5.0), 4 * 1024 * 1024);
        assert_eq!(speedtest_target(20.0), 8 * 1024 * 1024);
        assert_eq!(speedtest_target(120.0), 24 * 1024 * 1024);
    }
}
