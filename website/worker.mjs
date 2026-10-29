const REPOSITORY = 'sajjadka21/ratatoskr';
// The cached shape changed when Linux was added; a new key avoids reading the old one.
const RELEASES_CACHE_KEY = new Request('https://ratatoskr-site.internal/stable-releases-v2');
const linuxNames = version => [`Ratatoskr_${version}_amd64.deb`, `Ratatoskr-${version}-1.x86_64.rpm`,
  `Ratatoskr_${version}_amd64.AppImage`, `Ratatoskr-${version}-linux-x86_64.tar.gz`];

async function getStableReleases() {
  const cache = caches.default;
  const cached = await cache.match(RELEASES_CACHE_KEY);
  if (cached) return cached.json();

  const response = await fetch(`https://api.github.com/repos/${REPOSITORY}/releases?per_page=30`, {
    headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'Ratatoskr-site' },
  });
  if (!response.ok) throw new Error(`GitHub release lookup failed: ${response.status}`);
  const releases = (await response.json()).filter(release =>
    !release.draft && !release.prerelease && /^v\d+\.\d+\.\d+$/.test(release.tag_name));
  const windows = releases.find(release => {
    const version = release.tag_name.slice(1);
    const names = new Set(release.assets.map(asset => asset.name));
    return [`Ratatoskr_${version}_x64-setup.exe`, `Ratatoskr_${version}_x64_en-US.msi`,
      'Ratatoskr-portable.zip', 'latest.json', 'SHA256SUMS.txt'].every(name => names.has(name));
  });
  const android = releases.find(release => {
    const names = new Set(release.assets.map(asset => asset.name));
    return ['Ratatoskr-android.apk', 'Ratatoskr-android-armeabi-v7a.apk', 'SHA256SUMS.txt']
      .every(name => names.has(name));
  });
  if (!windows || !android) throw new Error('Complete stable Windows and Android releases are required');
  // Optional: without it the page keeps the Linux links it was built with.
  const linux = releases.find(release => {
    const names = new Set(release.assets.map(asset => asset.name));
    return [...linuxNames(release.tag_name.slice(1)), 'SHA256SUMS.txt'].every(name => names.has(name));
  }) || null;

  const result = { windows, android, linux };
  await cache.put(RELEASES_CACHE_KEY, new Response(JSON.stringify(result), {
    headers: { 'Content-Type': 'application/json', 'Cache-Control': 'public, max-age=300' },
  }));
  return result;
}

