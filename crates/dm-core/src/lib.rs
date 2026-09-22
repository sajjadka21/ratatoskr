pub mod control;
pub mod queue;
pub mod resume;
pub mod retry;
pub mod service;
#[cfg(test)]
pub mod testing;
pub mod throughput;

use crate::control::{StopReason, TaskControl};
use percent_encoding::percent_decode_str;
use reqwest::{
    Client, StatusCode, Url,
    header::{
        ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_TYPE, ETAG, HeaderMap,
        LAST_MODIFIED, RANGE,
    },
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt},
};

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("progress callback failed: {0}")]
    ProgressCallback(String),

    #[error("transfer stopped by the user")]
    Stopped(StopReason),

    #[error("transfer ended after {actual} of {expected} bytes")]
    IncompleteTransfer { expected: u64, actual: u64 },
}

impl DownloadError {
    pub fn redacted_message(&self) -> String {
        match self {
            Self::InvalidUrl(message) => format!("invalid URL: {message}"),
            Self::UnsupportedScheme(scheme) => {
                format!("unsupported URL scheme: {scheme}")
            }
            Self::Http(error) if error.is_timeout() => "HTTP request timed out".to_owned(),
            Self::Http(error) if error.is_connect() => {
                "could not connect to the download server".to_owned()
            }
            Self::Http(error) => error
                .status()
                .map(|status| format!("HTTP request failed with status {status}"))
                .unwrap_or_else(|| "HTTP request failed".to_owned()),
            Self::Io(error) => format!("filesystem error: {error}"),
            Self::ProgressCallback(_) => "could not persist download progress".to_owned(),
            Self::Stopped(StopReason::Pause) => "paused".to_owned(),
            Self::Stopped(StopReason::Cancel) => "cancelled".to_owned(),
            Self::IncompleteTransfer { expected, actual } => {
                format!("the server sent {actual} of {expected} bytes, so the file is incomplete")
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, DownloadError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadMetadata {
    pub source_url: String,
    pub final_url: String,
    pub filename: String,
    pub content_type: Option<String>,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

/// Progress as the service reports it: the byte counts the engine actually
/// wrote, plus the measurements derived from them. Everything here comes from
/// real transfer data — nothing is interpolated to make a bar move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: Option<u64>,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub metadata: DownloadMetadata,
    pub final_path: PathBuf,
    pub downloaded_bytes: u64,
}

/// What the source says about itself before any byte is written. Range
/// support here is a verified fact, not a header the server merely advertised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProbe {
    pub final_url: String,
    pub filename: String,
    pub content_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub range_supported: bool,
}

/// One attempt at moving bytes, starting either from zero or from what a
/// previous attempt left in the partial file.
#[derive(Debug, Clone)]
pub struct TransferRequest<'a> {
    pub source_url: &'a str,
    pub temp_path: &'a Path,
    pub destination_path: &'a Path,
    pub start_offset: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferOutcome {
    pub final_path: PathBuf,
    pub downloaded_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Downloader {
    client: Client,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .user_agent("DownloadManager/0.1")
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .build()?;

        Ok(Self { client })
    }

    /// Asks the source what it can do before committing to a transfer.
    ///
    /// HEAD is used when it answers, because it is the cheapest way to read
    /// metadata, but it is never trusted on its own: range support is
    /// confirmed by asking for a single byte and requiring a `206` with a
    /// `Content-Range`, which is the only answer that proves a later ranged
    /// request will work.
    pub async fn probe(&self, source_url: &str) -> Result<SourceProbe> {
        let url = validate_source_url(source_url)?;

        let head_headers = match self
            .client
            .head(url.clone())
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => Some(response.headers().clone()),
            _ => None,
        };

        let response = self
            .client
            .get(url)
            .header(ACCEPT_ENCODING, "identity")
            .header(RANGE, "bytes=0-0")
            .send()
            .await?
            .error_for_status()?;

        let final_url = response.url().clone();
        let status = response.status();
        let headers = response.headers().clone();
        let content_length = response.content_length();

        // Drop the body without reading it: on a server that ignored the
        // range this would otherwise be the entire file.
        drop(response);

        let content_range = header_text(&headers, CONTENT_RANGE);
        let range_supported = status == StatusCode::PARTIAL_CONTENT && content_range.is_some();

        let total_bytes = if range_supported {
            content_range
                .as_deref()
                .and_then(total_from_content_range)
                .or_else(|| head_headers.as_ref().and_then(content_length_of))
        } else {
            content_length.or_else(|| head_headers.as_ref().and_then(content_length_of))
        };

        let filename = sanitize_filename(&determine_filename(
            &headers,
            head_headers.as_ref(),
            &final_url,
        ));

        Ok(SourceProbe {
            final_url: final_url.to_string(),
            filename,
            content_type: header_text(&headers, CONTENT_TYPE).or_else(|| {
                head_headers
                    .as_ref()
                    .and_then(|h| header_text(h, CONTENT_TYPE))
            }),
            total_bytes,
            etag: header_text(&headers, ETAG)
                .or_else(|| head_headers.as_ref().and_then(|h| header_text(h, ETAG))),
            last_modified: header_text(&headers, LAST_MODIFIED).or_else(|| {
                head_headers
                    .as_ref()
                    .and_then(|h| header_text(h, LAST_MODIFIED))
            }),
            range_supported,
        })
    }

    /// Reserves a destination and its partial file. Reserving both together
    /// keeps two concurrent tasks for the same filename from planning the
    /// same path.
    pub async fn plan_paths(
        &self,
        destination_directory: impl AsRef<Path>,
        filename: &str,
    ) -> Result<(PathBuf, PathBuf)> {
        let directory = destination_directory.as_ref();
        fs::create_dir_all(directory).await?;

        reserve_paths(directory, filename).await.map_err(Into::into)
    }

    /// Moves bytes into the partial file and, when the whole file has arrived,
    /// finalizes it.
    ///
    /// A pause or cancel takes effect between chunks; the partial file is left
    /// exactly as far as it got, so the caller decides what happens to it.
    pub async fn transfer<F>(
        &self,
        request: TransferRequest<'_>,
        control: &TaskControl,
        mut on_progress: F,
    ) -> Result<TransferOutcome>
    where
        F: FnMut(DownloadProgress) -> Result<()> + Send,
    {
        let url = validate_source_url(request.source_url)?;

        if let Some(parent) = request.temp_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut downloaded_bytes = request.start_offset;

        // Everything already arrived in an earlier attempt; only finalization
        // is left.
        if downloaded_bytes > 0 && request.total_bytes == Some(downloaded_bytes) {
            return finalize_transfer(
                request.temp_path,
                request.destination_path,
                downloaded_bytes,
            )
            .await;
        }

        let mut response = self.request_body(&url, downloaded_bytes).await?;

        // A server that ignores the range answers 200 with the whole file.
        // Appending that to existing bytes would corrupt it, so the transfer
        // restarts instead.
        if downloaded_bytes > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
            downloaded_bytes = 0;
        }

        let mut file = open_partial_file(request.temp_path, downloaded_bytes).await?;

        on_progress(DownloadProgress {
            downloaded_bytes,
            total_bytes: request.total_bytes,
        })?;

        loop {
            let chunk = tokio::select! {
                biased;

                reason = control.stopped() => {
                    file.flush().await?;
                    file.sync_all().await?;
                    return Err(DownloadError::Stopped(reason));
                }

                chunk = response.chunk() => chunk?,
            };

            let Some(chunk) = chunk else {
                break;
            };

            file.write_all(&chunk).await?;
            downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);

            on_progress(DownloadProgress {
                downloaded_bytes,
                total_bytes: request.total_bytes,
            })?;
        }

        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        // The byte count is the only integrity evidence available without a
        // hash, so a short transfer fails rather than being renamed into
        // place as a complete file.
        if let Some(expected) = request.total_bytes
            && downloaded_bytes != expected
        {
            return Err(DownloadError::IncompleteTransfer {
                expected,
                actual: downloaded_bytes,
            });
        }

        finalize_transfer(
            request.temp_path,
            request.destination_path,
            downloaded_bytes,
        )
        .await
    }

