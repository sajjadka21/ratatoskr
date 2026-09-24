//! The download list as a file: a spreadsheet (CSV) or plain links.

use crate::traffic::local_time;
use dm_common::DownloadRecord;

/// A CSV spreadsheet of the downloads. Starts with a byte-order mark so
/// Excel reads Persian names correctly, and neutralises cells that a
/// spreadsheet would run as a formula.
pub fn downloads_csv(records: &[DownloadRecord], utc_offset_seconds: i32) -> String {
    let mut csv = String::from("\u{feff}name,status,size_bytes,added,completed,link,file\r\n");
    for record in records {
        let time = |at: Option<i64>| {
            at.map(|at| local_time(at, utc_offset_seconds))
                .unwrap_or_default()
        };
        let size = record
            .total_bytes
            .map(|bytes| bytes.to_string())
            .unwrap_or_default();
        let row = [
            record.filename.clone().unwrap_or_default(),
            record.status.to_string(),
            size,
            time(Some(record.created_at)),
            time(record.completed_at),
            record.source_url.clone(),
            record.destination_path.clone().unwrap_or_default(),
        ];
        let cells: Vec<String> = row.iter().map(|cell| cell_text(cell)).collect();
        csv.push_str(&cells.join(","));
        csv.push_str("\r\n");
    }
    csv
}

/// One link per line, the way the link grabber and other download managers
/// read a list.
pub fn links_text(records: &[DownloadRecord]) -> String {
    let mut text = String::new();
    for record in records {
        text.push_str(&record.source_url);
        text.push_str("\r\n");
    }
    text
}

fn cell_text(value: &str) -> String {
    // A leading = + - @ (or a tab/return) makes spreadsheets evaluate the
    // cell; a quote in front keeps it text.
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dm_common::{DownloadPriority, DownloadStatus};

    fn record(name: &str, url: &str) -> DownloadRecord {
        DownloadRecord {
            id: "1".into(),
            source_url: url.into(),
            resolved_url: None,
            filename: Some(name.into()),
            destination_path: Some("D:\\Downloads\\x".into()),
            temp_path: None,
            mime_type: None,
            total_bytes: Some(1_024),
            downloaded_bytes: 1_024,
            etag: None,
            last_modified: None,
            range_supported: None,
            status: DownloadStatus::Completed,
            queue_id: None,
            priority: DownloadPriority::Normal,
            queue_position: None,
            created_at: 1_789_862_400,
            started_at: None,
            completed_at: Some(1_789_862_400 + 90),
            attempts: 0,
            retry_at: None,
            error_code: None,
            error_message: None,
        }
    }

    #[test]
    fn rows_are_quoted_and_formulas_are_neutralised() {
        let csv = downloads_csv(
            &[
                record("گزارش, نهایی.pdf", "https://a.ir/x?a=1&b=\"2\""),
                record("=HYPERLINK(\"evil\")", "https://b.com/y"),
            ],
            12_600,
        );
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert!(lines[0].starts_with('\u{feff}'));
        assert_eq!(
            lines[1],
            "\"گزارش, نهایی.pdf\",completed,1024,2026-09-20 03:30,2026-09-20 03:31,\"https://a.ir/x?a=1&b=\"\"2\"\"\",D:\\Downloads\\x"
        );
        assert!(lines[2].starts_with("\"'=HYPERLINK(\"\"evil\"\")\""));
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn links_are_one_per_line() {
        assert_eq!(
            links_text(&[
                record("a", "https://a.ir/1"),
                record("b", "https://b.com/2")
            ]),
            "https://a.ir/1\r\nhttps://b.com/2\r\n"
        );
    }
}
