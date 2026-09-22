use reqwest::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Direct,
    Hls,
    Dash,
    UnsupportedProtected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaVariant {
    pub uri: String,
    pub bandwidth: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Parses the safe, metadata-only portion of an HLS master playlist. Segment
/// downloads still flow through the normal task engine; this parser never
/// fetches keys or attempts protected media handling.
pub fn parse_hls_master_playlist(base_url: &str, content: &str) -> Vec<MediaVariant> {
    let Ok(base) = Url::parse(base_url) else {
        return Vec::new();
    };
    let mut pending = None;
    let mut variants = Vec::new();
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(attributes) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let bandwidth = attribute(attributes, "BANDWIDTH").and_then(|value| value.parse().ok());
            let (width, height) = attribute(attributes, "RESOLUTION")
                .and_then(|value| value.split_once('x'))
                .and_then(|(width, height)| {
                    Some((
                        Some(width.parse::<u32>().ok()?),
                        Some(height.parse::<u32>().ok()?),
                    ))
                })
                .unwrap_or((None, None));
            pending = Some((bandwidth, width, height));
        } else if !line.starts_with('#')
            && let Some((bandwidth, width, height)) = pending.take()
            && let Ok(uri) = base.join(line)
        {
            variants.push(MediaVariant {
                uri: uri.to_string(),
                bandwidth,
                width,
                height,
            });
        }
    }
    variants
}

fn attribute<'a>(attributes: &'a str, key: &str) -> Option<&'a str> {
    attributes.split(',').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name.eq_ignore_ascii_case(key)).then_some(value.trim_matches('"'))
    })
}

/// Classifies only unprotected media entry points. It never attempts DRM
/// bypass; callers should create normal download tasks for direct resources.
pub fn classify_source(source_url: &str, mime_type: Option<&str>) -> Option<MediaKind> {
    let url = Url::parse(source_url).ok()?;
    let path = url.path().to_ascii_lowercase();
    let mime = mime_type.unwrap_or_default().to_ascii_lowercase();
    if path.ends_with(".m3u8") || mime.contains("mpegurl") {
        return Some(MediaKind::Hls);
    }
    if path.ends_with(".mpd") || mime.contains("dash+xml") {
        return Some(MediaKind::Dash);
    }
    if mime.contains("application/vnd.apple.mpegurl")
        || mime.contains("application/dash+xml")
        || path.contains("/license")
        || path.contains("/drm/")
    {
        return Some(MediaKind::UnsupportedProtected);
    }
    Some(MediaKind::Direct)
}

#[cfg(test)]
mod tests {
    use super::{MediaKind, classify_source};

    #[test]
    fn detects_hls_dash_and_direct_resources() {
        assert_eq!(
            classify_source("https://example.com/live/index.m3u8", None),
            Some(MediaKind::Hls)
        );
        assert_eq!(
            classify_source(
                "https://example.com/live/manifest",
                Some("application/dash+xml")
            ),
            Some(MediaKind::Dash)
        );
        assert_eq!(
            classify_source("https://example.com/video.mp4", Some("video/mp4")),
            Some(MediaKind::Direct)
        );
    }

    #[test]
    fn protected_entry_points_are_reported_without_bypass() {
        assert_eq!(
            classify_source("https://example.com/drm/license", None),
            Some(MediaKind::UnsupportedProtected)
        );
    }

    #[test]
    fn parses_hls_variants_without_touching_segments_or_keys() {
        let variants = super::parse_hls_master_playlist(
            "https://example.com/live/master.m3u8",
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=1600000,RESOLUTION=1280x720\nhigh.m3u8",
        );
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[1].height, Some(720));
        assert_eq!(variants[1].uri, "https://example.com/live/high.m3u8");
    }
}
