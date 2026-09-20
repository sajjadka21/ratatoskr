use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Created,
    Probing,
    Queued,
    Downloading,
    Paused,
    Retrying,
    Finalizing,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Probing => "probing",
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Retrying => "retrying",
            Self::Finalizing => "finalizing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDownloadStatusError {
    value: String,
}

impl fmt::Display for ParseDownloadStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown download status: {}", self.value)
    }
}

impl std::error::Error for ParseDownloadStatusError {}

impl FromStr for DownloadStatus {
    type Err = ParseDownloadStatusError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(Self::Created),
            "probing" => Ok(Self::Probing),
            "queued" => Ok(Self::Queued),
            "downloading" => Ok(Self::Downloading),
            "paused" => Ok(Self::Paused),
            "retrying" => Ok(Self::Retrying),
            "finalizing" => Ok(Self::Finalizing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(ParseDownloadStatusError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub source_url: String,
    pub resolved_url: Option<String>,
    pub filename: Option<String>,
    pub destination_path: Option<String>,
    pub temp_path: Option<String>,
    pub mime_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub range_supported: Option<bool>,
    pub status: DownloadStatus,
    pub queue_position: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCompletion {
    pub resolved_url: String,
    pub filename: String,
    pub destination_path: String,
    pub mime_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::DownloadStatus;
    use std::str::FromStr;

    #[test]
    fn status_string_roundtrip() {
        let statuses = [
            DownloadStatus::Created,
            DownloadStatus::Probing,
            DownloadStatus::Queued,
            DownloadStatus::Downloading,
            DownloadStatus::Paused,
            DownloadStatus::Retrying,
            DownloadStatus::Finalizing,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ];

        for status in statuses {
            let encoded = status.to_string();
            let decoded = DownloadStatus::from_str(&encoded).unwrap();
            assert_eq!(decoded, status);
        }
    }

    #[test]
    fn rejects_unknown_status() {
        assert!(DownloadStatus::from_str("unknown").is_err());
    }
}
