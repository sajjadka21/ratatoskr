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

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Probing | Self::Queued)
                | (Self::Probing, Self::Downloading | Self::Failed)
                | (Self::Queued, Self::Created | Self::Probing)
                | (Self::Downloading, Self::Finalizing | Self::Failed)
                | (Self::Finalizing, Self::Completed | Self::Failed)
        )
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPriority {
    Low,
    Normal,
    High,
    VeryHigh,
}

impl DownloadPriority {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::VeryHigh => "very_high",
        }
    }
}

impl fmt::Display for DownloadPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DownloadPriority {
    type Err = ParseDownloadPriorityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "very_high" => Ok(Self::VeryHigh),
            other => Err(ParseDownloadPriorityError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDownloadPriorityError {
    value: String,
}

impl fmt::Display for ParseDownloadPriorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown download priority: {}", self.value)
    }
}

impl std::error::Error for ParseDownloadPriorityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    Running,
    Stopped,
}

impl QueueState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

impl fmt::Display for QueueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for QueueState {
    type Err = ParseQueueStateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            other => Err(ParseQueueStateError {
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseQueueStateError {
    value: String,
}

impl fmt::Display for ParseQueueStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown queue state: {}", self.value)
    }
}

impl std::error::Error for ParseQueueStateError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub state: QueueState,
    pub sort_order: i64,
    pub max_concurrent: u32,
    pub max_concurrent_per_host: Option<u32>,
    pub default_priority: DownloadPriority,
    pub created_at: i64,
    pub updated_at: i64,
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
    pub queue_id: Option<String>,
    pub priority: DownloadPriority,
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
    use super::{DownloadPriority, DownloadStatus, QueueState};
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

    #[test]
    fn phase_one_lifecycle_transitions_are_legal() {
        assert!(DownloadStatus::Created.can_transition_to(DownloadStatus::Probing));
        assert!(DownloadStatus::Probing.can_transition_to(DownloadStatus::Downloading));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Finalizing));
        assert!(DownloadStatus::Finalizing.can_transition_to(DownloadStatus::Completed));
    }

    #[test]
    fn active_phase_one_states_can_fail() {
        assert!(DownloadStatus::Probing.can_transition_to(DownloadStatus::Failed));
        assert!(DownloadStatus::Downloading.can_transition_to(DownloadStatus::Failed));
        assert!(DownloadStatus::Finalizing.can_transition_to(DownloadStatus::Failed));
    }

    #[test]
    fn phase_one_lifecycle_rejects_invalid_jumps() {
        assert!(!DownloadStatus::Created.can_transition_to(DownloadStatus::Downloading));
        assert!(!DownloadStatus::Created.can_transition_to(DownloadStatus::Completed));
        assert!(!DownloadStatus::Completed.can_transition_to(DownloadStatus::Downloading));
        assert!(!DownloadStatus::Failed.can_transition_to(DownloadStatus::Completed));
    }

    #[test]
    fn queue_state_string_roundtrip() {
        for state in [QueueState::Running, QueueState::Stopped] {
            assert_eq!(QueueState::from_str(state.as_str()).unwrap(), state);
        }
    }

    #[test]
    fn download_priority_string_roundtrip() {
        let priorities = [
            DownloadPriority::Low,
            DownloadPriority::Normal,
            DownloadPriority::High,
            DownloadPriority::VeryHigh,
        ];

        for priority in priorities {
            assert_eq!(
                DownloadPriority::from_str(priority.as_str()).unwrap(),
                priority
            );
        }
    }
}