export function updateReleaseLinks(html, releases, language) {
  const fa = language === 'fa';
  const windows = releases.windows;
  const android = releases.android;
  const windowsVersion = windows.tag_name.slice(1);
  const androidVersion = android.tag_name.slice(1);
  const linux = releases.linux || null;
  const linuxVersion = linux ? linux.tag_name.slice(1) : null;
  const windowsAssets = new Map(windows.assets.map(asset => [asset.name, asset.browser_download_url]));
  const androidAssets = new Map(android.assets.map(asset => [asset.name, asset.browser_download_url]));
  const linuxAssets = new Map((linux?.assets || []).map(asset => [asset.name, asset.browser_download_url]));
  const publishedDate = new Intl.DateTimeFormat(fa ? 'fa-IR' : 'en', {
    dateStyle: 'medium', timeZone: fa ? 'Asia/Tehran' : 'UTC',
  }).format(new Date(windows.published_at));

  html = html.replace(/href="https:\/\/github\.com\/sajjadka21\/ratatoskr\/releases\/download\/[^/]+\/([^"]+)"/g,
    (match, name) => {
      const currentWindowsName = name
        .replace(/^Ratatoskr_\d+\.\d+\.\d+_x64-setup\.exe$/, `Ratatoskr_${windowsVersion}_x64-setup.exe`)
        .replace(/^Ratatoskr_\d+\.\d+\.\d+_x64_en-US\.msi$/, `Ratatoskr_${windowsVersion}_x64_en-US.msi`);
      const currentLinuxName = linuxVersion ? name
        .replace(/^Ratatoskr_\d+\.\d+\.\d+_amd64\.(deb|AppImage)$/, `Ratatoskr_${linuxVersion}_amd64.$1`)
        .replace(/^Ratatoskr-\d+\.\d+\.\d+-1\.x86_64\.rpm$/, `Ratatoskr-${linuxVersion}-1.x86_64.rpm`)
        .replace(/^Ratatoskr-\d+\.\d+\.\d+-linux-x86_64\.tar\.gz$/, `Ratatoskr-${linuxVersion}-linux-x86_64.tar.gz`) : name;
      const url = windowsAssets.get(currentWindowsName) || linuxAssets.get(currentLinuxName) || androidAssets.get(name);
      return url ? `href="${url}"` : match;
    });
  html = html.replace(/(<span id="hero-status">)[\s\S]*?(<\/span>)/,
    `$1${fa ? `نسخهٔ ${windowsVersion} منتشر شد · اندروید ${androidVersion}` : `Version ${windowsVersion} is available · Android ${androidVersion}`}$2`);
  html = html.replace(/(<p class="release-state" id="release-state"><span><\/span>)[\s\S]*?(<\/p>)/,
    `$1${fa ? `نسخهٔ منتشرشده · ${windowsVersion}` : `Published release · ${windowsVersion}`}$2`);
  html = html.replace(/(<h2 id="download-title">)[\s\S]*?(<\/h2>)/,
    `$1${fa ? `دریافت راتاتوسک ${windowsVersion}` : `Get Ratatoskr ${windowsVersion}`}$2`);
  html = html.replace(/(<p id="release-date">)[\s\S]*?(<\/p>)/,
    `$1${fa ? 'تاریخ انتشار: ' : 'Released: '}${publishedDate}$2`);
  html = html.replace(/(<a id="release-notes" href=")[^"]+/, `$1${windows.html_url}`);
  // Each button ends with the version of the file it links to.
  for (const [platform, version] of [['windows', windowsVersion], ['linux', linuxVersion], ['android', androidVersion]]) {
    if (!version) continue;
    html = html.replace(new RegExp(`<article data-platform="${platform}">[\\s\\S]*?</article>`), article =>
      article.replace(/ · \d+\.\d+\.\d+(?=<\/a>)/g, ` · ${version}`));
  }
  const checksum = (id, url, label) => {
    html = html.replace(new RegExp(`<a id="${id}" href="[^"]*"([^>]*)>[^<]*</a>`), `<a id="${id}" href="${url}"$1>${label}</a>`);
  };
  const sharedDesktop = linux && linux.tag_name === windows.tag_name;
  checksum('checksums-desktop', windowsAssets.get('SHA256SUMS.txt'), sharedDesktop
    ? (fa ? 'SHA-256 ویندوز و لینوکس' : 'Windows & Linux SHA-256') : (fa ? 'SHA-256 ویندوز' : 'Windows SHA-256'));
  if (linux) checksum('checksums-linux', linuxAssets.get('SHA256SUMS.txt'), `${fa ? 'SHA-256 لینوکس' : 'Linux SHA-256'} · ${linuxVersion}`);
  checksum('checksums-android', androidAssets.get('SHA256SUMS.txt'), fa ? `SHA-256 اندروید · ${androidVersion}` : `Android SHA-256 · ${androidVersion}`);
  return html;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (!['GET', 'HEAD'].includes(request.method)) {
      return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
    }
    if (url.pathname === '/fa') {
      url.pathname = '/fa/';
      return Response.redirect(url.href, 308);
    }

    const asset = await env.ASSETS.fetch(request);
    if (!asset.headers.get('content-type')?.includes('text/html')) return asset;
    let body = await asset.text();
    try {
      const releases = await getStableReleases();
      body = updateReleaseLinks(body, releases, url.pathname.startsWith('/fa') ? 'fa' : 'en');
    } catch (error) {
      console.error('Live release refresh failed; serving bundled release links.', error);
    }

    return new Response(request.method === 'HEAD' ? null : body, {
      status: asset.status,
      headers: {
        'Content-Type': 'text/html; charset=utf-8',
        'Cache-Control': 'no-cache',
        'X-Content-Type-Options': 'nosniff',
        'Referrer-Policy': 'strict-origin-when-cross-origin',
        'X-Frame-Options': 'DENY',
        'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
        'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'; object-src 'none'",
      },
    });
  },
};
