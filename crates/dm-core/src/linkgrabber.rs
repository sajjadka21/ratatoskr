use crate::Downloader;
use reqwest::Url;
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{sync::Semaphore, task::JoinSet};

/// Most links one pattern may expand to. A typo such as `[1-100000]` would
/// otherwise create a hundred thousand tasks.
pub const MAX_GENERATED_LINKS: usize = 1_000;

/// Most links checked in one request, matching what the browser may hand
/// over in one LinkGrabber launch.
pub const MAX_PROBED_LINKS: usize = 500;

/// Links checked at the same time. Checking is one or two small requests per
/// link, but hundreds at once would still look like an attack to a server.
pub const PROBE_CONCURRENCY: usize = 4;

/// A link that does not answer within this long is reported as unreachable
/// instead of holding up the rest of the batch.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCandidate {
    pub url: String,
    pub host: String,
    pub extension: Option<String>,
}

/// Extracts direct HTTP(S) links from plain text or pasted HTML. The parser
/// is intentionally conservative: it never executes markup and only returns
/// normalized URLs accepted by the download engine.
pub fn extract_links(input: &str) -> Vec<LinkCandidate> {
    // Links keep the order they appear in, so a page's numbered parts stay in
    // sequence instead of sorting part10 before part2.
    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    for token in
        input.split(|character: char| character.is_whitespace() || "<>\"'`".contains(character))
    {
        let token = token.trim_matches(|character: char| "<>\"'`([{".contains(character));
        let candidate = token.trim_end_matches([',', '.', ';', '!', '?', ')', ']', '}']);
        if let Some(link) = candidate_for(candidate)
            && seen.insert(link.url.clone())
        {
            candidates.push(link);
        }
    }
    candidates
}

/// Validates and normalizes one link, or returns `None` if the download
/// engine would refuse it.
pub fn candidate_for(value: &str) -> Option<LinkCandidate> {
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let extension = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, value)| value.to_ascii_lowercase());

    Some(LinkCandidate {
        url: url.to_string(),
        host,
        extension,
    })
}

/// File types that are worth offering as a download when a link to one is
/// copied. Web pages, scripts and images embedded in pages are left alone.
const DOWNLOAD_EXTENSIONS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "exe", "msi", "msix", "appx",
    "apk", "dmg", "pkg", "deb", "rpm", "appimage", "iso", "img", "vhd", "vhdx", "mp4", "mkv",
    "webm", "mov", "avi", "wmv", "flv", "m4v", "ts", "mp3", "m4a", "aac", "flac", "wav", "ogg",
    "opus", "wma", "pdf", "epub", "djvu", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "torrent",
    "bin", "part1", "001",
];

/// Most copied text looked at; anything longer is not a link someone copied.
const MAX_WATCHED_TEXT: usize = 64 * 1024;
/// Most links offered from one copy.
const MAX_WATCHED_LINKS: usize = 50;

/// The links in copied text that look like downloads: files of a known type,
/// or video pages yt-dlp can read. Used by the clipboard watcher, so it
/// stays quiet for ordinary web pages.
pub fn downloadable_links(text: &str) -> Vec<String> {
    if text.len() > MAX_WATCHED_TEXT {
        return Vec::new();
    }
    extract_links(text)
        .into_iter()
        .filter(|link| {
            crate::ytdlp::handles(&link.url)
                || link
                    .extension
                    .as_deref()
                    .is_some_and(|extension| DOWNLOAD_EXTENSIONS.contains(&extension))
                || link.extension.as_deref().is_some_and(|extension| {
                    extension.starts_with('r')
                        && extension[1..].chars().all(|c| c.is_ascii_digit())
                        && extension.len() == 3
                })
        })
        .map(|link| link.url)
        .take(MAX_WATCHED_LINKS)
        .collect()
}

