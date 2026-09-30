# Releasing Ratatosk

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

Set the address installed copies read, in `src-tauri/tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "endpoints": ["https://github.com/OWNER/REPO/releases/latest/download/latest.json"]
  }
}
```

Until this is set, Settings → Updates says updates are not set up, and the
app contacts nothing.

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

`scripts\make-portable.cmd` writes `target\portable\Ratatosk-portable.zip`.
Unzip it anywhere (a USB drive works); the file `portable.txt` next to
`Ratatosk.exe` makes the app keep its database, tools and web view data in a
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
