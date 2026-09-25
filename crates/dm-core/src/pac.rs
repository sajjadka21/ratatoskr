//! Proxy auto-configuration (PAC): a small script, usually served by the
//! proxy client itself (v2rayN's "PAC mode", for one), that decides per site
//! whether to use a proxy.
//!
//! On Windows the script is run by WinHTTP, the same component Windows uses
//! for it, so nothing here executes JavaScript itself. Answers are kept for a
//! few minutes per site. Elsewhere there is no PAC support and the caller
//! falls back to the ordinary proxy settings.

use reqwest::Url;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long an answer for one site is reused.
const ANSWER_TTL: Duration = Duration::from_secs(300);
/// How long a failure to run the script is remembered before trying again.
const FAILURE_TTL: Duration = Duration::from_secs(30);

/// The script Windows is set to use ("Use setup script" in the proxy
/// settings), if any.
pub fn system_script_url() -> Option<String> {
    #[cfg(windows)]
    {
        windows::ie_auto_config_url()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Whether `text` can be used as a script address.
pub fn validate_script_url(text: &str) -> Option<Url> {
    let url = Url::parse(text.trim()).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

/// What the script said for a site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacAnswer {
    Direct,
    /// A proxy address such as `http://127.0.0.1:10809`.
    Proxy(String),
}

pub struct PacResolver {
    script_url: String,
    answers: Mutex<HashMap<String, (Instant, Option<PacAnswer>)>>,
    #[cfg(windows)]
    session: Option<windows::Session>,
}

impl std::fmt::Debug for PacResolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PacResolver")
            .field("script_url", &self.script_url)
            .finish_non_exhaustive()
    }
}

impl PacResolver {
    pub fn new(script_url: impl Into<String>) -> Self {
        Self {
            script_url: script_url.into(),
            answers: Mutex::new(HashMap::new()),
            #[cfg(windows)]
            session: windows::Session::open(),
        }
    }

    pub fn script_url(&self) -> &str {
        &self.script_url
    }

    /// The script's answer for `target`, or `None` when it could not be run
    /// (the caller then uses its fallback).
    pub fn answer_for(&self, target: &Url) -> Option<PacAnswer> {
        let key = format!("{}://{}", target.scheme(), target.host_str().unwrap_or(""));
        if let Ok(answers) = self.answers.lock()
            && let Some((at, answer)) = answers.get(&key)
        {
            let ttl = if answer.is_some() {
                ANSWER_TTL
            } else {
                FAILURE_TTL
            };
            if at.elapsed() < ttl {
                return answer.clone();
            }
        }
        let answer = self
            .evaluate(target)
            .map(|list| match first_proxy(&list, target.scheme()) {
                Some(proxy) => PacAnswer::Proxy(proxy),
                None => PacAnswer::Direct,
            });
        if let Ok(mut answers) = self.answers.lock() {
            if answers.len() > 512 {
                answers.clear();
            }
            answers.insert(key, (Instant::now(), answer.clone()));
        }
        answer
    }

    #[cfg(windows)]
    fn evaluate(&self, target: &Url) -> Option<String> {
        let session = self.session.as_ref()?;
        windows::proxy_for(session, target.as_str(), &self.script_url)
    }

    #[cfg(not(windows))]
    fn evaluate(&self, _target: &Url) -> Option<String> {
        None
    }
}

/// The first usable proxy in a PAC or WinHTTP proxy list, as a URL; `None`
/// when the list says to go direct (or offers nothing usable).
///
/// Understands `PROXY host:port; DIRECT`, `SOCKS5 host:port`, plain
/// `host:port` entries and WinHTTP's per-scheme `http=host:port` form.
pub fn first_proxy(list: &str, scheme: &str) -> Option<String> {
    for entry in list
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let upper = entry.to_ascii_uppercase();
        if upper == "DIRECT" {
            return None;
        }
        let (kind, address) = match entry.split_once(char::is_whitespace) {
            Some((kind, address)) => (kind.to_ascii_uppercase(), address.trim()),
            None => (String::new(), entry),
        };
        let proxy = match kind.as_str() {
            "PROXY" | "HTTP" => format!("http://{}", strip_scheme(address)),
            "HTTPS" => format!("https://{}", strip_scheme(address)),
            "SOCKS" | "SOCKS5" => format!("socks5h://{}", strip_scheme(address)),
            "SOCKS4" => continue,
            "" => match address.split_once('=') {
                Some((for_scheme, address)) => {
                    if !for_scheme.eq_ignore_ascii_case(scheme) {
                        continue;
                    }
                    format!("http://{}", strip_scheme(address))
                }
                None if address.contains("://") => address.to_owned(),
                None => format!("http://{address}"),
            },
            _ => continue,
        };
        if crate::network::validate_proxy_url(&proxy).is_ok() {
            return Some(proxy);
        }
    }
    None
}

fn strip_scheme(address: &str) -> &str {
    address
        .split_once("://")
        .map_or(address, |(_, rest)| rest)
        .trim_end_matches('/')
}

