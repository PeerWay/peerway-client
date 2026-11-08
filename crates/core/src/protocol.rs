//! Types shared by the desktop node, the coordination server and the wire
//! protocol used between two devices.

use crate::record::PeerRecord;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SERVER_URL: &str = "https://localhost:8443";
pub const FREE_QUOTA_BYTES: u64 = 5 * 1024 * 1024 * 1024;
pub const FREE_RETENTION_DAYS: i64 = 30;
/// Default logical quotas. The coordination server's environment variables
/// are authoritative; these are only fallbacks for clients/UI.
pub const PRO_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024;
pub const TEAM_QUOTA_BYTES: u64 = 25 * 1024 * 1024 * 1024;
pub const MAX_CHUNK_BYTES: usize = 48 * 1024;
pub const MAX_FRAME_BYTES: usize = 65_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Free,
    Pro,
    Team,
}

impl Plan {
    pub fn as_str(&self) -> &'static str {
        match self {
            Plan::Free => "free",
            Plan::Pro => "pro",
            Plan::Team => "team",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Self {
        match value {
            "pro" => Plan::Pro,
            "team" => Plan::Team,
            _ => Plan::Free,
        }
    }

    pub fn quota_bytes(&self) -> u64 {
        match self {
            Plan::Free => FREE_QUOTA_BYTES,
            Plan::Pro => PRO_QUOTA_BYTES,
            Plan::Team => TEAM_QUOTA_BYTES,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Plan::Free => "Gratis",
            Plan::Pro => "Pro",
            Plan::Team => "Team",
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP API (node <-> coordination server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub device_name: String,
    pub ed25519_pub: String,
    pub x25519_pub: String,
    /// Optional during the 0.1.x transition; required from 0.2 onwards.
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub device_name: String,
    pub ed25519_pub: String,
    pub x25519_pub: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeResponse {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordResetConfirmRequest {
    pub token: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub app_hash: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeSessionRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInfo {
    pub plan: Plan,
    pub label: String,
    pub quota_bytes: u64,
    pub price_usd: u32,
    /// 0 means files are kept while the plan is active.
    #[serde(default)]
    pub retention_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlansResponse {
    pub cloud_enabled: bool,
    #[serde(default)]
    pub free: Option<PlanInfo>,
    /// Paid plans are omitted while the free-only beta is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pro: Option<PlanInfo>,
    /// Paid plans are omitted while the free-only beta is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<PlanInfo>,
    pub global_quota_bytes: u64,
    #[serde(default)]
    pub storage_backend: String,
    /// Server switch (`PEERWAY_PAID_PLANS_ENABLED`). Defaults to false.
    #[serde(default)]
    pub paid_plans_enabled: bool,
    /// True only when paid plans are enabled *and* PayPal is configured.
    #[serde(default)]
    pub paid_billing_enabled: bool,
    #[serde(default)]
    pub paypal_enabled: bool,
    #[serde(default)]
    pub paypal_client_id: Option<String>,
    #[serde(default)]
    pub paypal_currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceRequest {
    pub record: PeerRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceResponse {
    pub ok: bool,
    pub ttl: i64,
    pub observed_v4: Option<String>,
    pub observed_v6: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerLookup {
    pub record: PeerRecord,
    pub observed_v4: Option<String>,
    pub observed_v6: Option<String>,
    pub online: bool,
    pub last_seen: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PunchRequest {
    pub target_code: String,
    pub from_record: PeerRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreateRequest {
    pub target_code: String,
    pub transfer_id: String,
    pub from_record: PeerRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreated {
    pub tunnel_id: String,
    pub token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousRequest {
    pub record: PeerRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendezvousResponse {
    pub ok: bool,
    #[serde(default)]
    pub observed: Option<String>,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub ttl: i64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_kind: Option<String>,
}

/// Messages pushed by the coordination server over the signaling WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalMessage {
    Punch {
        from: String,
        record: PeerRecord,
        endpoints: Vec<String>,
    },
    TunnelIncoming {
        tunnel_id: String,
        token: String,
        from: String,
        transfer_id: String,
    },
    PeerOffline {
        code: String,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFileDto {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudListResponse {
    pub files: Vec<CloudFileDto>,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub plan: Plan,
    /// 0 means files are kept while the plan is active.
    #[serde(default)]
    pub retention_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    pub license_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayPalOrderRequest {
    pub plan: Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayPalOrderResponse {
    pub order_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayPalCaptureRequest {
    pub order_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub transfer_id: String,
    pub sender_record: PeerRecord,
    pub files: Vec<FileMeta>,
    pub total_bytes: u64,
    pub created_at: i64,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
}

// ---------------------------------------------------------------------------
// End-to-end wire protocol (device <-> device)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Control {
    Offer {
        transfer_id: String,
        sender_record: Box<PeerRecord>,
        files: Vec<FileMeta>,
        total_bytes: u64,
        #[serde(default)]
        comment: String,
    },
    Accept {
        transfer_id: String,
    },
    Reject {
        transfer_id: String,
        reason: String,
    },
    Resume {
        transfer_id: String,
        offsets: Vec<u64>,
    },
    FileStart {
        index: u32,
        name: String,
        size: u64,
    },
    FileEnd {
        index: u32,
        sha256: String,
    },
    Complete {
        transfer_id: String,
    },
    Done {
        transfer_id: String,
    },
    Error {
        message: String,
    },
    Ping,
}

#[derive(Debug, Clone)]
pub enum Frame {
    Control(Control),
    Data { file_index: u32, data: Vec<u8> },
}

impl Frame {
    pub fn control(control: Control) -> Self {
        Frame::Control(control)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        match self {
            Frame::Control(control) => {
                let mut out = Vec::with_capacity(256);
                out.push(1u8);
                serde_json::to_writer(&mut out, control)?;
                Ok(out)
            }
            Frame::Data { file_index, data } => {
                if data.len() > MAX_CHUNK_BYTES {
                    bail!("data chunk too large");
                }
                let mut out = Vec::with_capacity(5 + data.len());
                out.push(2u8);
                out.extend_from_slice(&file_index.to_le_bytes());
                out.extend_from_slice(data);
                Ok(out)
            }
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        match buf.first().copied() {
            Some(1) => Ok(Frame::Control(serde_json::from_slice(&buf[1..])?)),
            Some(2) => {
                if buf.len() < 5 {
                    bail!("truncated data frame");
                }
                let file_index = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                Ok(Frame::Data {
                    file_index,
                    data: buf[5..].to_vec(),
                })
            }
            _ => bail!("unknown frame type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_quotas() {
        assert_eq!(Plan::Free.quota_bytes(), 5 * 1024 * 1024 * 1024);
        assert!(Plan::Pro.quota_bytes() > Plan::Free.quota_bytes());
        assert_eq!(FREE_RETENTION_DAYS, 30);
        assert_eq!(Plan::from_str("team"), Plan::Team);
    }

    #[test]
    fn control_frame_roundtrip() {
        let frame = Frame::Control(Control::Accept {
            transfer_id: "abc".into(),
        });
        let encoded = frame.encode().unwrap();
        match Frame::decode(&encoded).unwrap() {
            Frame::Control(Control::Accept { transfer_id }) => assert_eq!(transfer_id, "abc"),
            _ => panic!("wrong frame"),
        }
    }

    #[test]
    fn offer_without_comment_still_decodes() {
        // Older clients do not send the optional note.
        use crate::identity::Identity;
        let identity = Identity::generate().unwrap();
        let record = crate::record::PeerRecord::new(
            &identity,
            "ABCDEFGH".into(),
            7475,
            7476,
            Vec::new(),
            "test".into(),
        )
        .unwrap();
        let offer = Offer {
            transfer_id: "abc".into(),
            sender_record: record,
            files: Vec::new(),
            total_bytes: 0,
            created_at: 0,
            comment: "nota".into(),
        };
        let mut value = serde_json::to_value(&offer).unwrap();
        value.as_object_mut().unwrap().remove("comment");
        let decoded: Offer = serde_json::from_value(value).unwrap();
        assert!(decoded.comment.is_empty());
    }

    #[test]
    fn data_frame_roundtrip() {
        let frame = Frame::Data {
            file_index: 7,
            data: vec![1, 2, 3],
        };
        let encoded = frame.encode().unwrap();
        match Frame::decode(&encoded).unwrap() {
            Frame::Data { file_index, data } => {
                assert_eq!(file_index, 7);
                assert_eq!(data, vec![1, 2, 3]);
            }
            _ => panic!("wrong frame"),
        }
    }
}
