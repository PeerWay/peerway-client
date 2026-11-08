use anyhow::{Context, Result};
use peerway_core::code;
use peerway_core::protocol::{Plan, DEFAULT_SERVER_URL};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    pub server_url: String,
    pub insecure_tls: bool,
    pub device_name: String,
    pub download_dir: String,
    pub ui_port: u16,
    #[serde(default = "default_quic_port")]
    pub rendezvous_port: u16,
    pub lan_discovery: bool,
    pub auto_accept_trusted: bool,
    pub trusted_codes: Vec<String>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default)]
    pub autostart_enabled: bool,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub install_id: String,
    #[serde(default)]
    pub telemetry_enabled: bool,
    #[serde(default)]
    pub telemetry_choice_made: bool,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default = "default_true")]
    pub notify_updates: bool,
    #[serde(default = "default_true")]
    pub notify_transfers: bool,
    #[serde(default)]
    pub force_relay: bool,
    #[serde(default = "default_lan_port")]
    pub lan_port: u16,
    /// Temporary code, kept so it survives restarts. Empty means "generate".
    #[serde(default)]
    pub code: String,
}

fn default_auto_update() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    for key in ["PEERWAY_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if value.to_ascii_lowercase().starts_with("en") {
                return "en".into();
            }
            if value.to_ascii_lowercase().starts_with("es") {
                return "es".into();
            }
        }
    }
    "es".into()
}

fn default_lan_port() -> u16 {
    7475
}

fn default_quic_port() -> u16 {
    8444
}

fn default_server_url() -> String {
    option_env!("PEERWAY_DEFAULT_SERVER_URL")
        .unwrap_or(DEFAULT_SERVER_URL)
        .to_string()
}

fn default_insecure_tls() -> bool {
    if let Some(value) = option_env!("PEERWAY_DEFAULT_INSECURE_TLS") {
        return matches!(value, "1" | "true" | "yes");
    }
    let url = default_server_url();
    url.contains("localhost") || url.contains("127.0.0.1")
}

impl Default for NodeConfig {
    fn default() -> Self {
        let device_name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Mi PC".into());
        let download_dir = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|p| p.join("Downloads")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("PeerWay")
            .to_string_lossy()
            .to_string();
        Self {
            server_url: default_server_url(),
            insecure_tls: default_insecure_tls(),
            device_name,
            download_dir,
            ui_port: 8787,
            rendezvous_port: default_quic_port(),
            lan_discovery: true,
            auto_accept_trusted: true,
            trusted_codes: Vec::new(),
            auto_update: true,
            autostart_enabled: false,
            language: default_language(),
            install_id: String::new(),
            telemetry_enabled: false,
            telemetry_choice_made: false,
            notifications_enabled: true,
            notify_updates: true,
            notify_transfers: true,
            force_relay: false,
            lan_port: default_lan_port(),
            code: String::new(),
        }
    }
}

impl NodeConfig {
    pub fn normalize(&mut self) {
        self.server_url = self.server_url.trim_end_matches('/').to_string();
        self.language = if self.language.to_ascii_lowercase().starts_with("en") {
            "en".into()
        } else {
            "es".into()
        };
        self.install_id = self.install_id.trim().to_string();
        if self.install_id.len() < 16
            || self.install_id.len() > 128
            || !self.install_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            self.install_id = uuid::Uuid::new_v4().to_string();
        }
        self.trusted_codes = self
            .trusted_codes
            .iter()
            .filter_map(|c| code::normalize(c))
            .collect();
        self.trusted_codes.sort();
        self.trusted_codes.dedup();
    }
}

