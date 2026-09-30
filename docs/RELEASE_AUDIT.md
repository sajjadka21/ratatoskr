# بررسی انتشار Ratatoskr

بررسی‌شده در ۳۰ سپتامبر ۲۰۲۶ از API رسمی GitHub و کد محلی.

**ساخت ویندوز و اندروید موفق بوده، ولی هیچ ریلیز عمومی منتشر نشده است.**

| بخش | نتیجه | توضیح |
|---|---|---|
| Windows installer / portable | ساخت موفق | مرحلهٔ ساخت و امضای updater موفق بوده است؛ این نتیجه تأیید نصب عملی یا گواهی Authenticode ویندوز نیست. |
| Android APK | ساخت موفق | APK با کلید debug ساخته شده؛ اطلاعات keystore اختصاصی در این اجرای CI خالی بوده است. |
| انتشار عمومی v1.0.0 | ناموفق | مرحلهٔ Checksums روی پوشه‌های `portable` و `release` اجرا شده و با `Is a directory` متوقف شده است. مرحلهٔ Create the release اجرا نشده. |
| آخرین CI شاخهٔ main | موفق | Frontend، Rust، آزمون واحد و debug build اندروید موفق‌اند؛ آزمون نصب روی ویندوز یا گوشی نیستند. |

## مدارک

- [اجرای Release مورد بررسی](https://github.com/sajjadka21/ratatoskr/actions/runs/36757559197)
- [آخرین CI موفق](https://github.com/sajjadka21/ratatoskr/actions/runs/36761120636)
- [صفحهٔ Releases](https://github.com/sajjadka21/ratatoskr/releases)
- [فرایند انتشار](https://github.com/sajjadka21/ratatoskr/blob/main/.github/workflows/release.yml)
- [پیکربندی امضای Android](https://github.com/sajjadka21/ratatoskr/blob/main/android/app/build.gradle.kts)

## اصلاح لازم

۱. فایل‌های artifactهای ویندوز توی زیرپوشه‌ها هستند. پیش از checksum و `gh release create`، فایل‌های نهایی باید به یک پوشهٔ جدا منتقل یا کپی شوند؛ checksum فقط روی فایل‌ها اجرا شود. نسخهٔ اصلاح‌شدهٔ پیشنهادی فرایند در `ratatoskr-release-workflow-fixed.yml` آماده است. این فایل هنوز در GitHub اعمال یا اجرا نشده است.

۲. برای انتشار Android باید یک keystore ثابت با رمز، alias و رمز کلید در secrets مربوط به فرایند وجود داشته باشد. از تغییر مکرر کلید جلوگیری کنید، چون نسخه‌های بعدی باید با کلید سازگار امضا شوند. هیچ کلید خصوصی یا secret در این بررسی خوانده یا صادر نشده است.

۳. نصب نسخهٔ نصبی ویندوز، اجرای portable، ارتباط مرورگر، ادامهٔ دانلود پس از توقف، به‌روزرسانی امضاشده و نصب/دانلود پس‌زمینه روی یک دستگاه Android باید عملاً بررسی شوند.

## تفاوت محلی و GitHub

پوشهٔ `D:\project\download-manager` تغییرات ثبت‌نشدهٔ زیادی دارد و با شاخهٔ GitHub یکسان نیست. فایل‌های نصبی موجود در `target/release/bundle` محلی هنوز نام قدیمی **Download Manager** دارند. `productName` و عنوان پنجره در کد فعلی محلی **Ratatoskr** است؛ اصلاح نام در کد، نام باینری قدیمی را تغییر نمی‌دهد و بازسازی لازم است.

endpoint به‌روزرسانی در `tauri.conf.json` محلی خالی است، ولی نسخهٔ GitHub endpoint ریلیز را دارد. شناسهٔ Android محلی `ir.ratatosk.app` و در GitHub `app.ratatoskr.android` است. پیش از ساخت نهایی، checkout انتخاب‌شده و شناسهٔ اپ باید مشخص و ثابت باشند.

## تصاویر سایت

پنج تصویر از رابط واقعی React همین کد محلی در مرورگر گرفته شده‌اند: چهار تم در حالت فهرست خالی و صفحهٔ تنظیمات. دادهٔ دانلود، سرعت یا شمارندهٔ ساختگی اضافه نشده است. این تصاویر **اسکرین‌شات نسخهٔ نصب‌شدهٔ ویندوز یا گوشی اندرویدی نیستند**. این محدودیت روی خود سایت هم نوشته شده است.
