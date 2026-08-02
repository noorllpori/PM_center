use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;
pub const MAX_AVATAR_BYTES: usize = 64 * 1024;
pub const HISTORY_LIMIT: usize = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub server_id: String,
    pub name: String,
    pub protocol_version: u16,
    pub requires_password: bool,
    pub max_transfer_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    pub device_id: String,
    pub display_name: String,
    pub department: String,
    pub avatar_base64: Option<String>,
    pub avatar_hash: Option<String>,
    pub profile_revision: u64,
    pub online: bool,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayMessage {
    pub id: String,
    pub conversation_id: String,
    pub from_id: String,
    pub from_name: String,
    pub to_id: Option<String>,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOffer {
    pub transfer_id: String,
    pub from_id: String,
    pub from_name: String,
    pub to_id: String,
    pub display_name: String,
    pub kind: String,
    pub item_count: u64,
    pub total_bytes: u64,
    pub mime_type: Option<String>,
    pub content_hash: String,
    pub payload_format: String,
    pub manifest: serde_json::Value,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    Register {
        device_id: String,
        display_name: String,
        department: String,
        avatar_base64: Option<String>,
        avatar_hash: Option<String>,
        profile_revision: u64,
        credential: Option<String>,
        shared_password: Option<String>,
    },
    Ping {
        nonce: String,
        sent_at: u64,
    },
    SendMessage {
        message: RelayMessage,
    },
    FileOffer {
        offer: FileOffer,
    },
    FileResponse {
        transfer_id: String,
        accepted: bool,
    },
    CancelTransfer {
        transfer_id: String,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    Registered {
        info: ServerInfo,
        credential: String,
        devices: Vec<DeviceProfile>,
        history: Vec<RelayMessage>,
    },
    Error {
        code: String,
        message: String,
    },
    Pong {
        nonce: String,
        sent_at: u64,
        server_at: u64,
    },
    Presence {
        devices: Vec<DeviceProfile>,
    },
    Message {
        message: RelayMessage,
    },
    MessageAck {
        message_id: String,
    },
    FileOffer {
        offer: FileOffer,
    },
    FileResponse {
        transfer_id: String,
        accepted: bool,
        reason: Option<String>,
    },
    FileDownload {
        transfer_id: String,
        url: String,
        token: String,
    },
    FileUpload {
        transfer_id: String,
        url: String,
        token: String,
    },
    FileProgress {
        transfer_id: String,
        forwarded_bytes: u64,
        total_bytes: u64,
    },
    FileComplete {
        transfer_id: String,
    },
    FileCancelled {
        transfer_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedTestResult {
    pub latency_ms: f64,
    pub download_bytes_per_second: u64,
    pub upload_bytes_per_second: u64,
    pub tested_at: u64,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_messages_use_versioned_tagged_json() {
        let value = serde_json::to_value(ClientMessage::Ping {
            nonce: "n".into(),
            sent_at: 1,
        })
        .unwrap();
        assert_eq!(value["type"], "ping");
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}
