use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Ready,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthState,
    pub message: Option<String>,
}

impl ComponentHealth {
    pub fn ready() -> Self {
        Self {
            status: HealthState::Ready,
            message: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: HealthState::Error,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub core: ComponentHealth,
    pub storage: ComponentHealth,
    pub database: ComponentHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfoResponse {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadTaskEventKind {
    Progress,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTaskEvent {
    pub kind: DownloadTaskEventKind,
    pub download_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    /// Measured transfer rate, present only on progress events for a transfer
    /// that has run long enough to measure.
    pub bytes_per_second: Option<u64>,
    /// Seconds remaining at the measured rate, present only when the total
    /// size is known and the transfer is moving.
    pub eta_seconds: Option<u64>,
    pub status: String,
    pub download: Option<DownloadListItemResponse>,
}

impl DownloadTaskEvent {
    pub fn progress(download_id: impl Into<String>, progress: TransferProgressResponse) -> Self {
        Self {
            kind: DownloadTaskEventKind::Progress,
            download_id: download_id.into(),
            downloaded_bytes: progress.downloaded_bytes,
            total_bytes: progress.total_bytes,
            bytes_per_second: progress.bytes_per_second,
            eta_seconds: progress.eta_seconds,
            status: "downloading".to_owned(),
            download: None,
        }
    }

    pub fn updated(download: DownloadListItemResponse) -> Self {
        Self {
            kind: DownloadTaskEventKind::Updated,
            download_id: download.id.clone(),
            downloaded_bytes: download.downloaded_bytes,
            total_bytes: download.total_bytes,
            bytes_per_second: None,
            eta_seconds: None,
            status: download.status.clone(),
            download: Some(download),
        }
    }
}

/// Serialized transfer measurements. Every field is measured by the engine;
/// nothing here is interpolated by the presentation layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgressResponse {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: Option<u64>,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadListItemResponse {
    pub id: String,
    pub source_url: String,
    pub resolved_url: Option<String>,
    pub filename: Option<String>,
    pub destination_path: Option<String>,
    pub mime_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: String,
    pub queue_id: Option<String>,
    pub priority: String,
    pub queue_position: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueResponse {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub state: String,
    pub sort_order: i64,
    pub max_concurrent: u32,
    pub max_concurrent_per_host: Option<u32>,
    pub default_priority: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueRunnerEventKind {
    QueueUpdated,
    TaskProgress,
    TaskUpdated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRunnerEventResponse {
    pub kind: QueueRunnerEventKind,
    pub queue: Option<QueueResponse>,
    pub download: Option<DownloadListItemResponse>,
    pub download_id: Option<String>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: Option<u64>,
    pub eta_seconds: Option<u64>,
    /// The queue the event came from, so a listener can attribute an event
    /// without having to track which runner it subscribed to.
    pub queue_id: String,
}

impl QueueRunnerEventResponse {
    pub fn queue_updated(queue: QueueResponse) -> Self {
        Self {
            kind: QueueRunnerEventKind::QueueUpdated,
            queue_id: queue.id.clone(),
            queue: Some(queue),
            download: None,
            download_id: None,
            downloaded_bytes: None,
            total_bytes: None,
            bytes_per_second: None,
            eta_seconds: None,
        }
    }

    pub fn task_progress(
        queue_id: impl Into<String>,
        download_id: impl Into<String>,
        progress: TransferProgressResponse,
    ) -> Self {
        Self {
            kind: QueueRunnerEventKind::TaskProgress,
            queue_id: queue_id.into(),
            queue: None,
            download: None,
            download_id: Some(download_id.into()),
            downloaded_bytes: Some(progress.downloaded_bytes),
            total_bytes: progress.total_bytes,
            bytes_per_second: progress.bytes_per_second,
            eta_seconds: progress.eta_seconds,
        }
    }

    pub fn task_updated(queue_id: impl Into<String>, download: DownloadListItemResponse) -> Self {
        Self {
            kind: QueueRunnerEventKind::TaskUpdated,
            queue_id: queue_id.into(),
            queue: None,
            download: Some(download),
            download_id: None,
            downloaded_bytes: None,
            total_bytes: None,
            bytes_per_second: None,
            eta_seconds: None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{
        ComponentHealth, DownloadListItemResponse, DownloadTaskEvent, DownloadTaskEventKind,
        HealthState, QueueResponse, QueueRunnerEventKind, QueueRunnerEventResponse,
        TransferProgressResponse,
    };

    fn sample_download() -> DownloadListItemResponse {
        DownloadListItemResponse {
            id: "task-1".to_owned(),
            source_url: "https://example.com/file.bin".to_owned(),
            resolved_url: None,
            filename: None,
            destination_path: None,
            mime_type: None,
            total_bytes: None,
            downloaded_bytes: 0,
            status: "created".to_owned(),
            queue_id: None,
            priority: "normal".to_owned(),
            queue_position: None,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error_code: None,
            error_message: None,
        }
    }

    #[test]
    fn ready_component_has_no_error_message() {
        let health = ComponentHealth::ready();
        assert_eq!(health.status, HealthState::Ready);
        assert!(health.message.is_none());
    }

    #[test]
    fn error_component_contains_message() {
        let health = ComponentHealth::error("database unavailable");
        assert_eq!(health.status, HealthState::Error);
        assert_eq!(health.message.as_deref(), Some("database unavailable"));
    }

    #[test]
    fn progress_event_contains_only_measured_task_progress() {
        let event = DownloadTaskEvent::progress(
            "task-1",
            TransferProgressResponse {
                downloaded_bytes: 512,
                total_bytes: Some(1_024),
                bytes_per_second: Some(256),
                eta_seconds: Some(2),
            },
        );

        assert_eq!(event.kind, DownloadTaskEventKind::Progress);
        assert_eq!(event.download_id, "task-1");
        assert_eq!(event.downloaded_bytes, 512);
        assert_eq!(event.total_bytes, Some(1_024));
        assert_eq!(event.bytes_per_second, Some(256));
        assert_eq!(event.eta_seconds, Some(2));
        assert_eq!(event.status, "downloading");
        assert!(event.download.is_none());
    }

    #[test]
    fn a_record_event_carries_no_rate_of_its_own() {
        let event = DownloadTaskEvent::updated(sample_download());

        assert!(event.bytes_per_second.is_none());
        assert!(event.eta_seconds.is_none());
    }

    #[test]
    fn queue_progress_events_name_their_queue() {
        let event = QueueRunnerEventResponse::task_progress(
            "default",
            "task-1",
            TransferProgressResponse {
                downloaded_bytes: 128,
                total_bytes: None,
                bytes_per_second: Some(64),
                eta_seconds: None,
            },
        );

        assert_eq!(event.queue_id, "default");
        assert_eq!(event.download_id.as_deref(), Some("task-1"));
        assert_eq!(event.bytes_per_second, Some(64));
        assert!(event.eta_seconds.is_none());
    }

    #[test]
    fn updated_event_carries_the_authoritative_record() {
        let download = sample_download();

        let event = DownloadTaskEvent::updated(download.clone());

        assert_eq!(event.kind, DownloadTaskEventKind::Updated);
        assert_eq!(event.download_id, download.id);
        assert_eq!(event.download, Some(download));
    }

    #[test]
    fn queue_updated_event_contains_only_the_queue_record() {
        let queue = QueueResponse {
            id: "default".to_owned(),
            name: "Default Queue".to_owned(),
            enabled: true,
            state: "running".to_owned(),
            sort_order: 0,
            max_concurrent: 3,
            max_concurrent_per_host: Some(2),
            default_priority: "normal".to_owned(),
            created_at: 1,
            updated_at: 2,
        };
        let event = QueueRunnerEventResponse::queue_updated(queue.clone());

        assert_eq!(event.kind, QueueRunnerEventKind::QueueUpdated);
        assert_eq!(event.queue, Some(queue));
        assert!(event.download.is_none());
        assert!(event.download_id.is_none());
    }
}
