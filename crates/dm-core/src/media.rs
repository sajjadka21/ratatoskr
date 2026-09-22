use reqwest::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Direct,
    Hls,
    Dash,
    UnsupportedProtected,
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
}
