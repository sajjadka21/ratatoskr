//! How downloads reach the network: directly, through the system proxy, or
//! through a proxy the user set, with hosts that always go direct.
//!
//! Many users in Iran run a local proxy client (v2ray, xray, clash...) for
//! international sites. Sending domestic downloads through it wastes the
//! proxy's international volume and is usually slower, so domestic hosts can
//! bypass it. Proxy credentials are refused: this application never stores
//! passwords.

use crate::traffic::{classify_host, parse_host_list};
use dm_storage::{Storage, TrafficScope};
use reqwest::{Client, Proxy, Url};
use std::time::Duration;
use thiserror::Error;

/// How long a connection may stay silent before it counts as dead. A
/// connection that stays open but stops delivering (common behind
/// filtering and VPNs) otherwise holds its range forever, and the
/// download stops at 99%.
pub const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(30);

pub const SETTING_PROXY_MODE: &str = "network_proxy_mode";
pub const SETTING_PROXY_URL: &str = "network_proxy_url";
pub const SETTING_DIRECT_HOSTS: &str = "network_direct_hosts";
pub const SETTING_DOMESTIC_DIRECT: &str = "network_domestic_direct";
pub const SETTING_DOMESTIC_HOSTS: &str = "traffic_domestic_hosts";
pub const SETTING_PAC_URL: &str = "network_pac_url";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyMode {
    /// Every request goes straight to the server.
    Off,
    /// Whatever proxy the operating system is configured with.
    #[default]
    System,
    /// The proxy in `proxy_url`, except for hosts that go direct.
    Manual,
    /// A proxy auto-configuration script at `pac_url` decides per site.
    Pac,
}

impl ProxyMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::System => "system",
            Self::Manual => "manual",
            Self::Pac => "pac",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "system" => Some(Self::System),
            "manual" => Some(Self::Manual),
            "pac" => Some(Self::Pac),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSettings {
    pub mode: ProxyMode,
    pub proxy_url: Option<String>,
    /// The proxy auto-configuration script, for `ProxyMode::Pac`.
    pub pac_url: Option<String>,
    /// Hosts (and their subdomains) that never use the proxy.
    pub direct_hosts: Vec<String>,
    /// Domestic hosts never use the proxy either.
    pub domestic_direct: bool,
    /// Extra domains counted as domestic, besides `.ir`.
    pub domestic_hosts: Vec<String>,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            proxy_url: None,
            pac_url: None,
            direct_hosts: Vec::new(),
            domestic_direct: true,
            domestic_hosts: Vec::new(),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NetworkError {
    #[error("the proxy address is not valid; use something like socks5://127.0.0.1:10808")]
    InvalidProxyUrl,

    #[error("proxy addresses must be http, https, socks5 or socks5h")]
    UnsupportedProxyScheme,

    #[error(
        "proxy addresses with a user name or password are not accepted: passwords are never stored"
    )]
    ProxyCredentials,

    #[error("a manual proxy needs an address")]
    MissingProxyUrl,

    #[error("the setup script must be an http or https address, such as http://127.0.0.1:10810/pac")]
    InvalidPacUrl,
}

impl NetworkSettings {
    /// Reads the settings; anything missing or unreadable falls back to the
    /// default, so a damaged value can never stop downloads altogether.
    pub fn load(storage: &Storage) -> Self {
        let text = |key: &str| storage.get_setting(key).ok().flatten();
        let mut settings = Self {
            mode: text(SETTING_PROXY_MODE)
                .as_deref()
                .and_then(ProxyMode::parse)
                .unwrap_or_default(),
            proxy_url: text(SETTING_PROXY_URL).filter(|value| !value.trim().is_empty()),
            pac_url: text(SETTING_PAC_URL).filter(|value| !value.trim().is_empty()),
            direct_hosts: text(SETTING_DIRECT_HOSTS)
                .map(|value| parse_host_list(&value))
                .unwrap_or_default(),
            domestic_direct: text(SETTING_DOMESTIC_DIRECT).as_deref() != Some("false"),
            domestic_hosts: text(SETTING_DOMESTIC_HOSTS)
                .map(|value| parse_host_list(&value))
                .unwrap_or_default(),
        };
        if matches!(settings.mode, ProxyMode::Manual | ProxyMode::Pac) && settings.validate().is_err() {
            settings.mode = ProxyMode::System;
        }
        settings
    }

