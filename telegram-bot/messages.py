"""Bot texts. Persian for Persian-language users, English for everyone else."""

FA = {
    "start": (
        "سلام! من ربات راتاتوسک هستم 🐿\n\n"
        "لینک یک ویدیو یا آهنگ را بفرستید (یوتیوب، اینستاگرام، اسپاتیفای، توییتر و بسیاری سایت‌های دیگر) "
        "تا کیفیتش را انتخاب کنید و فایل را برایتان بفرستم.\n\n"
        "راهنما: /help"
    ),
    "help": (
        "• لینک را بفرستید؛ کیفیت ویدیو یا «فقط صدا» را انتخاب کنید.\n"
        "• اسپاتیفای: آهنگ از روی نام و خواننده در یوتیوب پیدا و به صورت صدا فرستاده می‌شود "
        "(فایل خود اسپاتیفای نیست).\n"
        "• اینستاگرام: فقط پست‌ها و ریل‌های عمومی.\n"
        "• فایل‌های بزرگ‌تر از {limit} به‌خاطر محدودیت تلگرام فرستاده نمی‌شوند؛ کیفیت پایین‌تر بخواهید.\n"
        "• فقط محتوایی را دانلود کنید که حق دانلودش را دارید."
    ),
    "no_link": "یک لینک (http یا https) بفرستید.",
    "bad_link": "این لینک پذیرفته نشد.",
    "busy": "یک دانلود از شما در حال انجام است. کمی صبر کنید.",
    "cooldown": "لطفاً چند ثانیه بعد دوباره امتحان کنید.",
    "not_allowed": "این ربات خصوصی است.",
    "checking": "در حال بررسی لینک…",
    "cannot_read": "نتوانستم این لینک را بخوانم. ممکن است خصوصی، حذف‌شده یا محدود به برخی کشورها باشد.",
    "choose": "کیفیت را انتخاب کنید:",
    "video_btn": "🎬 {height}p",
    "best_btn": "🎬 بهترین کیفیت",
    "audio_btn": "🎵 فقط صدا",
    "too_long": "این ویدیو از {minutes} دقیقه بلندتر است و دانلود نمی‌شود.",
    "downloading": "در حال دانلود…",
    "uploading": "در حال ارسال…",
    "too_big": "فایل بزرگ‌تر از {limit} است و تلگرام اجازه‌ی ارسالش را نمی‌دهد. کیفیت پایین‌تر را امتحان کنید.",
    "failed": "دانلود انجام نشد.",
    "expired": "این انتخاب منقضی شده؛ لینک را دوباره بفرستید.",
    "spotify_only_tracks": "فقط لینک آهنگ اسپاتیفای پشتیبانی می‌شود.",
    "spotify_found": "🎵 {name}\n(از روی نام، صدا از یوتیوب پیدا شد؛ فایل خود اسپاتیفای نیست.)",
    "spotify_none": "اطلاعات این آهنگ خوانده نشد.",
}

EN = {
    "start": (
        "Hi! I'm the Ratatosk bot 🐿\n\n"
        "Send me a link to a video or song (YouTube, Instagram, Spotify, Twitter and many more) "
        "and choose the quality.\n\nHelp: /help"
    ),
    "help": (
        "• Send a link, then pick a video quality or audio only.\n"
        "• Spotify: the song is found on YouTube by title and artist and sent as audio "
        "(it is not Spotify's own file).\n"
        "• Instagram: public posts and reels only.\n"
        "• Files over {limit} cannot be sent because of Telegram's limit; choose a lower quality.\n"
        "• Only download what you have the right to download."
    ),
    "no_link": "Send a link (http or https).",
    "bad_link": "That link was not accepted.",
    "busy": "You already have a download running. Please wait.",
    "cooldown": "Please try again in a few seconds.",
    "not_allowed": "This bot is private.",
    "checking": "Checking the link…",
    "cannot_read": "I could not read this link. It may be private, removed or limited to some countries.",
    "choose": "Choose the quality:",
    "video_btn": "🎬 {height}p",
    "best_btn": "🎬 Best quality",
    "audio_btn": "🎵 Audio only",
    "too_long": "This video is longer than {minutes} minutes and will not be downloaded.",
    "downloading": "Downloading…",
    "uploading": "Sending…",
    "too_big": "The file is larger than {limit}, which Telegram does not allow. Try a lower quality.",
    "failed": "The download failed.",
    "expired": "That choice expired; send the link again.",
    "spotify_only_tracks": "Only Spotify track links are supported.",
    "spotify_found": "🎵 {name}\n(Audio found on YouTube by name; it is not Spotify's own file.)",
    "spotify_none": "I could not read this track.",
}


def text(language_code: str | None, key: str, **values) -> str:
    table = FA if (language_code or "").lower().startswith("fa") else EN
    return table[key].format(**values) if values else table[key]
