"""Pure helpers for the Ratatosk Telegram bot: no network, no Telegram, so
they can be tested on their own."""

from __future__ import annotations

import html as html_lib
import ipaddress
import re
from dataclasses import dataclass
from urllib.parse import urlparse

URL_RE = re.compile(r"https?://[^\s<>\"']+", re.IGNORECASE)
TRAILING = ".,;:!?)]}»"

# Video heights offered, highest first.
HEIGHTS = (2160, 1440, 1080, 720, 480, 360, 240)


def extract_url(text: str) -> str | None:
    """The first web address in a message, without trailing punctuation."""
    match = URL_RE.search(text or "")
    if not match:
        return None
    return match.group(0).rstrip(TRAILING)


def is_public_http_url(url: str) -> bool:
    """True for http(s) addresses that do not point inside the server's own
    network. Names are checked here; `resolves_publicly` checks what a name
    resolves to, because a public-looking name can point at a private address."""
    try:
        parts = urlparse(url)
    except ValueError:
        return False
    if parts.scheme not in ("http", "https") or not parts.hostname:
        return False
    host = parts.hostname.lower().rstrip(".")
    if host == "localhost" or host.endswith((".local", ".internal", ".localhost", ".lan")):
        return False
    try:
        return is_public_ip(host)
    except ValueError:
        return "." in host  # a name, not an address


def is_public_ip(value: str) -> bool:
    """Raises ValueError when `value` is not an IP address."""
    address = ipaddress.ip_address(value)
    return not (
        address.is_private
        or address.is_loopback
        or address.is_link_local
        or address.is_multicast
        or address.is_reserved
        or address.is_unspecified
    )


def resolved_addresses_are_public(addresses: list[str]) -> bool:
    return bool(addresses) and all(is_public_ip(a) for a in addresses)


# --- Spotify -------------------------------------------------------------

SPOTIFY_RE = re.compile(
    r"^https?://open\.spotify\.com/(?:intl-[a-z]{2}/)?(track|album|playlist|episode|show|artist)/([A-Za-z0-9]+)",
    re.IGNORECASE,
)


def spotify_kind(url: str) -> str | None:
    match = SPOTIFY_RE.match(url)
    return match.group(1).lower() if match else None


def _meta(page: str, prop: str) -> str | None:
    for pattern in (
        rf'<meta[^>]+property="{prop}"[^>]+content="([^"]*)"',
        rf'<meta[^>]+content="([^"]*)"[^>]+property="{prop}"',
    ):
        found = re.search(pattern, page, re.IGNORECASE)
        if found:
            return html_lib.unescape(found.group(1)).strip()
    return None


def parse_spotify_track(page: str) -> tuple[str, str] | None:
    """(title, artist) of a track from its public page's Open Graph tags.
    The description reads like "Artist · Song · 2020"."""
    title = _meta(page, "og:title")
    if not title:
        return None
    description = _meta(page, "og:description") or ""
    artist = ""
    parts = [p.strip() for p in re.split(r"\s+[·•]\s+", description) if p.strip()]
    if parts and parts[0].lower() != title.lower():
        artist = parts[0]
    return title, artist


def spotify_search_query(title: str, artist: str) -> str:
    words = " ".join(part for part in (artist.strip(), title.strip(), "audio") if part)
    return f"ytsearch1:{words}"


# --- choices -------------------------------------------------------------

@dataclass(frozen=True)
class Choice:
    kind: str  # "video" or "audio"
    height: int | None = None

    def encode(self, token: str) -> str:
        return f"{'v' if self.kind == 'video' else 'a'}:{self.height or 0}:{token}"


def decode_choice(data: str) -> tuple[Choice, str] | None:
    parts = (data or "").split(":")
    if len(parts) != 3 or parts[0] not in ("v", "a") or not parts[1].isdigit():
        return None
    height = int(parts[1])
    if parts[0] == "a":
        return Choice("audio"), parts[2]
    return Choice("video", height or None), parts[2]


def offered_heights(available: list[int | None]) -> list[int]:
    """The video heights worth offering, from what the video has. Each height
    the video has is rounded down to a standard one (540 becomes 480); a
    height below every standard is kept as it is. At most four, highest
    first."""
    buckets: set[int] = set()
    for height in available:
        if not height:
            continue
        standard = next((s for s in HEIGHTS if s <= height), height)
        buckets.add(standard)
    return sorted(buckets, reverse=True)[:4]


def video_format(height: int | None) -> str:
    if not height:
        return "bv*+ba/b"
    return f"bv*[height<={height}]+ba/b[height<={height}]/b"


def human_size(size: int) -> str:
    value = float(size)
    for unit in ("B", "KB", "MB", "GB"):
        if value < 1024 or unit == "GB":
            return f"{value:.0f} {unit}" if unit == "B" else f"{value:.1f} {unit}"
        value /= 1024
    return f"{size} B"


def safe_filename(name: str, fallback: str = "file") -> str:
    cleaned = re.sub(r'[\\/:*?"<>|\x00-\x1f]', "_", name).strip(" .")
    return cleaned[:120] or fallback