    pub fn save(&self, storage: &Storage) -> dm_storage::Result<()> {
        storage.set_setting(SETTING_PROXY_MODE, self.mode.as_str())?;
        storage.set_setting(SETTING_PROXY_URL, self.proxy_url.as_deref().unwrap_or(""))?;
        storage.set_setting(SETTING_PAC_URL, self.pac_url.as_deref().unwrap_or(""))?;
        storage.set_setting(SETTING_DIRECT_HOSTS, &self.direct_hosts.join("\n"))?;
        storage.set_setting(
            SETTING_DOMESTIC_DIRECT,
            if self.domestic_direct {
                "true"
            } else {
                "false"
            },
        )?;
        storage.set_setting(SETTING_DOMESTIC_HOSTS, &self.domestic_hosts.join("\n"))?;
        Ok(())
    }

    /// Checks the proxy address when a manual proxy is chosen. The address
    /// is kept even in other modes, so switching back needs no retyping.
    pub fn validate(&self) -> Result<(), NetworkError> {
        match (&self.proxy_url, self.mode) {
            (Some(url), _) => validate_proxy_url(url).map(|_| ())?,
            (None, ProxyMode::Manual) => return Err(NetworkError::MissingProxyUrl),
            (None, _) => {}
        }
        match (&self.pac_url, self.mode) {
            (Some(url), _) if crate::pac::validate_script_url(url).is_none() => {
                Err(NetworkError::InvalidPacUrl)
            }
            (None, ProxyMode::Pac) => Err(NetworkError::InvalidPacUrl),
            _ => Ok(()),
        }
    }

    /// Whether requests to `host` skip the manual proxy. Addresses on this
    /// computer or the local network always do.
    pub fn goes_direct(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if is_local_host(&host) {
            return true;
        }
        let listed = self
            .direct_hosts
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")));
        listed
            || (self.domestic_direct
                && classify_host(&host, &self.domestic_hosts) == TrafficScope::Domestic)
    }

    /// Builds the HTTP client these settings describe.
    pub fn build_client(&self) -> Result<Client, reqwest::Error> {
        self.build_client_with(DEFAULT_STALL_TIMEOUT)
    }

    /// The client, with `stall_timeout` as the longest a connection may go
    /// without delivering a byte.
    ///
    /// HTTP/1.1 only: over HTTP/2 every range request to a server would
    /// share one TCP connection, and the point of several connections is
    /// several TCP connections. On long, lossy routes each connection's
    /// window, not the line, limits the speed, and HTTP/2 would quietly
    /// turn eight connections back into one.
    pub fn build_client_with(&self, stall_timeout: Duration) -> Result<Client, reqwest::Error> {
        let builder = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(stall_timeout)
            .http1_only()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd();

        let builder = match self.mode {
            ProxyMode::Off => builder.no_proxy(),
            // Windows may be set to a setup script (v2rayN's PAC mode sets
            // one); the ordinary proxy settings are the fallback.
            ProxyMode::System => builder.proxy(system_proxy(
                self.clone(),
                crate::pac::system_script_url().map(crate::pac::PacResolver::new),
            )),
            ProxyMode::Pac => builder.proxy(system_proxy(
                self.clone(),
                self.pac_url.clone().map(crate::pac::PacResolver::new),
            )),
            ProxyMode::Manual => match self
                .proxy_url
                .as_deref()
                .and_then(|url| validate_proxy_url(url).ok())
            {
                Some(proxy) => {
                    let routing = self.clone();
                    builder.proxy(Proxy::custom(move |target: &Url| {
                        let host = target.host_str()?;
                        (!routing.goes_direct(host)).then(|| proxy.clone())
                    }))
                }
                None => builder,
            },
        };

        builder.build()
    }
}

