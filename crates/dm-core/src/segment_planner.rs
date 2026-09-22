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

/// Creates a deterministic, gap-free inclusive map. The map intentionally
/// contains at most four segments per worker: enough parallelism to keep a
/// worker busy when one range stalls, while the worker pool remains bounded.
pub fn plan_segments(
    download_id: &str,
    temp_path: &Path,
    total_bytes: u64,
    max_connections: usize,
) -> Result<Vec<DownloadSegment>, SegmentPlanError> {
    if total_bytes == 0 {
        return Err(SegmentPlanError::EmptyResource);
    }
    if max_connections == 0 {
        return Err(SegmentPlanError::InvalidConcurrency);
    }

    let max_segments = max_connections
        .checked_mul(4)
        .ok_or(SegmentPlanError::ResourceTooLarge)?;
    let total_as_usize = usize::try_from(total_bytes).unwrap_or(usize::MAX);
    let segment_count = max_segments.clamp(1, 256).min(total_as_usize.max(1));
    let count_u64 = u64::try_from(segment_count).map_err(|_| SegmentPlanError::ResourceTooLarge)?;
    let width = total_bytes.div_ceil(count_u64);
    let mut segments = Vec::with_capacity(segment_count);

    for index in 0..segment_count {
        let index_u64 = u64::try_from(index).map_err(|_| SegmentPlanError::ResourceTooLarge)?;
        let start_byte = index_u64
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
            temp_path: format!("{}.segment-{index:04}.part", temp_path.to_string_lossy()),
            status: SegmentStatus::Pending,
        });
    }

    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn plans_contiguous_ranges_with_exact_coverage() {
        let directory = tempdir().unwrap();
        let segments = plan_segments("task", &directory.path().join("file.part"), 101, 3).unwrap();

        assert_eq!(segments.len(), 12);
        assert_eq!(segments.first().unwrap().start_byte, 0);
        assert_eq!(segments.last().unwrap().end_byte, 100);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].end_byte + 1, pair[1].start_byte);
        }
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.expected_bytes().unwrap())
                .sum::<u64>(),
            101
        );
    }

    #[test]
    fn small_resources_use_one_byte_ranges_without_overlap() {
        let directory = tempdir().unwrap();
        let segments = plan_segments("task", &directory.path().join("file.part"), 3, 8).unwrap();

        assert_eq!(segments.len(), 3);
        assert!(
            segments
                .iter()
                .all(|segment| segment.start_byte == segment.end_byte)
        );
    }

    #[test]
    fn rejects_empty_resources_and_zero_workers() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("file.part");
        assert_eq!(
            plan_segments("task", &path, 0, 1),
            Err(SegmentPlanError::EmptyResource)
        );
        assert_eq!(
            plan_segments("task", &path, 1, 0),
            Err(SegmentPlanError::InvalidConcurrency)
        );
    }
}
