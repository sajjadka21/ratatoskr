"""Ratatosk Telegram bot: send a link, choose a quality, get the file.

Settings come from environment variables (see .env.example). The bot token is
never logged, and nothing about users is stored: pending choices live in
memory for ten minutes.
"""

from __future__ import annotations

import asyncio
import logging
import os
import secrets
import shutil
import socket
import tempfile
import time
from pathlib import Path
from urllib.parse import urlparse
from urllib.request import Request, urlopen

import yt_dlp
from telegram import InlineKeyboardButton, InlineKeyboardMarkup, Update
from telegram.ext import (
    Application,
    CallbackQueryHandler,
    CommandHandler,
    ContextTypes,
    MessageHandler,
    filters,
)

from core import (
    Choice,
    decode_choice,
    extract_url,
    human_size,
    is_public_http_url,
    offered_heights,
    parse_spotify_track,
    resolved_addresses_are_public,
    safe_filename,
    spotify_kind,
    spotify_search_query,
    video_format,
)
from messages import text

log = logging.getLogger("ratatosk-bot")

# Telegram's own servers accept 50 MB; a local Bot API server accepts 2 GB.
UPLOAD_LIMIT = int(os.environ.get("UPLOAD_LIMIT_MB", "49")) * 1024 * 1024
MAX_MINUTES = int(os.environ.get("MAX_MINUTES", "90"))
COOLDOWN_SECONDS = 5
PENDING_SECONDS = 600
ALLOWED = {
    int(value) for value in os.environ.get("ALLOWED_USER_IDS", "").replace(" ", "").split(",") if value
}
# Optional, set by the person running the bot (never by users): a cookies
# file lets yt-dlp read sites that want a signed-in visitor.
COOKIES_FILE = os.environ.get("COOKIES_FILE") or None

busy_users: set[int] = set()
last_request: dict[int, float] = {}


def language(update: Update) -> str | None:
    user = update.effective_user
    return user.language_code if user else None


async def allowed(update: Update) -> bool:
    user = update.effective_user
    if ALLOWED and (not user or user.id not in ALLOWED):
        if update.effective_message:
            await update.effective_message.reply_text(text(language(update), "not_allowed"))
        return False
    return True


