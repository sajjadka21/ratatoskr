//! Collecting the files a web site links to: read a page, pick out the links
//! to files, and follow links to further pages up to a chosen depth.
//!
//! The crawl is deliberately gentle: one page at a time with a short pause,
//! bounded in depth, pages and files, and it never leaves the starting host
//! unless asked to. Pages are read but never run.

use crate::linkgrabber::{DOWNLOAD_EXTENSIONS, LinkCandidate, candidate_for};
use reqwest::Url;
use std::{
    collections::{HashSet, VecDeque},
    future::Future,
    time::Duration,
};

/// Deepest a crawl may follow links from the first page.
pub const MAX_DEPTH: u8 = 3;
/// Most pages read in one crawl.
pub const MAX_PAGES: usize = 200;
/// Most files collected in one crawl.
pub const MAX_FILES: usize = 2_000;
/// Pause between two pages, so a small server is not hammered.
const PAGE_PAUSE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteGrabOptions {
    /// 0 reads only the first page.
    pub depth: u8,
    /// Stay on the host of the first page.
    pub same_host: bool,
    /// Stay inside the folder of the first page, such as `/files/` for
    /// `https://a.test/files/index.html`.
    pub within_folder: bool,
    /// File types to collect. Empty means the usual download types.
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SiteGrabResult {
    pub files: Vec<LinkCandidate>,
    pub pages_read: usize,
    /// The crawl stopped at a limit before it ran out of pages.
    pub truncated: bool,
}

/// The links a page holds, made absolute and without fragments. `base` is the
/// page's own address. Only `href`, `src` and `data-src` attributes are read;
/// scripts are not run and inline text is ignored.
pub fn page_links(base: &Url, html: &str) -> Vec<Url> {
    let lower = html.to_ascii_lowercase();
    let mut links = Vec::new();
    for attribute in ["href", "src", "data-src"] {
        let mut from = 0;
        while let Some(found) = lower[from..].find(attribute) {
            let at = from + found;
            from = at + attribute.len();
            // `data-src` must not also be counted as `src`.
            if attribute == "src" && at > 0 && lower.as_bytes()[at - 1] == b'-' {
                continue;
            }
            if at > 0
                && !matches!(
                    lower.as_bytes()[at - 1],
                    b' ' | b'\t' | b'\r' | b'\n' | b'"' | b'\''
                )
            {
                continue;
            }
            let rest = html[from..].trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                continue;
            };
            let rest = rest.trim_start();
            let value = match rest.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let inner = &rest[1..];
                    match inner.find(quote) {
                        Some(end) => &inner[..end],
                        None => continue,
                    }
                }
                Some(_) => rest
                    .split(|c: char| c.is_whitespace() || c == '>')
                    .next()
                    .unwrap_or_default(),
                None => continue,
            };
            let value = value.trim();
            if value.is_empty()
                || value.starts_with('#')
                || value.starts_with("javascript:")
                || value.starts_with("mailto:")
                || value.starts_with("data:")
            {
                continue;
            }
            let decoded = value.replace("&amp;", "&");
            if let Ok(mut url) = base.join(&decoded)
                && matches!(url.scheme(), "http" | "https")
            {
                url.set_fragment(None);
                links.push(url);
            }
        }
    }
    links
}

fn is_page(url: &Url) -> bool {
    let name = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    match name.rsplit_once('.') {
        None => true,
        Some((_, extension)) => matches!(
            extension.to_ascii_lowercase().as_str(),
            "html" | "htm" | "xhtml" | "php" | "asp" | "aspx" | "jsp" | "shtml"
        ),
    }
}

fn wanted(candidate: &LinkCandidate, extensions: &[String]) -> bool {
    let Some(extension) = candidate.extension.as_deref() else {
        return false;
    };
    if extensions.is_empty() {
        DOWNLOAD_EXTENSIONS.contains(&extension)
    } else {
        extensions.iter().any(|wanted| wanted == extension)
    }
}

