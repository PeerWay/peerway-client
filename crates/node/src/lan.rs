//! UDP broadcast discovery for the local network.
//!
//! This is deliberately independent from the coordination server: two devices
//! on the same LAN can find each other and transfer files with no internet at
//! all.

use crate::node::Node;
use crate::state::LanPeer;
use anyhow::{Context, Result};
use peerway_core::record::LanAnnouncement;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

const MAGIC: &[u8] = b"PEERWAY1\n";

pub fn spawn(node: Arc<Node>) {
    tokio::spawn(async move {
        if let Err(error) = run(node).await {
            tracing::warn!(?error, "descubrimiento LAN desactivado");
        }
    });
}

async fn bind(port: u16) -> Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    socket.set_nonblocking(true)?;
    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket).context("udp socket")
}

async fn run(node: Arc<Node>) -> Result<()> {
    let config = node.config();
    let port = config.lan_port;
    let socket = bind(port).await?;
    let socket = Arc::new(socket);
    tracing::info!(port, "descubrimiento LAN activo");

    let mut broadcast = tokio::time::interval(Duration::from_secs(5));
    broadcast.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut cleanup = tokio::time::interval(Duration::from_secs(10));
    let mut buf = vec![0u8; 65_535];

    loop {
        tokio::select! {
            _ = broadcast.tick() => {
                if !node.ui_visible() {
                    continue;
                }
                let record = match node.lan_record() {
                    Ok(record) => record,
                    Err(error) => {
                        tracing::warn!(?error, "no se pudo firmar el registro LAN");
                        continue;
                    }
                };
                let announcement = LanAnnouncement { record };
                let Ok(json) = serde_json::to_vec(&announcement) else { continue };
                let mut payload = Vec::with_capacity(MAGIC.len() + json.len());
                payload.extend_from_slice(MAGIC);
                payload.extend_from_slice(&json);
                for addr in broadcast_targets(port) {
                    let _ = socket.send_to(&payload, addr).await;
                }
            }
            result = socket.recv_from(&mut buf) => {
                let Ok((len, source)) = result else { continue };
                if len <= MAGIC.len() || &buf[..MAGIC.len()] != MAGIC {
                    continue;
                }
                let Ok(announcement) = serde_json::from_slice::<LanAnnouncement>(&buf[MAGIC.len()..len]) else {
                    continue;
                };
                let record = announcement.record;
                if record.verify().is_err() || !record.is_fresh(180) {
                    continue;
                }
                // The announcement carries a derived alias, so compare identities
                // instead of codes to ignore our own broadcast.
                if record.ed25519_pub == node.record().ed25519_pub || record.quic_port_v4 == 0 {
                    continue;
                }
                let addr = SocketAddr::new(source.ip(), record.quic_port_v4);
                node.lan_upsert(LanPeer {
                    record,
                    addr,
                    last_seen: std::time::Instant::now(),
                });
            }
            _ = cleanup.tick() => {
                node.lan_cleanup();
            }
        }
    }
}

fn broadcast_targets(port: u16) -> Vec<SocketAddr> {
    let mut targets = vec![SocketAddr::from(([255, 255, 255, 255], port))];
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if let if_addrs::IfAddr::V4(v4) = interface.addr {
                if v4.ip.is_loopback() || v4.ip.is_link_local() {
                    continue;
                }
                let ip = u32::from(v4.ip);
                let mask = u32::from(v4.netmask);
                let broadcast = Ipv4Addr::from(ip | !mask);
                targets.push(SocketAddr::from((broadcast, port)));
            }
        }
    }
    targets
}
