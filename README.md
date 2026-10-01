# Ratatoskr landing page

سایت معرفی دوزبانهٔ Ratatoskr با تصاویر رابط واقعی پروژه، حالت روشن ملایم و حالت تاریک.

- فارسی: https://ratatoskr.hivenest.ir/fa/
- English: https://ratatoskr.hivenest.ir/
- انتشار فعلی: [1.0.2](https://github.com/sajjadka21/ratatoskr/releases/tag/v1.0.2)
- میزبانی: Cloudflare Workers Static Assets، Worker موجود ratatoskr-site.

## محتوا و تصاویر

Wordmark با O درختی، آیکون برنامه، چهار آیکون/پترن و رنگ‌های تم از پروژهٔ دانلود منیجر گرفته شده‌اند. پنج screenshot از رابط واقعی React در مرورگر با فهرست خالی‌اند؛ تصویر دستگاه فیزیکی یا دانلود ساختگی نیستند. منشأ و checksum تصاویر در APP_REFERENCE.json ثبت شده است.

فونت‌های Manrope و Vazirmatn خودمیزبان هستند. آیکون‌های عمومی از مجموعهٔ Ant Design با مجوز MIT انتخاب شده‌اند؛ مجوز فونت‌ها و آیکون‌ها همراه assets است.

محتوا روی قابلیت‌های موجود تمرکز دارد: فارسی/تقویم شمسی، ترافیک داخلی و بین‌المللی، موتور مشترک GUI/CLI و ذخیره‌سازی محلی. تفاوت امکانات و مجوز اندروید و دسکتاپ در FAQ روشن شده است.

## فایل‌های اصلی

| مسیر | کاربرد |
|---|---|
| src/index.html | ساختار و متن انگلیسی |
| scripts/fa.mjs | ترجمهٔ فارسی |
| src/styles.css و src/header.css | چیدمان، هدر، دو حالت رنگ و نمایش موبایل |
| src/app.js | منو، تغییر حالت رنگ و پیش‌نمایش تم‌ها |
| site.config.json | دامنه و لینک‌های واقعی انتشار |
| scripts/release-content.mjs | درج لینک‌های دانلود در HTML هر دو زبان؛ دانلود به JavaScript وابسته نیست |
| scripts/sync-release.mjs | همگام‌سازی انتشار stable با نام فایل‌های مشخص و بررسی checksumها |
| scripts/build.mjs | ساخت صفحات فارسی/انگلیسی، SEO، sitemap و صفحهٔ 404 |
| dist/ | خروجی آمادهٔ انتشار |
| DEPLOYMENT.md | اطلاعات انتشار Cloudflare |
| docs/RELEASE_AUDIT.md | گزارش انتشار برنامه و حدود بررسی |
| docs/release-workflow.yml | روند انتشار واقعی و اعمال‌شده در مخزن برنامه |

## اجرای محلی

Node.js 22 یا جدیدتر:

```sh
npm ci
npm run build
npm run preview
```

پیش‌نمایش روی http://127.0.0.1:4173 اجرا می‌شود.

## انتشار نسخهٔ بعدی

پس از انتشار موفق فایل‌ها در GitHub:

```sh
npm run sync-release
npm run build
npm run deploy
```

همگام‌سازی شماره و تاریخ انتشار، چهار لینک اصلی، هش و حجم فایل‌ها را از همان انتشار عمومی می‌گیرد. اگر فایل یا checksum لازم موجود نباشد، بدون تغییر تنظیمات متوقف می‌شود. برای نام‌گذاری متفاوت خروجی‌ها، mappings را آگاهانه اصلاح کنید. پیش از انتشار، لینک‌ها و متن را در پیش‌نمایش بررسی کنید.

برای deploy با Wrangler به حساب Cloudflare صاحب همین Worker وارد شوید. هیچ توکن، کلید امضا یا رمز عبوری در این بسته قرار ندارد. HTTPS و زیردامنه فعال‌اند؛ تغییر دیگری در IRNIC لازم نیست.
