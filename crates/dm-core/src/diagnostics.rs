//! A plain-text report for asking for help: versions, settings that
//! matter, the database's health and recent problems.
//!
//! The report is meant to be pasted into an issue or sent to someone, so it
//! carries no links beyond their host (a link's path and query can hold
//! tokens), no proxy address, no command text and no user name from paths.

use std::fmt::Write as _;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Problem {
    /// Local `YYYY-MM-DD HH:MM`.
    pub at: String,
    pub status: String,
    pub code: Option<String>,
    pub host: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticsFacts {
    pub created_at: String,
    pub app_version: String,
    pub os: String,
    pub schema_version: i32,
    pub database_bytes: u64,
    pub integrity: String,
    /// `(status, count)`, every status that has tasks.
    pub statuses: Vec<(String, u32)>,
    /// `direct`, `system` or `manual (<scheme>)`.
    pub route: String,
    pub domestic_direct: bool,
    pub direct_hosts: usize,
    pub max_connections: u32,
    pub speed_limit: Option<u64>,
    pub auto_adopt_links: bool,
    pub polite_hosts: usize,
    pub ffmpeg: Option<String>,
    pub defender: bool,
    pub post_steps: Vec<&'static str>,
    pub download_folder: String,
    pub free_bytes: Option<u64>,
    pub problems: Vec<Problem>,
}

pub fn render_report(facts: &DiagnosticsFacts) -> String {
    let yes = |value: bool| if value { "yes" } else { "no" };
    let mut text = String::new();
    let _ = writeln!(text, "Ratatosk diagnostics report");
    let _ = writeln!(text, "Created: {}", facts.created_at);
    let _ = writeln!(text);
    let _ = writeln!(text, "Application: {}", facts.app_version);
    let _ = writeln!(text, "System: {}", facts.os);
    let _ = writeln!(
        text,
        "Database: schema {}, {}, integrity {}",
        facts.schema_version,
        size(facts.database_bytes),
        facts.integrity
    );
    let statuses = if facts.statuses.is_empty() {
        "none".to_owned()
    } else {
        facts
            .statuses
            .iter()
            .map(|(status, count)| format!("{status} {count}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(text, "Downloads: {statuses}");
    let _ = writeln!(text);
    let _ = writeln!(
        text,
        "Network: {}; Iranian sites direct: {}; always-direct hosts: {}",
        facts.route,
        yes(facts.domestic_direct),
        facts.direct_hosts
    );
    let _ = writeln!(
        text,
        "Engine: up to {} connections; speed limit: {}; fresh links adopted: {}; gentle hosts: {}",
        facts.max_connections,
        facts
            .speed_limit
            .map(|limit| format!("{}/s", size(limit)))
            .unwrap_or_else(|| "none".to_owned()),
        yes(facts.auto_adopt_links),
        facts.polite_hosts
    );
    let _ = writeln!(
        text,
        "FFmpeg: {}",
        facts.ffmpeg.as_deref().unwrap_or("not found")
    );
    let _ = writeln!(
        text,
        "Windows Defender: {}",
        if facts.defender { "found" } else { "not found" }
    );
    let _ = writeln!(
        text,
        "After download: {}",
        if facts.post_steps.is_empty() {
            "nothing".to_owned()
        } else {
            facts.post_steps.join(", ")
        }
    );
    let _ = writeln!(
        text,
        "Download folder: {} ({} free)",
        facts.download_folder,
        facts
            .free_bytes
            .map(size)
            .unwrap_or_else(|| "unknown".to_owned())
    );
    let _ = writeln!(text);
    if facts.problems.is_empty() {
        let _ = writeln!(text, "Recent problems: none");
    } else {
        let _ = writeln!(text, "Recent problems:");
        for problem in &facts.problems {
            let _ = writeln!(
                text,
                "- {} {} [{}] {}: {}",
                problem.at,
                problem.status,
                problem.code.as_deref().unwrap_or("-"),
                if problem.host.is_empty() {
                    "?"
                } else {
                    &problem.host
                },
                redact_urls(&problem.message)
            );
        }
    }
    text
}

/// Replaces every link in `text` with its scheme and host.
pub fn redact_urls(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = find_link(rest) {
        output.push_str(&rest[..start]);
        let link = &rest[start..];
        let end = link
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')'))
            .unwrap_or(link.len());
        let host = crate::stats::host_of(&link[..end]);
        let scheme = if link
            .get(..5)
            .is_some_and(|start| start.eq_ignore_ascii_case("https"))
        {
            "https"
        } else {
            "http"
        };
        output.push_str(&format!(
            "{scheme}://{}/…",
            if host.is_empty() { "?" } else { &host }
        ));
        rest = &link[end..];
    }
    output.push_str(rest);
    output
}

fn find_link(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    match (lower.find("http://"), lower.find("https://")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Replaces the user's home folder at the start of `path` with `~`.
pub fn hide_home(path: &str, home: Option<&str>) -> String {
    match home.filter(|home| !home.is_empty()) {
        Some(home) if path.to_lowercase().starts_with(&home.to_lowercase()) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_owned(),
    }
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_keep_only_their_host() {
        assert_eq!(
            redact_urls(
                "GET https://cdn.Example.com/f.iso?token=SECRET failed (see http://a.ir/x)"
            ),
            "GET https://cdn.example.com/… failed (see http://a.ir/…)"
        );
        assert_eq!(redact_urls("no links here"), "no links here");
        assert_eq!(redact_urls("HTTPS://x.org/a?k=1"), "https://x.org/…");
    }

    #[test]
    fn the_home_folder_is_hidden() {
        assert_eq!(
            hide_home("C:\\Users\\Sajjad\\Downloads", Some("c:\\users\\sajjad")),
            "~\\Downloads"
        );
        assert_eq!(
            hide_home("D:\\Downloads", Some("C:\\Users\\Sajjad")),
            "D:\\Downloads"
        );
        assert_eq!(hide_home("D:\\Downloads", None), "D:\\Downloads");
    }

    #[test]
    fn the_report_names_settings_but_never_secrets() {
        let facts = DiagnosticsFacts {
            created_at: "2026-09-24 17:00".into(),
            app_version: "1.0.0".into(),
            os: "windows x86_64".into(),
            schema_version: 13,
            database_bytes: 3 * 1024 * 1024,
            integrity: "ok".into(),
            statuses: vec![("completed".into(), 12), ("failed".into(), 1)],
            route: "manual (socks5)".into(),
            domestic_direct: true,
            max_connections: 8,
            problems: vec![Problem {
                at: "2026-09-24 16:00".into(),
                status: "failed".into(),
                code: Some("http_403".into()),
                host: "aka.ms".into(),
                message: "403 from https://aka.ms/vs?sig=abc".into(),
            }],
            ..DiagnosticsFacts::default()
        };
        let report = render_report(&facts);
        assert!(report.contains("Database: schema 13, 3.0 MB, integrity ok"));
        assert!(report.contains("Downloads: completed 12, failed 1"));
        assert!(report.contains("Network: manual (socks5); Iranian sites direct: yes"));
        assert!(
            report
                .contains("- 2026-09-24 16:00 failed [http_403] aka.ms: 403 from https://aka.ms/…")
        );
        assert!(!report.contains("sig=abc"));
        assert!(report.contains("(unknown free)"));
    }
}