    /// Issues the body request, retrying without a range when the server
    /// rejects the one we asked for.
    async fn request_body(&self, url: &Url, start_offset: u64) -> Result<reqwest::Response> {
        let mut builder = self
            .client
            .get(url.clone())
            .header(ACCEPT_ENCODING, "identity");

        if start_offset > 0 {
            builder = builder.header(RANGE, format!("bytes={start_offset}-"));
        }

        let response = builder.send().await?;

        // The stored offset no longer fits the resource; start over rather
        // than failing a transfer that can simply begin again.
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE && start_offset > 0 {
            return Ok(self
                .client
                .get(url.clone())
                .header(ACCEPT_ENCODING, "identity")
                .send()
                .await?
                .error_for_status()?);
        }

        Ok(response.error_for_status()?)
    }
}

/// Opens the partial file positioned at `offset`, discarding anything beyond
/// it so a resumed transfer cannot leave stale bytes in the middle of a file.
async fn open_partial_file(temp_path: &Path, offset: u64) -> Result<tokio::fs::File> {
    if offset == 0 {
        return Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(temp_path)
            .await?);
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(temp_path)
        .await?;

    file.set_len(offset).await?;
    file.seek(std::io::SeekFrom::Start(offset)).await?;

    Ok(file)
}

/// Publishes the finished partial file under its destination name, picking a
/// fresh name when something else claimed the original in the meantime.
async fn finalize_transfer(
    temp_path: &Path,
    destination_path: &Path,
    downloaded_bytes: u64,
) -> Result<TransferOutcome> {
    let final_path = if fs::try_exists(destination_path).await? {
        let directory = destination_path.parent().unwrap_or(Path::new("."));
        let filename = destination_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("download.bin");

        available_destination_path(directory, filename).await?
    } else {
        destination_path.to_path_buf()
    };

    fs::rename(temp_path, &final_path).await?;

    Ok(TransferOutcome {
        final_path,
        downloaded_bytes,
    })
}

