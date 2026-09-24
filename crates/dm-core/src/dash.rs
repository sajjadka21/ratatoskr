//! MPEG-DASH manifests (`.mpd`) that are not protected: finding the
//! picture and sound representations and listing their segments.
//!
//! Supported: static (on-demand) manifests with one period, `BaseURL`
//! chains, `SegmentTemplate` with `$Number$` or a `SegmentTimeline`,
//! `SegmentList`, and single-file `SegmentBase` representations. Refused:
//! any `ContentProtection` (DRM) on what would be downloaded, and live
//! (`dynamic`) manifests.

use crate::hls::HlsError;
use reqwest::Url;

/// One piece of a track: address and optional `(length, offset)` range.
pub type Part = (String, Option<(u64, u64)>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    /// Picture and sound in one representation.
    Muxed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Representation {
    pub id: String,
    pub kind: TrackKind,
    pub bandwidth: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub init: Option<Part>,
    pub segments: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub video: Vec<Representation>,
    pub audio: Vec<Representation>,
    pub muxed: Vec<Representation>,
}

impl Manifest {
    /// The best picture at or below `max_height`, and the best sound.
    pub fn choose(&self, max_height: Option<u32>) -> Result<Vec<&Representation>, HlsError> {
        fn rank(item: &&Representation) -> (u32, u64) {
            (item.height.unwrap_or(0), item.bandwidth.unwrap_or(0))
        }
        fn pick(list: &[Representation], max_height: Option<u32>) -> Option<&Representation> {
            list.iter()
                .filter(|item| {
                    max_height.is_none_or(|limit| item.height.is_none_or(|height| height <= limit))
                })
                .max_by_key(rank)
                .or_else(|| list.iter().min_by_key(rank))
        }
        if let Some(video) = pick(&self.video, max_height) {
            let audio = self
                .audio
                .iter()
                .max_by_key(|item| item.bandwidth.unwrap_or(0));
            return Ok(std::iter::once(video).chain(audio).collect());
        }
        if let Some(muxed) = pick(&self.muxed, max_height) {
            return Ok(vec![muxed]);
        }
        if let Some(audio) = self
            .audio
            .iter()
            .max_by_key(|item| item.bandwidth.unwrap_or(0))
        {
            return Ok(vec![audio]);
        }
        Err(HlsError::Empty)
    }
}

/// Parses a manifest fetched from `base`.
pub fn parse_manifest(base: &Url, text: &str) -> Result<Manifest, HlsError> {
    let document = roxmltree::Document::parse(text)
        .map_err(|error| HlsError::Malformed(format!("not a DASH manifest: {error}")))?;
    let mpd = document.root_element();
    if mpd.tag_name().name() != "MPD" {
        return Err(HlsError::Malformed("not a DASH manifest".to_owned()));
    }
    if mpd.attribute("type") == Some("dynamic") {
        return Err(HlsError::Live);
    }
    let periods: Vec<_> = children(mpd, "Period").collect();
    let [period] = periods.as_slice() else {
        return Err(HlsError::Malformed(
            "manifests with several periods are not supported".to_owned(),
        ));
    };
    let duration = period
        .attribute("duration")
        .or_else(|| mpd.attribute("mediaPresentationDuration"))
        .and_then(parse_duration);

    let base = resolve_base(&resolve_base(base, mpd)?, *period)?;
    let mut manifest = Manifest::default();
    let mut protected_only = true;
    let mut any_set = false;

    for set in children(*period, "AdaptationSet") {
        any_set = true;
        let set_protected = children(set, "ContentProtection").next().is_some();
        let set_base = resolve_base(&base, set)?;
        for representation in children(set, "Representation") {
            let protected = set_protected
                || children(representation, "ContentProtection")
                    .next()
                    .is_some();
            if protected {
                continue;
            }
            protected_only = false;
            let kind = kind_of(set, representation);
            let rep_base = resolve_base(&set_base, representation)?;
            let (init, segments) = segments_of(set, representation, &rep_base, duration)?;
            let number = |name: &str| {
                representation
                    .attribute(name)
                    .or_else(|| set.attribute(name))
                    .and_then(|value| value.parse().ok())
            };
            let item = Representation {
                id: representation
                    .attribute("id")
                    .unwrap_or_default()
                    .to_owned(),
                kind: kind.clone(),
                bandwidth: representation
                    .attribute("bandwidth")
                    .and_then(|value| value.parse().ok()),
                width: number("width"),
                height: number("height"),
                init,
                segments,
            };
            match kind {
                TrackKind::Video => manifest.video.push(item),
                TrackKind::Audio => manifest.audio.push(item),
                TrackKind::Muxed => manifest.muxed.push(item),
            }
        }
    }

    if any_set && protected_only {
        return Err(HlsError::Protected);
    }
    if manifest.video.is_empty() && manifest.audio.is_empty() && manifest.muxed.is_empty() {
        return Err(HlsError::Empty);
    }
    Ok(manifest)
}

fn children<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &'static str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

fn resolve_base(base: &Url, node: roxmltree::Node<'_, '_>) -> Result<Url, HlsError> {
    match children(node, "BaseURL")
        .next()
        .and_then(|element| element.text())
    {
        Some(text) => base
            .join(text.trim())
            .map_err(|_| HlsError::Malformed("bad BaseURL".to_owned())),
        None => Ok(base.clone()),
    }
}

fn kind_of(set: roxmltree::Node<'_, '_>, representation: roxmltree::Node<'_, '_>) -> TrackKind {
    let text = |name: &str| {
        representation
            .attribute(name)
            .or_else(|| set.attribute(name))
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    let mime = text("mimeType");
    let content = set
        .attribute("contentType")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let codecs = text("codecs");
    let has_audio_codec =
        codecs.contains("mp4a") || codecs.contains("ac-3") || codecs.contains("opus");
    let has_video_codec = codecs.contains("avc")
        || codecs.contains("hvc")
        || codecs.contains("hev")
        || codecs.contains("vp9")
        || codecs.contains("av01");
    if (mime.starts_with("video") || content == "video") && has_audio_codec && has_video_codec {
        TrackKind::Muxed
    } else if mime.starts_with("audio") || content == "audio" {
        TrackKind::Audio
    } else {
        TrackKind::Video
    }
}

/// The template or list that applies: the representation's own, else the
/// adaptation set's.
fn inherited<'a, 'input>(
    set: roxmltree::Node<'a, 'input>,
    representation: roxmltree::Node<'a, 'input>,
    name: &'static str,
) -> Option<roxmltree::Node<'a, 'input>> {
    children(representation, name)
        .next()
        .or_else(|| children(set, name).next())
}

fn segments_of(
    set: roxmltree::Node<'_, '_>,
    representation: roxmltree::Node<'_, '_>,
    base: &Url,
    duration_seconds: Option<f64>,
) -> Result<(Option<Part>, Vec<Part>), HlsError> {
    let join = |relative: &str| {
        base.join(relative)
            .map(|url| url.to_string())
            .map_err(|_| HlsError::Malformed(format!("bad segment address {relative:?}")))
    };
    let id = representation.attribute("id").unwrap_or_default();
    let bandwidth = representation.attribute("bandwidth").unwrap_or_default();

    if let Some(template) = inherited(set, representation, "SegmentTemplate") {
        let attribute = |name: &str| {
            template.attribute(name).or_else(|| {
                // The adaptation set's template may carry what the
                // representation's leaves out.
                children(set, "SegmentTemplate")
                    .next()
                    .and_then(|outer| outer.attribute(name))
            })
        };
        let media = attribute("media")
            .ok_or_else(|| HlsError::Malformed("SegmentTemplate without media".to_owned()))?;
        let timescale: u64 = attribute("timescale")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1)
            .max(1);
        let start_number: u64 = attribute("startNumber")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1);
        let init = attribute("initialization")
            .map(|pattern| join(&fill_template(pattern, id, bandwidth, 0, 0)))
            .transpose()?
            .map(|url| (url, None));

        let mut segments = Vec::new();
        let timeline = children(template, "SegmentTimeline").next().or_else(|| {
            children(set, "SegmentTemplate")
                .next()
                .and_then(|outer| children(outer, "SegmentTimeline").next())
        });
        if let Some(timeline) = timeline {
            let mut time: u64 = 0;
            let mut number = start_number;
            let end = duration_seconds.map(|seconds| (seconds * timescale as f64).round() as u64);
            for entry in children(timeline, "S") {
                if let Some(start) = entry.attribute("t").and_then(|value| value.parse().ok()) {
                    time = start;
                }
                let length: u64 = entry
                    .attribute("d")
                    .and_then(|value| value.parse().ok())
                    .filter(|length| *length > 0)
                    .ok_or_else(|| {
                        HlsError::Malformed("timeline entry without duration".to_owned())
                    })?;
                let repeat: i64 = entry
                    .attribute("r")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let count = if repeat < 0 {
                    let end = end.ok_or_else(|| {
                        HlsError::Malformed("open-ended timeline without a duration".to_owned())
                    })?;
                    end.saturating_sub(time).div_ceil(length)
                } else {
                    repeat as u64 + 1
                };
                for _ in 0..count.min(MAX_SEGMENTS as u64) {
                    segments.push((
                        join(&fill_template(media, id, bandwidth, number, time))?,
                        None,
                    ));
                    time += length;
                    number += 1;
                }
                if segments.len() >= MAX_SEGMENTS {
                    break;
                }
            }
        } else {
            let length: u64 = attribute("duration")
                .and_then(|value| value.parse().ok())
                .filter(|length| *length > 0)
                .ok_or_else(|| {
                    HlsError::Malformed("SegmentTemplate without duration or timeline".to_owned())
                })?;
            let total = duration_seconds.ok_or_else(|| {
                HlsError::Malformed("the manifest does not say how long it is".to_owned())
            })?;
            let count = ((total * timescale as f64) / length as f64).ceil() as u64;
            for index in 0..count.min(MAX_SEGMENTS as u64) {
                let number = start_number + index;
                segments.push((
                    join(&fill_template(media, id, bandwidth, number, index * length))?,
                    None,
                ));
            }
        }
        return Ok((init, segments));
    }

    if let Some(list) = inherited(set, representation, "SegmentList") {
        let init = children(list, "Initialization")
            .next()
            .map(|element| {
                let url = element
                    .attribute("sourceURL")
                    .map_or_else(|| Ok(base.to_string()), join)?;
                Ok::<_, HlsError>((url, element.attribute("range").and_then(parse_range)))
            })
            .transpose()?;
        let segments = children(list, "SegmentURL")
            .map(|element| {
                let url = element
                    .attribute("media")
                    .map_or_else(|| Ok(base.to_string()), join)?;
                Ok((url, element.attribute("mediaRange").and_then(parse_range)))
            })
            .collect::<Result<Vec<_>, HlsError>>()?;
        return Ok((init, segments));
    }

    // SegmentBase, or nothing at all: the representation is one file.
    Ok((None, vec![(base.to_string(), None)]))
}

