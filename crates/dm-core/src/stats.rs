//! Figures for the statistics page, computed from the download records and
//! the traffic counters the engine already keeps. Nothing here is sampled
//! or estimated.

use crate::traffic::local_day;
use dm_common::{DownloadRecord, DownloadStatus};
use dm_storage::TrafficTotals;
use std::collections::HashMap;

/// Most hosts listed by name; the rest are counted together.
const TOP_HOSTS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivityDay {
    /// Local `YYYY-MM-DD`.
    pub day: String,
    pub domestic_bytes: u64,
    pub international_bytes: u64,
    pub completed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamedTotal {
    pub name: String,
    pub count: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadStats {
    /// Every day of the period, oldest first, including quiet ones.
    pub days: Vec<ActivityDay>,
    pub period_completed: u32,
    pub period_domestic_bytes: u64,
    pub period_international_bytes: u64,
    pub all_completed: u32,
    pub all_completed_bytes: u64,
    pub failed: u32,
    pub active: u32,
    /// Hosts of the files finished in the period, by bytes; the rest are
    /// summed into a last entry named `""`.
    pub top_hosts: Vec<NamedTotal>,
    /// Lower-case extensions (without the dot, `""` for none) of the files
    /// finished in the period, by bytes.
    pub extensions: Vec<NamedTotal>,
    /// The largest file finished in the period.
    pub largest: Option<NamedTotal>,
}

/// Builds the figures for `days` (local days, oldest first).
pub fn build_stats(
    records: &[DownloadRecord],
    traffic: &[(String, TrafficTotals)],
    days: Vec<String>,
    utc_offset_seconds: i32,
) -> DownloadStats {
    let mut stats = DownloadStats {
        days: days
            .iter()
            .map(|day| ActivityDay {
                day: day.clone(),
                ..ActivityDay::default()
            })
            .collect(),
        ..DownloadStats::default()
    };
    let index: HashMap<&str, usize> = days
        .iter()
        .enumerate()
        .map(|(position, day)| (day.as_str(), position))
        .collect();

    for (day, totals) in traffic {
        if let Some(&position) = index.get(day.as_str()) {
            let entry = &mut stats.days[position];
            entry.domestic_bytes += totals.domestic_bytes;
            entry.international_bytes += totals.international_bytes;
            stats.period_domestic_bytes += totals.domestic_bytes;
            stats.period_international_bytes += totals.international_bytes;
        }
    }

    let mut hosts: HashMap<String, NamedTotal> = HashMap::new();
    let mut extensions: HashMap<String, NamedTotal> = HashMap::new();

    for record in records {
        match record.status {
            DownloadStatus::Failed => stats.failed += 1,
            DownloadStatus::Downloading
            | DownloadStatus::Probing
            | DownloadStatus::Queued
            | DownloadStatus::Paused
            | DownloadStatus::Retrying
            | DownloadStatus::Finalizing => stats.active += 1,
            _ => {}
        }
        if record.status != DownloadStatus::Completed {
            continue;
        }
        let bytes = record.total_bytes.unwrap_or(record.downloaded_bytes);
        stats.all_completed += 1;
        stats.all_completed_bytes += bytes;

        let Some(position) = record
            .completed_at
            .map(|at| local_day(at, utc_offset_seconds))
            .and_then(|day| index.get(day.as_str()).copied())
        else {
            continue;
        };
        stats.days[position].completed += 1;
        stats.period_completed += 1;

        let name = record.filename.clone().unwrap_or_default();
        add(&mut hosts, host_of(&record.source_url), bytes);
        add(&mut extensions, extension_of(&name), bytes);
        if stats
            .largest
            .as_ref()
            .is_none_or(|largest| bytes > largest.bytes)
        {
            stats.largest = Some(NamedTotal {
                name,
                count: 1,
                bytes,
            });
        }
    }

    let mut hosts = sorted(hosts);
    if hosts.len() > TOP_HOSTS {
        let rest = hosts.split_off(TOP_HOSTS - 1);
        hosts.push(NamedTotal {
            name: String::new(),
            count: rest.iter().map(|host| host.count).sum(),
            bytes: rest.iter().map(|host| host.bytes).sum(),
        });
    }
    stats.top_hosts = hosts;
    stats.extensions = sorted(extensions);
    stats
}

fn add(totals: &mut HashMap<String, NamedTotal>, name: String, bytes: u64) {
    let entry = totals.entry(name.clone()).or_insert_with(|| NamedTotal {
        name,
        ..NamedTotal::default()
    });
    entry.count += 1;
    entry.bytes += bytes;
}

fn sorted(totals: HashMap<String, NamedTotal>) -> Vec<NamedTotal> {
    let mut list: Vec<NamedTotal> = totals.into_values().collect();
    list.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    list
}

/// The host of a link without `www.`, or `""` when it has none.
pub fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .map(|host| host.strip_prefix("www.").map(str::to_owned).unwrap_or(host))
        .unwrap_or_default()
}

fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty() && !extension.is_empty() && extension.len() <= 8 =>
        {
            extension.to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dm_common::DownloadPriority;

    const DAY: i64 = 86_400;
    /// 2026-09-20 00:00 UTC.
    const SEP_20: i64 = 1_789_862_400;

    fn record(
        url: &str,
        name: &str,
        status: DownloadStatus,
        bytes: u64,
        completed_at: Option<i64>,
    ) -> DownloadRecord {
        DownloadRecord {
            id: name.to_owned(),
            source_url: url.to_owned(),
            resolved_url: None,
            filename: Some(name.to_owned()),
            destination_path: None,
            temp_path: None,
            mime_type: None,
            total_bytes: Some(bytes),
            downloaded_bytes: bytes,
            etag: None,
            last_modified: None,
            range_supported: None,
            status,
            queue_id: None,
            priority: DownloadPriority::Normal,
            queue_position: None,
            created_at: 0,
            started_at: None,
            completed_at,
            attempts: 0,
            retry_at: None,
            error_code: None,
            error_message: None,
        }
    }

    fn days() -> Vec<String> {
        vec![
            "2026-09-20".into(),
            "2026-09-21".into(),
            "2026-09-22".into(),
        ]
    }

    #[test]
    fn every_day_of_the_period_is_listed_with_its_traffic_and_files() {
        let records = vec![
            record(
                "https://www.Example.com/a.ISO",
                "a.ISO",
                DownloadStatus::Completed,
                700,
                Some(SEP_20 + 3_600),
            ),
            record(
                "https://example.com/b.zip",
                "b.zip",
                DownloadStatus::Completed,
                300,
                Some(SEP_20 + 2 * DAY),
            ),
            // Before the period: counted in all-time figures only.
            record(
                "https://old.org/c.pdf",
                "c.pdf",
                DownloadStatus::Completed,
                50,
                Some(SEP_20 - 5 * DAY),
            ),
            record("https://x.ir/d", "d", DownloadStatus::Failed, 0, None),
            record(
                "https://x.ir/e.mp4",
                "e.mp4",
                DownloadStatus::Paused,
                10,
                None,
            ),
        ];
        let traffic = vec![
            (
                "2026-09-19".to_owned(),
                TrafficTotals {
                    domestic_bytes: 9,
                    international_bytes: 9,
                },
            ),
            (
                "2026-09-20".to_owned(),
                TrafficTotals {
                    domestic_bytes: 100,
                    international_bytes: 600,
                },
            ),
            (
                "2026-09-22".to_owned(),
                TrafficTotals {
                    domestic_bytes: 300,
                    international_bytes: 0,
                },
            ),
        ];

        let stats = build_stats(&records, &traffic, days(), 0);

        assert_eq!(stats.days.len(), 3);
        assert_eq!(stats.days[0].completed, 1);
        assert_eq!(stats.days[0].international_bytes, 600);
        assert_eq!(
            stats.days[1],
            ActivityDay {
                day: "2026-09-21".into(),
                ..ActivityDay::default()
            }
        );
        assert_eq!(stats.days[2].domestic_bytes, 300);
        assert_eq!(stats.period_completed, 2);
        assert_eq!(stats.period_domestic_bytes, 400);
        assert_eq!(stats.period_international_bytes, 600);
        assert_eq!(stats.all_completed, 3);
        assert_eq!(stats.all_completed_bytes, 1_050);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.active, 1);
        assert_eq!(
            stats.top_hosts,
            vec![NamedTotal {
                name: "example.com".into(),
                count: 2,
                bytes: 1_000
            }]
        );
        assert_eq!(stats.extensions[0].name, "iso");
        assert_eq!(stats.extensions[1].name, "zip");
        assert_eq!(stats.largest.unwrap().name, "a.ISO");
    }

    #[test]
    fn a_download_belongs_to_its_local_day() {
        // 22:00 UTC on the 19th is already the 20th in Tehran (+03:30).
        let records = vec![record(
            "https://a.ir/f.bin",
            "f.bin",
            DownloadStatus::Completed,
            1,
            Some(SEP_20 - 2 * 3_600),
        )];
        assert_eq!(build_stats(&records, &[], days(), 0).period_completed, 0);
        assert_eq!(
            build_stats(&records, &[], days(), 12_600).days[0].completed,
            1
        );
    }

    #[test]
    fn hosts_beyond_the_top_are_summed_together() {
        let records: Vec<_> = (0..9)
            .map(|index| {
                record(
                    &format!("https://host{index}.com/f"),
                    &format!("f{index}"),
                    DownloadStatus::Completed,
                    100 - index,
                    Some(SEP_20),
                )
            })
            .collect();
        let stats = build_stats(&records, &[], days(), 0);
        assert_eq!(stats.top_hosts.len(), TOP_HOSTS);
        let rest = stats.top_hosts.last().unwrap();
        assert_eq!(rest.name, "");
        assert_eq!(rest.count, 4);
        assert_eq!(
            stats.extensions,
            vec![NamedTotal {
                name: String::new(),
                count: 9,
                bytes: (92..=100).sum()
            }]
        );
    }
}