/// Expands a pattern into validated links, keeping the pattern's order so a
/// numbered series stays numbered.
pub fn generate_links(pattern: &str) -> Result<Vec<LinkCandidate>, PatternError> {
    let mut seen = std::collections::HashSet::new();
    Ok(expand_pattern(pattern)?
        .iter()
        .filter_map(|link| candidate_for(link))
        .filter(|candidate| seen.insert(candidate.url.clone()))
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PatternError {
    #[error("the pattern has no range such as [1-20] or [a-z]")]
    NoRange,

    #[error("a range must count upwards, such as [1-20]")]
    Backwards,

    #[error("the pattern would create more than {MAX_GENERATED_LINKS} links")]
    TooMany,
}

/// One `[start-end]` placeholder inside a pattern.
#[derive(Debug, Clone)]
enum Range {
    Numbers { start: u64, end: u64, width: usize },
    Letters { start: char, end: char },
}

impl Range {
    fn parse(body: &str) -> Option<Result<Self, PatternError>> {
        let (start, end) = body.split_once('-')?;

        if !start.is_empty()
            && !end.is_empty()
            && start.bytes().all(|byte| byte.is_ascii_digit())
            && end.bytes().all(|byte| byte.is_ascii_digit())
        {
            let (first, last) = (start.parse::<u64>().ok()?, end.parse::<u64>().ok()?);

            if last < first {
                return Some(Err(PatternError::Backwards));
            }

            // `[01-20]` keeps two digits; `[1-20]` does not pad.
            let width = if start.len() > 1 && start.starts_with('0') {
                start.len()
            } else {
                0
            };

            return Some(Ok(Self::Numbers {
                start: first,
                end: last,
                width,
            }));
        }

        let mut first = start.chars();
        let mut last = end.chars();

        match (first.next(), first.next(), last.next(), last.next()) {
            (Some(a), None, Some(b), None)
                if a.is_ascii_alphabetic()
                    && b.is_ascii_alphabetic()
                    && a.is_ascii_lowercase() == b.is_ascii_lowercase() =>
            {
                if b < a {
                    Some(Err(PatternError::Backwards))
                } else {
                    Some(Ok(Self::Letters { start: a, end: b }))
                }
            }
            _ => None,
        }
    }

    fn len(&self) -> u64 {
        match self {
            Self::Numbers { start, end, .. } => end - start + 1,
            Self::Letters { start, end } => (*end as u64) - (*start as u64) + 1,
        }
    }

    fn values(&self) -> Vec<String> {
        match self {
            Self::Numbers { start, end, width } => (*start..=*end)
                .map(|value| format!("{value:0width$}", width = *width))
                .collect(),
            Self::Letters { start, end } => (*start..=*end).map(String::from).collect(),
        }
    }
}

/// Expands a sequential pattern such as `https://a.test/part[01-20].rar`
/// into the links it stands for. Several ranges multiply out, so
/// `[1-2]/[a-b]` gives four links. Brackets that are not a range, such as an
/// IPv6 host, are kept as they are.
pub fn expand_pattern(pattern: &str) -> Result<Vec<String>, PatternError> {
    // Literal text and ranges, in order.
    let mut pieces: Vec<Result<String, Range>> = Vec::new();
    let mut literal = String::new();
    let mut rest = pattern.trim();

    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']').map(|offset| open + offset) else {
            break;
        };

        match Range::parse(&rest[open + 1..close]) {
            Some(Ok(range)) => {
                literal.push_str(&rest[..open]);
                pieces.push(Ok(std::mem::take(&mut literal)));
                pieces.push(Err(range));
            }
            Some(Err(error)) => return Err(error),
            None => literal.push_str(&rest[..=close]),
        }

        rest = &rest[close + 1..];
    }

    literal.push_str(rest);
    pieces.push(Ok(literal));

    let ranges = pieces
        .iter()
        .filter_map(|piece| piece.as_ref().err())
        .collect::<Vec<_>>();

    if ranges.is_empty() {
        return Err(PatternError::NoRange);
    }

    let total = ranges
        .iter()
        .try_fold(1_u64, |product, range| product.checked_mul(range.len()))
        .filter(|total| *total <= MAX_GENERATED_LINKS as u64)
        .ok_or(PatternError::TooMany)?;

    let mut links = vec![String::new()];

    for piece in pieces {
        links = match piece {
            Ok(text) => links.into_iter().map(|link| link + &text).collect(),
            Err(range) => {
                let values = range.values();
                links
                    .into_iter()
                    .flat_map(|link| values.iter().map(move |value| format!("{link}{value}")))
                    .collect()
            }
        };
    }

    debug_assert_eq!(links.len() as u64, total);
    Ok(links)
}

/// What checking one link found out, without downloading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkProbe {
    pub url: String,
    pub reachable: bool,
    pub filename: Option<String>,
    pub total_bytes: Option<u64>,
    pub content_type: Option<String>,
    pub range_supported: bool,
    /// Why the link could not be checked. Never contains the URL.
    pub error: Option<String>,
}

