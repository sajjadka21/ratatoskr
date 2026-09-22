use reqwest::Url;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserHandoff {
    pub url: String,
    pub filename_hint: Option<String>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BrowserHandoffError {
    #[error("browser handoff URL is invalid")]
    InvalidUrl,
    #[error("browser handoff URL must use HTTP or HTTPS")]
    UnsupportedScheme,
    #[error("browser handoff URL must not contain credentials")]
    CredentialsInUrl,
    #[error("browser handoff metadata is too long")]
    MetadataTooLong,
}

impl BrowserHandoff {
    pub fn validate(self) -> Result<Self, BrowserHandoffError> {
        let parsed = Url::parse(&self.url).map_err(|_| BrowserHandoffError::InvalidUrl)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(BrowserHandoffError::UnsupportedScheme);
        }
        if parsed.username() != "" || parsed.password().is_some() {
            return Err(BrowserHandoffError::CredentialsInUrl);
        }
        if self
            .filename_hint
            .as_deref()
            .is_some_and(|value| value.len() > 512)
            || self
                .referrer
                .as_deref()
                .is_some_and(|value| value.len() > 2_048)
            || self
                .user_agent
                .as_deref()
                .is_some_and(|value| value.len() > 512)
        {
            return Err(BrowserHandoffError::MetadataTooLong);
        }
        Ok(Self {
            url: parsed.to_string(),
            ..self
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserHandoff, BrowserHandoffError};

    #[test]
    fn accepts_safe_http_context_without_persisting_credentials() {
        let request = BrowserHandoff {
            url: "https://example.com/file.zip".to_owned(),
            filename_hint: Some("file.zip".to_owned()),
            referrer: Some("https://example.com/".to_owned()),
            user_agent: None,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn rejects_credentials_and_non_http_schemes() {
        let mut request = BrowserHandoff {
            url: "https://user:pass@example.com/file".to_owned(),
            filename_hint: None,
            referrer: None,
            user_agent: None,
        };
        assert_eq!(
            request.clone().validate(),
            Err(BrowserHandoffError::CredentialsInUrl)
        );
        request.url = "file:///tmp/file".to_owned();
        assert_eq!(
            request.validate(),
            Err(BrowserHandoffError::UnsupportedScheme)
        );
    }
}