/// A manifest listing more segments than this is refused rather than read.
const MAX_SEGMENTS: usize = 200_000;

/// Fills `$RepresentationID$`, `$Bandwidth$`, `$Number$`, `$Time$` (with an
/// optional `%0Nd` width) and `$$`.
fn fill_template(pattern: &str, id: &str, bandwidth: &str, number: u64, time: u64) -> String {
    let mut output = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('$') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('$') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let token = &after[..end];
        let (name, width) = match token.split_once("%0") {
            Some((name, format)) => (name, format.trim_end_matches('d').parse::<usize>().ok()),
            None => (token, None),
        };
        let value = match name {
            "" => "$".to_owned(),
            "RepresentationID" => id.to_owned(),
            "Bandwidth" => bandwidth.to_owned(),
            "Number" => number.to_string(),
            "Time" => time.to_string(),
            _ => format!("${token}$"),
        };
        match width {
            Some(width) => output.push_str(&format!("{value:0>width$}")),
            None => output.push_str(&value),
        }
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    output
}

/// `a-b` (inclusive) as `(length, offset)`.
fn parse_range(value: &str) -> Option<(u64, u64)> {
    let (start, end) = value.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    (end >= start).then(|| (end - start + 1, start))
}

/// ISO 8601 durations as DASH uses them: `PT1H2M3.5S`, `P1DT2H`.
fn parse_duration(value: &str) -> Option<f64> {
    let value = value.strip_prefix('P')?;
    let (days, time) = match value.split_once('T') {
        Some((days, time)) => (days, time),
        None => (value, ""),
    };
    let mut seconds = 0.0;
    if !days.is_empty() {
        seconds += days.strip_suffix('D')?.parse::<f64>().ok()? * 86_400.0;
    }
    let mut number = String::new();
    for character in time.chars() {
        match character {
            'H' => seconds += number.parse::<f64>().ok()? * 3600.0,
            'M' => seconds += number.parse::<f64>().ok()? * 60.0,
            'S' => seconds += number.parse::<f64>().ok()?,
            _ => {
                number.push(character);
                continue;
            }
        }
        number.clear();
    }
    Some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://cdn.example.com/movie/manifest.mpd").unwrap()
    }

    const TEMPLATE: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT9.5S">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate timescale="1000" duration="4000" startNumber="1"
        initialization="$RepresentationID$/init.mp4" media="$RepresentationID$/seg-$Number%03d$.m4s"/>
      <Representation id="v360" bandwidth="800000" width="640" height="360"/>
      <Representation id="v720" bandwidth="2400000" width="1280" height="720"/>
    </AdaptationSet>
    <AdaptationSet mimeType="audio/mp4">
      <Representation id="a128" bandwidth="128000">
        <BaseURL>audio/</BaseURL>
        <SegmentTemplate timescale="48000" initialization="init.mp4" media="t$Time$.m4s">
          <SegmentTimeline><S t="0" d="96000" r="1"/><S d="48000"/></SegmentTimeline>
        </SegmentTemplate>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;

    #[test]
    fn templates_and_timelines_expand_to_every_segment() {
        let manifest = parse_manifest(&base(), TEMPLATE).unwrap();
        assert_eq!(manifest.video.len(), 2);
        let chosen = manifest.choose(None).unwrap();
        assert_eq!(chosen.len(), 2, "picture and sound");
        let video = chosen[0];
        assert_eq!(video.height, Some(720));
        assert_eq!(
            video.init.as_ref().unwrap().0,
            "https://cdn.example.com/movie/v720/init.mp4"
        );
        assert_eq!(video.segments.len(), 3, "9.5 s in 4 s segments");
        assert_eq!(
            video.segments[2].0,
            "https://cdn.example.com/movie/v720/seg-003.m4s"
        );

        let audio = chosen[1];
        assert_eq!(audio.kind, TrackKind::Audio);
        assert_eq!(
            audio
                .segments
                .iter()
                .map(|part| part.0.rsplit('/').next().unwrap())
                .collect::<Vec<_>>(),
            vec!["t0.m4s", "t96000.m4s", "t192000.m4s"]
        );
        assert_eq!(
            audio.init.as_ref().unwrap().0,
            "https://cdn.example.com/movie/audio/init.mp4"
        );

        assert_eq!(manifest.choose(Some(480)).unwrap()[0].height, Some(360));
    }

    #[test]
    fn segment_lists_and_single_files_are_understood() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT10S"><Period>
          <BaseURL>https://files.example.com/</BaseURL>
          <AdaptationSet mimeType="video/mp4" codecs="avc1.4d401f,mp4a.40.2">
            <Representation id="both" bandwidth="1000">
              <BaseURL>movie.mp4</BaseURL>
              <SegmentList>
                <Initialization range="0-99"/>
                <SegmentURL mediaRange="100-599"/>
                <SegmentURL mediaRange="600-999"/>
              </SegmentList>
            </Representation>
          </AdaptationSet>
          <AdaptationSet mimeType="audio/mp4">
            <Representation id="whole" bandwidth="64000"><BaseURL>audio.m4a</BaseURL><SegmentBase indexRange="0-10"/></Representation>
          </AdaptationSet>
        </Period></MPD>"#;
        let manifest = parse_manifest(&base(), text).unwrap();
        let muxed = &manifest.muxed[0];
        assert_eq!(
            muxed.init,
            Some((
                "https://files.example.com/movie.mp4".to_owned(),
                Some((100, 0))
            ))
        );
        assert_eq!(
            muxed.segments[1],
            (
                "https://files.example.com/movie.mp4".to_owned(),
                Some((400, 600))
            )
        );
        assert_eq!(
            manifest.audio[0].segments,
            vec![("https://files.example.com/audio.m4a".to_owned(), None)]
        );
        assert_eq!(
            manifest.choose(None).unwrap().len(),
            1,
            "muxed needs no second track"
        );
    }

    #[test]
    fn protected_live_and_multi_period_manifests_are_refused() {
        let protected = r#"<MPD type="static" mediaPresentationDuration="PT4S"><Period>
          <AdaptationSet mimeType="video/mp4"><ContentProtection schemeIdUri="urn:uuid:edef8ba9"/>
            <Representation id="v" bandwidth="1"><SegmentTemplate duration="4" media="$Number$.m4s"/></Representation>
          </AdaptationSet></Period></MPD>"#;
        assert_eq!(parse_manifest(&base(), protected), Err(HlsError::Protected));

        let live = r#"<MPD type="dynamic"><Period/></MPD>"#;
        assert_eq!(parse_manifest(&base(), live), Err(HlsError::Live));

        let two = r#"<MPD type="static"><Period/><Period/></MPD>"#;
        assert!(matches!(
            parse_manifest(&base(), two),
            Err(HlsError::Malformed(_))
        ));
        assert!(matches!(
            parse_manifest(&base(), "<html/>"),
            Err(HlsError::Malformed(_))
        ));
    }

    #[test]
    fn durations_and_templates_follow_the_standard() {
        assert_eq!(parse_duration("PT1H2M3.5S"), Some(3723.5));
        assert_eq!(parse_duration("P1DT1S"), Some(86_401.0));
        assert_eq!(parse_duration("PT9.5S"), Some(9.5));
        assert_eq!(
            fill_template("$RepresentationID$_$Number%05d$_$$.m4s", "v1", "5", 42, 0),
            "v1_00042_$.m4s"
        );
        assert_eq!(fill_template("t$Time$.m4s", "", "", 0, 900), "t900.m4s");
        assert_eq!(parse_range("100-599"), Some((500, 100)));
    }
}
