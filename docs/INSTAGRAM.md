# Downloading from Instagram with Ratatoskr

راهنمای فارسی پایین همین صفحه است · Persian guide below.

Ratatoskr downloads Instagram **posts, reels and IGTV videos** with the yt-dlp
engine bundled in the app (Windows) or inside the Android app. Nothing goes
through a server of ours.

## Windows

Pick whichever is easiest:

1. **Paste the link.** In Instagram copy the link (⋯ → *Copy link*, or the
   address bar), then in Ratatoskr press `Ctrl+N`. The link is recognised and
   the quality list appears. Choose one; it downloads into your folder.
2. **Browser extension.** Install it from Settings → Browser extension. On an
   Instagram post or reel, right-click → **Download this video with
   Ratatoskr**.
3. **Drop box.** Turn on the floating drop box in Settings and drag the link
   (or the selected text) onto it.
4. **Command line.** `tosk add "https://www.instagram.com/reel/…"`.

Recognised addresses: `/p/…`, `/reel/…`, `/reels/…`, `/tv/…`.

## Android

1. In the Instagram app open the post or reel, press **Share** (paper-plane
   icon) and pick **Ratatoskr** in the share sheet. If it is not in the first
   row, choose *More* (⋯).
2. A small window lists the qualities. Tap one — the window closes and you
   return to Instagram while the download continues in the background, with a
   progress notification and a stop button.
3. Files are saved in `Downloads/Ratatosk`. No storage permission is needed.

Shortcuts: *Copy link* in Instagram, then use the **Ratatoskr quick-settings
tile** ("Download copied link"), or the paste button inside the app. Add the
tile once from the quick-settings *Edit* screen.

## What works and what does not

| Works | Does not (yet) |
|---|---|
| Public posts, reels, IGTV | Stories and highlights |
| Videos and photos in a public post | Private accounts |
| Several qualities when Instagram offers them | Posts that ask you to log in |

Instagram changes often. If a link that used to work fails, update the engine:
Windows — it updates itself once a day when automatic updates are on; Android — press
*Update download engine* on the home screen. Then try again.

If Instagram is filtered where you are, turn on your VPN for the whole phone or
computer so the download goes through it as well.

The app stores no Instagram cookies or passwords. For the Telegram bot,
logged-in-only posts are possible with a cookie file of an account that you
control (`COOKIES_FILE`), see [telegram-bot/README.md](../telegram-bot/README.md).

Please only save content you own or have permission to keep.

---

## راهنمای فارسی

راتاتوسک **پست، ریل و IGTV** اینستاگرام را با موتور yt-dlp که داخل خود برنامه است دانلود می‌کند؛ چیزی از سرور ما رد نمی‌شود.

### ویندوز

۱. **چسباندن لینک:** در اینستاگرام لینک را کپی کنید (⋯ ← *Copy link*)، در راتاتوسک `Ctrl+N` را بزنید؛ لینک شناخته می‌شود و لیست کیفیت‌ها می‌آید.
۲. **افزونه‌ی مرورگر:** از تنظیمات ← افزونه‌ی مرورگر نصب کنید؛ روی پست یا ریل راست‌کلیک ← **Download this video with Ratatoskr**.
۳. **جعبه‌ی شناور (Drop box):** در تنظیمات روشنش کنید و لینک را روی آن بکشید.
۴. **خط فرمان:** `tosk add "https://www.instagram.com/reel/…"`.

### اندروید

۱. در اپ اینستاگرام پست یا ریل را باز کنید، **Share** را بزنید و **Ratatoskr** را انتخاب کنید (اگر در ردیف اول نبود، *More*).
۲. پنجره‌ی کوچک کیفیت‌ها باز می‌شود؛ یکی را بزنید. پنجره بسته می‌شود، به اینستاگرام برمی‌گردید و دانلود در پس‌زمینه با اعلان پیشرفت ادامه دارد.
۳. فایل‌ها در `Downloads/Ratatosk` ذخیره می‌شوند (بدون مجوز حافظه).

میان‌بر: در اینستاگرام *Copy link* بزنید و از **کاشی تنظیمات سریع** («دانلود لینک کپی‌شده») یا دکمه‌ی چسباندن داخل برنامه استفاده کنید. کاشی را یک بار از بخش ویرایش تنظیمات سریع اضافه کنید.

### چه چیزی کار می‌کند و چه چیزی نه

کار می‌کند: پست‌ها، ریل‌ها و IGTV عمومی. کار نمی‌کند (فعلاً): استوری و هایلایت، حساب‌های خصوصی، و پست‌هایی که ورود می‌خواهند.

اگر لینکی که قبلاً کار می‌کرد خطا داد، موتور را به‌روز کنید (ویندوز: روزی یک‌بار خودکار؛ اندروید: «به‌روزرسانی موتور دانلود»). اگر اینستاگرام در شبکه‌ی شما فیلتر است، فیلترشکن را روی کل دستگاه روشن کنید. برنامه هیچ کوکی یا رمز اینستاگرامی ذخیره نمی‌کند.
