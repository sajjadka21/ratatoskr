use crate::DownloadError;
use reqwest::StatusCode;
use std::time::Duration;

/// Whether a failure is worth another attempt.
///
/// The specification is explicit that some failures must never be retried:
/// authentication, permanent 4xx answers, invalid URLs, a full disk, denied
/// access, and anything the user stopped on purpose. Retrying those wastes
/// the attempt budget and, worse, hammers a server that already said no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    Retryable,
    Permanent,
}

pub fn classify_failure(error: &DownloadError) -> FailureClass {
    match error {
        // The user asked for this; it is not a failure to recover from.
        DownloadError::Stopped(_) => FailureClass::Permanent,

        DownloadError::InvalidUrl(_) | DownloadError::UnsupportedScheme(_) => {
            FailureClass::Permanent
        }

        DownloadError::InvalidRangeResponse { .. } | DownloadError::SegmentOverflow { .. } => {
            FailureClass::Permanent
        }

        DownloadError::HttpStatus { status } => reqwest::StatusCode::from_u16(*status)
            .map(classify_status)
            .unwrap_or(FailureClass::Permanent),

        // A short transfer is usually a dropped connection, which is exactly
        // what resuming is for.
        DownloadError::IncompleteTransfer { .. } => FailureClass::Retryable,

        // A storage failure means the database is unhappy; another transfer
        // attempt will not help.
        DownloadError::ProgressCallback(_) => FailureClass::Permanent,

        DownloadError::Io(error) => classify_io(error.kind()),

        // What the stream is does not change on a second attempt.
        DownloadError::Stream(_) | DownloadError::TooLarge { .. } | DownloadError::Ffmpeg(_) => {
            FailureClass::Permanent
        }

        DownloadError::Http(error) => match error.status() {
            Some(status) => classify_status(status),
            None if error.is_timeout() || error.is_connect() || error.is_request() => {
                FailureClass::Retryable
            }
            None if error.is_body() || error.is_decode() => FailureClass::Retryable,
            None => FailureClass::Permanent,
        },
    }
}

fn classify_status(status: StatusCode) -> FailureClass {
    match status {
        // Explicit "come back later" answers.
        StatusCode::REQUEST_TIMEOUT
        | StatusCode::TOO_EARLY
        | StatusCode::TOO_MANY_REQUESTS
        | StatusCode::SERVICE_UNAVAILABLE => FailureClass::Retryable,

        // Authentication and authorization will not change by themselves.
        StatusCode::UNAUTHORIZED
        | StatusCode::FORBIDDEN
        | StatusCode::PROXY_AUTHENTICATION_REQUIRED => FailureClass::Permanent,

        status if status.is_server_error() => FailureClass::Retryable,
        status if status.is_client_error() => FailureClass::Permanent,
        _ => FailureClass::Permanent,
    }
}

fn classify_io(kind: std::io::ErrorKind) -> FailureClass {
    match kind {
        std::io::ErrorKind::PermissionDenied
        | std::io::ErrorKind::NotFound
        | std::io::ErrorKind::AlreadyExists
        | std::io::ErrorKind::InvalidInput
        | std::io::ErrorKind::StorageFull => FailureClass::Permanent,
        _ => FailureClass::Retryable,
    }
}

/// A bounded retry schedule: exponential backoff with jitter, and a hard stop.
///
/// Jitter is supplied by the caller rather than drawn inside, so the schedule
/// stays a pure function that tests can pin down exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub maximum_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_secs(2),
            maximum_delay: Duration::from_secs(300),
        }
    }
}

impl RetryPolicy {
    /// The delay before attempt number `attempts`, or `None` once the budget
    /// is spent.
    pub fn delay_for(self, attempts: u32) -> Option<Duration> {
        self.delay_for_with_jitter(attempts, jitter_ratio())
    }

    pub fn delay_for_with_jitter(self, attempts: u32, jitter_ratio: f64) -> Option<Duration> {
        if attempts == 0 || attempts >= self.max_attempts {
            return None;
        }

        let exponent = attempts.saturating_sub(1).min(16);
        let scaled = self
            .base_delay
            .saturating_mul(2_u32.saturating_pow(exponent))
            .min(self.maximum_delay);

        // Spread retries out so a batch of tasks that failed together does
        // not come back in lockstep.
        let jitter = scaled.mul_f64(jitter_ratio.clamp(0.0, 1.0) * 0.25);

        Some((scaled + jitter).min(self.maximum_delay))
    }
}

