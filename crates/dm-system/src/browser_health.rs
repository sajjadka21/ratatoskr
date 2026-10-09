use std::collections::BTreeSet;

pub const BROWSERS: [&str; 5] = ["chrome", "edge", "brave", "chromium", "firefox"];
/// Keep the app's status close to the extension's one-minute heartbeat. A
/// longer cache window made a stopped or disabled extension look connected.
pub const HEARTBEAT_MAX_AGE_SECONDS: u64 = 90;

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
            (age <= HEARTBEAT_MAX_AGE_SECONDS).then(|| browser.to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
