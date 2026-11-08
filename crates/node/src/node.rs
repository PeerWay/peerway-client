use crate::api_client::{ApiClient, ClientError, UploadSpeed};
use crate::config::{
    load_config, load_session, save_config, save_session, NodeConfig, Paths, SessionData,
};
use crate::state::{FileView, LanPeer, TransferView, UiEvent};
use anyhow::Result;
use peerway_core::code;
use peerway_core::identity::Identity;
use peerway_core::net;
use peerway_core::protocol::{AuthResponse, MeResponse};
use peerway_core::record::PeerRecord;
use quinn::Endpoint;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, oneshot, Notify};
use tokio_util::sync::CancellationToken;

pub(crate) struct PendingDecision {
    pub tx: oneshot::Sender<bool>,
}

struct TelemetryCounts {
    completed: u64,
    failed: u64,
    relay: u64,
    methods: HashMap<String, u64>,
}

pub struct Node {
    pub(crate) identity: Identity,
    pub(crate) paths: Paths,
    pub(crate) config: RwLock<NodeConfig>,
    pub(crate) session: RwLock<Option<SessionData>>,
    pub(crate) code: RwLock<String>,
    pub(crate) record: RwLock<PeerRecord>,
    pub(crate) endpoint_v4: Endpoint,
    pub(crate) endpoint_v6: Option<Endpoint>,
    pub(crate) port_v4: u16,
    pub(crate) port_v6: u16,
    pub(crate) observed_v4: RwLock<Option<SocketAddr>>,
    pub(crate) observed_v6: RwLock<Option<SocketAddr>>,
    pub(crate) lan_peers: RwLock<HashMap<String, LanPeer>>,
    pub(crate) transfers: RwLock<HashMap<String, TransferView>>,
    pub(crate) shares: Mutex<HashMap<String, crate::share::Share>>,
    pub(crate) cancels: Mutex<HashMap<String, CancellationToken>>,
    pub(crate) pending: Mutex<HashMap<String, PendingDecision>>,
    pub(crate) events: broadcast::Sender<UiEvent>,
    pub(crate) http: ApiClient,
    pub(crate) signal_notify: Notify,
    pub(crate) announce_notify: Notify,
    pub(crate) code_lock: tokio::sync::Mutex<()>,
    pub(crate) server_online: AtomicBool,
    pub(crate) code_epoch: AtomicU64,
    pub(crate) last_error: RwLock<Option<String>>,
    pub(crate) upload_speed: RwLock<Option<UploadSpeed>>,
    pub(crate) started: AtomicBool,
    /// False while the UI is hidden, so discovery can stop broadcasting.
    pub(crate) ui_visible: AtomicBool,
}

