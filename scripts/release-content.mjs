// Render only links recorded from the verified, published GitHub release.
export function releaseContent(html, config, lang, escape) {
  const release = config.release;
  if (!release?.downloads?.length) return html;
  const fa = lang === 'fa';
  const version = escape(release.version);
  const ready = fa ? `نسخهٔ ${version} منتشر شد` : `Version ${version} is available`;
  const byId = (id, content) => {
    const pattern = new RegExp(`(<(p|span)[^>]*id="${id}"[^>]*>)[\\s\\S]*?(</\\2>)`);
    if (!pattern.test(html)) throw new Error(`Missing release field: ${id}`);
    html = html.replace(pattern, (_, start, tag, end) => start + content + end);
  };
  byId('hero-status', ready);
  byId('release-state', `<span></span>${fa ? 'نسخهٔ منتشرشده' : 'Published release'} · ${version}`);
  byId('release-summary', fa ? 'فایل مناسب دستگاهت را انتخاب کن. نسخهٔ ویندوز نصبی و پرتابل دارد؛ اندروید همراه دانلود رسانه است.' : 'Choose the file for your device. Windows offers an installer and a portable edition; Android is the media-download companion.');
  const descriptions = {
    windows: fa ? 'ویندوز ۱۰ و ۱۱، نسخهٔ ۶۴ بیتی. برای شروع، نصب‌کننده پیشنهاد می‌شود. نسخهٔ پرتابل بدون نصب اجرا می‌شود.' : 'Windows 10/11, 64-bit. The installer is recommended. The portable edition runs from a writable folder.',
    android: fa ? 'اندروید ۱۰ به بالا. لینک رسانه را به اشتراک بگذار، کیفیت را انتخاب کن و پیشرفت را در اعلان ببین.' : 'Android 10 and later. Share a media link, choose a quality, and follow progress in the notification.',
  };
  for (const platform of ['windows', 'android']) {
    const files = release.downloads.filter(item => item.platform === platform);
    html = html.replace(new RegExp(`(<article data-platform="${platform}">[\\s\\S]*?</div>)<p>[\\s\\S]*?</p>[\\s\\S]*?</article>`), (_, heading) => {
      const buttons = files.map((item, index) => `<a class="button button-gold download-file${index ? ' secondary' : ''}" href="${escape(item.url)}" rel="noopener noreferrer">${escape(fa ? item.labelFa : item.label)}</a>`).join('');
      return `${heading}<p>${descriptions[platform]}</p>${buttons}</article>`;
    });
  }
  const date = new Intl.DateTimeFormat(fa ? 'fa-IR' : 'en', { dateStyle: 'medium', timeZone: fa ? 'Asia/Tehran' : 'UTC' }).format(new Date(release.publishedAt));
  byId('release-date', (fa ? 'تاریخ انتشار: ' : 'Released: ') + escape(date));
  html = html.replace(/(id="release-notes" href=")[^"]*/, '$1' + escape(release.notesUrl));
  const answer = fa ? 'بله. فایل‌های ویندوز و اندروید از انتشار رسمی GitHub در دسترس‌اند و SHA-256 آن‌ها بررسی شده است. نصب و دانلود روی دستگاه واقعی هنوز مستقلاً تأیید نشده است.' : 'Yes. Windows and Android files are available from the official GitHub release, and their SHA-256 checksums have been verified. Physical-device installation and download tests have not yet been independently verified.';
  byId('release-answer', answer);
  const checksum = escape(release.checksumsUrl);
  html = html.replace(/<\/div>\r?\n  <\/section>\r?\n  <section id="faq"/, `<a href="${checksum}" rel="noopener noreferrer">${fa ? 'بررسی SHA-256 فایل‌ها' : 'File checksums (SHA-256)'}</a></div>\n  </section>\n  <section id="faq"`);
  return html;
}
