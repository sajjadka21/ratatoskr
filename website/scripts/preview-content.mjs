export function previewContent(html,config,lang,escape){
 const marker='<!-- PREVIEW_RELEASE -->';
 if(!html.includes(marker))throw Error('Missing preview slot');
 if(!config.preview)return html.replace(marker,'');
 const preview=config.preview,fa=lang==='fa';
 for(const key of ['windowsUrl','androidUrl','notesUrl','checksumsUrl']){
  const url=new URL(preview[key]);
  if(url.protocol!=='https:'||url.hostname!=='github.com'||url.username||url.password)throw Error('Invalid preview link');
 }
 if(!/^\d+\.\d+\.\d+$/.test(preview.version))throw Error('Invalid preview version');
 const title=preview.stable?(fa?'تازه در نسخهٔ '+preview.version:'New in version '+preview.version):(fa?'تازه در پیش‌انتشار '+preview.version:'New in preview '+preview.version);
 const features=fa?[
  ['کپی عنوان‌ها، دریافت لینک‌ها','بخشی از صفحه را انتخاب و کپی کن؛ در ویندوز، آدرس پشت عنوان لینک‌ها هم وارد پنجرهٔ افزودن دانلود می‌شود.'],
  ['صفی که باقی می‌ماند','ویندوز منوی شروع و توقف صف و پنجرهٔ تنظیمات و زمان‌بندی دارد. در اندروید صف‌های نام‌دار، تاریخچه و ترتیب فایل‌ها پس از باز شدن دوباره باقی می‌مانند. ادامهٔ دانلود به پشتیبانی سرور بستگی دارد.'],
  ['از اینستاگرام به دانلود','لینک عمومی را به Ratatoskr اشتراک بگذار؛ کیفیت، صدا، عکس و موارد آلبوم را انتخاب کن. حالت سریع لینک را در صف می‌گذارد و برمی‌گردد.'],
  ['فایل تکراری، با اطلاع تو','تاریخچه و فایل‌های قابل‌دسترسی مقصد جدا بررسی می‌شوند. فایل مشابه صرفاً اطلاع داده می‌شود؛ نسخهٔ دوباره با (1)، (2) و ادامهٔ شمارش ذخیره می‌شود.'],
  ['اینترنت و ظاهر، به انتخاب تو','Wi-Fi، اینترنت بدون محدودیت مصرف و رومینگ را جدا تنظیم کن. چهار تم برند با حالت روشن ملایم، تیره یا هماهنگ با سیستم.']
 ]:[
  ['Copy labels. Import their links.','Select and copy a webpage region. On Windows, Add download also finds the URLs behind the copied link labels.'],
  ['A queue that stays','Windows has a queue menu and a settings and scheduling dialog. Android named queues, history and ordering survive relaunch. Resuming depends on server support.'],
  ['From Instagram to download','Share a public link to Ratatoskr; choose quality, audio, photos and album items. Quick mode queues the link and returns you to the source app.'],
  ['Duplicates, on your terms','History and accessible destination files are checked separately. Similar files are informational hints; another copy receives (1), (2), and subsequent suffixes.'],
  ['Your network. Your appearance.','Choose Wi-Fi, unmetered and roaming preferences separately. Four brands with softer light, dark or system appearance.']
 ];
 if(preview.version==='1.1.0')features.shift();
 const current = Number(preview.version.split('.')[0]) > 1 || Number(preview.version.split('.')[1]) >= 2;
 if(current)features.push(...(fa?[
  ['دانلود فایل یا بررسی رسانه؟','حالت خودکار لینک را بررسی می‌کند تا فایل یا رسانهٔ قابل دانلود را پیدا کند. حالت مستقیم برای آدرس خود فایل است؛ لینک صفحه به‌جای فایل ذخیره نمی‌شود.'],
  ['وقتی دانلود متوقف می‌شود','در اندروید پیشرفت دانلود را می‌بینی؛ ردیف‌های مناسب با لمس دوباره امتحان یا ادامه داده می‌شوند. خطاها در یک اعلان با شمارنده جمع می‌شوند و ویبرهٔ اختیاری دارند.']
 ]:[
  ['A file link or a media page?','Automatic checks the link for a file or available media. Direct is for the file URL itself; a webpage is not saved as your download.'],
  ['When a transfer stops','Android displays transfer progress. Tap eligible failed or paused rows to retry or resume. Failures share one counted notification, with optional vibration.']
 ]));
 const singleAction = Number(preview.version.split('.')[0]) > 1 || Number(preview.version.split('.')[1]) >= 3;
 if(singleAction)features[5]=fa?
  ['یک لینک، یک دکمهٔ دانلود','لینک فایل یا صفحهٔ رسانه را وارد کن و «دانلود» را بزن. روش دانلود خودکار انتخاب می‌شود؛ برای رسانه می‌توانی کیفیت را انتخاب کنی. رابط اندروید با دکمهٔ طلایی افزودن، صف‌های نام‌دار، فیلتر و مرتب‌سازی همراه است.']:
  ['One link. One Download button.','Paste a file or media page link and choose Download. Routing is automatic; media keeps quality selection. Android includes a gold Add button, named queues, filtering and sorting.'];
 if(singleAction)features.push(...(fa?[
  ['زمان و تقویم هماهنگ','فیلتر تاریخ اندروید از تقویم انتخاب‌شده در تنظیمات استفاده می‌کند. برای زمان‌بندی، تاریخ و ساعت تعیین کن و فایل را همین حالا بگیر، ذخیره کن یا به صف بفرست.'],
  ['خروج کامل، به انتخاب تو','در ویندوز و اندروید، گزینهٔ خروج کامل دانلودها را متوقف می‌کند و کارهای پس‌زمینه را می‌بندد. فایل‌های نیمه‌تمام برای ادامهٔ بعدی حفظ می‌شوند؛ ادامه به پشتیبانی منبع بستگی دارد.']
 ]:[
  ['Your time. Your calendar.','Android date filtering follows the calendar selected in Settings. Scheduling includes date and time; download now, save for later or send a file to a queue.'],
  ['Exit completely, when you choose','Windows and Android can pause downloads and close background work on complete exit. Partial files stay available for later; resume depends on the source.']
 ]));
 const atLeast=(version,minimum)=>{const a=version.split('.').map(Number),b=minimum.split('.').map(Number);for(let i=0;i<3;i++)if(a[i]!==b[i])return a[i]>b[i];return true;};
 const linuxFiles=(config.release?.downloads||[]).some(item=>item.platform==='linux');
 // The newest changes lead the list.
 if(atLeast(preview.version,'1.3.7'))features.unshift(fa?
  ['وقتی دانلود ناموفق است','دلیل خطا به زبان خودت نمایش داده می‌شود. اگر خود لینک منقضی شده یا رد شده باشد، به‌جای تلاش دوباره‌ای که باز شکست می‌خورد، تازه‌سازی لینک پیشنهاد می‌شود. ردیف دانلودهای تمام‌شده دکمهٔ «نمایش در پوشه» دارد.']:
  ['Clearer when a download fails','A failed row says why, in your language. When the link itself has expired or is refused, Ratatoskr offers a fresh link instead of a retry that would fail again. Finished rows offer Show in folder.']);
 if(linuxFiles&&atLeast(preview.version,'1.3.6'))features.unshift(fa?
  ['حالا روی لینوکس','دبیان، اوبونتو، فدورا، اوپن‌سوزه، آرچ و بقیه: بستهٔ ⁦.deb⁩ یا ⁦.rpm⁩، فایل AppImage یا فایل فشردهٔ ساده. افزونهٔ مرورگر، اجرا هنگام ورود و کارهای پایان دانلود آن‌جا هم کار می‌کنند.']:
  ['Now on Linux','Debian, Ubuntu, Fedora, openSUSE, Arch and more: a .deb or .rpm package, an AppImage or a plain archive. The browser extension, start at sign-in and after-download actions work there too.']);
 features.splice(6);
 const screenshotVersion=preview.androidScreenshotVersion||preview.version;
 if(!/^\d+\.\d+\.\d+$/.test(screenshotVersion))throw Error('Invalid screenshot version');
 const screenshotSuffix=preview.androidScreenshotVersion?'-'+screenshotVersion:'';
 const gallery=current&&preview.androidScreenshots?`<div class="preview-android" aria-label="${fa?'رابط اندروید':'Android interface'}"><figure><img src="/assets/android-home-1.3.0.webp" width="320" height="640" loading="lazy" alt="${fa?'صفحهٔ اصلی Ratatoskr در اندروید':'Ratatoskr Android home screen'}"><figcaption>${fa?'صفحهٔ اصلی':'Home screen'}</figcaption></figure><figure><img src="/assets/android-settings-1.3.0.webp" width="320" height="640" loading="lazy" alt="${fa?'تنظیمات فارسی با تم Forest Rune':'Persian settings in Forest Rune'}"><figcaption>${fa?'تنظیمات فارسی':'Persian settings'}</figcaption></figure></div><p class="preview-caution">${fa?'تصاویر از اجرای واقعی نسخهٔ '+escape(screenshotVersion)+' در شبیه‌ساز Android 15 گرفته شده‌اند.':'Captured from the actual '+escape(screenshotVersion)+' app running in an Android 15 emulator.'}</p>`:'';
 const cards=features.map(([heading,text])=>`<article><h3>${escape(heading)}</h3><p>${escape(text)}</p></article>`).join('');
 const compatibility=current&&preview.android32Url&&!preview.androidUpgradeBlocked?(()=>{
  const url=new URL(preview.android32Url);
  if(url.protocol!=='https:'||url.hostname!=='github.com'||url.username||url.password)throw Error('Invalid Android variant');
  return `<p class="preview-caution">${linuxFiles?(fa?'ویندوز ۱۰/۱۱، ۶۴ بیتی · لینوکس ۶۴ بیتی با WebKitGTK 4.1 · اندروید ۸ به بالا. دکمهٔ اندروید برای گوشی‌های ARM64 است.':'Windows 10/11, x64 · Linux x64 with WebKitGTK 4.1 · Android 8+. The Android button downloads the ARM64 package.'):(fa?'ویندوز ۱۰/۱۱، ۶۴ بیتی · اندروید ۸ به بالا. دکمهٔ اندروید برای گوشی‌های ARM64 است.':'Windows 10/11, x64 · Android 8+. The Android button downloads the ARM64 package.')} <a class="text-link" href="${escape(url.href)}">${fa?'نسخهٔ گوشی‌های ARM قدیمی‌تر (۳۲ بیتی)':'Download for older 32-bit ARM phones'}</a></p>`;
 })():'';
 const androidAction=preview.androidUpgradeBlocked?`<p class="preview-caution" role="status">${fa?'دریافت پیش‌انتشار اندروید فعلاً متوقف است: امضای این نسخه با نسخهٔ قبلی فرق دارد. برای حفظ صف و تاریخچه، برنامهٔ قبلی را حذف نکن. نسخهٔ سازگار در دست بررسی است.':'Android preview downloads are on hold: this version uses a different signing certificate from the previous release. Keep your existing app installed to preserve its queue and history. A compatible upgrade is under review.'}</p>`:`<a class="button button-gold" href="${escape(preview.androidUrl)}">${preview.stable?(fa?'دریافت اندروید':'Download Android'):(fa?'پیش‌انتشار اندروید':'Android preview')}</a>`;
 const upgradeNotice=preview.androidUpgradeRestored?`<p class="preview-caution">${fa?'امضای این بسته با اندروید ۱.۰.۲ و ۱.۱.۱ سازگار است. اگر پیش‌انتشار اندروید ۱.۲.۲ را نصب کرده‌ای، مستقیم به این نسخه آپدیت نمی‌شود؛ برنامه را برای دور زدن خطا حذف نکن، چون صف و تاریخچه ممکن است پاک شود.':'This package uses the same Android signer as 1.0.2 and 1.1.1. If you installed Android preview 1.2.2, it cannot update directly to this version. Keep that app installed: uninstalling it may delete its queue and history.'}</p>`:'';
 const caution=preview.stable?(fa?'این نسخه منتشر شده است؛ تست کامل گوشی واقعی و همهٔ لینک‌های زندهٔ اینستاگرام هنوز تکمیل نشده است. محتوای خصوصی اینستاگرام تضمین نمی‌شود.':'This version is published; full physical-device acceptance and all live Instagram cases remain unverified. Private Instagram content is not promised.'):fa?'نسخهٔ آزمایشی است؛ نصب و دانلود واقعی روی دستگاه‌ها هنوز باید تأیید شود. محتوای خصوصی اینستاگرام و نصب افزونه از فروشگاه تضمین نمی‌شود.':'A preview for testing: physical installation and live transfers still need acceptance. Private Instagram content and marketplace extension installation are not promised.';
 return html.replace(marker,`<section id="preview" class="wrap section preview-section" aria-labelledby="preview-title"><p class="section-kicker">${preview.stable?(fa?'قابلیت‌های نسخهٔ تازه':'The latest release'):(fa?'برای امتحان قابلیت‌های تازه':'Try the next release')}</p><h2 id="preview-title">${escape(title)}</h2><div class="preview-features">${cards}</div>${gallery}<p class="preview-caution">${caution}</p><div class="hero-actions"><a class="button button-gold" href="${escape(preview.windowsUrl)}">${preview.stable?(fa?'دریافت ویندوز':'Download Windows'):(fa?'پیش‌انتشار ویندوز':'Windows preview')}</a>${linuxFiles&&preview.stable?`<a class="button button-gold" href="#download">${fa?'دریافت لینوکس':'Download Linux'}</a>`:''}${androidAction}<a class="text-link" href="${escape(preview.notesUrl)}">${fa?'همهٔ فایل‌ها و یادداشت نسخه':'All files and release notes'}</a></div>${compatibility}${upgradeNotice}<p class="release-links"><a href="${escape(preview.checksumsUrl)}">${preview.stable?(fa?'بررسی SHA-256 نسخه':'Release SHA-256 checksums'):(fa?'بررسی SHA-256 پیش‌انتشار':'Preview SHA-256 checksums')}</a></p></section>`);
}