#[cfg(windows)]
mod windows {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS,
        WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO, WinHttpCloseHandle,
        WinHttpGetIEProxyConfigForCurrentUser, WinHttpGetProxyForUrl, WinHttpOpen,
        WinHttpSetTimeouts,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Reads and frees a string WinHTTP allocated.
    fn take(pointer: *mut u16) -> Option<String> {
        if pointer.is_null() {
            return None;
        }
        // SAFETY: WinHTTP returns a NUL-terminated UTF-16 string that the
        // caller owns and must release with GlobalFree.
        unsafe {
            let mut length = 0;
            while *pointer.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            GlobalFree(pointer.cast());
            Some(text)
        }
    }

    pub struct Session(*mut core::ffi::c_void);

    // SAFETY: a WinHTTP session handle may be used from any thread.
    unsafe impl Send for Session {}
    unsafe impl Sync for Session {}

    impl Session {
        pub fn open() -> Option<Self> {
            let agent = wide("Ratatosk");
            // SAFETY: plain FFI call with valid, NUL-terminated arguments.
            let handle = unsafe {
                WinHttpOpen(
                    agent.as_ptr(),
                    WINHTTP_ACCESS_TYPE_NO_PROXY,
                    null(),
                    null(),
                    0,
                )
            };
            if handle.is_null() {
                return None;
            }
            // SAFETY: `handle` is a valid session handle.
            unsafe {
                WinHttpSetTimeouts(handle, 5_000, 5_000, 5_000, 5_000);
            }
            Some(Self(handle))
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            // SAFETY: the handle came from WinHttpOpen and is closed once.
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }

    /// The script's proxy list for `url`, `DIRECT` when it says so, or
    /// `None` when the script could not be fetched or run.
    pub fn proxy_for(session: &Session, url: &str, script_url: &str) -> Option<String> {
        let url = wide(url);
        let script = wide(script_url);
        let mut options = WINHTTP_AUTOPROXY_OPTIONS {
            dwFlags: WINHTTP_AUTOPROXY_CONFIG_URL,
            dwAutoDetectFlags: 0,
            lpszAutoConfigUrl: script.as_ptr(),
            lpvReserved: null_mut(),
            dwReserved: 0,
            fAutoLogonIfChallenged: 0,
        };
        let mut info = WINHTTP_PROXY_INFO {
            dwAccessType: 0,
            lpszProxy: null_mut(),
            lpszProxyBypass: null_mut(),
        };
        // SAFETY: every pointer is valid for the duration of the call.
        let ok = unsafe { WinHttpGetProxyForUrl(session.0, url.as_ptr(), &mut options, &mut info) };
        let list = take(info.lpszProxy);
        let _ = take(info.lpszProxyBypass);
        if ok == 0 {
            return None;
        }
        if info.dwAccessType == WINHTTP_ACCESS_TYPE_NO_PROXY {
            return Some("DIRECT".to_owned());
        }
        Some(list.unwrap_or_else(|| "DIRECT".to_owned()))
    }

    pub fn ie_auto_config_url() -> Option<String> {
        let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG {
            fAutoDetect: 0,
            lpszAutoConfigUrl: null_mut(),
            lpszProxy: null_mut(),
            lpszProxyBypass: null_mut(),
        };
        // SAFETY: `config` is a valid, writable structure.
        let ok = unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) };
        let script = take(config.lpszAutoConfigUrl);
        let _ = take(config.lpszProxy);
        let _ = take(config.lpszProxyBypass);
        if ok == 0 {
            return None;
        }
        script.filter(|url| !url.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_lists_are_read_in_order() {
        assert_eq!(
            first_proxy("PROXY 127.0.0.1:10809; DIRECT", "https"),
            Some("http://127.0.0.1:10809".into())
        );
        assert_eq!(first_proxy("DIRECT", "https"), None);
        assert_eq!(first_proxy("DIRECT; PROXY 1.2.3.4:80", "https"), None);
        assert_eq!(
            first_proxy("SOCKS5 127.0.0.1:10808", "http"),
            Some("socks5h://127.0.0.1:10808".into())
        );
        assert_eq!(
            first_proxy("127.0.0.1:8080", "https"),
            Some("http://127.0.0.1:8080".into())
        );
        assert_eq!(
            first_proxy("http=10.0.0.1:3128;https=10.0.0.2:3129", "https"),
            Some("http://10.0.0.2:3129".into())
        );
        assert_eq!(
            first_proxy(
                "SOCKS4 1.1.1.1:1080; PROXY user@bad; PROXY 2.2.2.2:80",
                "http"
            ),
            Some("http://2.2.2.2:80".into())
        );
        assert_eq!(first_proxy("", "http"), None);
    }

    #[test]
    fn script_addresses_must_be_web_links_without_passwords() {
        assert!(validate_script_url("http://127.0.0.1:10810/pac/?t=1").is_some());
        assert!(validate_script_url("https://example.com/proxy.pac").is_some());
        assert!(validate_script_url("file:///C:/proxy.pac").is_none());
        assert!(validate_script_url("http://user:pw@example.com/p.pac").is_none());
        assert!(validate_script_url("not a link").is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn without_windows_the_script_is_not_run() {
        let resolver = PacResolver::new("http://127.0.0.1:1/pac");
        let target = Url::parse("https://example.com/file.zip").unwrap();
        assert_eq!(resolver.answer_for(&target), None);
        assert_eq!(system_script_url(), None);
    }
}
