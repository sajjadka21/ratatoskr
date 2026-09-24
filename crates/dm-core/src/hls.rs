//! HLS streams that are not protected: playlists, the choice of quality,
//! and decrypting segments encrypted with plain AES-128.
//!
//! Anything that looks like DRM — SAMPLE-AES, a key format other than
//! `identity`, a session key of that kind — is refused with a clear reason.
//! Nothing here tries to get around protection. Live streams are refused
//! too: they never end, so there is no file to finish.

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use reqwest::Url;
use thiserror::Error;

/// Largest playlist read, so a wrong link cannot fill memory.
pub const MAX_PLAYLIST_BYTES: usize = 8 * 1024 * 1024;
/// Largest single media segment read into memory.
pub const MAX_SEGMENT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HlsError {
    #[error("this stream is protected (DRM) and cannot be downloaded")]
    Protected,
    #[error("this is a live stream; only complete (video-on-demand) streams can be downloaded")]
    Live,
    #[error(
        "every quality of this stream keeps its sound in a separate track, which needs FFmpeg to join; that is not supported yet"
    )]
    NeedsMuxing,
    #[error("the playlist lists no playable streams")]
    Empty,
    #[error("the playlist is not valid: {0}")]
    Malformed(String),
    #[error("a segment could not be decrypted")]
    Decryption,
    #[error("DASH streams (.mpd) are not supported yet")]
    Dash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Set when the sound is a separate rendition that would need muxing.
    pub audio_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentKey {
    pub uri: String,
    /// Explicit IV; otherwise the media sequence number is used.
    pub iv: Option<[u8; 16]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegment {
    pub uri: String,
    /// `(length, offset)` within the resource.
    pub byte_range: Option<(u64, u64)>,
    pub key: Option<SegmentKey>,
    pub sequence: u64,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlaylist {
    /// The `EXT-X-MAP` initialisation section of fragmented MP4 streams.
    pub init: Option<(String, Option<(u64, u64)>)>,
    pub segments: Vec<MediaSegment>,
}

impl MediaPlaylist {
    /// Fragmented MP4 streams become `.mp4`; transport streams `.ts`.
    pub fn extension(&self) -> &'static str {
        if self.init.is_some() { "mp4" } else { "ts" }
    }

    pub fn duration_seconds(&self) -> f64 {
        self.segments
            .iter()
            .map(|segment| segment.duration_seconds)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    Master(Vec<Variant>),
    Media(MediaPlaylist),
}

/// Parses a playlist fetched from `base`.
pub fn parse_playlist(base: &Url, text: &str) -> Result<Playlist, HlsError> {
    let mut lines = text
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty());
    if lines.next() != Some("#EXTM3U") {
        return Err(HlsError::Malformed(
            "it does not start with #EXTM3U".to_owned(),
        ));
    }
    let lines: Vec<&str> = lines.collect();

    for line in &lines {
        if let Some(attributes) = line
            .strip_prefix("#EXT-X-SESSION-KEY:")
            .or_else(|| line.strip_prefix("#EXT-X-KEY:"))
        {
            // Checked up front for both kinds of playlist.
            key_method(&parse_attributes(attributes))?;
        }
    }

    if lines
        .iter()
        .any(|line| line.starts_with("#EXT-X-STREAM-INF"))
    {
        return parse_master(base, &lines).map(Playlist::Master);
    }
    parse_media(base, &lines).map(Playlist::Media)
}

fn parse_master(base: &Url, lines: &[&str]) -> Result<Vec<Variant>, HlsError> {
    let mut variants = Vec::new();
    let mut pending: Option<Vec<(String, String)>> = None;
    for line in lines {
        if let Some(attributes) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending = Some(parse_attributes(attributes));
        } else if !line.starts_with('#')
            && let Some(attributes) = pending.take()
        {
            let uri = base
                .join(line)
                .map_err(|_| HlsError::Malformed(format!("bad stream address {line:?}")))?;
            let value = |key: &str| {
                attributes
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(key))
                    .map(|(_, value)| value.clone())
            };
            let (width, height) = value("RESOLUTION")
                .and_then(|resolution| {
                    let (width, height) = resolution.split_once(['x', 'X'])?;
                    Some((width.parse().ok(), height.parse().ok()))
                })
                .unwrap_or((None, None));
            variants.push(Variant {
                uri: uri.to_string(),
                bandwidth: value("BANDWIDTH").and_then(|value| value.parse().ok()),
                width,
                height,
                audio_group: value("AUDIO"),
            });
        }
    }
    if variants.is_empty() {
        return Err(HlsError::Empty);
    }
    Ok(variants)
}