/// Checks links the way a download would start, but stops after the
/// capability probe. At most `PROBE_CONCURRENCY` links are checked at once,
/// and each is given `PROBE_TIMEOUT`. Results come back in input order.
pub async fn probe_links(downloader: &Downloader, urls: Vec<String>) -> Vec<LinkProbe> {
    probe_links_with(downloader, urls, PROBE_CONCURRENCY, PROBE_TIMEOUT).await
}

pub async fn probe_links_with(
    downloader: &Downloader,
    urls: Vec<String>,
    concurrency: usize,
    timeout: Duration,
) -> Vec<LinkProbe> {
    let slots = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut checks = JoinSet::new();

    for (index, url) in urls.into_iter().take(MAX_PROBED_LINKS).enumerate() {
        let downloader = downloader.clone();
        let slots = Arc::clone(&slots);

        checks.spawn(async move {
            let _slot = slots.acquire_owned().await;
            let result = tokio::time::timeout(timeout, downloader.probe(&url)).await;

            let probe = match result {
                Ok(Ok(probe)) => LinkProbe {
                    url,
                    reachable: true,
                    filename: Some(probe.filename),
                    total_bytes: probe.total_bytes,
                    content_type: probe.content_type,
                    range_supported: probe.range_supported,
                    error: None,
                },
                Ok(Err(error)) => unreachable(url, error.redacted_message()),
                Err(_) => unreachable(url, "the server did not answer in time".to_owned()),
            };

            (index, probe)
        });
    }

    let mut probes = Vec::new();

    while let Some(result) = checks.join_next().await {
        if let Ok(done) = result {
            probes.push(done);
        }
    }

    probes.sort_by_key(|(index, _)| *index);
    probes.into_iter().map(|(_, probe)| probe).collect()
}

