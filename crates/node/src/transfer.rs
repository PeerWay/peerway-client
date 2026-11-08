use crate::node::{files_view, Node};
use crate::state::{FileView, TransferView, UiEvent};
use anyhow::{bail, Context, Result};
use peerway_core::code;
use peerway_core::crypto::SecureChannel;
use peerway_core::protocol::{Control, FileMeta, Frame, Offer, MAX_CHUNK_BYTES};
use peerway_core::record::PeerRecord;
use peerway_core::wire::Channel;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const MAX_FILES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024 * 1024;
const OFFER_TIMEOUT: Duration = Duration::from_secs(600);

struct Target {
    record: PeerRecord,
    lan_addrs: Vec<SocketAddr>,
    observed_v4: Option<SocketAddr>,
    observed_v6: Option<SocketAddr>,
    online: bool,
}

/// How a transfer reached the peer. `key` is the stable analytics slug; the
/// label is what the UI shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    LanV4,
    LanV6,
    DirectV4,
    DirectV6,
    HolePunch,
    Relay,
}

impl Method {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Method::LanV4 | Method::LanV6 => "LAN",
            Method::DirectV4 => "P2P directo",
            Method::DirectV6 => "IPv6 directo",
            Method::HolePunch => "NAT traversal",
            Method::Relay => "Relay HTTPS",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Method::LanV4 => "lan_v4",
            Method::LanV6 => "lan_v6",
            Method::DirectV4 => "direct_v4",
            Method::DirectV6 => "direct_v6",
            Method::HolePunch => "hole_punch",
            Method::Relay => "relay",
        }
    }

    fn from_remote(remote: SocketAddr) -> Method {
        let lan = remote.ip().is_loopback() || is_private(remote.ip());
        match (remote.is_ipv6(), lan) {
            (true, true) => Method::LanV6,
            (false, true) => Method::LanV4,
            (true, false) => Method::DirectV6,
            (false, false) => Method::DirectV4,
        }
    }

    fn timeout(self) -> u64 {
        match self {
            Method::LanV4 | Method::LanV6 => 2,
            Method::HolePunch => 7,
            _ => 4,
        }
    }
}

struct Established {
    secure: SecureChannel,
    method: Method,
    _connection: Option<quinn::Connection>,
}

impl Node {
    async fn lookup_target(&self, input: &str) -> Result<Target> {
        let normalized = code::normalize(input).context("código de destino inválido")?;
        if normalized == self.code() {
            bail!("no puedes enviarte archivos a ti mismo");
        }

        let lan_peer = self
            .lan_peers()
            .into_iter()
            .find(|p| p.record.code == normalized);
        let mut lan_addrs: Vec<SocketAddr> = Vec::new();
        if let Some(peer) = &lan_peer {
            lan_addrs.push(peer.addr);
            lan_addrs.extend(peer.record.lan_endpoints());
        }

        let server_peer = if lan_peer.is_some() {
            // LAN-first: don't make an offline LAN transfer wait for a dead
            // coordination server.
            tokio::time::timeout(Duration::from_secs(2), self.http.peer(&normalized))
                .await
                .ok()
                .and_then(|result| result.ok())
                .flatten()
        } else {
            self.http.peer(&normalized).await.ok().flatten()
        };

        let mut observed_v4 = None;
        let mut observed_v6 = None;
        let mut online = false;
        let mut server_record = None;
        if let Some(peer) = server_peer {
            peer.record
                .verify()
                .context("el registro del destino no es válido")?;
            if peer.record.code != normalized {
                bail!("el servidor devolvió un registro con otro código");
            }
            observed_v4 = peer.observed_v4.and_then(|s| s.parse().ok());
            observed_v6 = peer.observed_v6.and_then(|s| s.parse().ok());
            online = peer.online;
            server_record = Some(peer.record);
        }

        // If both LAN and server know the code but with different identities,
        // the server binding wins and LAN addresses are discarded.
        let (record, mut lan_addrs) = match (server_record, lan_peer) {
            (Some(server), Some(lan)) if server.ed25519_pub == lan.record.ed25519_pub => {
                lan_addrs.extend(server.lan_endpoints());
                (server, lan_addrs)
            }
            (Some(server), _) => (server.clone(), server.lan_endpoints()),
            (None, Some(lan)) => (lan.record, lan_addrs),
            (None, None) => {
                bail!("no se encontró ese código. Verifica que PeerWay esté abierto en el otro equipo")
            }
        };
        lan_addrs.sort();
        lan_addrs.dedup();
        Ok(Target {
            record,
            lan_addrs,
            observed_v4,
            observed_v6,
            online,
        })
    }

