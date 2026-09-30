import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from core import (  # noqa: E402
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


def test_url_is_found_without_trailing_punctuation():
    assert extract_url("look: https://youtu.be/abc123, nice") == "https://youtu.be/abc123"
    assert extract_url("(https://example.com/a?b=1)") == "https://example.com/a?b=1"
    assert extract_url("no link here") is None


def test_private_and_local_addresses_are_refused():
    for bad in (
        "http://localhost/x",
        "http://127.0.0.1/x",
        "http://10.0.0.5/x",
        "http://192.168.1.1/",
        "http://169.254.169.254/latest/meta-data",
        "http://[::1]/",
        "http://printer.local/",
        "ftp://example.com/file",
        "file:///etc/passwd",
        "http://intranet/",
    ):
        assert not is_public_http_url(bad), bad
    assert is_public_http_url("https://www.instagram.com/reel/abc/")
    assert is_public_http_url("http://93.184.216.34/file.zip")


def test_a_name_resolving_inside_the_network_is_refused():
    assert resolved_addresses_are_public(["93.184.216.34"])
    assert not resolved_addresses_are_public(["93.184.216.34", "10.1.2.3"])
    assert not resolved_addresses_are_public([])


def test_spotify_links_are_recognised():
    assert spotify_kind("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=x") == "track"
    assert spotify_kind("https://open.spotify.com/intl-fa/album/abc") == "album"
    assert spotify_kind("https://youtube.com/watch?v=1") is None


def test_spotify_track_is_read_from_the_page():
    page = (
        '<meta property="og:title" content="Never Gonna Give You Up"/>'
        '<meta property="og:description" content="Rick Astley · Whenever You Need Somebody · Song · 1987"/>'
    )
    assert parse_spotify_track(page) == ("Never Gonna Give You Up", "Rick Astley")
    assert parse_spotify_track("<html></html>") is None
    assert spotify_search_query("Song", "Artist") == "ytsearch1:Artist Song audio"
    assert spotify_search_query("Song", "") == "ytsearch1:Song audio"


def test_choices_round_trip_and_bad_data_is_refused():
    data = Choice("video", 720).encode("tok")
    assert decode_choice(data) == (Choice("video", 720), "tok")
    assert decode_choice(Choice("audio").encode("t")) == (Choice("audio"), "t")
    assert len(data) < 64
    for bad in ("", "x:1:t", "v:abc:t", "v:1", "v:1:t:u"):
        assert decode_choice(bad) is None


def test_only_heights_the_video_has_are_offered():
    assert offered_heights([360, 720, 1080]) == [1080, 720, 360]
    assert offered_heights([144, 240]) == [240, 144]
    assert offered_heights([1080, 2160, 1440, 720, 480, 360]) == [2160, 1440, 1080, 720]
    assert offered_heights([None]) == []
    assert offered_heights([540]) == [480]


def test_format_selector_caps_height():
    assert "height<=720" in video_format(720)
    assert video_format(None) == "bv*+ba/b"


def test_helpers():
    assert human_size(512) == "512 B"
    assert human_size(5 * 1024 * 1024) == "5.0 MB"
    assert safe_filename('a/b:c*?.mp4') == "a_b_c__.mp4"
    assert safe_filename("...") == "file"
