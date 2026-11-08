//! Best-effort automatic port mapping for the IPv4 QUIC endpoint.
//!
//! The client tries PCP (RFC 6887), UPnP IGD and NAT-PMP (RFC 6886), renews
//! the lease automatically and removes it when the process exits.

use port_control_client::{Config, Method, PortMapping, Protocol};
use std::num::NonZeroU16;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

static MAPPING: OnceLock<Mutex<String>> = OnceLock::new();

fn state() -> &'static Mutex<String> {
    MAPPING.get_or_init(|| Mutex::new("none".into()))
}

/// Which mapping is currently in place: `pcp`, `upnp`, `natpmp` or `none`.
pub fn mapping_method() -> String {
    state().lock().unwrap().clone()
}

pub fn spawn(port_v4: u16) {
    let Some(port) = NonZeroU16::new(port_v4) else {
        return;
    };

    tokio::spawn(async move {
        let config = Config::new(Protocol::Udp, port)
            .methods([Method::Pcp, Method::Upnp, Method::NatPmp])
            .description("PeerWay QUIC")
            .lifetime(Duration::from_secs(3600))
            .retry_interval(Duration::from_secs(300));
        let mapping = PortMapping::start(config);
        let mut last_state = String::new();

        loop {
            let current = match mapping.mapping() {
                Some(mapping) => format!("{:?} {}", mapping.method, mapping.external),
                None => mapping
                    .last_error()
                    .map(|error| format!("{error:?}"))
                    .unwrap_or_default(),
            };
            if current != last_state {
                if let Some(mapping) = mapping.mapping() {
                    tracing::info!(
                        external = %mapping.external,
                        method = ?mapping.method,
                        local_port = port_v4,
                        "puerto UDP abierto automáticamente"
                    );
                } else if let Some(error) = mapping.last_error() {
                    tracing::debug!(?error, "mapeo de puerto no disponible todavía");
                }
                last_state = current;
            }
            let slug = mapping
                .mapping()
                .map(|mapping| format!("{:?}", mapping.method).to_ascii_lowercase())
                .unwrap_or_else(|| "none".into());
            {
                let mut current = state().lock().unwrap();
                if *current != slug {
                    *current = slug;
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}
