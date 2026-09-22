use crate::SourceProbe;

/// What a previous attempt left behind, as the record and the disk describe it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StoredTransfer {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Size of the partial file, or `None` when it is no longer there.
    pub partial_bytes_on_disk: Option<u64>,
}

/// Why partial bytes could not be reused. Surfaced to the user, because
/// silently restarting a large transfer is exactly the kind of thing a
/// download manager should explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartReason {
    NothingTransferred,
    PartialFileMissing,
    RangeNotSupported,
    SourceChanged,
    SizeChanged,
}

impl RestartReason {
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::NothingTransferred => "starting from the beginning",
            Self::PartialFileMissing => {
                "the partial file is gone, so the download starts from the beginning"
            }
            Self::RangeNotSupported => {
                "the server does not support resuming, so the download starts from the beginning"
            }
            Self::SourceChanged => {
                "the file on the server changed, so the download starts from the beginning"
            }
            Self::SizeChanged => {
                "the file size on the server changed, so the download starts from the beginning"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumePlan {
    StartFromZero(RestartReason),
    ContinueFrom(u64),
    /// Everything was already transferred; only finalization is left.
    AlreadyComplete(u64),
}

/// Decides whether partial bytes may be reused.
///
/// The rule the specification insists on is that bytes are never appended
/// blindly: the remote identity has to still match. Validators decide when
/// the server offers them; when it offers none, an unchanged total size is the
/// weakest evidence accepted, and anything less restarts the transfer.
pub fn plan_resume(stored: &StoredTransfer, probe: &SourceProbe) -> ResumePlan {
    if stored.downloaded_bytes == 0 {
        return ResumePlan::StartFromZero(RestartReason::NothingTransferred);
    }

    let Some(on_disk) = stored.partial_bytes_on_disk else {
        return ResumePlan::StartFromZero(RestartReason::PartialFileMissing);
    };

    // Trust the file over the record: a process killed between writing and
    // persisting leaves the record ahead of or behind the disk.
    let offset = stored.downloaded_bytes.min(on_disk);

    if offset == 0 {
        return ResumePlan::StartFromZero(RestartReason::PartialFileMissing);
    }

    if validators_disagree(stored, probe) {
        return ResumePlan::StartFromZero(RestartReason::SourceChanged);
    }

    if let (Some(stored_total), Some(probe_total)) = (stored.total_bytes, probe.total_bytes)
        && stored_total != probe_total
    {
        return ResumePlan::StartFromZero(RestartReason::SizeChanged);
    }

    if let Some(total) = probe.total_bytes.or(stored.total_bytes) {
        if offset > total {
            return ResumePlan::StartFromZero(RestartReason::SizeChanged);
        }

        if offset == total {
            return ResumePlan::AlreadyComplete(offset);
        }
    }

    if !probe.range_supported {
        return ResumePlan::StartFromZero(RestartReason::RangeNotSupported);
    }

    ResumePlan::ContinueFrom(offset)
}

/// True when the source offers a validator that does not match the one the
/// partial file was downloaded with. Two absent validators are not a
/// disagreement; a validator that disappeared is not evidence of a change.
fn validators_disagree(stored: &StoredTransfer, probe: &SourceProbe) -> bool {
    let etag_changed = matches!(
        (stored.etag.as_deref(), probe.etag.as_deref()),
        (Some(stored), Some(current)) if stored != current
    );

    let modified_changed = matches!(
        (stored.last_modified.as_deref(), probe.last_modified.as_deref()),
        (Some(stored), Some(current)) if stored != current
    );

    etag_changed || modified_changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> SourceProbe {
        SourceProbe {
            final_url: "https://example.com/file.bin".to_owned(),
            filename: "file.bin".to_owned(),
            content_type: None,
            total_bytes: Some(1_000),
            etag: Some("\"v1\"".to_owned()),
            last_modified: None,
            range_supported: true,
        }
    }

    fn stored() -> StoredTransfer {
        StoredTransfer {
            downloaded_bytes: 400,
            total_bytes: Some(1_000),
            etag: Some("\"v1\"".to_owned()),
            last_modified: None,
            partial_bytes_on_disk: Some(400),
        }
    }

    #[test]
    fn continues_when_the_source_still_matches() {
        assert_eq!(
            plan_resume(&stored(), &probe()),
            ResumePlan::ContinueFrom(400)
        );
    }

    #[test]
    fn refuses_to_append_to_content_that_changed() {
        let mut probe = probe();
        probe.etag = Some("\"v2\"".to_owned());

        assert_eq!(
            plan_resume(&stored(), &probe),
            ResumePlan::StartFromZero(RestartReason::SourceChanged)
        );
    }

    #[test]
    fn refuses_to_append_when_last_modified_moved() {
        let mut stored = stored();
        stored.etag = None;
        stored.last_modified = Some("Wed, 21 Oct 2026 07:28:00 GMT".to_owned());

        let mut probe = probe();
        probe.etag = None;
        probe.last_modified = Some("Thu, 22 Oct 2026 09:00:00 GMT".to_owned());

        assert_eq!(
            plan_resume(&stored, &probe),
            ResumePlan::StartFromZero(RestartReason::SourceChanged)
        );
    }

    #[test]
    fn refuses_to_append_when_the_size_changed() {
        let mut probe = probe();
        probe.etag = None;
        probe.total_bytes = Some(2_000);

        let mut stored = stored();
        stored.etag = None;

        assert_eq!(
            plan_resume(&stored, &probe),
            ResumePlan::StartFromZero(RestartReason::SizeChanged)
        );
    }

    #[test]
    fn accepts_an_unchanged_size_when_the_server_offers_no_validator() {
        let mut probe = probe();
        probe.etag = None;

        let mut stored = stored();
        stored.etag = None;

        assert_eq!(plan_resume(&stored, &probe), ResumePlan::ContinueFrom(400));
    }

    #[test]
    fn restarts_when_the_server_cannot_resume() {
        let mut probe = probe();
        probe.range_supported = false;

        assert_eq!(
            plan_resume(&stored(), &probe),
            ResumePlan::StartFromZero(RestartReason::RangeNotSupported)
        );
    }

    #[test]
    fn restarts_when_the_partial_file_is_gone() {
        let mut stored = stored();
        stored.partial_bytes_on_disk = None;

        assert_eq!(
            plan_resume(&stored, &probe()),
            ResumePlan::StartFromZero(RestartReason::PartialFileMissing)
        );
    }

    #[test]
    fn trusts_the_file_when_the_record_is_ahead_of_it() {
        let mut stored = stored();
        stored.downloaded_bytes = 400;
        stored.partial_bytes_on_disk = Some(256);

        assert_eq!(
            plan_resume(&stored, &probe()),
            ResumePlan::ContinueFrom(256)
        );
    }

    #[test]
    fn reports_a_transfer_that_is_already_complete() {
        let mut stored = stored();
        stored.downloaded_bytes = 1_000;
        stored.partial_bytes_on_disk = Some(1_000);

        assert_eq!(
            plan_resume(&stored, &probe()),
            ResumePlan::AlreadyComplete(1_000)
        );
    }

    #[test]
    fn starts_from_zero_when_nothing_was_transferred() {
        let stored = StoredTransfer::default();

        assert_eq!(
            plan_resume(&stored, &probe()),
            ResumePlan::StartFromZero(RestartReason::NothingTransferred)
        );
    }
}