async def start(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if await allowed(update):
        await update.message.reply_text(text(language(update), "start"))


async def help_command(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if await allowed(update):
        await update.message.reply_text(
            text(language(update), "help", limit=human_size(UPLOAD_LIMIT))
        )


async def resolves_publicly(url: str) -> bool:
    host = urlparse(url).hostname or ""
    try:
        infos = await asyncio.to_thread(socket.getaddrinfo, host, None)
    except OSError:
        return False
    return resolved_addresses_are_public(sorted({info[4][0] for info in infos}))


def base_options(directory: str | None = None) -> dict:
    options: dict = {
        "quiet": True,
        "no_warnings": True,
        "noplaylist": True,
        "playlist_items": "1",
        "socket_timeout": 30,
        "retries": 3,
        "restrictfilenames": False,
    }
    if directory:
        options["outtmpl"] = os.path.join(directory, "%(title).80s [%(id)s].%(ext)s")
    if COOKIES_FILE and os.path.exists(COOKIES_FILE):
        options["cookiefile"] = COOKIES_FILE
    return options


def probe(target: str) -> dict:
    with yt_dlp.YoutubeDL(base_options()) as ydl:
        info = ydl.extract_info(target, download=False)
    if info and info.get("entries"):
        info = next(iter(info["entries"]), None)
    return info or {}


def fetch_spotify(url: str) -> tuple[str, str] | None:
    request = Request(url, headers={"User-Agent": "Mozilla/5.0 (compatible; RatatoskBot)"})
    with urlopen(request, timeout=15) as response:  # noqa: S310 (public spotify host, checked)
        page = response.read(400_000).decode("utf-8", "replace")
    return parse_spotify_track(page)


async def on_message(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    if not await allowed(update) or not update.message:
        return
    lang = language(update)
    user_id = update.effective_user.id
    url = extract_url(update.message.text or "")
    if not url:
        await update.message.reply_text(text(lang, "no_link"))
        return
    if not is_public_http_url(url) or not await resolves_publicly(url):
        await update.message.reply_text(text(lang, "bad_link"))
        return
    if time.monotonic() - last_request.get(user_id, 0) < COOLDOWN_SECONDS:
        await update.message.reply_text(text(lang, "cooldown"))
        return
    if user_id in busy_users:
        await update.message.reply_text(text(lang, "busy"))
        return
    last_request[user_id] = time.monotonic()

    note = await update.message.reply_text(text(lang, "checking"))
    target = url
    spotify_name = None
    try:
        kind = spotify_kind(url)
        if kind:
            if kind != "track":
                await note.edit_text(text(lang, "spotify_only_tracks"))
                return
            meta = await asyncio.to_thread(fetch_spotify, url)
            if not meta:
                await note.edit_text(text(lang, "spotify_none"))
                return
            spotify_name = f"{meta[1]} — {meta[0]}" if meta[1] else meta[0]
            target = spotify_search_query(meta[0], meta[1])
        info = await asyncio.to_thread(probe, target)
    except Exception:  # noqa: BLE001 - any failure reads as "cannot read"
        log.info("probe failed for a link")
        await note.edit_text(text(lang, "cannot_read"))
        return
    if not info:
        await note.edit_text(text(lang, "cannot_read"))
        return
    duration = info.get("duration") or 0
    if duration and duration > MAX_MINUTES * 60:
        await note.edit_text(text(lang, "too_long", minutes=MAX_MINUTES))
        return

    token = secrets.token_urlsafe(6)
    pending = context.user_data.setdefault("pending", {})
    for key in [k for k, v in pending.items() if time.monotonic() - v["at"] > PENDING_SECONDS]:
        del pending[key]
    pending[token] = {"target": target, "at": time.monotonic(), "title": info.get("title") or ""}

    if spotify_name:
        choices = [Choice("audio")]
        heading = text(lang, "spotify_found", name=spotify_name)
    else:
        heights = offered_heights([f.get("height") for f in info.get("formats", [])])
        choices = [Choice("video", h) for h in heights]
        if not choices and any(f.get("vcodec") not in (None, "none") for f in info.get("formats", [])):
            choices = [Choice("video", None)]
        choices.append(Choice("audio"))
        heading = f"{info.get('title') or ''}\n\n{text(lang, 'choose')}".strip()

    def label(choice: Choice) -> str:
        if choice.kind == "audio":
            return text(lang, "audio_btn")
        return text(lang, "video_btn", height=choice.height) if choice.height else text(lang, "best_btn")

    rows = [[InlineKeyboardButton(label(c), callback_data=c.encode(token))] for c in choices]
    await note.edit_text(heading, reply_markup=InlineKeyboardMarkup(rows))


def download(target: str, choice: Choice, directory: str) -> Path:
    options = base_options(directory)
    options["max_filesize"] = UPLOAD_LIMIT
    if choice.kind == "audio":
        options["format"] = "bestaudio[ext=m4a]/bestaudio/best"
    else:
        options["format"] = video_format(choice.height)
        options["merge_output_format"] = "mp4"
    with yt_dlp.YoutubeDL(options) as ydl:
        ydl.extract_info(target, download=True)
    files = sorted(Path(directory).iterdir(), key=lambda p: p.stat().st_size, reverse=True)
    files = [f for f in files if f.suffix not in (".part", ".ytdl", ".json")]
    if not files:
        raise RuntimeError("nothing downloaded")
    return files[0]


async def on_choice(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    query = update.callback_query
    await query.answer()
    if not await allowed(update):
        return
    lang = language(update)
    user_id = update.effective_user.id
    decoded = decode_choice(query.data)
    pending = context.user_data.get("pending", {})
    entry = pending.pop(decoded[1], None) if decoded else None
    if not decoded or not entry or time.monotonic() - entry["at"] > PENDING_SECONDS:
        await query.edit_message_text(text(lang, "expired"))
        return
    if user_id in busy_users:
        await query.message.reply_text(text(lang, "busy"))
        return
    choice = decoded[0]
    busy_users.add(user_id)
    directory = tempfile.mkdtemp(prefix="ratatosk-")
    try:
        await query.edit_message_text(text(lang, "downloading"))
        try:
            path = await asyncio.to_thread(download, entry["target"], choice, directory)
        except Exception:  # noqa: BLE001
            log.info("download failed")
            await query.edit_message_text(text(lang, "too_big", limit=human_size(UPLOAD_LIMIT)) if choice.kind == "video" else text(lang, "failed"))
            return
        if path.stat().st_size > UPLOAD_LIMIT:
            await query.edit_message_text(text(lang, "too_big", limit=human_size(UPLOAD_LIMIT)))
            return
        await query.edit_message_text(text(lang, "uploading"))
        name = safe_filename(path.name)
        with path.open("rb") as handle:
            if choice.kind == "audio":
                await query.message.reply_audio(handle, filename=name, title=entry["title"][:60] or None)
            else:
                await query.message.reply_video(handle, filename=name, supports_streaming=True)
        await query.delete_message()
    except Exception:  # noqa: BLE001
        log.exception("sending failed")
        try:
            await query.edit_message_text(text(lang, "failed"))
        except Exception:  # noqa: BLE001
            pass
    finally:
        busy_users.discard(user_id)
        shutil.rmtree(directory, ignore_errors=True)


def main() -> None:
    token = os.environ.get("TELEGRAM_BOT_TOKEN")
    if not token:
        raise SystemExit("Set TELEGRAM_BOT_TOKEN (from @BotFather).")
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    # Libraries log request URLs, which contain the token; keep them quiet.
    logging.getLogger("httpx").setLevel(logging.WARNING)
    builder = Application.builder().token(token).read_timeout(600).write_timeout(600).connect_timeout(30)
    base_url = os.environ.get("BOT_API_URL")  # a local Bot API server, for 2 GB files
    if base_url:
        builder = builder.base_url(f"{base_url}/bot").base_file_url(f"{base_url}/file/bot").local_mode(True)
    app = builder.build()
    app.add_handler(CommandHandler("start", start))
    app.add_handler(CommandHandler("help", help_command))
    app.add_handler(CallbackQueryHandler(on_choice))
    app.add_handler(MessageHandler(filters.TEXT & ~filters.COMMAND, on_message))
    app.run_polling(drop_pending_updates=True)


if __name__ == "__main__":
    main()
