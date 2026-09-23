//! Browser session secrets handed over for a single task.
//!
//! A cookie header lets the engine fetch a file that needs the user's browser
//! login. It is the most sensitive value this application ever handles, so
//! it lives in memory only: it is never written to the database, never
//! logged, never sent to the interface, and never sent to any origin other
//! than the one the browser gave it for. A restart forgets it on purpose.

use reqwest::{Url, header::HeaderValue};
use std::fmt;
use thiserror::Error;

/// Browsers keep a request's `Cookie` header well below this; anything larger
/// is not a real session and is refused rather than truncated, because a
/// truncated cookie header silently sends a broken session.
pub const MAX_COOKIE_HEADER_BYTES: usize = 16 * 1024;

/// Why a handed-over session was refused. The messages never include the
/// cookie itself, so they are safe to show and to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SessionError {
    #[error("the browser session is empty")]
    Empty,

    #[error("the browser session is larger than any real cookie header")]
    TooLarge,

    #[error("the browser session contains characters that are not valid in a header")]
    InvalidCharacters,

    #[error("the download address cannot carry a browser session")]
    InvalidSource,
}

/// The scheme, host and port a session belongs to. Cookies are sent only when
/// all three match, so a session captured over HTTPS never travels over plain
/// HTTP and never reaches a different host, not even a subdomain.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    scheme: String,
    host: String,
    port: u16,
}

impl Origin {
    fn of(url: &Url) -> Option<Self> {
        Some(Self {
            scheme: url.scheme().to_ascii_lowercase(),
            host: url.host_str()?.to_ascii_lowercase(),
            port: url.port_or_known_default()?,
        })
    }
}

/// A cookie header bound to the origin it was captured for.
#[derive(Clone)]
pub struct BrowserSession {
    origin: Origin,
    cookie: HeaderValue,
}

impl BrowserSession {
    pub fn new(source_url: &str, cookie_header: &str) -> Result<Self, SessionError> {
        let url = Url::parse(source_url).map_err(|_| SessionError::InvalidSource)?;

        if !matches!(url.scheme(), "http" | "https") {
            return Err(SessionError::InvalidSource);
        }

        let origin = Origin::of(&url).ok_or(SessionError::InvalidSource)?;
        let cookie_header = cookie_header.trim();

        if cookie_header.is_empty() {
            return Err(SessionError::Empty);
        }

        if cookie_header.len() > MAX_COOKIE_HEADER_BYTES {
            return Err(SessionError::TooLarge);
        }

        // A line break would let the value start a header of its own. Every
        // control character is refused, not only CR and LF.
        if cookie_header.chars().any(char::is_control) {
            return Err(SessionError::InvalidCharacters);
        }

        let mut cookie = HeaderValue::from_bytes(cookie_header.as_bytes())
            .map_err(|_| SessionError::InvalidCharacters)?;

        // Keeps the value out of HTTP/2 header compression tables and out of
        // the client's own debug output.
        cookie.set_sensitive(true);

        Ok(Self { origin, cookie })
    }

    /// True when `url` is on exactly the origin the session was captured for.
    pub fn applies_to(&self, url: &Url) -> bool {
        Origin::of(url).is_some_and(|origin| origin == self.origin)
    }

    pub(crate) fn header(&self) -> &HeaderValue {
        &self.cookie
    }
}

impl fmt::Debug for BrowserSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BrowserSession")
            .field("scheme", &self.origin.scheme)
            .field("host", &self.origin.host)
            .field("port", &self.origin.port)
            .field("cookie", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "https://files.example.com/protected/report.pdf";
    const COOKIE: &str = "session=s3cr3t-value; theme=dark";

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn applies_only_to_the_exact_origin_it_was_captured_for() {
        let session = BrowserSession::new(SOURCE, COOKIE).unwrap();

        assert!(session.applies_to(&url("https://files.example.com/other/path.zip")));
        assert!(session.applies_to(&url("https://FILES.example.com:443/x")));

        assert!(
            !session.applies_to(&url("http://files.example.com/protected/report.pdf")),
            "a session captured over HTTPS must never travel over plain HTTP"
        );
        assert!(
            !session.applies_to(&url("https://cdn.example.com/report.pdf")),
            "a sibling host is a different origin"
        );
        assert!(
            !session.applies_to(&url("https://example.com/report.pdf")),
            "the parent domain is a different origin"
        );
        assert!(
            !session.applies_to(&url("https://files.example.com:8443/report.pdf")),
            "another port is a different origin"
        );
        assert!(!session.applies_to(&url("https://evil.test/files.example.com")));
    }

    #[test]
    fn refuses_values_that_could_inject_a_header() {
        for hostile in [
            "a=b\r\nAuthorization: Bearer stolen",
            "a=b\nX-Injected: 1",
            "a=b\0c",
            "a=b\tc",
        ] {
            assert_eq!(
                BrowserSession::new(SOURCE, hostile).unwrap_err(),
                SessionError::InvalidCharacters,
                "{hostile:?}"
            );
        }
    }

    #[test]
    fn refuses_empty_and_oversized_sessions() {
        assert_eq!(
            BrowserSession::new(SOURCE, "   ").unwrap_err(),
            SessionError::Empty
        );

        let oversized = format!("a={}", "x".repeat(MAX_COOKIE_HEADER_BYTES));
        assert_eq!(
            BrowserSession::new(SOURCE, &oversized).unwrap_err(),
            SessionError::TooLarge
        );
    }

    #[test]
    fn refuses_sources_that_are_not_http() {
        for source in ["file:///c:/secret.txt", "ftp://example.com/a", "not a url"] {
            assert_eq!(
                BrowserSession::new(source, COOKIE).unwrap_err(),
                SessionError::InvalidSource
            );
        }
    }

    #[test]
    fn never_prints_the_cookie() {
        let session = BrowserSession::new(SOURCE, COOKIE).unwrap();
        let printed = format!("{session:?} {:?}", session.header());

        assert!(!printed.contains("s3cr3t-value"), "{printed}");
        assert!(printed.contains("<redacted>"));
        assert!(session.header().is_sensitive());
    }

    #[test]
    fn errors_never_carry_the_cookie() {
        let error = BrowserSession::new(SOURCE, "session=s3cr3t-value\r\nX: 1").unwrap_err();

        assert!(!error.to_string().contains("s3cr3t-value"));
    }
}