/// The operating system's proxy, except for addresses on this computer or the
/// local network.
///
/// The HTTP library's own system-proxy support reads the Windows bypass list
/// (`ProxyOverride`) but treats entries such as `127.*` and `192.168.*` - the
/// ones proxy clients like v2rayN write - as domain names, so loopback and
/// LAN downloads (a NAS, a router) went through the proxy. The bypass for
/// local addresses is therefore decided here.
///
/// With a setup script, the script decides first; the operating system's
/// ordinary proxy is used when the script cannot be run. Sites that go
/// direct (Iranian sites, when chosen, and the always-direct list) never use
/// either.
fn system_proxy(routing: NetworkSettings, script: Option<crate::pac::PacResolver>) -> Proxy {
    use hyper_util::client::proxy::matcher::Matcher;
    let matcher = std::sync::Arc::new(Matcher::from_system());
    let auth = "http://example.com/"
        .parse::<http::Uri>()
        .ok()
        .and_then(|uri| matcher.intercept(&uri))
        .and_then(|intercept| intercept.basic_auth().cloned());
    let proxy = Proxy::custom(move |target: &Url| {
        let host = target.host_str()?;
        if routing.goes_direct(host) {
            return None;
        }
        if let Some(answer) = script.as_ref().and_then(|script| script.answer_for(target)) {
            return match answer {
                crate::pac::PacAnswer::Direct => None,
                crate::pac::PacAnswer::Proxy(proxy) => Some(proxy),
            };
        }
        let uri = target.as_str().parse::<http::Uri>().ok()?;
        matcher
            .intercept(&uri)
            .map(|intercept| intercept.uri().to_string())
    });
    match auth {
        Some(header) => proxy.custom_http_auth(header),
        None => proxy,
    }
}

/// An address on this computer or the local network, which no proxy should
/// carry: loopback, private and link-local addresses, `localhost`, `.local`
/// names, and names without a dot (Windows' `<local>`).
pub fn is_local_host(host: &str) -> bool {
    use std::net::IpAddr;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        Ok(IpAddr::V6(ip)) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return v4.is_loopback() || v4.is_private() || v4.is_link_local();
            }
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
        Err(_) => {
            host == "localhost"
                || host.ends_with(".localhost")
                || host.ends_with(".local")
                || (!host.is_empty() && !host.contains('.'))
        }
    }
}