    pub async fn send_files(
        self: &Arc<Self>,
        target: &str,
        paths: Vec<PathBuf>,
        comment: String,
    ) -> Result<String> {
        if paths.is_empty() {
            bail!("selecciona al menos un archivo");
        }
        if paths.len() > MAX_FILES {
            bail!("demasiados archivos en una sola transferencia");
        }
        // Optional note for the receiver; keep it short and flat.
        let comment: String = comment.trim().chars().take(200).collect();
        let mut local: Vec<(PathBuf, FileMeta)> = Vec::with_capacity(paths.len());
        let mut total = 0u64;
        for path in paths {
            let metadata = tokio::fs::metadata(&path)
                .await
                .with_context(|| format!("no se pudo leer {}", path.display()))?;
            if !metadata.is_file() {
                bail!("{} no es un archivo", path.display());
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "archivo".into());
            total = total.saturating_add(metadata.len());
            if total > MAX_TOTAL_BYTES {
                bail!("la transferencia supera el límite de 4 TiB");
            }
            local.push((
                path,
                FileMeta {
                    name,
                    size: metadata.len(),
                },
            ));
        }

        let target = self.lookup_target(target).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let (files, total_bytes) = files_view(
            &local
                .iter()
                .map(|(_, m)| (m.name.clone(), m.size))
                .collect::<Vec<_>>(),
        );
        let mut view = TransferView::new(
            id.clone(),
            "send",
            target.record.code.clone(),
            target.record.device_name.clone(),
            files,
            total_bytes,
        );
        if !comment.is_empty() {
            view.comment = Some(comment.clone());
        }
        self.upsert_transfer(view);
        let _ = self.cancel_token(&id);

        let node = self.clone();
        let transfer_id = id.clone();
        tokio::spawn(async move {
            if let Err(error) = node
                .run_send(transfer_id.clone(), target, local, comment)
                .await
            {
                if !node.is_canceled(&transfer_id) {
                    node.fail_transfer(&transfer_id, format!("{error:#}"));
                } else {
                    node.finish_cancel(&transfer_id);
                }
            }
        });
        Ok(id)
    }

