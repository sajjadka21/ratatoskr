use std::collections::BTreeSet;

pub const BROWSERS: [&str; 5] = ["chrome", "edge", "brave", "chromium", "firefox"];

/// Only a recent identified native message proves the extension contacted us.
pub fn connected_browsers(records: &[(String, String)], now: u64) -> Vec<String> {
    records
        .iter()
        .filter_map(|(key, value)| {
            let browser = key.strip_prefix("browser_ping_")?;
            if !BROWSERS.contains(&browser) {
                return None;
            }
            let seen = value.parse::<u64>().ok()?;
            let age = now.checked_sub(seen)?;
            (age <= 150).then(|| browser.to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