/// Parses a proxy address, refusing credentials and anything that is not a
/// bare scheme, host and port.
pub fn validate_proxy_url(text: &str) -> Result<Url, NetworkError> {
    let url = Url::parse(text.trim()).map_err(|_| NetworkError::InvalidProxyUrl)?;
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") {
        return Err(NetworkError::UnsupportedProxyScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NetworkError::ProxyCredentials);
    }
    let bare = url.host_str().is_some_and(|host| !host.is_empty())
        && url.port_or_known_default().is_some()
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none();
    if bare {
        Ok(url)
    } else {
        Err(NetworkError::InvalidProxyUrl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses_never_go_through_a_proxy() {
        for host in [
            "127.0.0.1",
            "10.0.0.5",
            "172.20.1.1",
            "192.168.1.10",
            "169.254.3.4",
            "[::1]",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.2",
            "localhost",
            "nas.local",
            "nas",
            "NAS.",
        ] {
            assert!(is_local_host(host), "{host}");
        }
        for host in [
            "8.8.8.8",
            "172.32.0.1",
            "2001:db8::1",
            "example.com",
            "aparat.com",
            "",
        ] {
            assert!(!is_local_host(host), "{host}");
        }

        let manual = NetworkSettings {
            mode: ProxyMode::Manual,
            proxy_url: Some("socks5://127.0.0.1:10808".to_owned()),
            pac_url: None,
            domestic_direct: false,
            ..NetworkSettings::default()
        };
        assert!(manual.goes_direct("192.168.1.10"));
        assert!(manual.goes_direct("127.0.0.1"));
        assert!(!manual.goes_direct("example.com"));
    }
    use tempfile::tempdir;

    #[test]
    fn proxy_addresses_must_be_bare_and_without_credentials() {
        assert!(validate_proxy_url("socks5://127.0.0.1:10808").is_ok());
        assert!(validate_proxy_url("http://127.0.0.1:10809/").is_ok());
        assert_eq!(
            validate_proxy_url("socks5://user:secret@127.0.0.1:1080"),
            Err(NetworkError::ProxyCredentials)
        );
        assert_eq!(
            validate_proxy_url("ftp://127.0.0.1:21"),
            Err(NetworkError::UnsupportedProxyScheme)
        );
        assert_eq!(
            validate_proxy_url("http://127.0.0.1:8080/path?x=1"),
            Err(NetworkError::InvalidProxyUrl)
        );
        assert_eq!(
            validate_proxy_url("not a url"),
            Err(NetworkError::InvalidProxyUrl)
        );
    }

    #[test]
    fn domestic_and_listed_hosts_skip_the_proxy() {
        let settings = NetworkSettings {
            mode: ProxyMode::Manual,
            proxy_url: Some("socks5://127.0.0.1:10808".to_owned()),
            pac_url: None,
            direct_hosts: vec!["lan.example".to_owned()],
            domestic_direct: true,
            domestic_hosts: vec!["arvancloud.com".to_owned()],
        };
        assert!(settings.goes_direct("dl.site.ir"));
        assert!(settings.goes_direct("cdn.arvancloud.com"));
        assert!(settings.goes_direct("nas.lan.example"));
        assert!(!settings.goes_direct("github.com"));

        let everything_proxied = NetworkSettings {
            domestic_direct: false,
            ..settings
        };
        assert!(!everything_proxied.goes_direct("dl.site.ir"));
    }

    #[test]
    fn settings_round_trip_and_a_broken_manual_proxy_falls_back() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        assert_eq!(NetworkSettings::load(&storage), NetworkSettings::default());

        let settings = NetworkSettings {
            mode: ProxyMode::Manual,
            proxy_url: Some("socks5://127.0.0.1:10808".to_owned()),
            pac_url: None,
            direct_hosts: vec!["lan.example".to_owned()],
            domestic_direct: false,
            domestic_hosts: vec!["aparat.com".to_owned()],
        };
        settings.save(&storage).unwrap();
        assert_eq!(NetworkSettings::load(&storage), settings);

        storage
            .set_setting(SETTING_PROXY_URL, "socks5://a:b@127.0.0.1:1")
            .unwrap();
        assert_eq!(NetworkSettings::load(&storage).mode, ProxyMode::System);
    }

    #[test]
    fn every_mode_builds_a_client() {
        for mode in [
            ProxyMode::Off,
            ProxyMode::System,
            ProxyMode::Manual,
            ProxyMode::Pac,
        ] {
            let settings = NetworkSettings {
                mode,
                proxy_url: Some("socks5h://127.0.0.1:10808".to_owned()),
                pac_url: Some("http://127.0.0.1:10810/pac".to_owned()),
                ..NetworkSettings::default()
            };
            assert!(settings.build_client().is_ok());
        }
    }

    #[test]
    fn a_setup_script_must_be_a_web_address() {
        let pac = NetworkSettings {
            mode: ProxyMode::Pac,
            pac_url: Some("http://127.0.0.1:10810/pac/?t=1".to_owned()),
            ..NetworkSettings::default()
        };
        assert_eq!(pac.validate(), Ok(()));
        assert_eq!(
            NetworkSettings {
                pac_url: None,
                ..pac.clone()
            }
            .validate(),
            Err(NetworkError::InvalidPacUrl)
        );
        assert_eq!(
            NetworkSettings {
                pac_url: Some("file:///c:/p.pac".to_owned()),
                ..pac.clone()
            }
            .validate(),
            Err(NetworkError::InvalidPacUrl)
        );

        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        pac.save(&storage).unwrap();
        assert_eq!(NetworkSettings::load(&storage), pac);
        storage.set_setting(SETTING_PAC_URL, "ftp://x/p").unwrap();
        assert_eq!(NetworkSettings::load(&storage).mode, ProxyMode::System);
    }
}
