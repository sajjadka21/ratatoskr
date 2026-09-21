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
    pub status: String,
    pub download: Option<DownloadListItemResponse>,
}

impl DownloadTaskEvent {
    pub fn progress(
        download_id: impl Into<String>,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Self {
        Self {
            kind: DownloadTaskEventKind::Progress,
            download_id: download_id.into(),
            downloaded_bytes,
            total_bytes,
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
            status: download.status.clone(),
            download: Some(download),
        }
    }
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
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}
#[cfg(test)]
mod tests {
    use super::{
        ComponentHealth, DownloadListItemResponse, DownloadTaskEvent, DownloadTaskEventKind,
        HealthState,
    };

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
    fn progress_event_contains_only_task_progress() {
        let event = DownloadTaskEvent::progress("task-1", 512, Some(1_024));

        assert_eq!(event.kind, DownloadTaskEventKind::Progress);
        assert_eq!(event.download_id, "task-1");
        assert_eq!(event.downloaded_bytes, 512);
        assert_eq!(event.total_bytes, Some(1_024));
        assert_eq!(event.status, "downloading");
        assert!(event.download.is_none());
    }

    #[test]
    fn updated_event_carries_the_authoritative_record() {
        let download = DownloadListItemResponse {
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
            created_at: 1,
            started_at: None,
            completed_at: None,
            error_code: None,
            error_message: None,
        };

        let event = DownloadTaskEvent::updated(download.clone());

        assert_eq!(event.kind, DownloadTaskEventKind::Updated);
        assert_eq!(event.download_id, download.id);
        assert_eq!(event.download, Some(download));
    }
}