fn parse_media(base: &Url, lines: &[&str]) -> Result<MediaPlaylist, HlsError> {
    let mut sequence: u64 = 0;
    let mut ended = false;
    let mut duration = 0.0;
    let mut key: Option<SegmentKey> = None;
    let mut byte_range: Option<(u64, Option<u64>)> = None;
    let mut last_range_end: Option<(String, u64)> = None;
    let mut init = None;
    let mut segments = Vec::new();

    for line in lines {
        if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            sequence = value
                .trim()
                .parse()
                .map_err(|_| HlsError::Malformed("bad media sequence".to_owned()))?;
        } else if *line == "#EXT-X-ENDLIST" || *line == "#EXT-X-PLAYLIST-TYPE:VOD" {
            ended = true;
        } else if let Some(value) = line.strip_prefix("#EXTINF:") {
            duration = value
                .split(',')
                .next()
                .and_then(|seconds| seconds.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
        } else if let Some(value) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            byte_range = Some(parse_byte_range(value)?);
        } else if let Some(attributes) = line.strip_prefix("#EXT-X-KEY:") {
            let attributes = parse_attributes(attributes);
            key = match key_method(&attributes)? {
                KeyMethod::None => None,
                KeyMethod::Aes128 => {
                    let uri = attribute(&attributes, "URI")
                        .ok_or_else(|| HlsError::Malformed("AES-128 key without URI".to_owned()))?;
                    let uri = base
                        .join(&uri)
                        .map_err(|_| HlsError::Malformed("bad key address".to_owned()))?;
                    Some(SegmentKey {
                        uri: uri.to_string(),
                        iv: attribute(&attributes, "IV")
                            .map(|iv| parse_iv(&iv))
                            .transpose()?,
                    })
                }
            };
        } else if let Some(attributes) = line.strip_prefix("#EXT-X-MAP:") {
            let attributes = parse_attributes(attributes);
            let uri = attribute(&attributes, "URI")
                .ok_or_else(|| HlsError::Malformed("EXT-X-MAP without URI".to_owned()))?;
            let uri = base
                .join(&uri)
                .map_err(|_| HlsError::Malformed("bad map address".to_owned()))?;
            let range = attribute(&attributes, "BYTERANGE")
                .map(|value| parse_byte_range(&value))
                .transpose()?
                .map(|(length, offset)| (length, offset.unwrap_or(0)));
            init = Some((uri.to_string(), range));
        } else if !line.starts_with('#') {
            let uri = base
                .join(line)
                .map_err(|_| HlsError::Malformed(format!("bad segment address {line:?}")))?
                .to_string();
            let range = byte_range.take().map(|(length, offset)| {
                let offset = offset.unwrap_or_else(|| {
                    last_range_end
                        .as_ref()
                        .filter(|(previous, _)| *previous == uri)
                        .map_or(0, |(_, end)| *end)
                });
                (length, offset)
            });
            if let Some((length, offset)) = range {
                last_range_end = Some((uri.clone(), offset + length));
            }
            segments.push(MediaSegment {
                uri,
                byte_range: range,
                key: key.clone(),
                sequence,
                duration_seconds: duration,
            });
            sequence = sequence.saturating_add(1);
            duration = 0.0;
        }
    }

    if !ended {
        return Err(HlsError::Live);
    }
    if segments.is_empty() {
        return Err(HlsError::Empty);
    }
    Ok(MediaPlaylist { init, segments })
}

/// The quality to download: the highest resolution (then bandwidth) at or
/// below `max_height`, among streams that carry their own sound.
pub fn choose_variant(variants: &[Variant], max_height: Option<u32>) -> Result<&Variant, HlsError> {
    let muxed: Vec<&Variant> = variants
        .iter()
        .filter(|variant| variant.audio_group.is_none())
        .collect();
    if muxed.is_empty() {
        return Err(HlsError::NeedsMuxing);
    }
    let fits = |variant: &&&Variant| {
        max_height.is_none_or(|limit| variant.height.is_none_or(|height| height <= limit))
    };
    let rank = |variant: &&&Variant| (variant.height.unwrap_or(0), variant.bandwidth.unwrap_or(0));
    muxed
        .iter()
        .filter(fits)
        .max_by_key(rank)
        .or_else(|| muxed.iter().min_by_key(rank))
        .copied()
        .ok_or(HlsError::Empty)
}