/// A cheap, dependency-free jitter source. Retry spacing does not need
/// cryptographic randomness, only the property that two tasks failing at the
/// same moment do not choose identical delays.
fn jitter_ratio() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);

    f64::from(nanos % 1_000) / 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_side_problems_are_worth_retrying() {
        for status in [
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::REQUEST_TIMEOUT,
        ] {
            assert_eq!(
                classify_status(status),
                FailureClass::Retryable,
                "{status} should be retried"
            );
        }
    }

    #[test]
    fn answers_that_will_not_change_are_never_retried() {
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::GONE,
            StatusCode::BAD_REQUEST,
        ] {
            assert_eq!(
                classify_status(status),
                FailureClass::Permanent,
                "{status} should not be retried"
            );
        }
    }

    #[test]
    fn local_problems_the_user_must_fix_are_never_retried() {
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::StorageFull,
            std::io::ErrorKind::NotFound,
        ] {
            assert_eq!(classify_io(kind), FailureClass::Permanent);
        }

        assert_eq!(
            classify_io(std::io::ErrorKind::Interrupted),
            FailureClass::Retryable
        );
    }

    #[test]
    fn a_stop_the_user_asked_for_is_not_retried() {
        assert_eq!(
            classify_failure(&DownloadError::Stopped(crate::control::StopReason::Pause)),
            FailureClass::Permanent
        );
    }

    #[test]
    fn an_interrupted_transfer_is_retried() {
        assert_eq!(
            classify_failure(&DownloadError::IncompleteTransfer {
                expected: 100,
                actual: 40
            }),
            FailureClass::Retryable
        );
    }

    #[test]
    fn typed_rate_limit_statuses_follow_http_retry_policy() {
        assert_eq!(
            classify_failure(&DownloadError::HttpStatus { status: 429 }),
            FailureClass::Retryable
        );
        assert_eq!(
            classify_failure(&DownloadError::HttpStatus { status: 503 }),
            FailureClass::Retryable
        );
        assert_eq!(
            classify_failure(&DownloadError::HttpStatus { status: 404 }),
            FailureClass::Permanent
        );
    }

    #[test]
    fn a_bad_url_is_never_retried() {
        assert_eq!(
            classify_failure(&DownloadError::InvalidUrl("nonsense".to_owned())),
            FailureClass::Permanent
        );
    }

    #[test]
    fn backoff_grows_with_each_attempt() {
        let policy = RetryPolicy::default();

        let first = policy.delay_for_with_jitter(1, 0.0).unwrap();
        let second = policy.delay_for_with_jitter(2, 0.0).unwrap();
        let third = policy.delay_for_with_jitter(3, 0.0).unwrap();

        assert_eq!(first, Duration::from_secs(2));
        assert_eq!(second, Duration::from_secs(4));
        assert_eq!(third, Duration::from_secs(8));
    }

    #[test]
    fn backoff_is_capped() {
        let policy = RetryPolicy {
            max_attempts: 50,
            base_delay: Duration::from_secs(2),
            maximum_delay: Duration::from_secs(60),
        };

        assert_eq!(
            policy.delay_for_with_jitter(20, 1.0).unwrap(),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn jitter_only_ever_adds_a_quarter() {
        let policy = RetryPolicy::default();

        let none = policy.delay_for_with_jitter(1, 0.0).unwrap();
        let full = policy.delay_for_with_jitter(1, 1.0).unwrap();

        assert_eq!(none, Duration::from_secs(2));
        assert_eq!(full, Duration::from_millis(2_500));
    }

    #[test]
    fn the_budget_is_bounded() {
        let policy = RetryPolicy {
            max_attempts: 3,
            ..RetryPolicy::default()
        };

        assert!(policy.delay_for_with_jitter(1, 0.0).is_some());
        assert!(policy.delay_for_with_jitter(2, 0.0).is_some());
        assert!(
            policy.delay_for_with_jitter(3, 0.0).is_none(),
            "the last allowed attempt must not schedule another"
        );
    }
}