fn unreachable(url: String, error: String) -> LinkProbe {
    LinkProbe {
        url,
        reachable: false,
        filename: None,
        total_bytes: None,
        content_type: None,
        range_supported: false,
        error: Some(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_download_links_are_offered_from_the_clipboard() {
        let copied = "see https://example.com/about.html and https://cdn.example.com/setup.exe, \
            https://files.example.org/part.r01 https://youtu.be/abc https://example.com/blog/post";
        assert_eq!(
            downloadable_links(copied),
            vec![
                "https://cdn.example.com/setup.exe",
                "https://files.example.org/part.r01",
                "https://youtu.be/abc",
            ]
        );
        assert!(downloadable_links("just words").is_empty());
        assert!(downloadable_links(&"x".repeat(70 * 1024)).is_empty());
    }
    use crate::testing::{DEFAULT_BODY, ServerBehaviour, TestServer};

    #[test]
    fn extracts_normalizes_and_deduplicates_plain_text_and_html_links() {
        let links = extract_links(
            "<a href=\"https://EXAMPLE.com/a.zip\">A</a> https://example.com/a.zip, https://example.com/b.mp4",
        );
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].host, "example.com");
        assert_eq!(links[0].extension.as_deref(), Some("zip"));
        assert_eq!(links[1].extension.as_deref(), Some("mp4"));
    }

    #[test]
    fn keeps_links_in_the_order_they_appear() {
        let links = extract_links(
            "https://a.test/part2.rar https://a.test/part10.rar https://a.test/part2.rar",
        );

        assert_eq!(
            links
                .iter()
                .map(|link| link.url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://a.test/part2.rar", "https://a.test/part10.rar"]
        );
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(extract_links("file:///tmp/a https://example.com/a").len() == 1);
    }

    #[test]
    fn generated_links_keep_their_numeric_order() {
        let links = generate_links("https://a.test/part[1-12].rar").unwrap();

        assert_eq!(links.len(), 12);
        assert_eq!(links[1].url, "https://a.test/part2.rar");
        assert_eq!(links[11].url, "https://a.test/part12.rar");
        assert_eq!(links[0].extension.as_deref(), Some("rar"));
    }

    #[test]
    fn generated_links_that_are_not_http_are_dropped() {
        assert!(generate_links("file:///c:/f[1-3].txt").unwrap().is_empty());
    }

    #[test]
    fn expands_a_numbered_sequence() {
        assert_eq!(
            expand_pattern("https://a.test/part[1-3].rar").unwrap(),
            vec![
                "https://a.test/part1.rar",
                "https://a.test/part2.rar",
                "https://a.test/part3.rar",
            ]
        );
    }

    #[test]
    fn keeps_zero_padding_when_the_range_uses_it() {
        assert_eq!(
            expand_pattern("https://a.test/e[08-10].mp4").unwrap(),
            vec![
                "https://a.test/e08.mp4",
                "https://a.test/e09.mp4",
                "https://a.test/e10.mp4",
            ]
        );
    }

    #[test]
    fn expands_letters_and_multiplies_several_ranges() {
        assert_eq!(
            expand_pattern("https://a.test/[1-2]/[a-b].zip").unwrap(),
            vec![
                "https://a.test/1/a.zip",
                "https://a.test/1/b.zip",
                "https://a.test/2/a.zip",
                "https://a.test/2/b.zip",
            ]
        );
    }

    #[test]
    fn leaves_brackets_that_are_not_ranges_alone() {
        assert_eq!(
            expand_pattern("http://[::1]:8080/f[1-2].bin").unwrap(),
            vec!["http://[::1]:8080/f1.bin", "http://[::1]:8080/f2.bin"]
        );
    }

    #[test]
    fn refuses_patterns_it_cannot_expand_safely() {
        assert_eq!(
            expand_pattern("https://a.test/file.zip").unwrap_err(),
            PatternError::NoRange
        );
        assert_eq!(
            expand_pattern("https://a.test/f[9-1].zip").unwrap_err(),
            PatternError::Backwards
        );
        assert_eq!(
            expand_pattern("https://a.test/f[1-100000].zip").unwrap_err(),
            PatternError::TooMany
        );
        assert_eq!(
            expand_pattern("https://a.test/[1-50]/[1-50].zip").unwrap_err(),
            PatternError::TooMany,
            "the cap applies to the product of all ranges"
        );
        assert_eq!(
            expand_pattern("https://a.test/[1-99999999999999999999].zip").unwrap_err(),
            PatternError::NoRange,
            "a number too large to read is not a range"
        );
    }

    #[tokio::test]
    async fn checks_links_without_downloading_them() {
        let server = TestServer::start(ServerBehaviour::default()).await;
        let downloader = Downloader::new().unwrap();

        let probes = probe_links(
            &downloader,
            vec![
                server.url("a.bin"),
                "http://127.0.0.1:1/offline.bin".to_owned(),
            ],
        )
        .await;

        assert_eq!(probes.len(), 2);
        assert!(probes[0].reachable);
        assert_eq!(probes[0].filename.as_deref(), Some("payload.bin"));
        assert_eq!(probes[0].total_bytes, Some(DEFAULT_BODY.len() as u64));
        assert!(probes[0].range_supported);
        assert!(!probes[1].reachable, "results stay in input order");
        assert!(!probes[1].error.as_deref().unwrap().contains("127.0.0.1"));
    }

    #[tokio::test]
    async fn never_checks_more_links_at_once_than_allowed() {
        // Every answer takes a moment, so checks that run together overlap
        // visibly at the server.
        let slow = || ServerBehaviour {
            response_delay: Some(Duration::from_millis(60)),
            ..ServerBehaviour::default()
        };
        let downloader = Downloader::new().unwrap();
        let urls = |server: &TestServer| {
            (0..12)
                .map(|index| server.url(&format!("{index}.bin")))
                .collect::<Vec<_>>()
        };

        // Without a real limit the server does see overlapping checks, so a
        // low peak below cannot be an artefact of the setup.
        let unlimited = TestServer::start(slow()).await;
        probe_links_with(&downloader, urls(&unlimited), 12, PROBE_TIMEOUT).await;
        assert!(
            unlimited.peak_concurrent_requests() > 2,
            "the setup must be able to show overlap, saw {}",
            unlimited.peak_concurrent_requests()
        );

        let limited = TestServer::start(slow()).await;
        let probes = probe_links_with(&downloader, urls(&limited), 2, PROBE_TIMEOUT).await;

        assert_eq!(probes.len(), 12);
        assert!(probes.iter().all(|probe| probe.reachable));
        assert!(
            limited.peak_concurrent_requests() <= 2,
            "at most two links may be checked at once, saw {}",
            limited.peak_concurrent_requests()
        );
    }

    #[tokio::test]
    async fn a_slow_link_is_reported_instead_of_blocking_the_batch() {
        let server = TestServer::start(ServerBehaviour {
            response_delay: Some(Duration::from_secs(5)),
            ..ServerBehaviour::default()
        })
        .await;
        let downloader = Downloader::new().unwrap();

        let probes = probe_links_with(
            &downloader,
            vec![server.url("slow.bin")],
            4,
            Duration::from_millis(200),
        )
        .await;

        assert!(!probes[0].reachable);
        assert_eq!(
            probes[0].error.as_deref(),
            Some("the server did not answer in time")
        );
    }
}
