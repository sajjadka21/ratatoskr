//! Counting downloaded bytes as domestic or international traffic.
//!
//! Iranian operators bill traffic from servers inside the country at a
//! lower rate than international traffic, and many plans cap international
//! volume separately. A host is treated as domestic when it is under the
//! `.ir` top-level domain or under a domain the user listed; everything else
//! counts as international. This is a heuristic: a foreign company can host
//! inside Iran and a `.ir` name can point abroad, which is why the list is
//! editable.

use dm_storage::TrafficScope;

/// Decides the scope of `host` (lowercase, without port).
pub fn classify_host(host: &str, domestic_suffixes: &[String]) -> TrafficScope {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "ir" || host.ends_with(".ir") {
        return TrafficScope::Domestic;
    }
    let listed = domestic_suffixes.iter().any(|suffix| {
        let suffix = suffix.trim().trim_start_matches('.').to_ascii_lowercase();
        !suffix.is_empty() && (host == suffix || host.ends_with(&format!(".{suffix}")))
    });
    if listed {
        TrafficScope::Domestic
    } else {
        TrafficScope::International
    }
}

/// Parses the user's list of domestic domains: one per line or separated by
/// commas or spaces. Anything that is not a plain host name is dropped.
pub fn parse_host_list(text: &str) -> Vec<String> {
    text.split(|character: char| character == ',' || character.is_whitespace())
        .map(|item| item.trim().trim_start_matches("*.").trim_start_matches('.'))
        .filter(|item| {
            !item.is_empty()
                && item.len() <= 253
                && item.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                })
        })
        .map(str::to_ascii_lowercase)
        .collect()
}

/// `YYYY-MM-DD` of a Unix timestamp at a fixed UTC offset.
pub fn local_day(unix_seconds: i64, utc_offset_seconds: i32) -> String {
    let days = (unix_seconds + i64::from(utc_offset_seconds)).div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Local date and time as `YYYY-MM-DD HH:MM`.
pub fn local_time(unix_seconds: i64, utc_offset_seconds: i32) -> String {
    let local = unix_seconds + i64::from(utc_offset_seconds);
    let minutes = local.rem_euclid(86_400) / 60;
    format!(
        "{} {:02}:{:02}",
        local_day(unix_seconds, utc_offset_seconds),
        minutes / 60,
        minutes % 60
    )
}

/// The first and last day of the calendar month containing `day`.
pub fn month_bounds(day: &str) -> (String, String) {
    let prefix = day.get(..7).unwrap_or("1970-01");
    (format!("{prefix}-01"), format!("{prefix}-31"))
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Hinnant's
/// algorithm), so no calendar crate is needed for a date string.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iranian_domains_and_listed_hosts_are_domestic() {
        let listed = vec!["arvancloud.com".to_owned()];
        assert_eq!(
            classify_host("dl.example.ir", &listed),
            TrafficScope::Domestic
        );
        assert_eq!(
            classify_host("EXAMPLE.IR.", &listed),
            TrafficScope::Domestic
        );
        assert_eq!(
            classify_host("cdn.arvancloud.com", &listed),
            TrafficScope::Domestic
        );
        assert_eq!(
            classify_host("arvancloud.com", &listed),
            TrafficScope::Domestic
        );
        assert_eq!(
            classify_host("notarvancloud.com", &listed),
            TrafficScope::International
        );
        assert_eq!(
            classify_host("github.com", &listed),
            TrafficScope::International
        );
        assert_eq!(
            classify_host("example.iran.com", &[]),
            TrafficScope::International
        );
    }

    #[test]
    fn host_lists_keep_only_plain_host_names() {
        assert_eq!(
            parse_host_list("aparat.com, *.arvancloud.ir\n.digikala.com  bad/path http://x"),
            vec!["aparat.com", "arvancloud.ir", "digikala.com"]
        );
    }

    #[test]
    fn days_follow_the_local_offset() {
        // 2026-09-23 21:00 UTC is already 24 September in Tehran (+03:30).
        let timestamp = 1_790_197_200;
        assert_eq!(local_day(timestamp, 0), "2026-09-23");
        assert_eq!(local_day(timestamp, 12_600), "2026-09-24");
        assert_eq!(local_day(0, 0), "1970-01-01");
        assert_eq!(local_day(951_782_400, 0), "2000-02-29");
    }

    #[test]
    fn month_bounds_cover_the_whole_month() {
        assert_eq!(
            month_bounds("2026-09-24"),
            ("2026-09-01".to_owned(), "2026-09-31".to_owned())
        );
    }
}
