//! QUIC endpoint helpers shared by devices and the coordination server.
//!
//! Peer certificates are self-signed and intentionally *not* pinned: the
//! actual authentication happens in the Noise layer. For the rendezvous
//! channel the server is authenticated by signed application messages and the
//! endpoint is only used for address discovery.

use anyhow::{Context, Result};
use quinn::Endpoint;
use rcgen::{CertificateParams, KeyPair, SanType};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Once};
use std::time::Duration;

static INIT: Once = Once::new();

pub fn install_crypto_provider() {
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub struct SelfSigned {
    pub cert_der: CertificateDer<'static>,
    pub key_der: PrivateKeyDer<'static>,
    pub cert_pem: String,
    pub key_pem: String,
}

/// Generate a self-signed certificate for this process. The identity is not
/// carried by the certificate; it is proven by the Noise handshake.
pub fn self_signed(domain: &str) -> Result<SelfSigned> {
    let mut params = CertificateParams::default();
    params.subject_alt_names = vec![
        SanType::DnsName(domain.try_into().context("invalid domain")?),
        SanType::DnsName("localhost".try_into().context("invalid localhost")?),
        SanType::IpAddress(IpAddr::from([127, 0, 0, 1])),
        SanType::IpAddress(std::net::Ipv6Addr::LOCALHOST.into()),
    ];
    let key_pair = KeyPair::generate().context("generate tls key")?;
    let cert = params.self_signed(&key_pair).context("sign tls cert")?;
    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    Ok(SelfSigned {
        cert_der,
        key_der,
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

fn transport() -> Result<Arc<quinn::TransportConfig>> {
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(Duration::from_secs(60)).context("idle timeout")?,
    ));
    transport.keep_alive_interval(Some(Duration::from_secs(10)));
    transport.max_concurrent_bidi_streams(128u32.into());
    // Generous windows: LAN transfers should be limited by disk/CPU, not by
    // the flow control window.
    transport.stream_receive_window(quinn::VarInt::from_u32(16 * 1024 * 1024));
    transport.receive_window(quinn::VarInt::from_u32(64 * 1024 * 1024));
    Ok(Arc::new(transport))
}

pub fn quic_server_config(alpn: &[u8], cert: &SelfSigned) -> Result<quinn::ServerConfig> {
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert_der.clone()], cert.key_der.clone_key())
        .context("tls server config")?;
    tls.alpn_protocols = vec![alpn.to_vec()];
    let crypto =
        quinn::crypto::rustls::QuicServerConfig::try_from(tls).context("quic server crypto")?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    config.transport_config(transport()?);
    Ok(config)
}

#[derive(Debug)]
struct NoVerification {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn quic_client_config(alpn: &[u8]) -> Result<quinn::ClientConfig> {
    install_crypto_provider();
    let provider = rustls::crypto::ring::default_provider();
    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(provider.clone()))
        .with_safe_default_protocol_versions()
        .context("tls versions")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerification {
            provider: Arc::new(provider),
        }))
        .with_no_client_auth();
    tls.alpn_protocols = vec![alpn.to_vec()];
    let crypto =
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).context("quic client crypto")?;
    let mut config = quinn::ClientConfig::new(Arc::new(crypto));
    config.transport_config(transport()?);
    Ok(config)
}

/// TLS client config that accepts self-signed certificates. Only for
/// development / self-hosted servers; production deployments use normal
/// certificate validation in reqwest.
pub fn insecure_tls_client_config() -> Result<Arc<rustls::ClientConfig>> {
    install_crypto_provider();
    let provider = rustls::crypto::ring::default_provider();
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(provider.clone()))
        .with_safe_default_protocol_versions()
        .context("tls versions")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerification {
            provider: Arc::new(provider),
        }))
        .with_no_client_auth();
    Ok(Arc::new(tls))
}

/// Bind a dual-purpose endpoint (accepts direct peer connections and can dial
/// out) on the given local address.
pub fn bind_peer_endpoint(bind: SocketAddr, cert: &SelfSigned) -> Result<Endpoint> {
    install_crypto_provider();
    let server = quic_server_config(crate::ALPN_PEER, cert)?;
    let mut endpoint = bind_endpoint(bind, Some(server))?;
    endpoint.set_default_client_config(quic_client_config(crate::ALPN_PEER)?);
    Ok(endpoint)
}

/// Bind a QUIC endpoint with explicit socket options. IPv6 sockets are marked
/// v6-only so a wildcard IPv4 socket can coexist on the same port.
pub fn bind_endpoint(
    bind: SocketAddr,
    server_config: Option<quinn::ServerConfig>,
) -> Result<Endpoint> {
    let domain = Domain::for_address(bind);
    let socket =
        Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)).context("create udp socket")?;
    if bind.is_ipv6() {
        socket.set_only_v6(true).context("set IPV6_V6ONLY")?;
    }
    socket.bind(&bind.into()).context("bind udp socket")?;
    socket.set_nonblocking(true)?;
    let std_socket: std::net::UdpSocket = socket.into();
    let runtime = quinn::default_runtime().context("no async runtime")?;
    Endpoint::new(
        quinn::EndpointConfig::default(),
        server_config,
        std_socket,
        runtime,
    )
    .context("quinn endpoint")
}

/// Client-only endpoint, used for rendezvous with the coordination server.
pub fn bind_client_endpoint(bind: SocketAddr) -> Result<Endpoint> {
    install_crypto_provider();
    let mut endpoint = Endpoint::client(bind).context("bind quic client endpoint")?;
    endpoint.set_default_client_config(quic_client_config(crate::ALPN_RENDEZVOUS)?);
    Ok(endpoint)
}

pub fn local_ips() -> Vec<IpAddr> {
    if_addrs::get_if_addrs()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter(|i| !i.is_loopback())
                .map(|i| i.ip())
                .collect()
        })
        .unwrap_or_default()
}
