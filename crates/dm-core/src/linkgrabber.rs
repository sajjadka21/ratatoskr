use reqwest::Url;
use std::collections::BTreeMap;

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
    let mut candidates = BTreeMap::new();
    for token in
        input.split(|character: char| character.is_whitespace() || "<>\"'`".contains(character))
    {
        let token = token.trim_matches(|character: char| "<>\"'`([{".contains(character));
        let candidate = token.trim_end_matches([',', '.', ';', '!', '?', ')', ']', '}']);
        let Ok(url) = Url::parse(candidate) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        let Some(host) = url.host_str() else { continue };
        let extension = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, value)| value.to_ascii_lowercase());
        let normalized = url.to_string();
        candidates
            .entry(normalized.clone())
            .or_insert(LinkCandidate {
                url: normalized,
                host: host.to_ascii_lowercase(),
                extension,
            });
    }
    candidates.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::extract_links;

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
    fn rejects_non_http_schemes() {
        assert!(extract_links("file:///tmp/a https://example.com/a").len() == 1);
    }
}