fn header_text(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn content_length_of(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

/// Reads the total size out of `bytes 0-0/1234`. A source that reports `*`
/// does not know its own size, and neither do we.
fn total_from_content_range(value: &str) -> Option<u64> {
    let total = value.rsplit('/').next()?.trim();

    if total == "*" {
        return None;
    }

    total.parse().ok()
}

pub(crate) fn validate_source_url(source_url: &str) -> Result<Url> {
    let parsed_url =
        Url::parse(source_url).map_err(|error| DownloadError::InvalidUrl(error.to_string()))?;

    match parsed_url.scheme() {
        "http" | "https" => Ok(parsed_url),
        scheme => Err(DownloadError::UnsupportedScheme(scheme.to_owned())),
    }
}

fn determine_filename(
    headers: &HeaderMap,
    fallback_headers: Option<&HeaderMap>,
    final_url: &Url,
) -> String {
    let disposition = header_text(headers, CONTENT_DISPOSITION)
        .or_else(|| fallback_headers.and_then(|headers| header_text(headers, CONTENT_DISPOSITION)));

    if let Some(value) = disposition.as_deref()
        && let Some(filename) = filename_from_content_disposition(value)
    {
        return filename;
    }

    final_url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .filter(|filename| !filename.is_empty())
        .map(|filename| {
            percent_decode_str(filename)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| "download.bin".to_owned())
}

fn filename_from_content_disposition(value: &str) -> Option<String> {
    for part in value.split(';').map(str::trim) {
        if let Some(encoded) = part.strip_prefix("filename*=") {
            let encoded = encoded.trim_matches('\"');
            let encoded = encoded
                .strip_prefix("UTF-8''")
                .or_else(|| encoded.strip_prefix("utf-8''"))
                .unwrap_or(encoded);

            let decoded = percent_decode_str(encoded).decode_utf8_lossy().into_owned();

            if !decoded.trim().is_empty() {
                return Some(decoded);
            }
        }
    }

    for part in value.split(';').map(str::trim) {
        if let Some(filename) = part.strip_prefix("filename=") {
            let filename = filename.trim().trim_matches('\"');

            if !filename.is_empty() {
                return Some(filename.to_owned());
            }
        }
    }

    None
}

fn sanitize_filename(filename: &str) -> String {
    let mut sanitized: String = filename
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '\"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();

    sanitized = sanitized.trim().trim_end_matches(['.', ' ']).to_owned();

    if sanitized.is_empty() {
        return "download.bin".to_owned();
    }

    let stem = Path::new(&sanitized)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();

    let reserved = matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );

    if reserved {
        sanitized.insert(0, '_');
    }

    sanitized
}

/// Reserves a destination name and its partial file together.
///
/// The partial file is created exclusively, which is what makes the
/// reservation atomic: two transfers that probe the same filename at the same
/// moment cannot both decide they own it and then fight over the same bytes.
async fn reserve_paths(directory: &Path, filename: &str) -> std::io::Result<(PathBuf, PathBuf)> {
    for candidate in candidate_names(filename) {
        let final_path = directory.join(&candidate);

        if fs::try_exists(&final_path).await? {
            continue;
        }

        let temp_path = temporary_path(&final_path);

        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .await
        {
            Ok(_) => return Ok((final_path, temp_path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "unable to allocate a unique destination filename",
    ))
}

/// Finds a free destination name without reserving it. Used at finalization,
/// where the rename happens immediately afterwards.
async fn available_destination_path(directory: &Path, filename: &str) -> std::io::Result<PathBuf> {
    for candidate in candidate_names(filename) {
        let path = directory.join(&candidate);

        if !fs::try_exists(&path).await? {
            return Ok(path);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "unable to allocate a unique destination filename",
    ))
}

/// `name.ext`, `name (1).ext`, `name (2).ext`, ... - the Windows convention
/// for a colliding download.
fn candidate_names(filename: &str) -> impl Iterator<Item = String> + '_ {
    let original = Path::new(filename);

    let stem = original
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download")
        .to_owned();

    let extension = original
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_owned);

    (0_u32..10_000).map(move |index| {
        if index == 0 {
            filename.to_owned()
        } else if let Some(extension) = &extension {
            format!("{stem} ({index}).{extension}")
        } else {
            format!("{stem} ({index})")
        }
    })
}

fn temporary_path(final_path: &Path) -> PathBuf {
    let mut value = final_path.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

#[derive(Debug, Default)]
pub struct CoreService;

impl CoreService {
    pub const fn new() -> Self {
        Self
    }

    pub const fn health_check(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{DEFAULT_BODY, ServerBehaviour, TestServer};
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn core_reports_ready() {
        let core = CoreService::new();
        assert!(core.health_check());
    }

    #[test]
    fn sanitizes_windows_filename() {
        assert_eq!(sanitize_filename("report:2026?.zip"), "report_2026_.zip");

        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
    }

    #[test]
    fn extracts_standard_content_disposition_filename() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"hello world.zip\"")
                .as_deref(),
            Some("hello world.zip")
        );
    }

    #[test]
    fn extracts_utf8_content_disposition_filename() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''hello%20world.zip")
                .as_deref(),
            Some("hello world.zip")
        );
    }

    #[tokio::test]
    async fn probing_confirms_range_support_with_a_real_partial_response() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let downloader = Downloader::new().unwrap();

        let probe = downloader.probe(&server.url("file.bin")).await.unwrap();

        assert!(probe.range_supported);
        assert_eq!(probe.total_bytes, Some(DEFAULT_BODY.len() as u64));
        assert_eq!(probe.filename, "payload.bin");
        assert_eq!(probe.etag.as_deref(), Some("\"v1\""));
    }

    #[tokio::test]
    async fn probing_reports_no_range_support_when_the_server_ignores_it() {
        let server = TestServer::start(ServerBehaviour {
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let downloader = Downloader::new().unwrap();

        let probe = downloader.probe(&server.url("file.bin")).await.unwrap();

        assert!(
            !probe.range_supported,
            "a 200 answer to a ranged request is not range support"
        );
        assert_eq!(probe.total_bytes, Some(DEFAULT_BODY.len() as u64));
    }

    #[tokio::test]
    async fn transfers_a_whole_file_and_removes_the_partial() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        let mut last_progress = None;

        let outcome = downloader
            .transfer(
                TransferRequest {
                    source_url: &server.url("file.bin"),
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: 0,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                |progress| {
                    last_progress = Some(progress);
                    Ok(())
                },
            )
            .await
            .unwrap();

        assert_eq!(outcome.downloaded_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(fs::read(&outcome.final_path).await.unwrap(), DEFAULT_BODY);
        assert!(!fs::try_exists(&temp).await.unwrap());
        assert_eq!(
            last_progress,
            Some(DownloadProgress {
                downloaded_bytes: DEFAULT_BODY.len() as u64,
                total_bytes: Some(DEFAULT_BODY.len() as u64),
            })
        );
    }

    #[tokio::test]
    async fn resumes_from_the_bytes_already_on_disk() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        // Stand in for an interrupted attempt that got this far.
        fs::write(&temp, b"the quick brown fox ").await.unwrap();

        let outcome = downloader
            .transfer(
                TransferRequest {
                    source_url: &server.url("file.bin"),
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: 20,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                |_| Ok(()),
            )
            .await
            .unwrap();

        assert_eq!(outcome.downloaded_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(
            fs::read(&outcome.final_path).await.unwrap(),
            DEFAULT_BODY,
            "a resumed transfer must produce exactly the original file"
        );
        assert_eq!(
            server.ranged_request_count(),
            1,
            "the resumed request must actually ask for a range"
        );
    }

    #[tokio::test]
    async fn restarts_when_the_server_ignores_the_requested_range() {
        let server = TestServer::start(ServerBehaviour {
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        fs::write(&temp, b"stale bytes that must not survive")
            .await
            .unwrap();

        let outcome = downloader
            .transfer(
                TransferRequest {
                    source_url: &server.url("file.bin"),
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: 32,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                |_| Ok(()),
            )
            .await
            .unwrap();

        assert_eq!(outcome.downloaded_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(
            fs::read(&outcome.final_path).await.unwrap(),
            DEFAULT_BODY,
            "a server that ignores the range must not have its body appended"
        );
    }

    #[tokio::test]
    async fn an_interrupted_body_never_becomes_a_finished_file() {
        let server = TestServer::start(ServerBehaviour {
            truncate_after: Some(10),
            ..ServerBehaviour::default()
        })
        .await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        let error = downloader
            .transfer(
                TransferRequest {
                    source_url: &server.url("file.bin"),
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: 0,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                |_| Ok(()),
            )
            .await
            .unwrap_err();

        // Either the HTTP layer notices the truncated body or the engine's
        // own byte check does; both must refuse to publish the file.
        assert!(
            matches!(
                error,
                DownloadError::Http(_) | DownloadError::IncompleteTransfer { .. }
            ),
            "unexpected error: {error:?}"
        );
        assert!(
            !fs::try_exists(&destination).await.unwrap(),
            "an incomplete transfer must not be published under its final name"
        );
    }

    #[tokio::test]
    async fn pausing_stops_between_chunks_and_keeps_the_partial_file() {
        let server = TestServer::start(ServerBehaviour {
            chunk_delay: Some(Duration::from_millis(20)),
            chunk_size: 4,
            ..ServerBehaviour::default()
        })
        .await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = Arc::new(TaskControl::new());
        let pauser = Arc::clone(&control);

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        let source_url = server.url("file.bin");

        let transfer = downloader.transfer(
            TransferRequest {
                source_url: &source_url,
                temp_path: &temp,
                destination_path: &destination,
                start_offset: 0,
                total_bytes: Some(DEFAULT_BODY.len() as u64),
            },
            &control,
            move |progress| {
                if progress.downloaded_bytes >= 8 {
                    pauser.request(StopReason::Pause);
                }

                Ok(())
            },
        );

        let error = transfer.await.unwrap_err();

        assert!(matches!(error, DownloadError::Stopped(StopReason::Pause)));

        let partial = fs::metadata(&temp).await.unwrap().len();

        assert!(
            (8..DEFAULT_BODY.len() as u64).contains(&partial),
            "the partial file must hold what was transferred: {partial}"
        );
        assert!(!fs::try_exists(&destination).await.unwrap());
    }

    #[tokio::test]
    async fn a_paused_transfer_can_be_finished_by_a_second_attempt() {
        let server = TestServer::start(ServerBehaviour {
            chunk_delay: Some(Duration::from_millis(20)),
            chunk_size: 4,
            ..ServerBehaviour::default()
        })
        .await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = Arc::new(TaskControl::new());
        let pauser = Arc::clone(&control);
        let source_url = server.url("file.bin");

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        let _ = downloader
            .transfer(
                TransferRequest {
                    source_url: &source_url,
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: 0,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                move |progress| {
                    if progress.downloaded_bytes >= 8 {
                        pauser.request(StopReason::Pause);
                    }

                    Ok(())
                },
            )
            .await;

        let resumed_from = fs::metadata(&temp).await.unwrap().len();
        let resumed_control = TaskControl::new();

        let outcome = downloader
            .transfer(
                TransferRequest {
                    source_url: &source_url,
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: resumed_from,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &resumed_control,
                |_| Ok(()),
            )
            .await
            .unwrap();

        assert_eq!(outcome.downloaded_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(fs::read(&outcome.final_path).await.unwrap(), DEFAULT_BODY);
    }

    #[tokio::test]
    async fn an_already_complete_partial_file_is_only_finalized() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        fs::write(&temp, DEFAULT_BODY).await.unwrap();

        let outcome = downloader
            .transfer(
                TransferRequest {
                    source_url: &server.url("file.bin"),
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: DEFAULT_BODY.len() as u64,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                |_| Ok(()),
            )
            .await
            .unwrap();

        assert_eq!(outcome.downloaded_bytes, DEFAULT_BODY.len() as u64);
        assert_eq!(
            server.request_count(),
            0,
            "nothing is left to fetch, so no request should be made"
        );
    }

    #[tokio::test]
    async fn a_colliding_destination_gets_its_own_name() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();

        let (destination, temp) = downloader
            .plan_paths(directory.path(), "payload.bin")
            .await
            .unwrap();

        // Something else claimed the name between planning and finalizing.
        fs::write(&destination, b"already here").await.unwrap();

        let outcome = downloader
            .transfer(
                TransferRequest {
                    source_url: &server.url("file.bin"),
                    temp_path: &temp,
                    destination_path: &destination,
                    start_offset: 0,
                    total_bytes: Some(DEFAULT_BODY.len() as u64),
                },
                &control,
                |_| Ok(()),
            )
            .await
            .unwrap();

        assert_ne!(outcome.final_path, destination);
        assert_eq!(
            fs::read(&destination).await.unwrap(),
            b"already here",
            "the existing file must be left alone"
        );
    }

    #[test]
    fn reads_the_total_size_out_of_a_content_range() {
        assert_eq!(total_from_content_range("bytes 0-0/1234"), Some(1_234));
        assert_eq!(total_from_content_range("bytes 0-0/*"), None);
        assert_eq!(total_from_content_range("nonsense"), None);
    }
}
