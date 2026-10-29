// Render only links recorded from the verified, published GitHub release.
export function releaseContent(html, config, lang, escape) {
  const release = config.release;
  if (!release?.downloads?.length) return html;
  const fa = lang === 'fa';
  const version = escape(release.version);
  const androidVersion = escape(release.androidVersion || release.version);
  const ready = fa ? `نسخهٔ ${version} منتشر شد` : `Version ${version} is available`;
  const byId = (id, content) => {
    const pattern = new RegExp(`(<(p|span)[^>]*id="${id}"[^>]*>)[\\s\\S]*?(</\\2>)`);
    if (!pattern.test(html)) throw new Error(`Missing release field: ${id}`);
    html = html.replace(pattern, (_, start, tag, end) => start + content + end);
  };
  byId('hero-status', config.preview && !config.preview.stable ? (fa ? 'پیش‌انتشار ' : 'Preview ') + escape(config.preview.version) : ready);
  byId('release-state', `<span></span>${fa ? 'نسخهٔ منتشرشده' : 'Published release'} · ${version}`);
  html = html.replace(/(<h2 id="download-title">)[\s\S]*?(<\/h2>)/, `$1${fa ? `دریافت راتاتوسک ${version}` : `Get Ratatoskr ${version}`}$2`);
  byId('release-summary', fa ? 'فایل مناسب دستگاهت را انتخاب کن. ویندوز نسخهٔ نصبی و پرتابل دارد، لینوکس برای همهٔ توزیع‌های اصلی بسته دارد و اندروید همراه دانلود رسانه است.' : 'Choose the file for your device. Windows offers an installer and a portable edition, Linux has packages for every major distribution, and Android is the media-download companion.');
  const descriptions = {
    windows: fa ? 'ویندوز ۱۰ و ۱۱، نسخهٔ ۶۴ بیتی. برای شروع، نصب‌کننده پیشنهاد می‌شود. نسخهٔ پرتابل بدون نصب اجرا می‌شود.' : 'Windows 10/11, 64-bit. The installer is recommended. The portable edition runs from a writable folder.',
    linux: fa ? 'هر توزیع ۶۴ بیتی با WebKitGTK 4.1: دبیان ۱۲ و بالاتر، اوبونتو ۲۲.۰۴ و بالاتر، فدورا ۳۷ و بالاتر، اوپن‌سوزه و آرچ.' : 'Any 64-bit distribution with WebKitGTK 4.1: Debian 12+, Ubuntu 22.04+, Fedora 37+, openSUSE and Arch.',
    android: fa ? 'اندروید ۸ به بالا، گوشی ARM64. لینک رسانه را به اشتراک بگذار، کیفیت را انتخاب کن و پیشرفت را در برنامه و اعلان ببین.' : 'Android 8 and later, ARM64 phones. Share a media link, choose a quality, and follow progress in the app and notification.',
  };
  for (const platform of ['windows', 'linux', 'android']) {
    const files = release.downloads.filter(item => item.platform === platform);
    // Without published Linux files the card is left out rather than shown empty.
    if (!files.length) { html = html.replace(new RegExp(`\\s*<article data-platform="${platform}">[\\s\\S]*?</article>`), ''); continue; }
    html = html.replace(new RegExp(`(<article data-platform="${platform}">[\\s\\S]*?</div>)<p>[\\s\\S]*?</p>[\\s\\S]*?</article>`), (_, heading) => {
      const buttons = files.map((item, index) => `<a class="button button-gold download-file${index ? ' secondary' : ''}" href="${escape(item.url)}" rel="noopener noreferrer">${escape(fa ? item.labelFa : item.label)} · ${escape(item.version || (platform === 'android' ? release.androidVersion || release.version : release.version))}</a>`).join('');
      return `${heading}<p>${descriptions[platform]}</p>${buttons}</article>`;
    });
  }
  const date = new Intl.DateTimeFormat(fa ? 'fa-IR' : 'en', { dateStyle: 'medium', timeZone: fa ? 'Asia/Tehran' : 'UTC' }).format(new Date(release.publishedAt));
  byId('release-date', (fa ? 'تاریخ انتشار: ' : 'Released: ') + escape(date));
  html = html.replace(/(id="release-notes" href=")[^"]*/, '$1' + escape(release.notesUrl));
  const answer = fa ? 'بله. فایل‌های ویندوز، لینوکس و اندروید از انتشار رسمی GitHub در دسترس‌اند و SHA-256 آن‌ها بررسی شده است. نصب و دانلود روی دستگاه واقعی هنوز مستقلاً تأیید نشده است.' : 'Yes. Windows, Linux and Android files are available from the official GitHub release, and their SHA-256 checksums have been verified. Physical-device installation and download tests have not yet been independently verified.';
  byId('release-answer', answer);
  const windowsChecksum = escape(release.checksumsUrl);
  const androidChecksum = escape(release.androidChecksumsUrl || release.checksumsUrl);
  const linuxChecksum = release.linuxChecksumsUrl ? escape(release.linuxChecksumsUrl) : null;
  // Windows and Linux normally share one release and one checksum list.
  const desktopLabel = linuxChecksum === windowsChecksum ? (fa ? 'SHA-256 ویندوز و لینوکس' : 'Windows & Linux SHA-256') : (fa ? 'SHA-256 ویندوز' : 'Windows SHA-256');
  const linuxLink = linuxChecksum && linuxChecksum !== windowsChecksum ? `<a id="checksums-linux" href="${linuxChecksum}" rel="noopener noreferrer">${fa ? 'SHA-256 لینوکس' : 'Linux SHA-256'} · ${escape(release.linuxVersion)}</a>` : '';
  html = html.replace('<a id="project-link"', `<a id="checksums-desktop" href="${windowsChecksum}" rel="noopener noreferrer">${desktopLabel}</a>${linuxLink}<a id="checksums-android" href="${androidChecksum}" rel="noopener noreferrer">${fa ? `SHA-256 اندروید · ${androidVersion}` : `Android SHA-256 · ${androidVersion}`}</a><a id="project-link"`);
  return html;
}
