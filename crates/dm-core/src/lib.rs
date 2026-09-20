pub mod service;

use percent_encoding::percent_decode_str;
use reqwest::{
    Client, Url,
    header::{ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_TYPE},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub metadata: DownloadMetadata,
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

    pub async fn download<F>(
        &self,
        source_url: &str,
        destination_directory: impl AsRef<Path>,
        mut on_progress: F,
    ) -> Result<DownloadOutcome>
    where
        F: FnMut(DownloadProgress) -> Result<()> + Send,
    {
        let parsed_url =
            Url::parse(source_url).map_err(|error| DownloadError::InvalidUrl(error.to_string()))?;

        match parsed_url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(DownloadError::UnsupportedScheme(scheme.to_owned()));
            }
        }

        let destination_directory = destination_directory.as_ref();
        fs::create_dir_all(destination_directory).await?;

        let mut response = self
            .client
            .get(parsed_url)
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await?
            .error_for_status()?;

        let final_url = response.url().clone();
        let headers = response.headers().clone();
        let total_bytes = response.content_length();

        let filename = determine_filename(&headers, &final_url);
        let filename = sanitize_filename(&filename);

        let final_path = unique_destination_path(destination_directory, &filename).await?;

        let temp_path = temporary_path(&final_path);

        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .await?;

        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let metadata = DownloadMetadata {
            source_url: source_url.to_owned(),
            final_url: final_url.to_string(),
            filename,
            content_type,
            total_bytes,
        };

        let mut downloaded_bytes = 0_u64;

        on_progress(DownloadProgress {
            downloaded_bytes,
            total_bytes,
        })?;

        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;

            downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);

            on_progress(DownloadProgress {
                downloaded_bytes,
                total_bytes,
            })?;
        }

        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        fs::rename(&temp_path, &final_path).await?;

        Ok(DownloadOutcome {
            metadata,
            final_path,
            downloaded_bytes,
        })
    }
}

fn determine_filename(headers: &reqwest::header::HeaderMap, final_url: &Url) -> String {
    if let Some(value) = headers
        .get(CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
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

async fn unique_destination_path(directory: &Path, filename: &str) -> std::io::Result<PathBuf> {
    let original = Path::new(filename);

    let stem = original
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");

    let extension = original.extension().and_then(|value| value.to_str());

    for index in 0_u32..10_000 {
        let candidate_name = if index == 0 {
            filename.to_owned()
        } else if let Some(extension) = extension {
            format!("{stem} ({index}).{extension}")
        } else {
            format!("{stem} ({index})")
        };

        let candidate = directory.join(candidate_name);
        let temp = temporary_path(&candidate);

        if !fs::try_exists(&candidate).await? && !fs::try_exists(&temp).await? {
            return Ok(candidate);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "unable to allocate a unique destination filename",
    ))
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
    use tempfile::tempdir;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

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
    async fn downloads_response_to_disk() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();

            let mut buffer = [0_u8; 2048];
            let _ = socket.read(&mut buffer).await.unwrap();

            let body = b"hello world";

            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Length: {}\r\n\
                 Content-Type: text/plain\r\n\
                 Content-Disposition: attachment; filename=\"hello.txt\"\r\n\
                 Connection: close\r\n\
                 \r\n",
                body.len()
            );

            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
            socket.shutdown().await.unwrap();
        });

        let directory = tempdir().unwrap();
        let downloader = Downloader::new().unwrap();

        let mut last_progress = None;

        let outcome = downloader
            .download(
                &format!("http://{address}/test"),
                directory.path(),
                |progress| {
                    last_progress = Some(progress);
                    Ok(())
                },
            )
            .await
            .unwrap();

        server.await.unwrap();

        let bytes = fs::read(&outcome.final_path).await.unwrap();

        assert_eq!(bytes, b"hello world");
        assert_eq!(outcome.metadata.filename, "hello.txt");
        assert_eq!(outcome.downloaded_bytes, 11);
        assert_eq!(
            last_progress,
            Some(DownloadProgress {
                downloaded_bytes: 11,
                total_bytes: Some(11),
            })
        );

        assert!(
            !fs::try_exists(temporary_path(&outcome.final_path))
                .await
                .unwrap()
        );
    }
}
