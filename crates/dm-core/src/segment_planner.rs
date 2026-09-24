use dm_common::{DownloadSegment, SegmentStatus};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SegmentPlanError {
    #[error("segmented transfers require a positive content length")]
    EmptyResource,

    #[error("segment concurrency must be positive")]
    InvalidConcurrency,

    #[error("resource is too large to plan on this platform")]
    ResourceTooLarge,
}

/// Creates a deterministic, gap-free inclusive map with one range per
/// connection, every range at least `min_segment_bytes` long (except when
/// the whole file is smaller). All ranges share `temp_path`: they are
/// written into one preallocated file. Ranges are split further while the
/// transfer runs, so a slow range at the end is shared out rather than
/// waited for.
pub fn plan_segments(
    download_id: &str,
    temp_path: &Path,
    total_bytes: u64,
    max_connections: usize,
    min_segment_bytes: u64,
) -> Result<Vec<DownloadSegment>, SegmentPlanError> {
    if total_bytes == 0 {
        return Err(SegmentPlanError::EmptyResource);
    }
    if max_connections == 0 {
        return Err(SegmentPlanError::InvalidConcurrency);
    }

    let by_size = total_bytes.div_ceil(min_segment_bytes.max(1));
    let connections =
        u64::try_from(max_connections.min(256)).map_err(|_| SegmentPlanError::ResourceTooLarge)?;
    let count = connections.min(by_size).max(1);
    let width = total_bytes.div_ceil(count);
    let path = temp_path.to_string_lossy().into_owned();
    let mut segments = Vec::new();

    for index in 0..count {
        let start_byte = index
            .checked_mul(width)
            .ok_or(SegmentPlanError::ResourceTooLarge)?;
        if start_byte >= total_bytes {
            break;
        }
        let end_byte = start_byte.saturating_add(width - 1).min(total_bytes - 1);
        segments.push(DownloadSegment {
            download_id: download_id.to_owned(),
            segment_index: u32::try_from(index).map_err(|_| SegmentPlanError::ResourceTooLarge)?,
            start_byte,
            end_byte,
            downloaded_bytes: 0,
            temp_path: path.clone(),
            status: SegmentStatus::Pending,
        });
    }

    Ok(segments)
}

/// Whether stored ranges still describe `[0, total_bytes)` exactly, with no
/// gap and no overlap, all in the one shared file.
pub fn covers_exactly(segments: &[DownloadSegment], total_bytes: u64, temp_path: &Path) -> bool {
    if segments.is_empty() || total_bytes == 0 {
        return false;
    }
    let path = temp_path.to_string_lossy();
    let mut ordered: Vec<&DownloadSegment> = segments.iter().collect();
    ordered.sort_by_key(|segment| segment.start_byte);
    let mut next = 0_u64;
    for segment in ordered {
        if segment.temp_path != path || segment.start_byte != next {
            return false;
        }
        next = segment.end_byte.saturating_add(1);
    }
    next == total_bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn plans_one_contiguous_range_per_connection_in_one_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("file.part");
        let segments = plan_segments("task", &path, 101, 3, 1).unwrap();

        assert_eq!(segments.len(), 3);
        assert_eq!(segments.first().unwrap().start_byte, 0);
        assert_eq!(segments.last().unwrap().end_byte, 100);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].end_byte + 1, pair[1].start_byte);
        }
        assert!(
            segments
                .iter()
                .all(|segment| segment.temp_path == path.to_string_lossy())
        );
        assert!(covers_exactly(&segments, 101, &path));
    }

    #[test]
    fn small_files_get_fewer_ranges_than_connections() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("file.part");
        let segments = plan_segments("task", &path, 1000, 16, 400).unwrap();
        assert_eq!(segments.len(), 3);

        let tiny = plan_segments("task", &path, 3, 8, 1).unwrap();
        assert_eq!(tiny.len(), 3);
    }

    #[test]
    fn coverage_detects_gaps_overlaps_and_foreign_files() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("file.part");
        let mut segments = plan_segments("task", &path, 100, 2, 1).unwrap();
        assert!(!covers_exactly(&segments, 101, &path));

        segments[1].start_byte += 1;
        assert!(!covers_exactly(&segments, 100, &path));

        let mut old_style = plan_segments("task", &path, 100, 2, 1).unwrap();
        old_style[0].temp_path = "file.part.segment-0000.part".to_owned();
        assert!(!covers_exactly(&old_style, 100, &path));
    }

    #[test]
    fn rejects_empty_resources_and_zero_workers() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("file.part");
        assert_eq!(
            plan_segments("task", &path, 0, 1, 1),
            Err(SegmentPlanError::EmptyResource)
        );
        assert_eq!(
            plan_segments("task", &path, 1, 0, 1),
            Err(SegmentPlanError::InvalidConcurrency)
        );
    }
}
