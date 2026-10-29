import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { updateReleaseLinks } from '../worker.mjs';

const asset = (name, tag) => ({ name, browser_download_url: `https://github.com/sajjadka21/ratatoskr/releases/download/${tag}/${name}` });

test('the Worker serves the latest independent Windows and Android stable releases', async () => {
  const html = `<span id="hero-status">old</span><p class="release-state" id="release-state"><span></span>old</p>
    <h2 id="download-title">old</h2><p id="release-date">old</p>
    <a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/Ratatoskr_1.3.0_x64-setup.exe">Windows</a>
    <a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/Ratatoskr-android.apk">Android</a>
    <a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/SHA256SUMS.txt">Checksums</a>
    <a id="release-notes" href="old">Release notes</a>
    <article data-platform="android"><a class="button button-gold download-file" href="x">Get APK · 1.2.0</a></article>
    <a id="checksums-desktop" href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/SHA256SUMS.txt">old</a><a id="checksums-android" href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/SHA256SUMS.txt">old</a>`;
  const releases = {
    windows: {
      tag_name: 'v1.3.1', published_at: '2026-10-05T10:00:00Z', html_url: 'https://github.com/sajjadka21/ratatoskr/releases/tag/v1.3.1',
      assets: [asset('Ratatoskr_1.3.1_x64-setup.exe', 'v1.3.1'), asset('SHA256SUMS.txt', 'v1.3.1')],
    },
    android: {
      tag_name: 'v1.3.0', assets: [asset('Ratatoskr-android.apk', 'v1.3.0'), asset('SHA256SUMS.txt', 'v1.3.0')],
    },
  };
  const result = updateReleaseLinks(html, releases, 'en');
  assert.match(result, /Ratatoskr_1\.3\.1_x64-setup\.exe/);
  assert.match(result, /releases\/download\/v1\.3\.0\/Ratatoskr-android\.apk/);
  assert.match(result, /Windows SHA-256/);
  assert.match(result, /Android SHA-256 · 1\.3\.0/);
  assert.match(result, /Published release · 1\.3\.1/);
  assert.match(result, /releases\/tag\/v1\.3\.1/);
});

test('the Worker source is an ES module with the assets binding as its only dependency', async () => {
  const source = await readFile(new URL('../worker.mjs', import.meta.url), 'utf8');
  assert.match(source, /env\.ASSETS\.fetch\(request\)/);
  assert.doesNotMatch(source, /^import\s/m);
});

test('the Worker keeps Linux links current and labels each button with its own version', () => {
  const html = `<article data-platform="windows"><a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.6/Ratatoskr_1.3.6_x64-setup.exe">Download for Windows · 1.3.6</a></article>
    <article data-platform="linux"><a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.6/Ratatoskr_1.3.6_amd64.deb">Debian / Ubuntu (.deb) · 1.3.6</a><a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.6/Ratatoskr-1.3.6-1.x86_64.rpm">rpm · 1.3.6</a><a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.6/Ratatoskr-1.3.6-linux-x86_64.tar.gz">tar · 1.3.6</a><a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.6/Ratatoskr_1.3.6_amd64.AppImage">AppImage · 1.3.6</a></article>
    <article data-platform="android"><a href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/Ratatoskr-android.apk">Download Android APK · 1.3.0</a></article>
    <a id="checksums-desktop" href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.6/SHA256SUMS.txt" rel="noopener noreferrer">old</a><a id="checksums-android" href="https://github.com/sajjadka21/ratatoskr/releases/download/v1.3.0/SHA256SUMS.txt" rel="noopener noreferrer">old</a>`;
  const desktop = ['Ratatoskr_1.3.7_x64-setup.exe', 'Ratatoskr_1.3.7_amd64.deb', 'Ratatoskr-1.3.7-1.x86_64.rpm',
    'Ratatoskr-1.3.7-linux-x86_64.tar.gz', 'Ratatoskr_1.3.7_amd64.AppImage', 'SHA256SUMS.txt'].map(name => asset(name, 'v1.3.7'));
  const release = { tag_name: 'v1.3.7', published_at: '2026-10-29T18:09:21Z', html_url: 'https://github.com/sajjadka21/ratatoskr/releases/tag/v1.3.7', assets: desktop };
  const result = updateReleaseLinks(html, {
    windows: release, linux: release,
    android: { tag_name: 'v1.3.0', assets: [asset('Ratatoskr-android.apk', 'v1.3.0'), asset('SHA256SUMS.txt', 'v1.3.0')] },
  }, 'en');
  for (const name of ['Ratatoskr_1.3.7_amd64.deb', 'Ratatoskr-1.3.7-1.x86_64.rpm', 'Ratatoskr-1.3.7-linux-x86_64.tar.gz', 'Ratatoskr_1.3.7_amd64.AppImage']) {
    assert.match(result, new RegExp(`releases/download/v1\.3\.7/${name.replaceAll('.', '\.')}`));
  }
  assert.doesNotMatch(result, /· 1\.3\.6/);
  assert.match(result, /Download Android APK · 1\.3\.0<\/a>/);
  assert.doesNotMatch(result, /1\.3\.0 · 1\.3\.0/);
  assert.match(result, /id="checksums-desktop" href="[^"]+v1\.3\.7\/SHA256SUMS\.txt"[^>]*>Windows &amp; Linux SHA-256|id="checksums-desktop" href="[^"]+v1\.3\.7\/SHA256SUMS\.txt"[^>]*>Windows & Linux SHA-256/);
  assert.match(result, /id="checksums-android" href="[^"]+v1\.3\.0\/SHA256SUMS\.txt"[^>]*>Android SHA-256 · 1\.3\.0/);
});