    async fn run_send(
        self: &Arc<Self>,
        id: String,
        target: Target,
        local: Vec<(PathBuf, FileMeta)>,
        comment: String,
    ) -> Result<()> {
        let files: Vec<FileMeta> = local.iter().map(|(_, m)| m.clone()).collect();
        let total: u64 = files.iter().map(|f| f.size).sum();
        let mut established = self.establish(&target, &id).await?;
        if self.is_canceled(&id) {
            return Ok(());
        }
        self.update_transfer(&id, |view| {
            view.channel = Some(established.method.label().to_string());
            view.method = Some(established.method.key().to_string());
            view.status = "waiting_accept".into();
        });

        let offer = Control::Offer {
            transfer_id: id.clone(),
            sender_record: Box::new(self.refresh_record()?),
            files: files.clone(),
            total_bytes: total,
            comment: comment.clone(),
        };
        established.secure.send(&Frame::Control(offer)).await?;

        let decision = tokio::time::timeout(OFFER_TIMEOUT, established.secure.recv())
            .await
            .map_err(|_| anyhow::anyhow!("el receptor no respondió a tiempo"))??;
        match decision {
            Frame::Control(Control::Accept { transfer_id }) if transfer_id == id => {
                self.update_transfer(&id, |view| {
                    view.status = "transferring".into();
                });
            }
            Frame::Control(Control::Reject { reason, .. }) => {
                self.update_transfer(&id, |view| {
                    view.status = "declined".into();
                    view.error = Some(reason.clone());
                    view.finished_at = Some(chrono::Utc::now().timestamp());
                });
                self.finish_cancel(&id);
                self.save_history();
                return Ok(());
            }
            Frame::Control(Control::Error { message }) => bail!("{message}"),
            _ => bail!("respuesta inesperada del receptor"),
        }

        let offsets = match established.secure.recv().await? {
            Frame::Control(Control::Resume { offsets, .. }) => offsets,
            Frame::Control(Control::Error { message }) => bail!("{message}"),
            _ => bail!("el receptor no envió el estado de reanudación"),
        };
        if let Err(error) = self
            .send_payload(&id, &mut established.secure, &local, &offsets, total)
            .await
        {
            let _ = established
                .secure
                .send(&Frame::Control(Control::Error {
                    message: format!("{error:#}"),
                }))
                .await;
            return Err(error);
        }
        if self.is_canceled(&id) {
            return Ok(());
        }
        // Wait for the receiver to fsync and verify the last file before
        // claiming success.
        let confirmation = tokio::time::timeout(Duration::from_secs(60), established.secure.recv())
            .await
            .map_err(|_| anyhow::anyhow!("el receptor no confirmó la recepción"))??;
        match confirmation {
            Frame::Control(Control::Done { transfer_id }) if transfer_id == id => {}
            Frame::Control(Control::Error { message }) => bail!("{message}"),
            _ => bail!("confirmación inesperada del receptor"),
        }
        established.secure.close().await;
        self.complete_transfer(&id, None);
        Ok(())
    }

    async fn establish(&self, target: &Target, transfer_id: &str) -> Result<Established> {
        let force_relay = self.config.read().unwrap().force_relay;
        let mut tried = HashSet::new();
        let mut candidates: Vec<(SocketAddr, Method)> = Vec::new();
        if !force_relay {
            for addr in target.lan_addrs.iter().take(6) {
                if tried.insert(*addr) {
                    let method = if addr.is_ipv6() {
                        Method::LanV6
                    } else {
                        Method::LanV4
                    };
                    candidates.push((*addr, method));
                }
            }
            if let Some(addr) = target.observed_v6 {
                if tried.insert(addr) {
                    candidates.push((addr, Method::DirectV6));
                }
            }
            if let Some(addr) = target.observed_v4 {
                if tried.insert(addr) {
                    candidates.push((addr, Method::DirectV4));
                }
            }
        }

        for (addr, method) in candidates {
            match self.try_direct(addr, &target.record, method).await {
                Ok(established) => return Ok(established),
                Err(error) => {
                    tracing::debug!(%addr, label = method.label(), ?error, "intento directo falló")
                }
            }
        }

        if target.online && !force_relay {
            if let Some(addr) = target.observed_v4 {
                let punch = self
                    .http
                    .punch(&target.record.code, &self.refresh_record()?)
                    .await;
                if let Err(error) = punch {
                    tracing::debug!(?error, "punch no disponible");
                }
                // Give the remote NAT mapping a moment to open.
                tokio::time::sleep(Duration::from_millis(250)).await;
                match self
                    .try_direct(addr, &target.record, Method::HolePunch)
                    .await
                {
                    Ok(established) => return Ok(established),
                    Err(error) => tracing::debug!(?error, "NAT traversal falló"),
                }
            }
        }

        if target.online {
            match self.try_relay(target, transfer_id).await {
                Ok(established) => return Ok(established),
                Err(error) => tracing::debug!(?error, "relay no disponible"),
            }
        }

        bail!(
            "no se pudo establecer una conexión con {}. Revisa que el receptor esté conectado y que el firewall permita UDP",
            code::format(&target.record.code)
        )
    }