#[derive(Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub config: PathBuf,
    pub identity: PathBuf,
    pub session: PathBuf,
    pub history: PathBuf,
    pub ui_port_file: PathBuf,
    pub local_token: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let home = match std::env::var("PEERWAY_HOME") {
            Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => dirs::config_dir()
                .or_else(|| dirs::home_dir().map(|p| p.join(".config")))
                .context("no se pudo determinar el directorio de configuración")?
                .join("PeerWay"),
        };
        std::fs::create_dir_all(&home)?;
        Ok(Self {
            config: home.join("config.json"),
            identity: home.join("identity.json"),
            session: home.join("session.json"),
            history: home.join("history.json"),
            ui_port_file: home.join("ui.port"),
            local_token: home.join("local.token"),
            home,
        })
    }
}

pub fn load_config(path: &Path) -> Result<NodeConfig> {
    let exists = path.exists();
    let mut config = if exists {
        let raw = std::fs::read_to_string(path)?;
        serde_json::from_str(&raw).unwrap_or_else(|error| {
            tracing::warn!(?error, "configuración inválida; usando valores por defecto");
            NodeConfig::default()
        })
    } else {
        NodeConfig::default()
    };
    if let Ok(url) = std::env::var("PEERWAY_SERVER_URL") {
        config.server_url = url;
    }
    if let Some(port) = std::env::var("PEERWAY_UI_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
    {
        config.ui_port = port;
    }
    if let Some(port) = std::env::var("PEERWAY_QUIC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
    {
        config.rendezvous_port = port;
    }
    if let Ok(name) = std::env::var("PEERWAY_DEVICE_NAME") {
        if !name.trim().is_empty() {
            config.device_name = name;
        }
    }
    if let Ok(language) = std::env::var("PEERWAY_LANGUAGE") {
        if !language.trim().is_empty() {
            config.language = language;
        }
    }
    if matches!(
        std::env::var("PEERWAY_TELEMETRY").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    ) {
        config.telemetry_enabled = true;
        config.telemetry_choice_made = true;
    }
    if matches!(
        std::env::var("PEERWAY_DISABLE_LAN").as_deref(),
        Ok("1") | Ok("true")
    ) {
        config.lan_discovery = false;
    }
    if matches!(
        std::env::var("PEERWAY_FORCE_RELAY").as_deref(),
        Ok("1") | Ok("true")
    ) {
        config.force_relay = true;
    }
    config.normalize();
    save_config(path, &config)?;
    Ok(config)
}

pub fn save_config(path: &Path, config: &NodeConfig) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(config)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionData {
    pub token: String,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    pub code: String,
    pub plan: Plan,
    pub plan_expires: Option<i64>,
    pub quota_bytes: u64,
    pub used_bytes: u64,
}

pub fn load_session(path: &Path) -> Option<SessionData> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_session(path: &Path, session: Option<&SessionData>) -> Result<()> {
    match session {
        Some(session) => {
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(session)?)?;
            std::fs::rename(&tmp, path)?;
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_normalizes_to_supported_values() {
        let mut config = NodeConfig::default();
        config.language = "en-US".into();
        config.normalize();
        assert_eq!(config.language, "en");

        config.language = "fr".into();
        config.normalize();
        assert_eq!(config.language, "es");
    }

    #[test]
    fn autostart_is_opt_in() {
        assert!(!NodeConfig::default().autostart_enabled);
        // Configs saved by older versions keep their value; only the missing
        // field falls back to off.
        let raw = r#"{"server_url":"https://api.peerway.io","insecure_tls":false,"device_name":"pc","download_dir":"/tmp","ui_port":8787,"lan_discovery":true,"auto_accept_trusted":true,"trusted_codes":[]}"#;
        let config: NodeConfig = serde_json::from_str(raw).unwrap();
        assert!(!config.autostart_enabled);
    }

    #[test]
    fn install_id_is_generated_and_stable() {
        let mut config = NodeConfig::default();
        config.install_id.clear();
        config.normalize();
        let install_id = config.install_id.clone();
        assert!(install_id.len() >= 16);
        config.normalize();
        assert_eq!(config.install_id, install_id);
        assert!(!config.telemetry_enabled);
    }
}
