use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileView {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferView {
    pub id: String,
    pub direction: String, // "send" | "receive"
    pub peer_code: String,
    pub peer_name: String,
    pub files: Vec<FileView>,
    pub total_bytes: u64,
    pub bytes_done: u64,
    pub status: String,
    pub channel: Option<String>,
    /// Stable slug of how the transfer connected, for analytics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Optional note from the sender, shown with the offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    #[serde(default)]
    pub folder: Option<String>,
}

impl TransferView {
    pub fn new(
        id: String,
        direction: &str,
        peer_code: String,
        peer_name: String,
        files: Vec<FileView>,
        total_bytes: u64,
    ) -> Self {
        Self {
            id,
            direction: direction.into(),
            peer_code,
            peer_name,
            files,
            total_bytes,
            bytes_done: 0,
            status: "connecting".into(),
            channel: None,
            method: None,
            comment: None,
            error: None,
            created_at: chrono::Utc::now().timestamp(),
            finished_at: None,
            folder: None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "connecting" | "offered" | "waiting_accept" | "transferring"
        )
    }
}

#[derive(Debug, Clone)]
pub struct LanPeer {
    pub record: peerway_core::record::PeerRecord,
    pub addr: std::net::SocketAddr,
    pub last_seen: std::time::Instant,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Changed,
    Transfer { transfer: TransferView },
    Incoming { transfer: TransferView },
    Code { code: String, formatted: String },
    Toast { level: String, message: String },
    CloudUpload { loaded: u64, total: u64 },
}
