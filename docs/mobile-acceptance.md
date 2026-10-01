# Mobile reliability acceptance

This checklist describes the implementation on `feature/mobile-reliability`. It is not a device certification or a published release announcement.

## Automated evidence

- Persistent SQLite queue, history, per-item output journal, unique task ownership and conditional state updates: Robolectric store/service tests.
- Pause/resume and process recovery: retained private workspaces and validated HTTP range/identity journals. Tests cover partial/oversized bodies, changed entities and save retry.
- Wi-Fi, unmetered and roaming are independent policies. Foreground and scheduled work share the concurrency limit. Pending system jobs preserve their original constraints unless preferences change.
- System background work runs bounded slices. A new run cannot complete an older run's parameters or acquire its writer before cleanup finishes.
- Photos, selected album items, audio extraction and actual available quality labels have metadata/policy regression tests. Public Instagram links enter through the Android share sheet; optional quick download queues immediately.
- Direct HTTP uses pinned, public-only DNS answers and guarded redirects. Native media requests use a scoped loopback proxy with end-to-end TLS; only HTTP, HTTPS, native HLS and native DASH formats are selected. Mixed public/private DNS answers and private numeric destinations are rejected. Proxy socket tests verify the transport itself; native engine interoperability remains a device acceptance gate.
- Desktop QR generation is local and accountless. The Android deep link validates one public URL, forbids credential fields, and opens the existing download choices. This is one-way link handoff, not device pairing or file transfer.
- Independent light/dark/system appearance and four brands, searchable grouped settings, keyboard focus and compact sidebar labels have frontend regression coverage.
- Browser connection uses a recent passive heartbeat. Out-of-order and failed replies cannot leave stale connection state. Firefox optional diagnostics consent is respected.
- Packaging builds a universal APK plus three ABI APKs and three browser-specific ZIPs. Release assembly requires every payload and checksums each one. A package is not a store approval.

## Physical acceptance before promoting a release

Record device/OS, app version, network, expected/actual result and output hash where relevant. Do not mark these complete from browser previews or unit tests.

1. Upgrade the signed APK over the previous public release without losing queue/history; verify certificate continuity and architecture compatibility.
2. Share a public Instagram reel, a photo and a mixed album; choose quality/items, test quick mode, and open/share the resulting files. Check friendly handling of login-required, deleted and rate-limited posts. Private-account access is not promised.
3. Download a large resumable file; pause, relaunch, lose Wi-Fi, reboot and resume. Compare final bytes with the source hash. Honor an explicit Android force-stop; do not promise automatic restart after it.
4. Switch Wi-Fi-only/unmetered/roaming policies while active. Confirm disallowed requests stop and system-job continuation respects the new policy. Test VPN networks whose underlying Wi-Fi is unavailable.
5. Exercise Android 15 background timeouts, process death and simultaneous foreground/system scheduling. Confirm no duplicate writers, duplicate output or indefinite foreground service.
6. Interrupt saving, exhaust storage and cancel a recovered task. Confirm no published partial file or orphan pending MediaStore entry.
7. Scan a desktop QR on Android with Ratatoskr installed. Test Persian names, signed file URLs and unsupported/private/oversized links. No download starts merely from parsing the payload.
8. Check Persian RTL and English, light/dark/system modes, large font, notification permission denied, notification open/share and audio-only playback.
9. On Windows, upgrade from the previous installer, test updater signature rejection/rollback, sleep/resume, low storage, long Persian names, large history and cancellable completion actions. Authenticode trust requires an actual signing certificate.
10. Install each extension from its packaged manifest and eventually its store listing. Verify browser restart, host missing, explicit connection check and Firefox data consent. Store publication requires store approval and assigned IDs.

## Release status

Unverified device items remain release gates. CI debug APKs are diagnostic artifacts, not signed distributable releases. Preserve the previous stable updater until a candidate has passed the applicable gates.