/// Crawls from `start`. `fetch` returns a page's text, or `None` when it
/// cannot be read (not found, not text, too large); such pages are skipped.
pub async fn grab_site<F, Fut>(
    start: &str,
    options: &SiteGrabOptions,
    fetch: F,
) -> Option<SiteGrabResult>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    let start = Url::parse(start).ok()?;
    if !matches!(start.scheme(), "http" | "https") || start.host_str().is_none() {
        return None;
    }
    let depth_limit = options.depth.min(MAX_DEPTH);
    let extensions: Vec<String> = options
        .extensions
        .iter()
        .map(|extension| {
            extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase()
        })
        .filter(|extension| !extension.is_empty())
        .collect();
    let folder = {
        let path = start.path();
        match path.rfind('/') {
            Some(at) => path[..=at].to_owned(),
            None => "/".to_owned(),
        }
    };

    let mut result = SiteGrabResult::default();
    let mut seen_pages: HashSet<String> = HashSet::new();
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(Url, u8)> = VecDeque::new();
    seen_pages.insert(start.as_str().to_owned());
    queue.push_back((start.clone(), 0));

    while let Some((page, depth)) = queue.pop_front() {
        if result.pages_read >= MAX_PAGES {
            result.truncated = true;
            break;
        }
        if result.pages_read > 0 {
            tokio::time::sleep(PAGE_PAUSE).await;
        }
        result.pages_read += 1;
        let Some(html) = fetch(page.as_str().to_owned()).await else {
            continue;
        };
        for link in page_links(&page, &html) {
            let Some(candidate) = candidate_for(link.as_str()) else {
                continue;
            };
            if wanted(&candidate, &extensions) {
                // A file on another host is still wanted when the crawl may
                // leave the host; with `same_host` it is left out.
                if options.same_host && link.host_str() != start.host_str() {
                    continue;
                }
                if result.files.len() >= MAX_FILES {
                    result.truncated = true;
                    continue;
                }
                if seen_files.insert(candidate.url.clone()) {
                    result.files.push(candidate);
                }
            } else if depth < depth_limit
                && is_page(&link)
                && (!options.same_host || link.host_str() == start.host_str())
                && (!options.within_folder
                    || link.host_str() != start.host_str()
                    || link.path().starts_with(&folder))
                && seen_pages.insert(link.as_str().to_owned())
            {
                queue.push_back((link, depth + 1));
            }
        }
    }
    if !queue.is_empty() {
        result.truncated = true;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn base() -> Url {
        Url::parse("https://a.test/files/index.html").unwrap()
    }

    #[test]
    fn links_are_read_from_attributes_and_made_absolute() {
        let html = r##"<a href="one.zip">1</a> <A HREF='/two.pdf#x'>2</A>
            <img data-src=three.png> <a href="#top">t</a>
            <a href="mailto:x@y.z">m</a> <a href="javascript:void(0)">j</a>
            <a href="dir/?a=1&amp;b=2">d</a> <script src="s.js"></script>"##;
        let links: Vec<String> = page_links(&base(), html)
            .into_iter()
            .map(|url| url.to_string())
            .collect();
        assert!(links.contains(&"https://a.test/files/one.zip".to_owned()));
        assert!(links.contains(&"https://a.test/two.pdf".to_owned()));
        assert!(links.contains(&"https://a.test/files/three.png".to_owned()));
        assert!(links.contains(&"https://a.test/files/dir/?a=1&b=2".to_owned()));
        assert!(!links.iter().any(|link| link.contains("mailto")));
        assert!(!links.iter().any(|link| link.contains("javascript")));
    }

    fn site(pages: &[(&str, &str)]) -> HashMap<String, String> {
        pages
            .iter()
            .map(|(url, html)| ((*url).to_owned(), (*html).to_owned()))
            .collect()
    }

    async fn crawl(
        pages: &HashMap<String, String>,
        start: &str,
        options: SiteGrabOptions,
    ) -> SiteGrabResult {
        grab_site(start, &options, |url| {
            let page = pages.get(&url).cloned();
            async move { page }
        })
        .await
        .unwrap()
    }

    fn options(depth: u8) -> SiteGrabOptions {
        SiteGrabOptions {
            depth,
            same_host: true,
            within_folder: true,
            extensions: Vec::new(),
        }
    }

    #[tokio::test]
    async fn files_are_collected_to_the_chosen_depth() {
        let pages = site(&[
            (
                "https://a.test/files/index.html",
                r#"<a href="a.zip">a</a><a href="sub/">sub</a>"#,
            ),
            (
                "https://a.test/files/sub/",
                r#"<a href="b.pdf">b</a><a href="deeper/">d</a>"#,
            ),
            (
                "https://a.test/files/sub/deeper/",
                r#"<a href="c.mp3">c</a>"#,
            ),
        ]);
        let start = "https://a.test/files/index.html";

        let top = crawl(&pages, start, options(0)).await;
        assert_eq!(top.files.len(), 1);
        assert_eq!(top.pages_read, 1);

        let one = crawl(&pages, start, options(1)).await;
        assert_eq!(one.files.len(), 2);

        let two = crawl(&pages, start, options(2)).await;
        let names: Vec<_> = two.files.iter().map(|f| f.url.as_str()).collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"https://a.test/files/sub/deeper/c.mp3"));
        assert!(!two.truncated);
    }

    #[tokio::test]
    async fn the_crawl_stays_on_the_host_and_in_the_folder() {
        let pages = site(&[
            (
                "https://a.test/files/index.html",
                r#"<a href="https://other.test/x.zip">x</a>
                   <a href="https://other.test/page.html">p</a>
                   <a href="/elsewhere/page.html">e</a>
                   <a href="in.zip">in</a>"#,
            ),
            ("https://other.test/page.html", r#"<a href="y.zip">y</a>"#),
            (
                "https://a.test/elsewhere/page.html",
                r#"<a href="z.zip">z</a>"#,
            ),
        ]);
        let start = "https://a.test/files/index.html";

        let result = crawl(&pages, start, options(2)).await;
        let urls: Vec<_> = result.files.iter().map(|f| f.url.as_str()).collect();
        assert_eq!(urls, vec!["https://a.test/files/in.zip"]);

        // Leaving the host and the folder finds more.
        let open = SiteGrabOptions {
            same_host: false,
            within_folder: false,
            ..options(2)
        };
        let result = crawl(&pages, start, open).await;
        assert_eq!(result.files.len(), 4);
    }

    #[tokio::test]
    async fn only_the_asked_for_types_are_collected() {
        let pages = site(&[(
            "https://a.test/index.html",
            r#"<a href="a.zip">a</a><img src="b.png"><a href="c.PNG">c</a>"#,
        )]);
        let images = SiteGrabOptions {
            extensions: vec![".png".to_owned()],
            ..options(0)
        };
        let result = crawl(&pages, "https://a.test/index.html", images).await;
        assert_eq!(result.files.len(), 2);
        assert!(
            result
                .files
                .iter()
                .all(|f| f.url.to_lowercase().ends_with(".png"))
        );
    }

    #[tokio::test]
    async fn a_page_linking_back_is_read_once() {
        let pages = site(&[
            (
                "https://a.test/index.html",
                r#"<a href="two.html">2</a><a href="a.zip">a</a>"#,
            ),
            (
                "https://a.test/two.html",
                r#"<a href="index.html">1</a><a href="a.zip">a</a>"#,
            ),
        ]);
        let result = crawl(&pages, "https://a.test/index.html", options(3)).await;
        assert_eq!(result.pages_read, 2);
        assert_eq!(result.files.len(), 1);
    }

    #[tokio::test]
    async fn an_address_that_is_not_http_is_refused() {
        let none = grab_site("file:///etc/passwd", &options(1), |_| async { None }).await;
        assert!(none.is_none());
    }
}