impl Node {
    pub async fn new(paths: Paths) -> Result<Arc<Self>> {
        let mut config = load_config(&paths.config)?;
        std::fs::create_dir_all(&config.download_dir).ok();
        // If the private identity was lost but the old installation id stayed
        // in config.json, the server would reject the new public key. Rotate
        // the id together with a newly generated identity.
        let identity_existed = paths.identity.exists();
        let identity = Identity::load_or_create(&paths.identity)?;
        if !identity_existed {
            config.install_id = uuid::Uuid::new_v4().to_string();
            save_config(&paths.config, &config)?;
        }
        let session = load_session(&paths.session).filter(|s| code::is_valid(&s.code));
        // Keep the temporary code across restarts; rotating it on every launch
        // forced everyone who had it to ask for it again.
        let stored_code = code::is_valid(&config.code).then(|| config.code.clone());
        let initial_code = session
            .as_ref()
            .map(|s| s.code.clone())
            .or(stored_code)
            .unwrap_or_else(code::generate_ephemeral);
        if config.code != initial_code {
            config.code = initial_code.clone();
            save_config(&paths.config, &config)?;
        }
        let self_signed = net::self_signed("peerway")?;
        let endpoint_v4 = net::bind_peer_endpoint("0.0.0.0:0".parse()?, &self_signed)?;
        let port_v4 = endpoint_v4.local_addr()?.port();
        let endpoint_v6 = match net::bind_peer_endpoint("[::]:0".parse()?, &self_signed) {
            Ok(endpoint) => Some(endpoint),
            Err(error) => {
                tracing::warn!(?error, "IPv6 no disponible para conexiones directas");
                None
            }
        };
        let port_v6 = endpoint_v6
            .as_ref()
            .and_then(|e| e.local_addr().ok())
            .map(|a| a.port())
            .unwrap_or(0);
        let lan_ips = local_lan_ips();
        let record = PeerRecord::new(
            &identity,
            initial_code.clone(),
            port_v4,
            port_v6,
            lan_ips,
            config.device_name.clone(),
        )?;
        let http = ApiClient::new(
            &config.server_url,
            config.insecure_tls,
            identity.clone(),
            config.install_id.clone(),
        )?;
        let (events, _) = broadcast::channel(512);

        let node = Arc::new(Self {
            identity,
            paths: paths.clone(),
            config: RwLock::new(config),
            session: RwLock::new(session),
            code: RwLock::new(initial_code),
            record: RwLock::new(record),
            endpoint_v4,
            endpoint_v6,
            port_v4,
            port_v6,
            observed_v4: RwLock::new(None),
            observed_v6: RwLock::new(None),
            lan_peers: RwLock::new(HashMap::new()),
            transfers: RwLock::new(HashMap::new()),
            shares: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            events,
            http,
            signal_notify: Notify::new(),
            announce_notify: Notify::new(),
            code_lock: tokio::sync::Mutex::new(()),
            server_online: AtomicBool::new(false),
            code_epoch: AtomicU64::new(0),
            last_error: RwLock::new(None),
            upload_speed: RwLock::new(None),
            started: AtomicBool::new(false),
            ui_visible: AtomicBool::new(true),
        });
        node.load_history();
        cleanup_staging(&node.paths.home);
        cleanup_partials(&node.download_dir());
        Ok(node)
    }

    pub fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let v4 = self.endpoint_v4.clone();
        let node = self.clone();
        tokio::spawn(async move { node.accept_loop(v4, 4).await });

        if let Some(v6) = self.endpoint_v6.clone() {
            let node = self.clone();
            tokio::spawn(async move { node.accept_loop(v6, 6).await });
        }

        crate::upnp::spawn(self.port_v4);

        if self.config.read().unwrap().lan_discovery {
            crate::lan::spawn(self.clone());
        }
        crate::signal::spawn(self.clone());
        crate::rendezvous::spawn(self.clone(), 4);
        crate::rendezvous::spawn(self.clone(), 6);
        let node = self.clone();
        tokio::spawn(async move {
            if let Err(error) = crate::share::serve(node, crate::share::SHARE_PORT).await {
                tracing::warn!(?error, "no se pudieron abrir los enlaces web temporales");
            }
        });