    async fn try_direct(
        &self,
        addr: SocketAddr,
        record: &PeerRecord,
        method: Method,
    ) -> Result<Established> {
        let endpoint = if addr.is_ipv4() {
            self.endpoint_v4.clone()
        } else {
            match &self.endpoint_v6 {
                Some(endpoint) => endpoint.clone(),
                None => bail!("IPv6 no disponible"),
            }
        };
        let connection = tokio::time::timeout(
            Duration::from_secs(method.timeout()),
            endpoint.connect(addr, "peerway")?,
        )
        .await
        .map_err(|_| anyhow::anyhow!("tiempo de conexión agotado"))??;
        let (send, recv) = tokio::time::timeout(Duration::from_secs(6), connection.open_bi())
            .await
            .map_err(|_| anyhow::anyhow!("no se pudo abrir el canal"))??;
        let channel = Channel::Quic { send, recv };
        let (secure, remote_static) = tokio::time::timeout(
            Duration::from_secs(15),
            SecureChannel::initiator(channel, &self.identity.x25519_secret_bytes()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("handshake cifrado agotado"))??;
        if hex::encode(remote_static) != record.x25519_pub {
            bail!("la identidad criptográfica del destino no coincide");
        }
        Ok(Established {
            secure,
            method,
            _connection: Some(connection),
        })
    }

    async fn try_relay(&self, target: &Target, transfer_id: &str) -> Result<Established> {
        let tunnel = self
            .http
            .create_tunnel(&target.record.code, transfer_id, &self.refresh_record()?)
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?;
        let stream = tokio::time::timeout(
            Duration::from_secs(25),
            crate::relay::connect(self, &tunnel.tunnel_id, &tunnel.token),
        )
        .await
        .map_err(|_| anyhow::anyhow!("el relay no se emparejó a tiempo"))??;
        let (secure, remote_static) = tokio::time::timeout(
            Duration::from_secs(20),
            SecureChannel::initiator(
                Channel::Duplex(stream),
                &self.identity.x25519_secret_bytes(),
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("handshake por relay agotado"))??;
        if hex::encode(remote_static) != target.record.x25519_pub {
            bail!("la identidad criptográfica del destino no coincide (relay)");
        }
        Ok(Established {
            secure,
            method: Method::Relay,
            _connection: None,
        })
    }

    async fn send_payload(
        &self,
        id: &str,
        secure: &mut SecureChannel,
        local: &[(PathBuf, FileMeta)],
        offsets: &[u64],
        total: u64,
    ) -> Result<()> {
        let mut sent_before = 0u64;
        let mut last_emit = Instant::now();
        for (index, (path, meta)) in local.iter().enumerate() {
            if self.is_canceled(id) {
                return Ok(());
            }
            secure
                .send(&Frame::Control(Control::FileStart {
                    index: index as u32,
                    name: meta.name.clone(),
                    size: meta.size,
                }))
                .await?;

            let offset = offsets.get(index).copied().unwrap_or(0).min(meta.size);
            let mut file = tokio::fs::File::open(path)
                .await
                .with_context(|| format!("abriendo {}", path.display()))?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; MAX_CHUNK_BYTES];
            if offset > 0 {
                file.seek(std::io::SeekFrom::Start(0)).await?;
                let mut remaining = offset;
                while remaining > 0 {
                    let want = remaining.min(buf.len() as u64) as usize;
                    let read = file.read(&mut buf[..want]).await?;
                    if read == 0 {
                        bail!("el archivo cambió mientras se enviaba");
                    }
                    hasher.update(&buf[..read]);
                    remaining -= read as u64;
                }
            }
            let mut sent = offset;
            while sent < meta.size {
                if self.is_canceled(id) {
                    return Ok(());
                }
                let want = (meta.size - sent).min(buf.len() as u64) as usize;
                let read = file.read(&mut buf[..want]).await?;
                if read == 0 {
                    bail!("el archivo se acortó durante el envío");
                }
                hasher.update(&buf[..read]);
                secure.send_data(index as u32, &buf[..read]).await?;
                sent += read as u64;
                if last_emit.elapsed() >= Duration::from_millis(120) {
                    self.emit_progress(id, sent_before + sent);
                    last_emit = Instant::now();
                }
            }
            if sent != meta.size {
                bail!("el archivo cambió de tamaño durante el envío");
            }
            let digest = hex::encode(hasher.finalize());
            secure
                .send(&Frame::Control(Control::FileEnd {
                    index: index as u32,
                    sha256: digest,
                }))
                .await?;
            sent_before += meta.size;
            self.emit_progress(id, sent_before.min(total));
        }
        secure
            .send(&Frame::Control(Control::Complete {
                transfer_id: id.to_string(),
            }))
            .await?;
        Ok(())
    }

    pub(crate) async fn handle_incoming_stream(
        self: &Arc<Self>,
        send: quinn::SendStream,
        recv: quinn::RecvStream,
        remote: SocketAddr,
    ) -> Result<()> {
        let channel = Channel::Quic { send, recv };
        let (mut secure, remote_static) = tokio::time::timeout(
            Duration::from_secs(20),
            SecureChannel::responder(channel, &self.identity.x25519_secret_bytes()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("handshake entrante agotado"))??;
        let frame = tokio::time::timeout(OFFER_TIMEOUT, secure.recv())
            .await
            .map_err(|_| anyhow::anyhow!("no llegó una oferta"))??;
        let method = Method::from_remote(remote);
        match frame {
            Frame::Control(Control::Offer {
                transfer_id,
                sender_record,
                files,
                total_bytes,
                comment,
            }) => {
                let offer = Offer {
                    transfer_id,
                    sender_record: *sender_record,
                    files,
                    total_bytes,
                    created_at: chrono::Utc::now().timestamp(),
                    comment,
                };
                self.handle_offer(&mut secure, &remote_static, offer, method, None)
                    .await
            }
            _ => bail!("se esperaba una oferta"),
        }
    }

    pub(crate) async fn handle_offer(
        self: &Arc<Self>,
        secure: &mut SecureChannel,
        remote_static: &[u8; 32],
        offer: Offer,
        method: Method,
        expected_transfer: Option<(&str, &str)>,
    ) -> Result<()> {
        if offer.sender_record.v != peerway_core::PROTOCOL_VERSION {
            let reason = format!(
                "versión de protocolo incompatible ({} frente a {}). Actualiza PeerWay en ambos equipos.",
                offer.sender_record.v,
                peerway_core::PROTOCOL_VERSION
            );
            let _ = secure
                .send(&Frame::Control(Control::Reject {
                    transfer_id: offer.transfer_id.clone(),
                    reason: reason.clone(),
                }))
                .await;
            bail!("{reason}");
        }
        offer
            .sender_record
            .verify()
            .context("la oferta tiene una firma inválida")?;
        if hex::encode(remote_static) != offer.sender_record.x25519_pub {
            bail!("la oferta no coincide con la identidad del canal cifrado");
        }
        if offer.sender_record.code == self.code() {
            bail!("una oferta no puede venir de tu propio código");
        }
        // When the coordination server is reachable, compare the claimed code
        // binding against the authoritative registry. Offline LAN transfers
        // still work (no server = no assertion).
        if self.server_online() {
            let lookup = tokio::time::timeout(
                Duration::from_secs(1),
                self.http.peer(&offer.sender_record.code),
            )
            .await;
            if let Ok(Ok(Some(peer))) = lookup {
                if peer.record.ed25519_pub != offer.sender_record.ed25519_pub {
                    let _ = secure
                        .send(&Frame::Control(Control::Reject {
                            transfer_id: offer.transfer_id.clone(),
                            reason: "el código no coincide con la identidad anunciada".into(),
                        }))
                        .await;
                    bail!("la identidad del emisor no coincide con el registro del servidor");
                }
            }
        }
        if uuid::Uuid::parse_str(&offer.transfer_id).is_err() {
            let _ = secure
                .send(&Frame::Control(Control::Reject {
                    transfer_id: offer.transfer_id.clone(),
                    reason: "id de transferencia inválido".into(),
                }))
                .await;
            bail!("id de transferencia inválido");
        }
        if let Some((transfer_id, from)) = expected_transfer {
            if transfer_id != offer.transfer_id || from != offer.sender_record.code {
                bail!("la oferta por relay no coincide con la señalización");
            }
        }
        let computed_total: u64 = offer.files.iter().map(|f| f.size).sum();
        if computed_total != offer.total_bytes
            || offer.files.is_empty()
            || offer.files.len() > MAX_FILES
            || offer.total_bytes > MAX_TOTAL_BYTES
        {
            let _ = secure
                .send(&Frame::Control(Control::Reject {
                    transfer_id: offer.transfer_id.clone(),
                    reason: "metadatos de transferencia inválidos".into(),
                }))
                .await;
            bail!("metadatos de transferencia inválidos");
        }

        let duplicate = {
            let transfers = self.transfers.read().unwrap();
            transfers
                .get(&offer.transfer_id)
                .map(|existing| existing.is_active())
        };
        if let Some(duplicate) = duplicate {
            let _ = secure
                .send(&Frame::Control(Control::Reject {
                    transfer_id: offer.transfer_id.clone(),
                    reason: if duplicate {
                        "transferencia duplicada".into()
                    } else {
                        "id de transferencia ya utilizado".into()
                    },
                }))
                .await;
            return Ok(());
        }

        let files: Vec<FileView> = offer
            .files
            .iter()
            .map(|f| FileView {
                name: f.name.clone(),
                size: f.size,
            })
            .collect();
        let mut view = TransferView::new(
            offer.transfer_id.clone(),
            "receive",
            offer.sender_record.code.clone(),
            offer.sender_record.device_name.clone(),
            files,
            offer.total_bytes,
        );
        let trusted = {
            let config = self.config();
            config.auto_accept_trusted && config.trusted_codes.contains(&offer.sender_record.code)
        };
        // Trusted senders skip the offer step entirely: if the transfer were
        // stored as "offered", the UI would show the acceptance banner anyway.
        view.status = if trusted { "transferring" } else { "offered" }.into();
        view.channel = Some(method.label().to_string());
        view.method = Some(method.key().to_string());
        // Peer-supplied text: truncate before it reaches the UI.
        let comment: String = offer.comment.chars().take(200).collect();
        if !comment.trim().is_empty() {
            view.comment = Some(comment.trim().to_string());
        }
        self.upsert_transfer(view.clone());
        let _ = self.cancel_token(&offer.transfer_id);

        if trusted {
            self.emit(UiEvent::Incoming {
                transfer: view.clone(),
            });
            self.emit(UiEvent::Toast {
                level: "info".into(),
                message: format!(
                    "Recibiendo automáticamente archivos de {} (contacto de confianza).",
                    offer.sender_record.device_name
                ),
            });
        } else {
            self.emit(UiEvent::Incoming {
                transfer: view.clone(),
            });
            self.emit(UiEvent::Toast {
                level: "info".into(),
                message: format!(
                    "{} quiere enviarte {} archivo(s)",
                    offer.sender_record.device_name,
                    offer.files.len()
                ),
            });
        }

        let accepted = if trusted {
            true
        } else {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.pending.lock().unwrap().insert(
                offer.transfer_id.clone(),
                crate::node::PendingDecision { tx },
            );
            match tokio::time::timeout(Duration::from_secs(300), rx).await {
                Ok(Ok(decision)) => decision,
                _ => {
                    self.pending.lock().unwrap().remove(&offer.transfer_id);
                    false
                }
            }
        };

        if !accepted {
            if self.is_canceled(&offer.transfer_id) {
                return Ok(());
            }
            let reason = "rechazada o sin respuesta".to_string();
            let _ = secure
                .send(&Frame::Control(Control::Reject {
                    transfer_id: offer.transfer_id.clone(),
                    reason: reason.clone(),
                }))
                .await;
            self.update_transfer(&offer.transfer_id, |view| {
                view.status = "declined".into();
                view.error = Some(reason);
                view.finished_at = Some(chrono::Utc::now().timestamp());
            });
            self.finish_cancel(&offer.transfer_id);
            self.save_history();
            return Ok(());
        }

        self.update_transfer(&offer.transfer_id, |view| {
            view.status = "transferring".into();
        });
        secure
            .send(&Frame::Control(Control::Accept {
                transfer_id: offer.transfer_id.clone(),
            }))
            .await?;

        let offsets = self.prepare_offsets(&offer).await?;
        secure
            .send(&Frame::Control(Control::Resume {
                transfer_id: offer.transfer_id.clone(),
                offsets,
            }))
            .await?;
        if let Err(error) = self.receive_payload(&offer, secure).await {
            let _ = secure
                .send(&Frame::Control(Control::Error {
                    message: format!("{error:#}"),
                }))
                .await;
            if !self.is_canceled(&offer.transfer_id) {
                self.fail_transfer(&offer.transfer_id, format!("{error:#}"));
            }
            return Err(error);
        }
        let _ = secure
            .send(&Frame::Control(Control::Done {
                transfer_id: offer.transfer_id.clone(),
            }))
            .await;
        Ok(())
    }

    async fn prepare_offsets(&self, offer: &Offer) -> Result<Vec<u64>> {
        let dir = self.part_dir(&offer.transfer_id);
        tokio::fs::create_dir_all(&dir).await?;
        let mut offsets = Vec::with_capacity(offer.files.len());
        for (index, file) in offer.files.iter().enumerate() {
            let part = dir.join(format!("{index}.part"));
            let size = tokio::fs::metadata(&part)
                .await
                .map(|m| m.len())
                .unwrap_or(0)
                .min(file.size);
            offsets.push(size);
        }
        Ok(offsets)
    }

    fn part_dir(&self, transfer_id: &str) -> PathBuf {
        self.download_dir()
            .join(".peerway-incoming")
            .join(transfer_id)
    }

    async fn receive_payload(&self, offer: &Offer, secure: &mut SecureChannel) -> Result<()> {
        let part_dir = self.part_dir(&offer.transfer_id);
        tokio::fs::create_dir_all(&part_dir).await?;
        let mut current: Option<(u32, tokio::fs::File)> = None;
        let mut completed = 0u64;
        let mut last_emit = Instant::now();

        loop {
            if self.is_canceled(&offer.transfer_id) {
                if let Some((_, file)) = current.take() {
                    let _ = file.sync_all().await;
                }
                return Ok(());
            }
            let frame = secure.recv().await?;
            match frame {
                Frame::Control(Control::FileStart { index, name, size }) => {
                    let meta = offer.files.get(index as usize).context("índice inválido")?;
                    if meta.name != name || meta.size != size {
                        bail!("el emisor cambió los metadatos del archivo");
                    }
                    if let Some((_, mut previous)) = current.take() {
                        previous.flush().await?;
                    }
                    let part = part_dir.join(format!("{index}.part"));
                    let mut file = tokio::fs::OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .write(true)
                        .open(&part)
                        .await?;
                    let existing = file.metadata().await?.len().min(size);
                    file.set_len(existing).await?;
                    file.seek(std::io::SeekFrom::Start(existing)).await?;
                    current = Some((index, file));
                }
                Frame::Data { file_index, data } => {
                    let Some((index, file)) = current.as_mut() else {
                        bail!("datos antes de file_start");
                    };
                    if *index != file_index {
                        bail!("datos para un archivo distinto");
                    }
                    file.write_all(&data).await?;
                    completed += data.len() as u64;
                    if last_emit.elapsed() >= Duration::from_millis(120) {
                        self.emit_progress(&offer.transfer_id, completed);
                        last_emit = Instant::now();
                    }
                }
                Frame::Control(Control::FileEnd { index, sha256 }) => {
                    let (current_index, mut file) =
                        current.take().context("file_end sin archivo")?;
                    if current_index != index {
                        bail!("file_end para un índice distinto");
                    }
                    file.flush().await?;
                    file.sync_all().await?;
                    drop(file);
                    let part = part_dir.join(format!("{index}.part"));
                    let actual = hash_file(&part).await?;
                    if actual != sha256 {
                        let _ = tokio::fs::remove_file(&part).await;
                        bail!("la verificación SHA-256 del archivo {index} falló");
                    }
                    let meta = &offer.files[index as usize];
                    let name = self.unique_download_path(&meta.name).await?;
                    tokio::fs::rename(&part, &name).await?;
                    completed = offer.files[..=index as usize].iter().map(|f| f.size).sum();
                    self.emit_progress(&offer.transfer_id, completed);
                }
                Frame::Control(Control::Complete { transfer_id }) => {
                    if transfer_id != offer.transfer_id {
                        bail!("complete con id incorrecto");
                    }
                    let _ = tokio::fs::remove_dir_all(&part_dir).await;
                    self.complete_transfer(
                        &offer.transfer_id,
                        Some(self.download_dir().to_string_lossy().to_string()),
                    );
                    return Ok(());
                }
                Frame::Control(Control::Error { message }) => bail!("{message}"),
                Frame::Control(Control::Ping) => {}
                _ => bail!("mensaje inesperado durante la recepción"),
            }
        }
    }

    async fn unique_download_path(&self, name: &str) -> Result<PathBuf> {
        let dir = self.download_dir();
        tokio::fs::create_dir_all(&dir).await?;
        let safe = sanitize_file_name(name);
        let candidate = dir.join(&safe);
        if !tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
            return Ok(candidate);
        }
        let path = Path::new(&safe);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "archivo".into());
        let ext = path.extension().map(|s| s.to_string_lossy().to_string());
        for n in 1..10_000 {
            let file = match &ext {
                Some(ext) => format!("{stem} ({n}).{ext}"),
                None => format!("{stem} ({n})"),
            };
            let candidate = dir.join(file);
            if !tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
                return Ok(candidate);
            }
        }
        bail!("no se pudo encontrar un nombre libre")
    }
}

async fn hash_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("archivo").trim();
    let cleaned: String = base
        .chars()
        .filter(|c| {
            !c.is_control()
                && *c != ':'
                && *c != '*'
                && *c != '?'
                && *c != '"'
                && *c != '<'
                && *c != '>'
                && *c != '|'
        })
        .take(180)
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "archivo".into()
    } else {
        cleaned
    }
}

fn is_private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_private(),
        std::net::IpAddr::V6(v6) => v6.is_unique_local(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_file_names() {
        assert_eq!(sanitize_file_name("../../secret.txt"), "secret.txt");
        assert_eq!(sanitize_file_name("..\\evil.exe"), "evil.exe");
        assert_eq!(sanitize_file_name("report:final?.pdf"), "reportfinal.pdf");
        assert_eq!(sanitize_file_name(".."), "archivo");
        assert_eq!(sanitize_file_name(""), "archivo");
    }

    #[test]
    fn transfer_ids_must_be_uuid() {
        assert!(uuid::Uuid::parse_str("274404d4-db88-46a6-b7da-a1b760c79d67").is_ok());
        assert!(uuid::Uuid::parse_str("../../etc/passwd").is_err());
    }
}