/// Decrypts one AES-128 segment. Without an explicit IV the media sequence
/// number, big-endian, is the IV, as the HLS specification says.
pub fn decrypt_segment(
    data: &mut Vec<u8>,
    key: &[u8],
    iv: Option<[u8; 16]>,
    sequence: u64,
) -> Result<(), HlsError> {
    let key: [u8; 16] = key.try_into().map_err(|_| HlsError::Decryption)?;
    let iv = iv.unwrap_or_else(|| {
        let mut iv = [0_u8; 16];
        iv[8..].copy_from_slice(&sequence.to_be_bytes());
        iv
    });
    let length = cbc::Decryptor::<Aes128>::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(data)
        .map_err(|_| HlsError::Decryption)?
        .len();
    data.truncate(length);
    Ok(())
}

enum KeyMethod {
    None,
    Aes128,
}

fn key_method(attributes: &[(String, String)]) -> Result<KeyMethod, HlsError> {
    let format_is_plain = attribute(attributes, "KEYFORMAT")
        .is_none_or(|format| format.eq_ignore_ascii_case("identity"));
    match attribute(attributes, "METHOD").as_deref() {
        None | Some("NONE") => Ok(KeyMethod::None),
        Some("AES-128") if format_is_plain => Ok(KeyMethod::Aes128),
        _ => Err(HlsError::Protected),
    }
}

fn attribute(attributes: &[(String, String)], key: &str) -> Option<String> {
    attributes
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.clone())
}

/// Splits `A=1,B="x,y",C=z`, respecting quoted commas.
fn parse_attributes(text: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut push = |item: &str| {
        if let Some((name, value)) = item.split_once('=') {
            attributes.push((
                name.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            ));
        }
    };
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            ',' if !quoted => {
                push(&current);
                current.clear();
            }
            _ => current.push(character),
        }
    }
    push(&current);
    attributes
}

fn parse_byte_range(value: &str) -> Result<(u64, Option<u64>), HlsError> {
    let bad = || HlsError::Malformed(format!("bad byte range {value:?}"));
    let value = value.trim().trim_matches('"');
    match value.split_once('@') {
        Some((length, offset)) => Ok((
            length.parse().map_err(|_| bad())?,
            Some(offset.parse().map_err(|_| bad())?),
        )),
        None => Ok((value.parse().map_err(|_| bad())?, None)),
    }
}

fn parse_iv(value: &str) -> Result<[u8; 16], HlsError> {
    let hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if hex.len() != 32 {
        return Err(HlsError::Malformed("IV must be 16 bytes".to_owned()));
    }
    let mut iv = [0_u8; 16];
    for (index, byte) in iv.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| HlsError::Malformed("IV is not hexadecimal".to_owned()))?;
    }
    Ok(iv)
}

/// A stream ready to download: the media playlist and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStream {
    pub media_url: String,
    pub playlist: MediaPlaylist,
    pub variant: Option<Variant>,
}

/// Fetches `url` and, when it is a master playlist, the best quality at or
/// below `max_height`.
pub async fn resolve_stream(
    downloader: &crate::Downloader,
    url: &str,
    max_height: Option<u32>,
    control: &crate::control::TaskControl,
) -> crate::Result<ResolvedStream> {
    let first = fetch_playlist(downloader, url, control).await?;
    match first {
        Playlist::Media(playlist) => Ok(ResolvedStream {
            media_url: url.to_owned(),
            playlist,
            variant: None,
        }),
        Playlist::Master(variants) => {
            let variant = choose_variant(&variants, max_height)?.clone();
            match fetch_playlist(downloader, &variant.uri, control).await? {
                Playlist::Media(playlist) => Ok(ResolvedStream {
                    media_url: variant.uri.clone(),
                    playlist,
                    variant: Some(variant),
                }),
                Playlist::Master(_) => Err(HlsError::Malformed(
                    "a quality points at another list of qualities".to_owned(),
                )
                .into()),
            }
        }
    }
}

