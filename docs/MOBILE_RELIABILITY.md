# Mobile reliability milestone

The Android companion already executes yt-dlp/FFmpeg in a Kotlin backend. This
milestone keeps that boundary: SQLite is authoritative for mobile jobs and the
foreground transfer runner owns their transitions. Activities render records and
send commands; they do not independently own download progress. Desktop download
state remains authoritative in Rust/SQLite. No critical state moves into React.

Tasks are committed before network access. Stable UUID workspaces live in private
app files, not disposable cache. Pause, network interruption, and recoverable
failure preserve partial data. Process recovery makes interrupted jobs paused and
offers explicit resume; Android force-stop and foreground time limits are honored.
No password, cookie, authorization header, or browser credential is persisted.

The requested milestone covers the audit findings A01-A15 and W01-W08, quick Share,
albums/photos where the public extractor exposes them, generic file downloads,
and accountless link handoff. Store publication and device certification are separate
external gates; links must never imply an extension store approval that did not occur.

Download URLs are private user data. Diagnostics contain typed error codes, never
raw extractor output or full signed URLs. The app does not add login harvesting,
background clipboard watching, overlay control, or Instagram Accessibility automation.

Acceptance scenarios are tracked in docs/mobile-acceptance.md. Targeted tests are
run between slices; repository-required checks run before the complete change ships.
