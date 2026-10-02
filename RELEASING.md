# Releasing Ratatoskr

Installed copies update themselves from a signed release. The app accepts an
update only if the installer's signature matches the public key built into it
(`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`).

## The signing key

The private key is `.signing/ratatosk-updater.key` (it has no password). It is
listed in `.gitignore` and must never be committed or shared.

- **Back it up** somewhere safe (a password manager, an encrypted drive).
- **If it is lost**, installed copies can no longer be updated: users would
  have to install the next version by hand, and the public key in
  `tauri.conf.json` would change with a new key.
- **If it leaks**, anyone could sign an update. Make a new key
  (`npx tauri signer generate -w .signing/ratatosk-updater.key`), put the new
  public key in `tauri.conf.json`, and release by hand once.

## One-time setup: where releases live

Releases live on GitHub: `https://github.com/sajjadka21/ratatoskr/releases`.
Installed copies read
`https://github.com/sajjadka21/ratatoskr/releases/latest/download/latest.json`
(`plugins.updater.endpoints` in `src-tauri/tauri.conf.json`). The repository
must be public for installed copies to reach it.

## Automatic releases (GitHub Actions)

`.github/workflows/release.yml` builds everything when a `v*` tag is pushed:
the signed Windows installer, `latest.json`, the portable zip and the Android
APK, then publishes the release after the reusable CI checks pass. Each
artifact is flattened into one staging folder, checked for duplicate names,
signed updater metadata is verified, and SHA-256 checksums are generated.
Assets are attached to a draft first, then the complete release is published.

Tag-triggered releases publish as previews by default and do not replace the
stable automatic-update target. After recording the applicable checks in
`docs/mobile-acceptance.md`, dispatch the workflow on the existing tag with
`promote_stable=true` only when deliberately promoting a fully reviewed release.
For an already published preview, promote its verified assets with GitHub's
release edit action rather than rebuilding/replacing binaries under the same
version. A new version is required if the binaries change.

Android publishes one APK per CPU (no universal APK, to keep downloads small); `Ratatoskr-android.apk` is the arm64 build. The three
browser ZIPs are separate submission packages; marketplace approval and actual
assigned extension IDs remain separate work. CI debug APKs are excluded from
release asset collection.

One-time setup, in the repository's Settings → Secrets and variables → Actions:

| Secret | What |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the whole text of `.signing/ratatosk-updater.key` (required) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | only if the key has a password (it currently has none) |
| `ANDROID_KEYSTORE_BASE64` | your persistent Android keystore, base64; required |
| `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | the keystore password, alias and key password; required |

Missing Android secrets fail the release. Debug builds use a separate debug
key and must never be distributed as a public release. The production key
created for 1.0.1 is `.signing/ratatoskr-android.p12` in the owner's local
project; its password is protected with Windows DPAPI under the same Windows
account and stored in GitHub Actions Secrets, never in source or logs.
Back up the keystore and its credentials securely. Never regenerate this key
for later updates. The public certificate is
`.signing/ratatoskr-android-certificate.der`.

The repository variable `ANDROID_SIGNING_CERT_SHA256` pins the expected APK
certificate. Current fingerprint:
`4b1ca19c7645b36c951f793494506a125d3e01e41a2ed5336d5d49dceb85e476`.
The release job rejects a different certificate, the wrong package/version,
or a debuggable APK. An old debug-signed copy with the same package ID must
be uninstalled before the production-signed version can be installed.

To release: raise the version in `src-tauri/tauri.conf.json`, `package.json`,
`src-tauri/Cargo.toml`, `crates/dm-cli/Cargo.toml`, both lockfiles and
`android/app/build.gradle.kts`; increment Android `versionCode`. Add curated
notes in `docs/releases/<version>.md`, run
`node scripts/verify-release-version.mjs v<version>`, commit, then push a new
immutable tag. Do not move an existing tag to repair a release. Manual runs
must select the version tag, not the main branch.

The Windows update signature is a Tauri/Minisign signature, not Windows
Authenticode. The current Windows binaries do not have an Authenticode
publisher certificate. Physical device installation/download tests remain
separate from automated build checks.

## Android licence and native sources

Android is GPL-3.0-only; desktop keeps PolyForm Noncommercial. Keep android/LICENSE and NOTICE inside the APK, and publish Ratatoskr-android-source.zip containing the same-version application/build sources and the SHA-256-pinned WebP source rebuilt for 16 KB compatibility. The release job recursively verifies final APK native payloads before upload. See LICENSES.md and android/NATIVE_COMPATIBILITY.md.

## Withdrawing a faulty release

Pause promotion by marking the release as a prerelease and removing the
landing download links if a serious fault is found. Do not replace binaries
behind an existing version: publish a higher patch version with corrected
notes and artifacts. Preserve user databases and portable `data` folders.
For 1.0.2, no earlier public stable release exists to promote instead. Version 1.0.1 was cancelled before publication.

## Releasing by hand (Windows)

## Each release

1. Raise `version` in `src-tauri/tauri.conf.json` (and `package.json`).
2. Run, with the address the files will be downloaded from:

   ```
   scripts\release.cmd https://github.com/OWNER/REPO/releases/download/v1.0.1
   ```

   This builds the installer, signs it and writes `latest.json` in
   `target\release\bundle\nsis`. Set `RELEASE_NOTES` first to include notes.
3. Create the GitHub release `v1.0.1` and upload the `…-setup.exe` and
   `latest.json` from that folder.

Installed copies check once a day (if allowed in Settings) and offer the new
version; nothing installs without the user choosing it.

## The portable version

`scripts\make-portable.cmd` writes `target\portable\Ratatoskr-portable.zip`.
Unzip it anywhere (a USB drive works); the file `portable.txt` next to
`Ratatoskr.exe` makes the app keep its database, tools and web view data in a
`data` folder beside it, and nothing in the user's profile. Delete
`portable.txt` to make a copy use the normal folders again. Portable copies do
not update themselves: replace the folder, keeping `data`.

## The browser extension in the stores

`node scripts/package-extension.mjs` writes
`target/extension/ratatosk-extension-<version>.zip`, the file every store
takes.

- **Firefox (free).** At addons.mozilla.org, submit the zip as "On your own"
  (self-distributed). Mozilla signs it, usually within minutes, and gives a
  `.xpi` file. Save it as `src-tauri/extras/ratatosk-firefox.xpi` before
  building: Settings then installs it in Firefox with one confirmation. A
  listed (public) entry is free too; put its address in `STORE_PAGES.firefox`
  in `src/components/settings/BrowserSection.tsx`.
- **Edge (free).** Microsoft Partner Center accounts for extensions cost
  nothing. After publishing, add the store's extension ID to
  `STORE_EXTENSION_IDS` in `crates/dm-system/src/browser_hosts.rs` and the
  address to `STORE_PAGES.edge`.
- **Chrome.** The Chrome Web Store asks a one-time 5 USD registration fee.
  Until then, Chrome users load the folder by hand (Settings opens it), or
  use the clipboard watching, which needs no extension at all.
