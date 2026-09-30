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
APK, then publishes the release.

One-time setup, in the repository's Settings → Secrets and variables → Actions:

| Secret | What |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the whole text of `.signing/ratatosk-updater.key` (required) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | only if the key has a password (it currently has none) |
| `ANDROID_KEYSTORE_BASE64` | your Android keystore, base64 (`base64 -w0 release.jks`); optional |
| `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | its passwords, when you set the keystore |

Without the Android secrets the APK is signed with a debug key: it installs
and works, but a later release signed with a real key cannot update over it.
Make the real keystore once and keep it (`keytool -genkeypair -v -keystore
release.jks -alias ratatoskr -keyalg RSA -keysize 2048 -validity 10000`).

To release: raise the version in `src-tauri/tauri.conf.json`, `package.json`,
`src-tauri/Cargo.toml` and `android/app/build.gradle.kts`, commit, then
`git tag v1.0.1 && git push origin v1.0.1`.

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
