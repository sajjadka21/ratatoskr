pub mod adaptive;
pub mod browser;
pub mod control;
pub mod dash;
pub mod diagnostics;
pub mod export;
pub mod ffmpeg;
pub mod hls;
pub mod linkgrabber;
pub mod media;
pub mod network;
pub mod postprocess;
pub mod queue;
pub mod ratelimit;
pub mod resume;
pub mod retry;
pub mod rules;
pub mod segment_planner;
pub mod service;
pub mod session;
pub mod slot;
pub mod stats;
#[cfg(test)]
pub mod testing;
pub mod throughput;
pub mod traffic;

use crate::control::{StopReason, TaskControl};
use crate::ratelimit::RateLimiter;
use crate::slot::RangeSlot;
use dm_common::RequestContext;
use percent_encoding::percent_decode_str;
use reqwest::{
    Client, RequestBuilder, StatusCode, Url,
    header::{
        ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_TYPE, COOKIE, ETAG, HeaderMap,
        HeaderValue, LAST_MODIFIED, RANGE, REFERER, USER_AGENT,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
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

    #[error("server returned an invalid ranged response")]
    InvalidRangeResponse {
        expected_start: u64,
        expected_end: u64,
        actual: String,
    },

    #[error("segment received more than its expected length")]
    SegmentOverflow { expected: u64, actual: u64 },

    #[error("server returned HTTP status {status}")]
    HttpStatus { status: u16 },

    #[error("{0}")]
    Stream(#[from] hls::HlsError),

    #[error("the server sent more than {limit} bytes for one part of a stream")]
    TooLarge { limit: usize },

    #[error("{0}")]
    Ffmpeg(String),
}

impl DownloadError {
    /// The HTTP status behind this error, whether the server's answer was
    /// checked by the engine or by the HTTP client.
    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::HttpStatus { status } => Some(*status),
            Self::Http(error) => error.status().map(|status| status.as_u16()),
            _ => None,
        }
    }

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
            Self::InvalidRangeResponse { .. } => {
                "the server did not honor the requested byte range".to_owned()
            }
            Self::SegmentOverflow { expected, actual } => {
                format!("the server sent {actual} bytes for a segment limited to {expected}")
            }
            Self::HttpStatus { status } => {
                format!("the download server returned HTTP status {status}")
            }
            Self::Stream(error) => error.to_string(),
            Self::Ffmpeg(message) => message.clone(),
            Self::TooLarge { limit } => {
                format!("the server sent more than {limit} bytes for one part of a stream")
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
    pub active_connections: Option<u32>,
    pub max_connections: Option<u32>,
    pub adaptive_reason: Option<&'static str>,
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

/// One connection's share of a segmented transfer. All connections write
/// into the same preallocated partial file.
#[derive(Debug, Clone)]
pub struct RangeTransferRequest<'a> {
    pub source_url: &'a str,
    pub file_path: &'a Path,
    pub total_bytes: u64,
    /// Written bytes between two flushes to disk.
    pub checkpoint_bytes: u64,
    /// Longest time between two flushes to disk.
    pub checkpoint_interval: Duration,
}

/// Reported while a range downloads: `written` bytes were just written, and,
/// after a flush, `durable_bytes` of the range are safely on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeProgress {
    pub written: u64,
    pub durable_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeTransferOutcome {
    pub downloaded_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferOutcome {
    pub final_path: PathBuf,
    pub downloaded_bytes: u64,
}

/// Sent when a task carries no browser User-Agent. Many CDNs refuse or
/// throttle clients that do not look like a browser, so the default matches
/// a current desktop browser and still names this application.
/// Longest `Retry-After` honoured, so a hostile answer cannot park a
/// download for days.
const MAX_RETRY_AFTER_SECONDS: u64 = 3600;

pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 DownloadManager/1.0";

#[derive(Debug, Clone)]
pub struct Downloader {
    client: Client,
    referrer: Option<HeaderValue>,
    user_agent: HeaderValue,
    /// Every limiter a transfer must satisfy: the application-wide limit,
    /// plus a per-task one when a rule caps the task.
    limiters: Vec<Arc<RateLimiter>>,
    /// The browser session of one task, attached only to requests on the
    /// origin it was captured for. Held in memory, never persisted.
    session: Option<Arc<session::BrowserSession>>,
    /// `Retry-After` answers seen per host, in seconds, until someone reads
    /// them. Shared by every clone of this downloader.
    retry_hints: Arc<std::sync::Mutex<std::collections::HashMap<String, u64>>>,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        Self::with_network(&network::NetworkSettings::default())
    }

    /// A downloader whose connections follow `settings`: direct, through the
    /// system proxy, or through the user's proxy with direct exceptions.
    pub fn with_network(settings: &network::NetworkSettings) -> Result<Self> {
        Self::with_network_and_stall(settings, network::DEFAULT_STALL_TIMEOUT)
    }

    /// As [`Downloader::with_network`], giving up on a connection that
    /// delivers nothing for `stall_timeout`.
    pub fn with_network_and_stall(
        settings: &network::NetworkSettings,
        stall_timeout: std::time::Duration,
    ) -> Result<Self> {
        let client = settings.build_client_with(stall_timeout)?;

        Ok(Self {
            client,
            referrer: None,
            user_agent: HeaderValue::from_static(DEFAULT_USER_AGENT),
            limiters: Vec::new(),
            session: None,
            retry_hints: Arc::default(),
        })
    }

    /// The same downloader, additionally bound by `limiter`. Limiters stack:
    /// a transfer moves at the slowest of them.
    pub fn with_limiter(&self, limiter: Arc<RateLimiter>) -> Self {
        let mut downloader = self.clone();
        downloader.limiters.push(limiter);
        downloader
    }

    /// Waits until the bytes just written fit every bandwidth limit. A pause
    /// or cancel ends the wait immediately.
    async fn throttle(&self, bytes: usize, control: &TaskControl) -> Result<()> {
        for limiter in &self.limiters {
            tokio::select! {
                biased;
                reason = control.stopped() => return Err(DownloadError::Stopped(reason)),
                () = limiter.acquire(bytes) => {}
            }
        }
        Ok(())
    }

    /// A downloader that sends the browser context of one task. The HTTP
    /// client and its connection pool are shared; only the headers differ.
    /// Values that are not valid header text are ignored rather than sent.
    pub fn with_context(&self, context: &RequestContext) -> Self {
        let valid = |value: &Option<String>| {
            value
                .as_deref()
                .and_then(|text| HeaderValue::from_str(text).ok())
        };

        Self {
            client: self.client.clone(),
            referrer: valid(&context.referrer),
            user_agent: valid(&context.user_agent).unwrap_or_else(|| self.user_agent.clone()),
            limiters: self.limiters.clone(),
            session: self.session.clone(),
            retry_hints: Arc::clone(&self.retry_hints),
        }
    }

    /// The same downloader, carrying one task's browser session.
    pub fn with_session(&self, session: Option<Arc<session::BrowserSession>>) -> Self {
        let mut downloader = self.clone();
        downloader.session = session;
        downloader
    }

    /// Remembers how long a server that answered 429 or 503 asked to be
    /// left alone. Only the delay-in-seconds form is read; the date form is
    /// rare for downloads and ignored.
    fn remember_retry_after(&self, response: &reqwest::Response) {
        if !matches!(response.status().as_u16(), 429 | 503) {
            return;
        }
        let Some(seconds) = header_text(response.headers(), reqwest::header::RETRY_AFTER)
            .and_then(|value| value.trim().parse::<u64>().ok())
        else {
            return;
        };
        if let (Some(host), Ok(mut hints)) = (response.url().host_str(), self.retry_hints.lock()) {
            hints.insert(
                host.to_ascii_lowercase(),
                seconds.min(MAX_RETRY_AFTER_SECONDS),
            );
        }
    }

    /// The wait a server asked for, if it asked; reading it clears it.
    pub fn take_retry_after(&self, url: &str) -> Option<Duration> {
        let host = Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
        self.retry_hints
            .lock()
            .ok()?
            .remove(&host)
            .map(Duration::from_secs)
    }

    fn request(&self, method: reqwest::Method, url: Url) -> RequestBuilder {
        // Decided per request, not per task: a transfer that moves to another
        // origin, such as a CDN after a redirect, must not carry the cookie.
        let cookie = self
            .session
            .as_ref()
            .filter(|session| session.applies_to(&url))
            .map(|session| session.header().clone());

        let builder = self
            .client
            .request(method, url)
            .header(USER_AGENT, self.user_agent.clone())
            .header(ACCEPT_ENCODING, "identity");

        let builder = match &self.referrer {
            Some(referrer) => builder.header(REFERER, referrer.clone()),
            None => builder,
        };

        match cookie {
            Some(cookie) => builder.header(COOKIE, cookie),
            None => builder,
        }
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
            .request(reqwest::Method::HEAD, url.clone())
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => Some(response.headers().clone()),
            _ => None,
        };

        let response = self
            .request(reqwest::Method::GET, url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await?;
        self.remember_retry_after(&response);
        let response = response.error_for_status()?;

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

            if let Err(error) = self.throttle(chunk.len(), control).await {
                file.flush().await?;
                file.sync_all().await?;
                return Err(error);
            }
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

    /// Downloads the range of `slot` straight into the shared partial file,
    /// at its own offset. A response is accepted only when it is a precise
    /// `206` for the requested range; a `200` or a mismatched
    /// `Content-Range` is rejected so that a server which ignores ranges can
    /// never write the wrong bytes into the file.
    ///
    /// The slot may shrink while this runs (another connection took over its
    /// tail); the transfer then stops at the new end. Every `checkpoint`
    /// bytes, and whenever it stops, the file is flushed to disk before the
    /// durable byte count is reported, so a recorded count never runs ahead
    /// of what a power cut would leave behind.
    pub async fn transfer_range<F>(
        &self,
        request: RangeTransferRequest<'_>,
        slot: &RangeSlot,
        control: &TaskControl,
        mut on_progress: F,
    ) -> Result<RangeTransferOutcome>
    where
        F: FnMut(RangeProgress) -> Result<()> + Send,
    {
        let start = slot.start();
        let mut written_to = slot.next();
        let requested_end = slot.end();
        if written_to > requested_end {
            return Ok(RangeTransferOutcome {
                downloaded_bytes: written_to - start,
            });
        }

        let url = validate_source_url(request.source_url)?;
        let response = self
            .request(reqwest::Method::GET, url)
            .header(RANGE, format!("bytes={written_to}-{requested_end}"))
            .send()
            .await?;

        self.remember_retry_after(&response);
        if response.status() != StatusCode::PARTIAL_CONTENT
            && (response.status().is_client_error() || response.status().is_server_error())
        {
            return Err(DownloadError::HttpStatus {
                status: response.status().as_u16(),
            });
        }

        let content_range = header_text(response.headers(), CONTENT_RANGE);
        let parsed_range = content_range.as_deref().and_then(parse_content_range);
        let valid_response = response.status() == StatusCode::PARTIAL_CONTENT
            && parsed_range.is_some_and(|(first, last, total)| {
                first == written_to && last == requested_end && total == Some(request.total_bytes)
            });

        if !valid_response {
            let actual = content_range.unwrap_or_else(|| response.status().to_string());
            return Err(DownloadError::InvalidRangeResponse {
                expected_start: written_to,
                expected_end: requested_end,
                actual,
            });
        }

        let mut file = OpenOptions::new()
            .write(true)
            .open(request.file_path)
            .await?;
        file.seek(std::io::SeekFrom::Start(written_to)).await?;

        let mut durable_to = written_to;
        let mut last_checkpoint = std::time::Instant::now();
        let mut response = response;

        let result: Result<()> = async {
            loop {
                let chunk = tokio::select! {
                    biased;
                    reason = control.stopped() => return Err(DownloadError::Stopped(reason)),
                    chunk = response.chunk() => chunk?,
                };

                let Some(chunk) = chunk else {
                    break;
                };

                let (offset, allowed) = slot.claim(chunk.len() as u64);
                if offset != written_to {
                    return Err(DownloadError::InvalidRangeResponse {
                        expected_start: written_to,
                        expected_end: slot.end(),
                        actual: "range claimed out of order".to_owned(),
                    });
                }
                let allowed_len = usize::try_from(allowed).unwrap_or(chunk.len());
                if allowed_len > 0 {
                    file.write_all(&chunk[..allowed_len]).await?;
                    written_to += allowed;
                    on_progress(RangeProgress {
                        written: allowed,
                        durable_bytes: None,
                    })?;
                }

                // The range ended here, either as requested or because its
                // tail was handed to another connection.
                if allowed_len < chunk.len() || written_to > slot.end() {
                    break;
                }

                if written_to - durable_to >= request.checkpoint_bytes
                    || last_checkpoint.elapsed() >= request.checkpoint_interval
                {
                    file.flush().await?;
                    file.sync_data().await?;
                    durable_to = written_to;
                    last_checkpoint = std::time::Instant::now();
                    on_progress(RangeProgress {
                        written: 0,
                        durable_bytes: Some(durable_to - start),
                    })?;
                }

                self.throttle(allowed_len, control).await?;
            }
            Ok(())
        }
        .await;

        // Whatever happened, what was written is made durable and reported,
        // so a pause or an error resumes from the right byte.
        let flushed = async {
            file.flush().await?;
            file.sync_data().await
        }
        .await;
        drop(file);
        if flushed.is_ok() && written_to > durable_to {
            on_progress(RangeProgress {
                written: 0,
                durable_bytes: Some(written_to - start),
            })?;
        }
        result?;
        flushed?;

        let end = slot.end();
        if written_to != end + 1 {
            return Err(DownloadError::IncompleteTransfer {
                expected: end + 1 - start,
                actual: written_to - start,
            });
        }

        Ok(RangeTransferOutcome {
            downloaded_bytes: written_to - start,
        })
    }

    /// Reads a whole small resource — a playlist, a key, one stream segment
    /// — into memory, never more than `limit` bytes. `range` is
    /// `(length, offset)`; a server that ignores it and sends the whole
    /// resource is handled by cutting the range out.
    pub async fn fetch_bytes(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
        limit: usize,
        control: &TaskControl,
    ) -> Result<Vec<u8>> {
        let url = validate_source_url(url)?;
        let mut builder = self.request(reqwest::Method::GET, url);
        if let Some((length, offset)) = range
            && length > 0
        {
            builder = builder.header(RANGE, format!("bytes={offset}-{}", offset + length - 1));
        }
        let response = builder.send().await?;
        self.remember_retry_after(&response);
        if !response.status().is_success() {
            return Err(DownloadError::HttpStatus {
                status: response.status().as_u16(),
            });
        }
        let whole_resource = response.status() != StatusCode::PARTIAL_CONTENT;

        let mut response = response;
        let mut data = Vec::new();
        loop {
            let chunk = tokio::select! {
                biased;
                reason = control.stopped() => return Err(DownloadError::Stopped(reason)),
                chunk = response.chunk() => chunk?,
            };
            let Some(chunk) = chunk else {
                break;
            };
            if data.len() + chunk.len()
                > limit.saturating_add(range.map_or(0, |(_, offset)| offset as usize))
            {
                return Err(DownloadError::TooLarge { limit });
            }
            data.extend_from_slice(&chunk);
            self.throttle(chunk.len(), control).await?;
        }

        if let Some((length, offset)) = range
            && whole_resource
        {
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            let end = start.saturating_add(usize::try_from(length).unwrap_or(usize::MAX));
            if end > data.len() {
                return Err(DownloadError::IncompleteTransfer {
                    expected: end as u64,
                    actual: data.len() as u64,
                });
            }
            data = data[start..end].to_vec();
        }
        if data.len() > limit {
            return Err(DownloadError::TooLarge { limit });
        }
        Ok(data)
    }

    /// Publishes a shared partial file once every range in it is complete.
    pub async fn finalize_shared_file(
        &self,
        temp_path: &Path,
        destination_path: &Path,
        total_bytes: u64,
    ) -> Result<TransferOutcome> {
        let length = fs::metadata(temp_path).await?.len();
        if length != total_bytes {
            return Err(DownloadError::IncompleteTransfer {
                expected: total_bytes,
                actual: length,
            });
        }
        finalize_transfer(temp_path, destination_path, total_bytes).await
    }

    /// Issues the body request, retrying without a range when the server
    /// rejects the one we asked for.
    async fn request_body(&self, url: &Url, start_offset: u64) -> Result<reqwest::Response> {
        let mut builder = self.request(reqwest::Method::GET, url.clone());

        if start_offset > 0 {
            builder = builder.header(RANGE, format!("bytes={start_offset}-"));
        }

        let response = builder.send().await?;
        self.remember_retry_after(&response);

        // The stored offset no longer fits the resource; start over rather
        // than failing a transfer that can simply begin again.
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE && start_offset > 0 {
            return Ok(self
                .request(reqwest::Method::GET, url.clone())
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

/// Creates (or re-creates) the partial file of a segmented transfer at its
/// full size, so every connection can write at its own offset.
pub async fn preallocate_partial_file(temp_path: &Path, total_bytes: u64) -> Result<()> {
    if let Some(parent) = temp_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let path = temp_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        // Before the size is set, so Windows never zero-fills the gaps.
        dm_system::sparse::mark_sparse(&file);
        file.set_len(total_bytes)?;
        file.sync_all()
    })
    .await
    .map_err(|error| DownloadError::Io(std::io::Error::other(error)))??;
    Ok(())
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

fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let (unit, range_and_total) = value.split_once(' ')?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return None;
    }

    let (range, total) = range_and_total.trim().split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = end.trim().parse().ok()?;
    if start > end {
        return None;
    }

    let total = total.trim();
    let total = if total == "*" {
        None
    } else {
        Some(total.parse().ok()?)
    };

    Some((start, end, total))
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

    fn range_request<'a>(url: &'a str, file_path: &'a Path) -> RangeTransferRequest<'a> {
        RangeTransferRequest {
            source_url: url,
            file_path,
            total_bytes: DEFAULT_BODY.len() as u64,
            checkpoint_bytes: 4,
            checkpoint_interval: Duration::from_secs(60),
        }
    }

    #[tokio::test]
    async fn writes_a_range_at_its_own_offset_in_the_shared_file() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let url = server.url("file.bin");
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();
        let temp = directory.path().join("file.bin.part");
        let total = DEFAULT_BODY.len() as u64;
        preallocate_partial_file(&temp, total).await.unwrap();
        let slot = RangeSlot::new(4, 14, 0);
        let mut durable = Vec::new();

        let outcome = downloader
            .transfer_range(range_request(&url, &temp), &slot, &control, |progress| {
                durable.extend(progress.durable_bytes);
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(outcome.downloaded_bytes, 11);
        let bytes = fs::read(&temp).await.unwrap();
        assert_eq!(bytes.len() as u64, total);
        assert_eq!(bytes[4..=14], DEFAULT_BODY[4..=14]);
        assert!(bytes[..4].iter().all(|byte| *byte == 0));
        // Durable counts only grow and end at the full range.
        assert!(durable.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(durable.last(), Some(&11));
    }

    #[tokio::test]
    async fn a_range_that_was_split_stops_at_its_new_end() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let url = server.url("file.bin");
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();
        let temp = directory.path().join("file.bin.part");
        let total = DEFAULT_BODY.len() as u64;
        preallocate_partial_file(&temp, total).await.unwrap();

        let head = RangeSlot::new(0, total - 1, 0);
        let (tail_start, tail_end) = head.split_off(4).unwrap();
        let tail = RangeSlot::new(tail_start, tail_end, 0);

        for slot in [&head, &tail] {
            downloader
                .transfer_range(range_request(&url, &temp), slot, &control, |_| Ok(()))
                .await
                .unwrap();
        }

        assert_eq!(fs::read(&temp).await.unwrap(), DEFAULT_BODY);
    }

    #[tokio::test]
    async fn rejects_a_server_that_ignores_segment_ranges() {
        let server = TestServer::start(ServerBehaviour {
            supports_range: false,
            ..ServerBehaviour::default()
        })
        .await;
        let url = server.url("file.bin");
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();
        let temp = directory.path().join("file.bin.part");
        preallocate_partial_file(&temp, DEFAULT_BODY.len() as u64)
            .await
            .unwrap();
        let slot = RangeSlot::new(4, 14, 0);

        let error = downloader
            .transfer_range(range_request(&url, &temp), &slot, &control, |_| Ok(()))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::InvalidRangeResponse { .. }));
        assert!(fs::read(&temp).await.unwrap().iter().all(|byte| *byte == 0));
    }

    #[tokio::test]
    async fn reports_rate_limit_statuses_as_typed_segment_errors() {
        let server = TestServer::start(ServerBehaviour {
            status: Some((429, "Too Many Requests")),
            ..ServerBehaviour::default()
        })
        .await;
        let url = server.url("file.bin");
        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();
        let control = TaskControl::new();
        let temp = directory.path().join("file.bin.part");
        preallocate_partial_file(&temp, DEFAULT_BODY.len() as u64)
            .await
            .unwrap();
        let slot = RangeSlot::new(0, 4, 0);

        let error = downloader
            .transfer_range(range_request(&url, &temp), &slot, &control, |_| Ok(()))
            .await
            .unwrap_err();

        assert!(matches!(error, DownloadError::HttpStatus { status: 429 }));
    }

    #[tokio::test]
    async fn a_manual_proxy_carries_requests_except_for_direct_hosts() {
        let proxy = TestServer::start(ServerBehaviour::default()).await;
        let proxy_url = proxy.url("");
        let settings = network::NetworkSettings {
            mode: network::ProxyMode::Manual,
            proxy_url: Some(proxy_url.trim_end_matches('/').to_owned()),
            direct_hosts: vec!["direct.invalid".to_owned()],
            domestic_direct: true,
            domestic_hosts: Vec::new(),
        };
        let downloader = Downloader::with_network(&settings).unwrap();

        // The host does not exist: only the proxy can answer.
        let probe = downloader
            .probe("http://files.proxied.invalid/file.bin")
            .await
            .unwrap();
        assert_eq!(probe.total_bytes, Some(DEFAULT_BODY.len() as u64));
        let through_proxy = proxy.request_count();
        assert!(through_proxy > 0);

        // Direct and domestic hosts never reach the proxy.
        assert!(
            downloader
                .probe("http://direct.invalid/file.bin")
                .await
                .is_err()
        );
        assert!(
            downloader
                .probe("http://dl.site.ir.invalid.ir/file.bin")
                .await
                .is_err()
        );
        assert_eq!(proxy.request_count(), through_proxy);
    }

    #[test]
    fn parses_content_range_only_when_its_shape_is_valid() {
        assert_eq!(
            parse_content_range("bytes 4-14/44"),
            Some((4, 14, Some(44)))
        );
        assert_eq!(parse_content_range("bytes 4-14/*"), Some((4, 14, None)));
        assert_eq!(parse_content_range("bytes 14-4/44"), None);
        assert_eq!(parse_content_range("items 4-14/44"), None);
    }

    #[test]
    fn reads_the_total_size_out_of_a_content_range() {
        assert_eq!(total_from_content_range("bytes 0-0/1234"), Some(1_234));
        assert_eq!(total_from_content_range("bytes 0-0/*"), None);
        assert_eq!(total_from_content_range("nonsense"), None);
    }
}