/// The qualities a playlist offers, for choosing before a download starts.
/// A media playlist offers exactly one.
pub async fn list_variants(
    downloader: &crate::Downloader,
    url: &str,
) -> crate::Result<Vec<Variant>> {
    let control = crate::control::TaskControl::new();
    match fetch_playlist(downloader, url, &control).await? {
        Playlist::Master(variants) => Ok(variants),
        Playlist::Media(_) => Ok(vec![Variant {
            uri: url.to_owned(),
            bandwidth: None,
            width: None,
            height: None,
            audio_group: None,
        }]),
    }
}

async fn fetch_playlist(
    downloader: &crate::Downloader,
    url: &str,
    control: &crate::control::TaskControl,
) -> crate::Result<Playlist> {
    let bytes = downloader
        .fetch_bytes(url, None, MAX_PLAYLIST_BYTES, control)
        .await?;
    let text = String::from_utf8_lossy(&bytes);
    let base =
        Url::parse(url).map_err(|error| crate::DownloadError::InvalidUrl(error.to_string()))?;
    Ok(parse_playlist(&base, &text)?)
}

/// A file name for the stream: the playlist's own name unless it is one of
/// the generic names players use, then the folder it sits in.
pub fn stream_filename(playlist_url: &str, extension: &str) -> String {
    const GENERIC: &[&str] = &[
        "index",
        "playlist",
        "master",
        "prog_index",
        "chunklist",
        "manifest",
        "main",
        "video",
        "media",
        "stream",
    ];
    let segments: Vec<String> = Url::parse(playlist_url)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .map(|parts| parts.map(|part| part.to_owned()).collect())
        })
        .unwrap_or_default();
    let stem = |name: &str| {
        let decoded = percent_encoding::percent_decode_str(name)
            .decode_utf8_lossy()
            .into_owned();
        decoded
            .rsplit_once('.')
            .map_or(decoded.clone(), |(stem, _)| stem.to_owned())
    };
    let name = segments
        .iter()
        .rev()
        .filter(|part| !part.is_empty())
        .map(|part| stem(part))
        .find(|candidate| {
            let lower = candidate.to_ascii_lowercase();
            !GENERIC.iter().any(|generic| lower.starts_with(generic))
                && !lower.chars().all(|character| character.is_ascii_digit())
        })
        .unwrap_or_else(|| "video".to_owned());
    format!("{name}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::BlockEncryptMut;

    fn base() -> Url {
        Url::parse("https://cdn.example.com/show/ep1/master.m3u8").unwrap()
    }

    #[test]
    fn master_playlists_list_every_quality_with_quoted_attributes() {
        let text = "#EXTM3U\n\
            #EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.4d401e,mp4a.40.2\"\n\
            360/index.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\"\n\
            720/index.m3u8\n";
        let Playlist::Master(variants) = parse_playlist(&base(), text).unwrap() else {
            panic!("expected a master playlist");
        };
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[1].height, Some(720));
        assert_eq!(
            variants[1].uri,
            "https://cdn.example.com/show/ep1/720/index.m3u8"
        );
        assert_eq!(choose_variant(&variants, None).unwrap().height, Some(720));
        assert_eq!(
            choose_variant(&variants, Some(480)).unwrap().height,
            Some(360)
        );
        assert_eq!(
            choose_variant(&variants, Some(144)).unwrap().height,
            Some(360)
        );
    }

    #[test]
    fn qualities_with_a_separate_sound_track_are_not_chosen() {
        let text = "#EXTM3U\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"en\",URI=\"audio.m3u8\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=1280x720,AUDIO=\"aud\"\n\
            720.m3u8\n";
        let Playlist::Master(variants) = parse_playlist(&base(), text).unwrap() else {
            panic!("expected a master playlist");
        };
        assert_eq!(choose_variant(&variants, None), Err(HlsError::NeedsMuxing));
    }

    #[test]
    fn media_playlists_carry_sequence_keys_ranges_and_init_sections() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-MEDIA-SEQUENCE:7\n\
            #EXT-X-MAP:URI=\"init.mp4\"\n\
            #EXTINF:6.0,\nseg-a.m4s\n\
            #EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\",IV=0x000102030405060708090a0b0c0d0e0f\n\
            #EXTINF:5.5,\n#EXT-X-BYTERANGE:1000@0\nall.m4s\n\
            #EXTINF:4.0,\n#EXT-X-BYTERANGE:500\nall.m4s\n\
            #EXT-X-KEY:METHOD=NONE\n#EXTINF:3.0,\nseg-d.m4s\n#EXT-X-ENDLIST\n";
        let Playlist::Media(playlist) = parse_playlist(&base(), text).unwrap() else {
            panic!("expected a media playlist");
        };
        assert_eq!(playlist.extension(), "mp4");
        assert_eq!(playlist.segments.len(), 4);
        assert_eq!(playlist.segments[0].sequence, 7);
        assert!(playlist.segments[0].key.is_none());
        let key = playlist.segments[1].key.as_ref().unwrap();
        assert_eq!(key.uri, "https://cdn.example.com/show/ep1/key.bin");
        assert_eq!(key.iv.unwrap()[15], 0x0f);
        assert_eq!(playlist.segments[1].byte_range, Some((1000, 0)));
        assert_eq!(
            playlist.segments[2].byte_range,
            Some((500, 1000)),
            "a range without offset continues the previous one"
        );
        assert!(playlist.segments[3].key.is_none());
        assert!((playlist.duration_seconds() - 18.5).abs() < f64::EPSILON);
    }

    #[test]
    fn drm_and_live_streams_are_refused() {
        let sample_aes = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://key\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(
            parse_playlist(&base(), sample_aes),
            Err(HlsError::Protected)
        );

        let widevine = "#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES-CTR,KEYFORMAT=\"urn:uuid:edef8ba9\"\n#EXT-X-STREAM-INF:BANDWIDTH=1\na.m3u8\n";
        assert_eq!(parse_playlist(&base(), widevine), Err(HlsError::Protected));

        let odd_format = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\",KEYFORMAT=\"com.example.drm\"\n#EXTINF:6,\na.ts\n#EXT-X-ENDLIST\n";
        assert_eq!(
            parse_playlist(&base(), odd_format),
            Err(HlsError::Protected)
        );

        let live = "#EXTM3U\n#EXTINF:6,\na.ts\n#EXTINF:6,\nb.ts\n";
        assert_eq!(parse_playlist(&base(), live), Err(HlsError::Live));

        assert!(matches!(
            parse_playlist(&base(), "<html>"),
            Err(HlsError::Malformed(_))
        ));
    }

    #[test]
    fn aes_128_segments_decrypt_with_explicit_or_sequence_iv() {
        let key = [7_u8; 16];
        let plain = b"transport stream bytes, not a multiple of sixteen".to_vec();
        for (iv, sequence) in [(Some([3_u8; 16]), 0_u64), (None, 42)] {
            let effective = iv.unwrap_or_else(|| {
                let mut iv = [0_u8; 16];
                iv[8..].copy_from_slice(&sequence.to_be_bytes());
                iv
            });
            let mut buffer = plain.clone();
            buffer.resize(plain.len() + 16, 0);
            let encrypted = cbc::Encryptor::<Aes128>::new(&key.into(), &effective.into())
                .encrypt_padded_mut::<Pkcs7>(&mut buffer, plain.len())
                .unwrap()
                .to_vec();
            let mut data = encrypted;
            decrypt_segment(&mut data, &key, iv, sequence).unwrap();
            assert_eq!(data, plain);
        }
        let mut garbage = vec![1_u8; 32];
        assert_eq!(
            decrypt_segment(&mut garbage, &[0_u8; 4], None, 0),
            Err(HlsError::Decryption)
        );
    }

    #[test]
    fn stream_files_are_named_after_the_show_not_the_playlist() {
        assert_eq!(
            stream_filename("https://cdn.example.com/show/ep1/720/index.m3u8", "ts"),
            "ep1.ts"
        );
        assert_eq!(
            stream_filename("https://cdn.example.com/lecture-07.m3u8", "mp4"),
            "lecture-07.mp4"
        );
        assert_eq!(
            stream_filename("https://cdn.example.com/master.m3u8", "ts"),
            "video.ts"
        );
    }
}
