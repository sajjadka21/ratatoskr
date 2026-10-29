import fs from 'node:fs/promises';

const configPath = new URL('../site.config.json', import.meta.url);
const config = JSON.parse(await fs.readFile(configPath, 'utf8'));
if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(config.repository)) {
  throw new Error('Set the real public owner/repository first');
}

const response = await fetch(`https://api.github.com/repos/${config.repository}/releases?per_page=30`, {
  headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'Ratatoskr-landing' },
});
if (!response.ok) throw new Error(`GitHub release lookup failed: ${response.status}`);
const releases = (await response.json())
  .filter(release => !release.draft && !release.prerelease && /^v\d+\.\d+\.\d+$/.test(release.tag_name));
if (!releases.length) throw new Error('A public stable version tag is required');

const assets = release => new Map((release.assets || []).map(asset => [asset.name, asset]));
const versionOf = release => release.tag_name.slice(1);
const hasAssets = (release, names) => names.every(name => assets(release).has(name));
const windowsRelease = releases.find(release => {
  const version = versionOf(release);
  return hasAssets(release, [
    `Ratatoskr_${version}_x64-setup.exe`,
    `Ratatoskr_${version}_x64-setup.exe.sig`,
    `Ratatoskr_${version}_x64_en-US.msi`,
    'Ratatoskr-portable.zip', 'latest.json', 'SHA256SUMS.txt',
  ]);
});
const androidRelease = releases.find(release => hasAssets(release, [
  'Ratatoskr-android.apk', 'Ratatoskr-android-armeabi-v7a.apk', 'SHA256SUMS.txt',
]));
if (!windowsRelease || !androidRelease) throw new Error('Could not find verified stable Windows and Android releases');
const linuxNamesFor = version => [`Ratatoskr_${version}_amd64.deb`, `Ratatoskr-${version}-1.x86_64.rpm`,
  `Ratatoskr_${version}_amd64.AppImage`, `Ratatoskr-${version}-linux-x86_64.tar.gz`];
// Linux is optional: releases before 1.3.6 have none, and the page then shows no Linux files.
const linuxRelease = releases.find(release => hasAssets(release, [...linuxNamesFor(versionOf(release)), 'SHA256SUMS.txt']));

async function verifiedAsset(release, name) {
  const value = assets(release).get(name);
  const expected = `https://github.com/${config.repository}/releases/download/${release.tag_name}/${name}`;
  if (!value || value.state !== 'uploaded' || value.size < 1 || value.browser_download_url !== expected ||
      !/^sha256:[a-f0-9]{64}$/.test(value.digest || '')) {
    throw new Error(`Missing or invalid published asset: ${release.tag_name}/${name}`);
  }

  const checksumAsset = assets(release).get('SHA256SUMS.txt');
  if (!checksumAsset || checksumAsset.state !== 'uploaded' || checksumAsset.size < 1) {
    throw new Error(`Missing checksum manifest: ${release.tag_name}`);
  }
  if (name === 'SHA256SUMS.txt') return value;
  let sums = verifiedAsset.checksums.get(release.tag_name);
  if (!sums) {
    const checksumResponse = await fetch(checksumAsset.browser_download_url);
    if (!checksumResponse.ok) throw new Error(`Checksum download failed: ${release.tag_name}`);
    sums = new Map((await checksumResponse.text()).trim().split(/\r?\n/).map(line => {
      const [digest, filename] = line.split(/\s+/, 2);
      return [filename, digest];
    }));
    verifiedAsset.checksums.set(release.tag_name, sums);
  }
  if (value.digest !== `sha256:${sums.get(name)}`) throw new Error(`Published checksum mismatch: ${name}`);
  return value;
}
verifiedAsset.checksums = new Map();

const windowsVersion = versionOf(windowsRelease);
const androidVersion = versionOf(androidRelease);
const windowsNames = [
  [`Ratatoskr_${windowsVersion}_x64-setup.exe`, 'windows', 'Download for Windows', 'دریافت نصب‌کنندهٔ ویندوز'],
  ['Ratatoskr-portable.zip', 'windows', 'Portable version', 'نسخهٔ قابل‌حمل'],
  [`Ratatoskr_${windowsVersion}_x64_en-US.msi`, 'windows', 'MSI installer', 'نصب‌کنندهٔ MSI'],
];
const androidNames = [
  ['Ratatoskr-android.apk', 'android', 'Download Android APK', 'دریافت نسخهٔ اندروید'],
];
const linuxVersion = linuxRelease ? versionOf(linuxRelease) : null;
const linuxNames = linuxRelease ? linuxNamesFor(linuxVersion).map((name, index) => [name, 'linux',
  ...[['Debian / Ubuntu (.deb)', 'دبیان و اوبونتو (⁦.deb⁩)'], ['Fedora / openSUSE (.rpm)', 'فدورا و اوپن‌سوزه (⁦.rpm⁩)'],
    ['AppImage, any distribution', 'AppImage، هر توزیع'], ['Archive (.tar.gz)', 'فایل فشرده (⁦.tar.gz⁩)']][index]]) : [];
const downloads = [];
for (const [release, mappings] of [[windowsRelease, windowsNames], [linuxRelease, linuxNames], [androidRelease, androidNames]]) {
  if (!release) continue;
  const version = versionOf(release);
  for (const [name, platform, label, labelFa] of mappings) {
    const asset = await verifiedAsset(release, name);
    downloads.push({ platform, version, label, labelFa, url: asset.browser_download_url,
      bytes: asset.size, sha256: asset.digest.slice(7) });
  }
}

const windowsVersionText = windowsVersion;
const androidVersionText = androidVersion;
const windowsChecksums = await verifiedAsset(windowsRelease, 'SHA256SUMS.txt');
const androidUrl = (await verifiedAsset(androidRelease, 'Ratatoskr-android.apk')).browser_download_url;
const android32Url = (await verifiedAsset(androidRelease, 'Ratatoskr-android-armeabi-v7a.apk')).browser_download_url;
config.stage = 'published-release';
config.release = {
  version: windowsVersionText,
  androidVersion: androidVersionText,
  publishedAt: windowsRelease.published_at,
  notesUrl: windowsRelease.html_url,
  checksumsUrl: windowsChecksums.browser_download_url,
  androidChecksumsUrl: (await verifiedAsset(androidRelease, 'SHA256SUMS.txt')).browser_download_url,
  linuxVersion,
  linuxChecksumsUrl: linuxRelease ? (await verifiedAsset(linuxRelease, 'SHA256SUMS.txt')).browser_download_url : null,
  downloads,
};
config.preview = {
  ...config.preview,
  version: windowsVersionText,
  androidVersion: androidVersionText,
  publishedAt: windowsRelease.published_at,
  notesUrl: windowsRelease.html_url,
  checksumsUrl: windowsChecksums.browser_download_url,
  androidChecksumsUrl: config.release.androidChecksumsUrl,
  windowsUrl: downloads.find(item => item.platform === 'windows')?.url,
  androidUrl,
  android32Url,
  stable: true,
};
await fs.writeFile(configPath, JSON.stringify(config, null, 2) + '\n');
console.log(`Connected Windows ${windowsVersionText}${linuxVersion ? `, Linux ${linuxVersion}` : ''} and Android ${androidVersionText} with verified download links.`);
