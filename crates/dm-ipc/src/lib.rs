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
pub struct DownloadProgressEvent {
    pub download_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDownloadResponse {
    pub id: String,
    pub filename: Option<String>,
    pub destination_path: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::{ComponentHealth, HealthState};

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
}