        let node = self.clone();
        tokio::spawn(async move { node.announce_loop().await });
        let node = self.clone();
        tokio::spawn(async move { node.session_refresh_loop().await });
        let node = self.clone();
        tokio::spawn(async move { node.telemetry_loop().await });
    }

    async fn accept_loop(self: Arc<Self>, endpoint: Endpoint, family: u8) {
        while let Some(incoming) = endpoint.accept().await {
            let node = self.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        let remote = connection.remote_address();
                        tracing::debug!(%remote, family, "conexión entrante");
                        node.handle_connection(connection).await;
                    }
                    Err(error) => tracing::debug!(?error, "handshake entrante falló"),
                }
            });
        }
    }

    async fn handle_connection(self: Arc<Self>, connection: quinn::Connection) {
        let remote = connection.remote_address();
        loop {
            match tokio::time::timeout(Duration::from_secs(120), connection.accept_bi()).await {
                Ok(Ok((send, recv))) => {
                    let node = self.clone();
                    tokio::spawn(async move {
                        if let Err(error) = node.handle_incoming_stream(send, recv, remote).await {
                            tracing::debug!(?error, "transferencia entrante terminó con error");
                        }
                    });
                }
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }
    }

    pub fn config(&self) -> NodeConfig {
        self.config.read().unwrap().clone()
    }

    pub fn code(&self) -> String {
        self.code.read().unwrap().clone()
    }

    pub fn formatted_code(&self) -> String {
        code::format(&self.code())
    }

    pub fn session(&self) -> Option<SessionData> {
        self.session.read().unwrap().clone()
    }

    pub async fn measure_upload(&self) -> Result<UploadSpeed, ClientError> {
        let speed = self.http.measure_upload().await?;
        *self.upload_speed.write().unwrap() = Some(speed.clone());
        self.emit(UiEvent::Changed);
        Ok(speed)
    }

    pub fn upload_speed(&self) -> Option<UploadSpeed> {
        self.upload_speed.read().unwrap().clone()
    }

    pub fn record(&self) -> PeerRecord {
        self.record.read().unwrap().clone()
    }

    /// Record announced on the LAN: the permanent code is replaced by a stable
    /// per-device alias, so a shared network never learns the real code.
    pub fn lan_record(&self) -> Result<PeerRecord> {
        let mut record = self.refresh_record()?;
        record.code = code::lan_alias(&self.identity.ed25519_public_hex());
        record.resign(&self.identity)?;
        Ok(record)
    }

    pub fn ui_visible(&self) -> bool {
        self.ui_visible.load(Ordering::Relaxed)
    }

    pub fn set_ui_visible(&self, visible: bool) {
        self.ui_visible.store(visible, Ordering::Relaxed);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UiEvent> {
        self.events.subscribe()
    }

    pub fn emit(&self, event: UiEvent) {
        let _ = self.events.send(event);
    }

    pub fn server_online(&self) -> bool {
        self.server_online.load(Ordering::Relaxed)
    }

    pub fn mark_server_online(&self, online: bool) {
        let was = self.server_online.swap(online, Ordering::Relaxed);
        if was != online {
            self.emit(UiEvent::Changed);
        }
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.read().unwrap().clone()
    }

    pub fn set_last_error(&self, message: Option<String>) {
        let changed = {
            let mut slot = self.last_error.write().unwrap();
            let changed = slot.as_deref() != message.as_deref();
            *slot = message;
            changed
        };
        if changed {
            self.emit(UiEvent::Changed);
        }
    }

    pub fn observed(&self) -> (Option<SocketAddr>, Option<SocketAddr>) {
        (
            *self.observed_v4.read().unwrap(),
            *self.observed_v6.read().unwrap(),
        )
    }

    pub fn set_observed(&self, v4: Option<SocketAddr>, v6: Option<SocketAddr>) {
        let mut changed = false;
        {
            let mut current = self.observed_v4.write().unwrap();
            if *current != v4 {
                *current = v4;
                changed = true;
            }
        }
        {
            let mut current = self.observed_v6.write().unwrap();
            if *current != v6 {
                *current = v6;
                changed = true;
            }
        }
        if changed {
            self.emit(UiEvent::Changed);
        }
    }

    pub fn refresh_record(&self) -> Result<PeerRecord> {
        let code = self.code();
        let config = self.config();
        let record = PeerRecord::new(
            &self.identity,
            code,
            self.port_v4,
            self.port_v6,
            local_lan_ips(),
            config.device_name,
        )?;
        *self.record.write().unwrap() = record.clone();
        Ok(record)
    }

    fn set_code(&self, new_code: String) -> Result<()> {
        let valid = code::is_valid(&new_code);
        if !valid {
            anyhow::bail!("código inválido");
        }
        *self.code.write().unwrap() = new_code.clone();
        {
            let mut config = self.config.write().unwrap();
            if config.code != new_code {
                config.code = new_code.clone();
                save_config(&self.paths.config, &config)?;
            }
        }
        self.code_epoch.fetch_add(1, Ordering::SeqCst);
        self.refresh_record()?;
        self.emit(UiEvent::Code {
            formatted: code::format(&new_code),
            code: new_code,
        });
        self.emit(UiEvent::Changed);
        self.signal_notify.notify_waiters();
        self.announce_notify.notify_waiters();
        Ok(())
    }

    /// Rotate the temporary code. Signed-in devices use their account code, so
    /// only temporary mode can rotate locally.
    pub fn rotate_code(&self) -> Result<String> {
        if self.session.read().unwrap().is_some() {
            anyhow::bail!("el código de la cuenta se cambia desde la cuenta");
        }
        let new_code = code::generate_ephemeral();
        self.set_code(new_code.clone())?;
        self.announce_notify.notify_waiters();
        Ok(code::format(&new_code))
    }

    /// Create a temporary LAN download link. Files are served from this
    /// computer until the expiry time; nothing is uploaded.
    pub fn create_share(&self, paths: Vec<PathBuf>, hours: u64) -> Result<String> {
        if paths.is_empty() {
            anyhow::bail!("selecciona al menos un archivo");
        }
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let metadata = std::fs::metadata(&path)
                .map_err(|_| anyhow::anyhow!("no se pudo leer {}", path.display()))?;
            if !metadata.is_file() {
                anyhow::bail!("{} no es un archivo", path.display());
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "archivo".into());
            files.push(crate::share::SharedFile {
                path,
                name,
                size: metadata.len(),
            });
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        let hours = hours.clamp(1, 168);
        self.shares.lock().unwrap().insert(
            token.clone(),
            crate::share::Share {
                files,
                expires_at: chrono::Utc::now().timestamp() + (hours as i64) * 3600,
            },
        );
        Ok(token)
    }

    pub fn lan_peers(&self) -> Vec<LanPeer> {
        self.lan_peers.read().unwrap().values().cloned().collect()
    }

    pub(crate) fn lan_upsert(&self, peer: LanPeer) {
        let mut peers = self.lan_peers.write().unwrap();
        let changed = match peers.get(&peer.record.code) {
            Some(existing) => {
                existing.addr != peer.addr
                    || existing.record.device_name != peer.record.device_name
                    || existing.record.ts != peer.record.ts
            }
            None => true,
        };
        peers.insert(peer.record.code.clone(), peer);
        drop(peers);
        if changed {
            self.emit(UiEvent::Changed);
        }
    }

    pub(crate) fn lan_cleanup(&self) {
        let mut peers = self.lan_peers.write().unwrap();
        let before = peers.len();
        peers.retain(|_, p| p.last_seen.elapsed() < Duration::from_secs(20));
        if peers.len() != before {
            drop(peers);
            self.emit(UiEvent::Changed);
        }
    }

    pub async fn announce_loop(self: Arc<Self>) {
        loop {
            self.announce_once().await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(40)) => {}
                _ = self.announce_notify.notified() => {}
            }
        }
    }

    pub async fn announce_once(&self) {
        let record = match self.refresh_record() {
            Ok(record) => record,
            Err(error) => {
                tracing::warn!(?error, "no se pudo firmar el registro");
                return;
            }
        };
        match self.http.announce(&record).await {
            Ok(response) => {
                self.mark_server_online(true);
                self.set_last_error(None);
                let v4 = response
                    .observed_v4
                    .and_then(|s| s.parse::<SocketAddr>().ok());
                let v6 = response
                    .observed_v6
                    .and_then(|s| s.parse::<SocketAddr>().ok());
                if v4.is_some() || v6.is_some() {
                    let (current_v4, current_v6) = self.observed();
                    self.set_observed(v4.or(current_v4), v6.or(current_v6));
                }
            }
            Err(error) => {
                self.mark_server_online(false);
                if error.status == Some(409) || error.status == Some(403) {
                    self.handle_code_conflict(&error).await;
                } else {
                    self.set_last_error(Some(error.message));
                }
            }
        }
    }

    pub(crate) async fn handle_code_conflict(&self, error: &ClientError) {
        let epoch = self.code_epoch.load(Ordering::SeqCst);
        let _guard = self.code_lock.lock().await;
        if self.code_epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        tracing::warn!(message = %error.message, "conflicto de código");
        if self.session.read().unwrap().is_some() {
            *self.session.write().unwrap() = None;
            let _ = save_session(&self.paths.session, None);
            self.emit(UiEvent::Toast {
                level: "warning".into(),
                message: "La cuenta se movió a otro dispositivo. Se generó un código temporal."
                    .into(),
            });
        } else {
            self.emit(UiEvent::Toast {
                level: "warning".into(),
                message: "Tu código temporal estaba ocupado. Se generó uno nuevo.".into(),
            });
        }
        let new_code = code::generate_ephemeral();
        if let Err(error) = self.set_code(new_code) {
            tracing::error!(?error, "no se pudo rotar el código");
        }
    }

    async fn session_refresh_loop(self: Arc<Self>) {
        self.refresh_session().await;
        loop {
            tokio::time::sleep(Duration::from_secs(120)).await;
            self.refresh_session().await;
        }
    }

    async fn telemetry_loop(self: Arc<Self>) {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut last_sent: Option<Instant> = None;
        loop {
            let config = self.config();
            let due = last_sent
                .map(|sent| sent.elapsed() >= Duration::from_secs(20 * 3600))
                .unwrap_or(true);
            if config.telemetry_enabled && due {
                let counts = self.telemetry_counts();
                let session = self.session();
                let plan = session
                    .as_ref()
                    .map(|session| session.plan.as_str())
                    .unwrap_or("free");
                let cloud_enabled = session.is_some();
                match self
                    .http
                    .telemetry_heartbeat(
                        peerway_core::APP_VERSION,
                        std::env::consts::OS,
                        plan,
                        cloud_enabled,
                        counts.completed,
                        counts.failed,
                        counts.relay,
                        &counts.methods,
                        &crate::upnp::mapping_method(),
                    )
                    .await
                {
                    Ok(()) => {
                        last_sent = Some(Instant::now());
                        tracing::debug!("telemetría anónima enviada");
                    }
                    Err(error) => tracing::debug!(?error, "no se pudo enviar telemetría"),
                }
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }

    fn telemetry_counts(&self) -> TelemetryCounts {
        let transfers = self.transfers.read().unwrap();
        let completed = transfers
            .values()
            .filter(|transfer| transfer.status == "completed")
            .count() as u64;
        let failed = transfers
            .values()
            .filter(|transfer| transfer.status == "failed")
            .count() as u64;
        let relay = transfers
            .values()
            .filter(|transfer| {
                transfer
                    .channel
                    .as_deref()
                    .map(|channel| channel.contains("Relay"))
                    .unwrap_or(false)
            })
            .count() as u64;
        let mut methods: HashMap<String, u64> = HashMap::new();
        for transfer in transfers
            .values()
            .filter(|transfer| transfer.status == "completed")
        {
            if let Some(method) = &transfer.method {
                *methods.entry(method.clone()).or_default() += 1;
            }
        }
        TelemetryCounts {
            completed,
            failed,
            relay,
            methods,
        }
    }

    pub async fn refresh_session(&self) {
        let Some(session) = self.session() else {
            return;
        };
        match self.http.me(&session.token).await {
            Ok(me) => self.apply_me(me),
            Err(error) => {
                if error.status == Some(401) {
                    tracing::warn!("sesión expirada; volviendo a modo temporal");
                    *self.session.write().unwrap() = None;
                    let _ = save_session(&self.paths.session, None);
                    // Keep the code: it is only rotated on purpose or on a
                    // real conflict, so people who have it can still reach us.
                    self.emit(UiEvent::Toast {
                        level: "warning".into(),
                        message: "Tu sesión expiró. Inicia sesión de nuevo.".into(),
                    });
                }
            }
        }
    }

    pub(crate) fn apply_me(&self, me: MeResponse) {
        let token = self
            .session
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.token.clone())
            .unwrap_or_default();
        *self.session.write().unwrap() = Some(SessionData {
            token,
            username: me.username,
            display_name: me.display_name,
            email: me.email,
            email_verified: me.email_verified,
            code: me.code,
            plan: me.plan,
            plan_expires: me.plan_expires,
            quota_bytes: me.quota_bytes,
            used_bytes: me.used_bytes,
        });
        self.emit(UiEvent::Changed);
    }

    fn apply_auth(&self, auth: AuthResponse) -> Result<()> {
        let session = SessionData {
            token: auth.token,
            username: auth.username,
            display_name: auth.display_name,
            email: auth.email,
            email_verified: auth.email_verified,
            code: auth.code.clone(),
            plan: auth.plan,
            plan_expires: auth.plan_expires,
            quota_bytes: auth.quota_bytes,
            used_bytes: auth.used_bytes,
        };
        save_session(&self.paths.session, Some(&session))?;
        *self.session.write().unwrap() = Some(session);
        self.set_code(auth.code)?;
        self.emit(UiEvent::Toast {
            level: "success".into(),
            message: "Sesión iniciada. Tu código ya es permanente.".into(),
        });
        Ok(())
    }

    pub async fn register(&self, username: &str, password: &str, email: &str) -> Result<()> {
        let config = self.config();
        let auth = self
            .http
            .register(
                username.trim(),
                password,
                &config.device_name,
                &self.identity.ed25519_public_hex(),
                &self.identity.x25519_public_hex(),
                email,
            )
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?;
        self.apply_auth(auth)?;
        self.announce_once().await;
        Ok(())
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<()> {
        let config = self.config();
        let auth = self
            .http
            .login(
                username.trim(),
                password,
                &config.device_name,
                &self.identity.ed25519_public_hex(),
                &self.identity.x25519_public_hex(),
            )
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?;
        self.apply_auth(auth)?;
        self.announce_once().await;
        Ok(())
    }

    pub async fn logout(&self) -> Result<()> {
        let token = self.session().map(|s| s.token);
        if let Some(token) = token {
            let _ = self.http.logout(&token).await;
        }
        *self.session.write().unwrap() = None;
        save_session(&self.paths.session, None)?;
        self.set_code(code::generate_ephemeral())?;
        self.announce_once().await;
        self.emit(UiEvent::Toast {
            level: "info".into(),
            message: "Sesión cerrada. Ahora tienes un código temporal.".into(),
        });
        Ok(())
    }

    pub fn update_config(&self, new_config: NodeConfig) -> Result<()> {
        let mut new_config = new_config;
        new_config.normalize();
        save_config(&self.paths.config, &new_config)?;
        let requires_restart = {
            let old = self.config.read().unwrap();
            old.server_url != new_config.server_url
                || old.insecure_tls != new_config.insecure_tls
                || old.rendezvous_port != new_config.rendezvous_port
                || old.lan_discovery != new_config.lan_discovery
        };
        *self.config.write().unwrap() = new_config.clone();
        if requires_restart {
            self.emit(UiEvent::Toast {
                level: "info".into(),
                message: "Reinicia PeerWay para aplicar los cambios de red/servidor.".into(),
            });
        }
        self.refresh_record().ok();
        self.announce_notify.notify_waiters();
        self.signal_notify.notify_waiters();
        self.emit(UiEvent::Changed);
        Ok(())
    }

    pub fn trust_code(&self, input: &str, trusted: bool) -> Result<()> {
        let normalized =
            code::normalize(input).ok_or_else(|| anyhow::anyhow!("código inválido"))?;
        if normalized == self.code() {
            anyhow::bail!("no puedes confiar en tu propio código");
        }
        let mut config = self.config();
        config.trusted_codes.retain(|c| c != &normalized);
        if trusted {
            config.trusted_codes.push(normalized);
        }
        config.normalize();
        save_config(&self.paths.config, &config)?;
        *self.config.write().unwrap() = config;
        self.emit(UiEvent::Changed);
        Ok(())
    }

    pub fn transfers(&self) -> Vec<TransferView> {
        let mut list: Vec<_> = self.transfers.read().unwrap().values().cloned().collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    pub fn transfer(&self, id: &str) -> Option<TransferView> {
        self.transfers.read().unwrap().get(id).cloned()
    }

    pub(crate) fn upsert_transfer(&self, view: TransferView) {
        self.transfers
            .write()
            .unwrap()
            .insert(view.id.clone(), view.clone());
        self.emit(UiEvent::Transfer {
            transfer: view.clone(),
        });
        self.emit(UiEvent::Changed);
    }

    pub(crate) fn update_transfer<F>(&self, id: &str, f: F) -> Option<TransferView>
    where
        F: FnOnce(&mut TransferView),
    {
        let updated = {
            let mut transfers = self.transfers.write().unwrap();
            let view = transfers.get_mut(id)?;
            f(view);
            view.clone()
        };
        self.emit(UiEvent::Transfer {
            transfer: updated.clone(),
        });
        Some(updated)
    }

    pub(crate) fn emit_progress(&self, id: &str, bytes_done: u64) {
        let view = {
            let mut transfers = self.transfers.write().unwrap();
            let Some(view) = transfers.get_mut(id) else {
                return;
            };
            view.bytes_done = bytes_done;
            view.clone()
        };
        self.emit(UiEvent::Transfer { transfer: view });
    }

    pub(crate) fn cancel_token(&self, id: &str) -> CancellationToken {
        let mut cancels = self.cancels.lock().unwrap();
        cancels
            .entry(id.to_string())
            .or_insert_with(CancellationToken::new)
            .clone()
    }

    pub(crate) fn finish_cancel(&self, id: &str) {
        self.cancels.lock().unwrap().remove(id);
    }

    pub fn accept_transfer(&self, id: &str, remember: bool) -> Result<()> {
        let pending = self.pending.lock().unwrap().remove(id);
        if let Some(pending) = pending {
            if remember {
                if let Some(view) = self.transfer(id) {
                    let _ = self.trust_code(&view.peer_code, true);
                }
            }
            let _ = pending.tx.send(true);
            self.update_transfer(id, |view| view.status = "accepted".into());
            Ok(())
        } else {
            anyhow::bail!("no hay una transferencia pendiente con ese id")
        }
    }

    pub fn decline_transfer(&self, id: &str) -> Result<()> {
        let pending = self.pending.lock().unwrap().remove(id);
        if let Some(pending) = pending {
            let _ = pending.tx.send(false);
            self.update_transfer(id, |view| {
                view.status = "declined".into();
                view.finished_at = Some(chrono::Utc::now().timestamp());
            });
            self.save_history();
            Ok(())
        } else {
            anyhow::bail!("no hay una transferencia pendiente con ese id")
        }
    }

    pub fn cancel_transfer(&self, id: &str) -> Result<()> {
        if let Some(token) = self.cancels.lock().unwrap().get(id).cloned() {
            token.cancel();
        }
        let pending = self.pending.lock().unwrap().remove(id);
        if let Some(pending) = pending {
            let _ = pending.tx.send(false);
        }
        let updated = self.update_transfer(id, |view| {
            if view.is_active() {
                view.status = "canceled".into();
                view.finished_at = Some(chrono::Utc::now().timestamp());
            }
        });
        if updated.is_some() {
            self.save_history();
        }
        Ok(())
    }

    pub(crate) fn is_canceled(&self, id: &str) -> bool {
        self.cancels
            .lock()
            .unwrap()
            .get(id)
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
    }

    pub(crate) fn fail_transfer(&self, id: &str, error: impl Into<String>) {
        let message = error.into();
        tracing::warn!(%message, "transferencia fallida");
        self.update_transfer(id, |view| {
            view.status = "failed".into();
            view.error = Some(message);
            view.finished_at = Some(chrono::Utc::now().timestamp());
        });
        self.finish_cancel(id);
        self.pending.lock().unwrap().remove(id);
        self.save_history();
    }

    pub(crate) fn complete_transfer(&self, id: &str, folder: Option<String>) {
        self.update_transfer(id, |view| {
            view.status = "completed".into();
            view.bytes_done = view.total_bytes;
            view.finished_at = Some(chrono::Utc::now().timestamp());
            if folder.is_some() {
                view.folder = folder;
            }
        });
        self.finish_cancel(id);
        self.save_history();
    }

    fn load_history(&self) {
        let Ok(raw) = std::fs::read_to_string(&self.paths.history) else {
            return;
        };
        let Ok(history) = serde_json::from_str::<Vec<TransferView>>(&raw) else {
            return;
        };
        let mut transfers = self.transfers.write().unwrap();
        for view in history.into_iter().filter(|v| !v.is_active()).take(100) {
            transfers.insert(view.id.clone(), view);
        }
    }

    pub(crate) fn clear_history(&self) {
        self.transfers
            .write()
            .unwrap()
            .retain(|_, view| view.is_active());
        self.save_history();
        self.emit(UiEvent::Changed);
    }

    pub(crate) fn save_history(&self) {
        let mut list: Vec<_> = self
            .transfers
            .read()
            .unwrap()
            .values()
            .filter(|v| !v.is_active())
            .cloned()
            .collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list.truncate(100);
        if let Ok(raw) = serde_json::to_vec_pretty(&list) {
            let tmp = self.paths.history.with_extension("tmp");
            if std::fs::write(&tmp, raw).is_ok() {
                let _ = std::fs::rename(&tmp, &self.paths.history);
            }
        }
    }

    pub fn download_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(self.config.read().unwrap().download_dir.clone())
    }

    pub fn open_download_dir(&self) -> Result<()> {
        let dir = self.download_dir();
        std::fs::create_dir_all(&dir)?;
        open::that(dir).map_err(|e| anyhow::anyhow!("no se pudo abrir la carpeta: {e}"))?;
        Ok(())
    }

    pub fn open_path(&self, path: &Path) -> Result<()> {
        let canonical_root = self
            .download_dir()
            .canonicalize()
            .unwrap_or_else(|_| self.download_dir());
        let Ok(canonical) = path.canonicalize() else {
            anyhow::bail!("la ruta no existe");
        };
        if !canonical.starts_with(&canonical_root) {
            anyhow::bail!("ruta fuera de la carpeta de descargas");
        }
        open::that(canonical).map_err(|e| anyhow::anyhow!("no se pudo abrir: {e}"))?;
        Ok(())
    }
}

fn older_than(path: &std::path::Path, days: u64) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    modified
        .elapsed()
        .map(|age| age.as_secs() > days * 86_400)
        .unwrap_or(false)
}

fn cleanup_staging(home: &std::path::Path) {
    let staging = home.join("staging");
    let Ok(entries) = std::fs::read_dir(&staging) else {
        return;
    };
    for entry in entries.flatten() {
        if older_than(&entry.path(), 7) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn cleanup_partials(download_dir: &std::path::Path) {
    let incoming = download_dir.join(".peerway-incoming");
    let Ok(entries) = std::fs::read_dir(&incoming) else {
        return;
    };
    for entry in entries.flatten() {
        if older_than(&entry.path(), 7) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn local_lan_ips() -> Vec<String> {
    let mut out: Vec<String> = net::local_ips()
        .into_iter()
        .filter(|ip| match ip {
            IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified(),
            IpAddr::V6(v6) => {
                !v6.is_loopback()
                    && !v6.is_unspecified()
                    && !v6.is_multicast()
                    && !v6.is_unicast_link_local()
            }
        })
        .map(|ip| ip.to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

pub fn files_view(files: &[(String, u64)]) -> (Vec<FileView>, u64) {
    let mut total = 0u64;
    let views = files
        .iter()
        .map(|(name, size)| {
            total += size;
            FileView {
                name: name.clone(),
                size: *size,
            }
        })
        .collect();
    (views, total)
}
