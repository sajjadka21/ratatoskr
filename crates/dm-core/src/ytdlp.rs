//! yt-dlp: videos from YouTube and similar sites, whose pages are not files
//! a plain HTTP download could fetch.
//!
//! The application keeps its own copy up to date (see `install_latest` and
//! `self_update`), and also uses one the user chose in Settings or has on
//! `PATH`. It is run with an argument list, never through a shell. It works
//! in a private folder next to the destination, so pausing (stopping the
//! program) leaves its partial files for the next run to continue, and
//! cancelling removes that folder only.

use crate::control::TaskControl;
use reqwest::Url;
use serde::Serialize;
use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};

/// Marks the lines this application asked yt-dlp to print.
const PROGRESS_TAG: &str = "RATATOSK|";
const FILE_TAG: &str = "RATATOSK_FILE|";
/// The prefix of the private working folder next to the destination.
pub const WORK_DIR_PREFIX: &str = ".ratatosk-";
/// Most of yt-dlp's error output kept for the message.
const MAX_ERROR_CHARS: usize = 500;

/// Sites whose pages go to yt-dlp. A direct link to a file on these hosts
/// (one that ends in a media extension) still downloads normally. Where a
/// site also has pages without video (posts, profiles), only the listed kinds
/// of path are taken.
const SITES: &[(&str, &[&str])] = &[
    ("youtube.com", &[]),
    ("youtu.be", &[]),
    ("youtube-nocookie.com", &[]),
    ("vimeo.com", &[]),
    ("dailymotion.com", &[]),
    ("twitch.tv", &[]),
    ("pornhub.com", &[]),
    ("pinterest.com", &["/pin/"]),
    ("x.com", &["/status/"]),
    ("twitter.com", &["/status/"]),
    ("instagram.com", &["/p/", "/reel/", "/reels/", "/tv/"]),
    (
        "facebook.com",
        &["/videos/", "/watch", "/reel/", "/share/v/", "/share/r/"],
    ),
    ("fb.watch", &[]),
    ("tiktok.com", &["/video/", "/t/"]),
    ("vm.tiktok.com", &[]),
    ("reddit.com", &["/comments/"]),
    ("v.redd.it", &[]),
    ("soundcloud.com", &[]),
    ("aparat.com", &["/v/"]),
    ("bilibili.com", &["/video/"]),
];

const FILE_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a", "aac", "ogg", "opus", "flac", "wav", "m3u8",
    "mpd", "ts", "zip", "rar", "7z", "exe", "msi", "iso", "pdf", "jpg", "jpeg", "png", "gif",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum YtDlpError {
    #[error("yt-dlp could not be started: {0}")]
    Start(String),
    #[error("yt-dlp could not download the video: {0}")]
    Failed(String),
    #[error("yt-dlp finished without naming the file it made")]
    NoFile,
    #[error("stopped")]
    Stopped,
    /// Stopped to start again with a different speed limit.
    #[error("restarting with a new speed limit")]
    Interrupted,
}

impl YtDlpError {
    /// Failures that another attempt may well get past: the network or the
    /// site being busy, not a private or removed video.
    pub fn is_temporary(&self) -> bool {
        match self {
            Self::Failed(message) => {
                let message = message.to_ascii_lowercase();
                [
                    "timed out",
                    "timeout",
                    "connection",
                    "temporarily",
                    "http error 429",
                    "http error 500",
                    "http error 502",
                    "http error 503",
                    "http error 504",
                    "unable to download webpage",
                    "network",
                ]
                .iter()
                .any(|needle| message.contains(needle))
            }
            _ => false,
        }
    }
}

/// How yt-dlp reaches the network, following the application's setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyChoice {
    /// Leave it to yt-dlp, which reads the operating system's proxy.
    System,
    /// Straight to the site, whatever the system says.
    Direct,
    /// This proxy (http, https or socks5).
    Url(String),
}

/// One run of yt-dlp.
#[derive(Debug, Clone)]
pub struct YtDlpRequest<'a> {
    pub url: &'a str,
    /// The private working folder; the finished file is left in it.
    pub work_dir: &'a Path,
    pub max_height: Option<u32>,
    /// Sound only, no picture.
    pub audio_only: bool,
    pub ffmpeg: Option<&'a Path>,
    pub proxy: ProxyChoice,
    /// Bytes per second.
    pub rate_limit: Option<u64>,
}

/// A progress line. yt-dlp downloads picture and sound one after the other,
/// so `downloaded` starts again from zero for the second part.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct YtDlpProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub bytes_per_second: Option<f64>,
    pub eta_seconds: Option<u64>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Line {
    Progress(YtDlpProgress),
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtDlp {
    path: PathBuf,
}

/// Whether a link is a page that yt-dlp should handle rather than a file.
pub fn handles(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    let path = url.path().to_ascii_lowercase();
    let known = SITES.iter().any(|(site, paths)| {
        (host == *site || host.ends_with(&format!(".{site}")))
            && (paths.is_empty() || paths.iter().any(|part| path.contains(part)))
    });
    if !known {
        return false;
    }
    let extension = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase());
    !extension.is_some_and(|extension| FILE_EXTENSIONS.contains(&extension.as_str()))
}

/// The quality chosen for one download, carried in the link's fragment
/// (`#rud-quality=720`, `best` or `audio`), which is never sent to a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityChoice {
    Best,
    Height(u32),
    AudioOnly,
}

impl QualityChoice {
    pub fn from_link(url: &str) -> Option<Self> {
        let fragment = url.split_once('#')?.1;
        let value = fragment
            .split('&')
            .find_map(|pair| pair.strip_prefix("rud-quality="))?;
        match value {
            "best" => Some(Self::Best),
            "audio" => Some(Self::AudioOnly),
            height => height
                .parse()
                .ok()
                .filter(|height| *height > 0)
                .map(Self::Height),
        }
    }
}

/// The link without its fragment, which only this application reads.
pub fn without_fragment(url: &str) -> &str {
    url.split_once('#').map_or(url, |(base, _)| base)
}

/// The format yt-dlp is asked for. Without FFmpeg only formats that already
/// hold picture and sound together can be used.
pub fn format_selector(max_height: Option<u32>, has_ffmpeg: bool, audio_only: bool) -> String {
    if audio_only {
        return "ba[ext=m4a]/ba/b".to_owned();
    }
    let cap = max_height
        .map(|height| format!("[height<={height}]"))
        .unwrap_or_default();
    if has_ffmpeg {
        format!("bv*{cap}+ba/b{cap}/bv*+ba/b")
    } else {
        format!("b{cap}/b")
    }
}

impl YtDlp {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// yt-dlp as configured (while that file exists), or else the first
    /// found in `tool_dirs` (the app's own kept-up-to-date copy, then the one
    /// shipped with it), next to the application, or on `PATH`.
    pub fn locate(configured: Option<&Path>, tool_dirs: &[PathBuf]) -> Option<Self> {
        // A chosen copy that has since been moved or deleted falls back to
        // the others rather than turning every video download into an error.
        if let Some(path) = configured.filter(|path| path.is_file()) {
            return Some(Self::new(path));
        }
        let beside_app = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let on_path = env::var_os("PATH")
            .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
            .unwrap_or_default();
        tool_dirs
            .iter()
            .cloned()
            .chain(beside_app)
            .chain(on_path)
            .map(|directory| directory.join(program_name()))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
    }

    /// Updates this copy in place with yt-dlp's own updater (`-U`), which
    /// verifies what it downloads. Returns the version afterwards.
    pub async fn self_update(&self, proxy: &ProxyChoice) -> Result<Option<String>, YtDlpError> {
        let mut command = self.command();
        command.arg("-U");
        push_proxy(&mut command, proxy);
        let output = command
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|error| YtDlpError::Start(error.to_string()))?;
        if !output.status.success() {
            return Err(YtDlpError::Failed(error_summary(
                &String::from_utf8_lossy(&output.stderr),
                output.status,
            )));
        }
        Ok(self.version().await)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `yt-dlp --version`, a date such as `2026.08.01`.
    pub async fn version(&self) -> Option<String> {
        let mut command = self.command();
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        let output = command.output().await.ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty() && line.len() < 40)
    }

    /// The arguments of one run, apart from the program itself.
    pub fn arguments(request: &YtDlpRequest<'_>) -> Vec<std::ffi::OsString> {
        let mut arguments: Vec<std::ffi::OsString> = vec![
            "--newline".into(),
            "--no-color".into(),
            "--no-playlist".into(),
            "--no-mtime".into(),
            "--continue".into(),
            "--encoding".into(),
            "utf-8".into(),
            "--progress".into(),
            "--progress-template".into(),
            format!(
                "download:{PROGRESS_TAG}%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.title)s"
            )
            .into(),
            "--print".into(),
            format!("after_move:{FILE_TAG}%(filepath)s").into(),
            "--paths".into(),
            request.work_dir.as_os_str().to_owned(),
            "--output".into(),
            "%(title).150B [%(id)s].%(ext)s".into(),
            "--format".into(),
            format_selector(
                request.max_height,
                request.ffmpeg.is_some(),
                request.audio_only,
            )
            .into(),
            // A playlist link must never download a whole list into one task.
            "--playlist-items".into(),
            "1".into(),
        ];
        if let Some(ffmpeg) = request.ffmpeg {
            arguments.push("--ffmpeg-location".into());
            arguments.push(ffmpeg.as_os_str().to_owned());
            if !request.audio_only {
                arguments.push("--merge-output-format".into());
                arguments.push("mp4/mkv".into());
            }
        }
        match &request.proxy {
            ProxyChoice::System => {}
            ProxyChoice::Direct => {
                arguments.push("--proxy".into());
                arguments.push("".into());
            }
            ProxyChoice::Url(url) => {
                arguments.push("--proxy".into());
                arguments.push(url.into());
            }
        }
        if let Some(limit) = request.rate_limit.filter(|limit| *limit > 0) {
            arguments.push("--limit-rate".into());
            arguments.push(limit.to_string().into());
        }
        // Everything after `--` is a link, never an option.
        arguments.push("--".into());
        arguments.push(without_fragment(request.url).into());
        arguments
    }

    /// Downloads the video into `request.work_dir` and returns the finished
    /// file. A pause or cancel stops the program and returns `Stopped`.
    pub async fn download<F>(
        &self,
        request: &YtDlpRequest<'_>,
        control: &TaskControl,
        on_progress: F,
    ) -> Result<PathBuf, YtDlpError>
    where
        F: FnMut(YtDlpProgress) + Send,
    {
        self.download_until(request, control, std::future::pending(), on_progress)
            .await
    }

    /// Like `download`, but stops (keeping the partial files, as a pause
    /// does) and returns `Interrupted` as soon as `interrupt` completes.
    pub async fn download_until<F, I>(
        &self,
        request: &YtDlpRequest<'_>,
        control: &TaskControl,
        interrupt: I,
        mut on_progress: F,
    ) -> Result<PathBuf, YtDlpError>
    where
        F: FnMut(YtDlpProgress) + Send,
        I: std::future::Future<Output = ()> + Send,
    {
        tokio::fs::create_dir_all(request.work_dir)
            .await
            .map_err(|error| YtDlpError::Start(error.to_string()))?;
        let mut command = self.command();
        command
            .args(Self::arguments(request))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|error| YtDlpError::Start(error.to_string()))?;

        let stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let errors = tokio::spawn(async move {
            let mut bytes = Vec::new();
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.read_to_end(&mut bytes).await;
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });

        let mut finished: Option<PathBuf> = None;
        let read = async {
            let Some(stdout) = stdout else {
                return;
            };
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match parse_line(&line) {
                    Some(Line::Progress(progress)) => on_progress(progress),
                    Some(Line::File(path)) => finished = Some(path),
                    None => {}
                }
            }
        };

        let status = tokio::select! {
            biased;
            _ = control.stopped() => {
                kill_tree(&mut child).await;
                return Err(YtDlpError::Stopped);
            }
            _ = interrupt => {
                kill_tree(&mut child).await;
                return Err(YtDlpError::Interrupted);
            }
            status = async {
                read.await;
                child.wait().await
            } => status,
        };
        let status = status.map_err(|error| YtDlpError::Start(error.to_string()))?;
        let errors = errors.await.unwrap_or_default();
        if !status.success() {
            return Err(YtDlpError::Failed(error_summary(&errors, status)));
        }
        let file = finished.ok_or(YtDlpError::NoFile)?;
        // A file outside the working folder would not be ours to move.
        let file = if file.is_absolute() {
            file
        } else {
            request.work_dir.join(file)
        };
        if !file.starts_with(request.work_dir) || !file.is_file() {
            return Err(YtDlpError::NoFile);
        }
        Ok(file)
    }

    /// What a link holds, without downloading it: a video's title and the
    /// qualities it comes in, or the videos of a playlist.
    pub async fn probe(
        &self,
        url: &str,
        proxy: &ProxyChoice,
        has_ffmpeg: bool,
    ) -> Result<VideoProbe, YtDlpError> {
        let mut info = self.dump(without_fragment(url), proxy).await?;
        // A channel lists its tabs; its videos are one level further in.
        if let Some(tab) = channel_tab(&info) {
            let mut inner = self.dump(&tab, proxy).await?;
            if inner.get("title").is_none()
                && let Some(title) = info.get("title").cloned()
            {
                inner["title"] = title;
            }
            info = inner;
        }
        Ok(VideoProbe::from_info(&info, has_ffmpeg))
    }

    /// yt-dlp's description of a link, without downloading it. Gives up (and
    /// stops yt-dlp with everything it started) after `PROBE_TIMEOUT`.
    async fn dump(&self, url: &str, proxy: &ProxyChoice) -> Result<serde_json::Value, YtDlpError> {
        let mut command = self.command();
        command.args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-playlist",
            "--no-warnings",
            "--no-color",
            "--encoding",
            "utf-8",
        ]);
        push_proxy(&mut command, proxy);
        command
            .arg("--")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|error| YtDlpError::Start(error.to_string()))?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let output = tokio::spawn(async move {
            let mut bytes = Vec::new();
            if let Some(stdout) = stdout.as_mut() {
                let _ = stdout.read_to_end(&mut bytes).await;
            }
            bytes
        });
        let errors = tokio::spawn(async move {
            let mut bytes = Vec::new();
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.read_to_end(&mut bytes).await;
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let status = match tokio::time::timeout(PROBE_TIMEOUT, child.wait()).await {
            Ok(status) => status.map_err(|error| YtDlpError::Start(error.to_string()))?,
            Err(_) => {
                kill_tree(&mut child).await;
                return Err(YtDlpError::Failed(
                    "the site did not answer in time".to_owned(),
                ));
            }
        };
        let output = output.await.unwrap_or_default();
        if !status.success() {
            return Err(YtDlpError::Failed(error_summary(
                &errors.await.unwrap_or_default(),
                status,
            )));
        }
        serde_json::from_slice(&output).map_err(|_| {
            YtDlpError::Failed("yt-dlp gave an answer that could not be read".to_owned())
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.path);
        command
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1");
        #[cfg(windows)]
        {
            // No console window flashing up behind the application.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }
}

/// How long looking a link up may take before giving up.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// At most this many videos of a playlist are listed.
const MAX_PLAYLIST_ENTRIES: usize = 500;

fn push_proxy(command: &mut Command, proxy: &ProxyChoice) {
    match proxy {
        ProxyChoice::System => {}
        ProxyChoice::Direct => {
            command.args(["--proxy", ""]);
        }
        ProxyChoice::Url(url) => {
            command.args(["--proxy", url]);
        }
    }
}

/// Stops yt-dlp together with anything it started (FFmpeg joining picture and
/// sound), so no helper keeps running and holding the files open.
async fn kill_tree(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(id) = child.id() {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &id.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .await;
    }
    let _ = child.kill().await;
}

/// One quality a video comes in, with roughly how large it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityOption {
    pub height: u32,
    pub bytes: Option<u64>,
}

/// One video of a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub url: String,
    pub title: Option<String>,
}

/// What a video link holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoProbe {
    pub title: Option<String>,
    pub duration_seconds: Option<u64>,
    /// Highest first. Empty for a playlist.
    pub qualities: Vec<QualityOption>,
    /// The size of the sound alone, when it can be downloaded separately.
    pub audio_bytes: Option<u64>,
    /// The videos, when the link is a playlist or channel.
    pub entries: Vec<PlaylistEntry>,
    pub is_playlist: bool,
}

impl VideoProbe {
    fn from_info(info: &serde_json::Value, has_ffmpeg: bool) -> Self {
        let text = |value: &serde_json::Value, key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        let title = text(info, "title");
        let is_playlist = info
            .get("_type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind == "playlist");
        if is_playlist {
            let entries = info
                .get("entries")
                .and_then(serde_json::Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| {
                            let url = entry_link(entry)?;
                            Some(PlaylistEntry {
                                url,
                                title: text(entry, "title"),
                            })
                        })
                        .take(MAX_PLAYLIST_ENTRIES)
                        .collect()
                })
                .unwrap_or_default();
            return Self {
                title,
                duration_seconds: None,
                qualities: Vec::new(),
                audio_bytes: None,
                entries,
                is_playlist: true,
            };
        }

        let duration = info
            .get("duration")
            .and_then(serde_json::Value::as_f64)
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0);
        let formats: Vec<&serde_json::Value> = info
            .get("formats")
            .and_then(serde_json::Value::as_array)
            .map(|formats| formats.iter().collect())
            .unwrap_or_default();
        // yt-dlp leaves a codec out when it does not know it (common for HLS
        // formats), and says "none" only when the track is absent.
        let field = |format: &serde_json::Value, key: &str| {
            format
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let height_of =
            |format: &serde_json::Value| format.get("height").and_then(serde_json::Value::as_u64);
        let has_picture = |format: &serde_json::Value| match field(format, "vcodec").as_deref() {
            Some("none") => false,
            Some(_) => true,
            None => height_of(format).is_some(),
        };
        let may_have_sound =
            |format: &serde_json::Value| field(format, "acodec").as_deref() != Some("none");
        let sound_only = |format: &serde_json::Value| {
            field(format, "acodec").is_some_and(|codec| codec != "none") && !has_picture(format)
        };
        let size = |format: &serde_json::Value| {
            format
                .get("filesize")
                .and_then(serde_json::Value::as_f64)
                .or_else(|| {
                    format
                        .get("filesize_approx")
                        .and_then(serde_json::Value::as_f64)
                })
                .or_else(|| {
                    let rate = rate(format);
                    (rate > 0.0).then(|| duration.map(|seconds| rate * 1000.0 / 8.0 * seconds))?
                })
                .filter(|bytes| bytes.is_finite() && *bytes > 0.0)
                .map(|bytes| bytes as u64)
        };

        let audio = best(formats.iter().copied().filter(|format| sound_only(format)));
        let audio_bytes = audio.and_then(size);

        let usable = |format: &serde_json::Value| {
            has_picture(format) && (has_ffmpeg || may_have_sound(format))
        };
        let mut heights: Vec<u32> = formats
            .iter()
            .copied()
            .filter(|format| usable(format))
            .filter_map(height_of)
            .filter(|height| *height >= 144)
            .map(|height| height as u32)
            .collect();
        heights.sort_unstable_by(|left, right| right.cmp(left));
        heights.dedup();

        let qualities = heights
            .into_iter()
            .map(|height| {
                let picture = best(
                    formats
                        .iter()
                        .copied()
                        .filter(|format| usable(format))
                        .filter(|format| height_of(format) == Some(u64::from(height))),
                );
                let bytes = picture.and_then(|picture| {
                    let picture_bytes = size(picture)?;
                    if may_have_sound(picture) {
                        Some(picture_bytes)
                    } else {
                        Some(picture_bytes + audio_bytes.unwrap_or(0))
                    }
                });
                QualityOption { height, bytes }
            })
            .collect();

        Self {
            title,
            duration_seconds: duration.map(|seconds| seconds.round() as u64),
            qualities,
            audio_bytes,
            entries: Vec::new(),
            is_playlist: false,
        }
    }
}

/// A playlist entry's page. Some sites give only the video's id as `url`;
/// then the page link, or a YouTube link built from the id, is used.
fn entry_link(entry: &serde_json::Value) -> Option<String> {
    let web = |key: &str| {
        entry
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .map(str::to_owned)
    };
    web("url").or_else(|| web("webpage_url")).or_else(|| {
        let youtube = entry
            .get("ie_key")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|key| key == "Youtube");
        let id = entry.get("id").and_then(serde_json::Value::as_str)?;
        (youtube
            && !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .then(|| format!("https://www.youtube.com/watch?v={id}"))
    })
}

/// For a channel, yt-dlp lists its tabs (videos, shorts, live) rather than
/// videos. The tab worth opening instead: "videos", or else the first.
fn channel_tab(info: &serde_json::Value) -> Option<String> {
    let entries = info.get("entries")?.as_array()?;
    let is_tab = |entry: &serde_json::Value| {
        entry
            .get("ie_key")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|key| key.ends_with("Tab"))
            || entry.get("_type").and_then(serde_json::Value::as_str) == Some("playlist")
    };
    if entries.is_empty() || !entries.iter().all(is_tab) {
        return None;
    }
    let links: Vec<String> = entries.iter().filter_map(entry_link).collect();
    links
        .iter()
        .find(|link| link.trim_end_matches('/').ends_with("/videos"))
        .or_else(|| links.first())
        .cloned()
}

/// A format's bit rate in kbit/s, 0 when not given.
fn rate(format: &serde_json::Value) -> f64 {
    format
        .get("tbr")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}

/// The format with the highest bit rate: the one yt-dlp would pick.
fn best<'a>(
    candidates: impl Iterator<Item = &'a serde_json::Value>,
) -> Option<&'a serde_json::Value> {
    candidates.max_by(|left, right| rate(left).total_cmp(&rate(right)))
}

/// The program's file name on this system.
pub const fn program_name() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

/// Where yt-dlp's official releases are published.
pub const RELEASE_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
/// yt-dlp is about 18 MB; anything much larger is not it.
const MAX_PROGRAM_BYTES: usize = 64 * 1024 * 1024;

/// The SHA-256 the release lists for `asset` in its `SHA2-256SUMS` file.
pub fn listed_checksum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Downloads the latest official yt-dlp to `target`, through the app's own
/// network route, and keeps it only if it matches the checksum the release
/// publishes.
pub async fn install_latest(
    downloader: &crate::Downloader,
    target: &Path,
) -> Result<(), YtDlpError> {
    use sha2::{Digest, Sha256};
    let control = TaskControl::new();
    let failed = |error: crate::DownloadError| YtDlpError::Failed(error.redacted_message());
    let sums = downloader
        .fetch_bytes(
            &format!("{RELEASE_BASE}/SHA2-256SUMS"),
            None,
            64 * 1024,
            &control,
        )
        .await
        .map_err(failed)?;
    let expected = listed_checksum(&String::from_utf8_lossy(&sums), program_name())
        .ok_or_else(|| YtDlpError::Failed("the release lists no checksum for yt-dlp".to_owned()))?;
    let program = downloader
        .fetch_bytes(
            &format!("{RELEASE_BASE}/{}", program_name()),
            None,
            MAX_PROGRAM_BYTES,
            &control,
        )
        .await
        .map_err(failed)?;
    let actual = format!("{:x}", Sha256::digest(&program));
    if actual != expected {
        return Err(YtDlpError::Failed(
            "the downloaded yt-dlp does not match its published checksum".to_owned(),
        ));
    }
    write_program(target, &program)
        .await
        .map_err(|error| YtDlpError::Start(error.to_string()))
}

/// Puts a program in place without ever leaving a half-written one.
pub async fn write_program(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut partial = target.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    tokio::fs::write(&partial, bytes).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755)).await?;
    }
    let _ = tokio::fs::remove_file(target).await;
    tokio::fs::rename(&partial, target).await
}

fn parse_line(line: &str) -> Option<Line> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix(FILE_TAG) {
        let path = rest.trim();
        return (!path.is_empty()).then(|| Line::File(PathBuf::from(path)));
    }
    let rest = line.trim_start().strip_prefix(PROGRESS_TAG)?;
    let mut fields = rest.splitn(6, '|');
    let downloaded = number(fields.next()?)? as u64;
    let total = fields.next().and_then(number);
    let estimate = fields.next().and_then(number);
    let speed = fields.next().and_then(number);
    let eta = fields.next().and_then(number);
    let title = fields
        .next()
        .map(str::trim)
        .filter(|title| !title.is_empty() && *title != "NA")
        .map(str::to_owned);
    Some(Line::Progress(YtDlpProgress {
        downloaded,
        total: total.or(estimate).map(|value| value as u64),
        bytes_per_second: speed,
        eta_seconds: eta.map(|value| value as u64),
        title,
    }))
}

fn number(field: &str) -> Option<f64> {
    field
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn error_summary(errors: &str, status: std::process::ExitStatus) -> String {
    let mut lines: Vec<&str> = errors
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("ERROR"))
        .collect();
    if lines.is_empty() {
        lines = errors
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("WARNING"))
            .collect();
    }
    let mut message = lines.join(" ");
    if message.chars().count() > MAX_ERROR_CHARS {
        message = message.chars().take(MAX_ERROR_CHARS).collect::<String>() + "…";
    }
    if message.is_empty() {
        message = format!("exit status {status}");
    }
    message
}

/// Whether `path` is one of this application's yt-dlp working folders, the
/// only kind of folder cancelling may remove.
pub fn is_work_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(WORK_DIR_PREFIX) && name.len() > WORK_DIR_PREFIX.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn video_pages_go_to_ytdlp_and_files_do_not() {
        assert!(handles("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(handles("https://youtu.be/dQw4w9WgXcQ"));
        assert!(handles("https://m.youtube.com/shorts/abc"));
        assert!(handles("https://www.aparat.com/v/abc12"));
        assert!(handles(
            "https://www.pornhub.com/view_video.php?viewkey=6abbc213c20ff"
        ));
        assert!(handles("https://www.pinterest.com/pin/123456789/"));
        assert!(!handles("https://example.com/watch?v=1"));
        assert!(!handles("https://notyoutube.com/watch?v=1"));
        assert!(!handles("https://cdn.aparat.com/video/file.mp4"));
        assert!(!handles("https://cdn.pornhub.com/video/file.mp4"));
        assert!(!handles("https://i.pinimg.com/originals/image.jpg"));
        assert!(!handles("ftp://youtube.com/watch"));
        assert!(!handles("not a link"));
    }

    #[test]
    fn the_format_follows_quality_and_ffmpeg() {
        assert_eq!(format_selector(None, true, false), "bv*+ba/b/bv*+ba/b");
        assert_eq!(
            format_selector(Some(720), true, false),
            "bv*[height<=720]+ba/b[height<=720]/bv*+ba/b"
        );
        assert_eq!(format_selector(Some(480), false, false), "b[height<=480]/b");
        assert_eq!(format_selector(Some(480), true, true), "ba[ext=m4a]/ba/b");
    }

    #[test]
    fn the_quality_is_read_from_the_fragment() {
        let choice = QualityChoice::from_link;
        assert_eq!(
            choice("https://youtu.be/x#rud-quality=720"),
            Some(QualityChoice::Height(720))
        );
        assert_eq!(
            choice("https://youtu.be/x#rud-quality=best"),
            Some(QualityChoice::Best)
        );
        assert_eq!(
            choice("https://youtu.be/x#rud-quality=audio"),
            Some(QualityChoice::AudioOnly)
        );
        assert_eq!(choice("https://youtu.be/x#rud-quality=0"), None);
        assert_eq!(choice("https://youtu.be/x"), None);
        assert_eq!(
            without_fragment("https://youtu.be/x#rud-quality=720"),
            "https://youtu.be/x"
        );
    }

    #[test]
    fn only_video_pages_of_social_sites_are_taken() {
        assert!(handles("https://x.com/someone/status/123"));
        assert!(!handles("https://x.com/someone"));
        assert!(handles(
            "https://www.reddit.com/r/videos/comments/abc/title/"
        ));
        assert!(!handles("https://www.reddit.com/r/videos/"));
        assert!(handles("https://www.instagram.com/reel/abc/"));
        assert!(!handles("https://www.instagram.com/someone/"));
        assert!(handles("https://www.bilibili.com/video/BV1xx"));
        assert!(handles("https://youtube.com/playlist?list=PL1"));
    }

    #[test]
    fn a_probe_lists_qualities_with_sizes_and_playlists_with_videos() {
        let video = serde_json::json!({
            "title": "A talk",
            "duration": 100.0,
            "formats": [
                {"format_id": "140", "vcodec": "none", "acodec": "mp4a", "tbr": 128.0, "filesize": 1_600_000},
                {"format_id": "18", "vcodec": "avc1", "acodec": "mp4a", "height": 360, "tbr": 500.0, "filesize": 6_000_000},
                {"format_id": "136", "vcodec": "avc1", "acodec": "none", "height": 720, "tbr": 1500.0, "filesize_approx": 18_000_000},
                {"format_id": "247", "vcodec": "vp9", "acodec": "none", "height": 720, "tbr": 1200.0},
                {"format_id": "sb0", "vcodec": "none", "acodec": "none", "height": 90}
            ]
        });
        let probe = VideoProbe::from_info(&video, true);
        assert_eq!(probe.title.as_deref(), Some("A talk"));
        assert_eq!(probe.duration_seconds, Some(100));
        assert_eq!(probe.audio_bytes, Some(1_600_000));
        assert_eq!(
            probe.qualities,
            vec![
                QualityOption {
                    height: 720,
                    bytes: Some(19_600_000)
                },
                QualityOption {
                    height: 360,
                    bytes: Some(6_000_000)
                },
            ]
        );
        // Without FFmpeg only formats with sound built in can be used.
        let alone = VideoProbe::from_info(&video, false);
        assert_eq!(
            alone
                .qualities
                .iter()
                .map(|option| option.height)
                .collect::<Vec<_>>(),
            vec![360]
        );

        let playlist = serde_json::json!({
            "_type": "playlist",
            "title": "Talks",
            "entries": [
                {"url": "https://www.youtube.com/watch?v=a", "title": "One"},
                {"url": "not-a-link"},
                {"url": "https://www.youtube.com/watch?v=b"}
            ]
        });
        let list = VideoProbe::from_info(&playlist, true);
        assert!(list.is_playlist);
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0].title.as_deref(), Some("One"));

        // Formats whose codecs yt-dlp does not know (HLS) still count.
        let hls = serde_json::json!({
            "formats": [
                {"format_id": "hls-720", "height": 720, "tbr": 2000.0},
                {"format_id": "hls-360", "height": 360, "tbr": 700.0}
            ],
            "duration": 10.0
        });
        let found = VideoProbe::from_info(&hls, false);
        assert_eq!(
            found.qualities,
            vec![
                QualityOption {
                    height: 720,
                    bytes: Some(2_500_000)
                },
                QualityOption {
                    height: 360,
                    bytes: Some(875_000)
                },
            ]
        );

        // Bare ids become links; a channel's tabs lead to its videos tab.
        let ids = serde_json::json!({
            "_type": "playlist",
            "entries": [{"url": "abc_-1", "id": "abc_-1", "ie_key": "Youtube"}]
        });
        assert_eq!(
            VideoProbe::from_info(&ids, true).entries[0].url,
            "https://www.youtube.com/watch?v=abc_-1"
        );
        let channel = serde_json::json!({
            "_type": "playlist",
            "entries": [
                {"_type": "url", "ie_key": "YoutubeTab", "url": "https://www.youtube.com/@a/shorts"},
                {"_type": "url", "ie_key": "YoutubeTab", "url": "https://www.youtube.com/@a/videos"}
            ]
        });
        assert_eq!(
            channel_tab(&channel).as_deref(),
            Some("https://www.youtube.com/@a/videos")
        );
        assert_eq!(channel_tab(&playlist), None);
    }

    #[test]
    fn arguments_keep_the_link_after_the_options_and_follow_the_route() {
        let work = Path::new("/downloads/.ratatosk-1");
        let request = YtDlpRequest {
            url: "https://youtu.be/-x",
            work_dir: work,
            max_height: Some(1080),
            audio_only: false,
            ffmpeg: Some(Path::new("/bin/ffmpeg")),
            proxy: ProxyChoice::Url("socks5://127.0.0.1:10808".into()),
            rate_limit: Some(500_000),
        };
        let arguments: Vec<String> = YtDlp::arguments(&request)
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        let end = arguments.len();
        assert_eq!(arguments[end - 2], "--");
        assert_eq!(arguments[end - 1], "https://youtu.be/-x");
        assert_eq!(
            arguments[arguments
                .iter()
                .position(|value| value == "--playlist-items")
                .unwrap()
                + 1],
            "1"
        );
        let at = |flag: &str| arguments.iter().position(|value| value == flag).unwrap();
        assert_eq!(arguments[at("--proxy") + 1], "socks5://127.0.0.1:10808");
        assert_eq!(arguments[at("--limit-rate") + 1], "500000");
        assert_eq!(arguments[at("--paths") + 1], "/downloads/.ratatosk-1");
        assert_eq!(arguments[at("--ffmpeg-location") + 1], "/bin/ffmpeg");

        let direct = YtDlpRequest {
            proxy: ProxyChoice::Direct,
            ffmpeg: None,
            rate_limit: None,
            ..request.clone()
        };
        let arguments: Vec<String> = YtDlp::arguments(&direct)
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        let proxy = arguments
            .iter()
            .position(|value| value == "--proxy")
            .unwrap();
        assert_eq!(arguments[proxy + 1], "");
        assert!(!arguments.iter().any(|value| value == "--limit-rate"));
        assert!(!arguments.iter().any(|value| value == "--ffmpeg-location"));

        let system = YtDlpRequest {
            proxy: ProxyChoice::System,
            ..request
        };
        assert!(
            !YtDlp::arguments(&system)
                .iter()
                .any(|value| value == "--proxy")
        );
    }

    #[test]
    fn progress_and_file_lines_are_read() {
        assert_eq!(
            parse_line("RATATOSK|1048576|NA|4194304.5|250000.0|12|Some | title"),
            Some(Line::Progress(YtDlpProgress {
                downloaded: 1_048_576,
                total: Some(4_194_304),
                bytes_per_second: Some(250_000.0),
                eta_seconds: Some(12),
                title: Some("Some | title".into()),
            }))
        );
        assert_eq!(
            parse_line("  RATATOSK|10|20|NA|NA|NA|NA\r"),
            Some(Line::Progress(YtDlpProgress {
                downloaded: 10,
                total: Some(20),
                ..YtDlpProgress::default()
            }))
        );
        assert_eq!(
            parse_line("RATATOSK_FILE|C:\\Downloads\\.ratatosk-1\\ویدیو [x].mp4"),
            Some(Line::File(PathBuf::from(
                "C:\\Downloads\\.ratatosk-1\\ویدیو [x].mp4"
            )))
        );
        assert_eq!(parse_line("[youtube] Extracting URL"), None);
        assert_eq!(parse_line("RATATOSK|NA|NA|NA|NA|NA|x"), None);
    }

    #[test]
    fn errors_are_summarised_and_sorted_into_temporary_or_not() {
        let temporary = YtDlpError::Failed("ERROR: unable to download webpage: timed out".into());
        assert!(temporary.is_temporary());
        let permanent = YtDlpError::Failed("ERROR: [youtube] x: Private video".into());
        assert!(!permanent.is_temporary());
        assert!(!YtDlpError::NoFile.is_temporary());
    }

    #[test]
    fn the_published_checksum_is_found_for_the_program() {
        let hash = "a".repeat(64);
        let sums = format!(
            "{} yt-dlp\n{}  yt-dlp.exe\n{} *yt-dlp_macos\n",
            "b".repeat(64),
            hash,
            "c".repeat(64)
        );
        assert_eq!(listed_checksum(&sums, "yt-dlp.exe"), Some(hash));
        assert_eq!(listed_checksum(&sums, "yt-dlp_macos"), Some("c".repeat(64)));
        assert_eq!(listed_checksum(&sums, "yt-dlp_x86.exe"), None);
        assert_eq!(listed_checksum("short yt-dlp.exe", "yt-dlp.exe"), None);
    }

    #[test]
    fn a_tool_folder_comes_before_the_path() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join(program_name()), b"").unwrap();
        assert_eq!(
            YtDlp::locate(None, &[directory.path().to_path_buf()]),
            Some(YtDlp::new(directory.path().join(program_name())))
        );
        // A chosen copy that is gone falls back to the others.
        assert_eq!(
            YtDlp::locate(
                Some(&directory.path().join("nope")),
                &[directory.path().to_path_buf()]
            ),
            Some(YtDlp::new(directory.path().join(program_name())))
        );
    }

    #[tokio::test]
    async fn a_program_is_replaced_whole() {
        let directory = tempdir().unwrap();
        let target = directory.path().join("tools").join(program_name());
        write_program(&target, b"one").await.unwrap();
        write_program(&target, b"two").await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");
        assert_eq!(
            std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn only_our_own_working_folders_count() {
        assert!(is_work_dir(Path::new("/d/.ratatosk-abc")));
        assert!(!is_work_dir(Path::new("/d/.ratatosk-")));
        assert!(!is_work_dir(Path::new("/d/Downloads")));
        assert!(!is_work_dir(Path::new("/")));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_run_reports_progress_and_the_finished_file() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().unwrap();
        let work = directory.path().join(".ratatosk-t");
        let script = directory.path().join("yt-dlp");
        // A stand-in that behaves like yt-dlp: progress, then the file.
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nmkdir -p '{w}'\necho 'RATATOSK|50|100|NA|10.0|5|Clip'\necho 'RATATOSK|100|100|NA|NA|0|Clip'\nprintf data > '{w}/Clip [x].mp4'\necho 'RATATOSK_FILE|{w}/Clip [x].mp4'\n",
                w = work.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let ytdlp = YtDlp::new(&script);
        let request = YtDlpRequest {
            url: "https://youtu.be/x",
            work_dir: &work,
            max_height: None,
            audio_only: false,
            ffmpeg: None,
            proxy: ProxyChoice::System,
            rate_limit: None,
        };
        let mut seen = Vec::new();
        let file = ytdlp
            .download(&request, &TaskControl::new(), |progress| {
                seen.push(progress.downloaded)
            })
            .await
            .unwrap();
        assert_eq!(seen, vec![50, 100]);
        assert_eq!(file, work.join("Clip [x].mp4"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_interrupted_run_stops_and_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().unwrap();
        let script = directory.path().join("yt-dlp");
        std::fs::write(
            &script,
            "#!/bin/sh\necho 'RATATOSK|10|100|NA|NA|NA|Clip'\nsleep 30\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let work = directory.path().join(".ratatosk-i");
        let request = YtDlpRequest {
            url: "https://youtu.be/x",
            work_dir: &work,
            max_height: None,
            audio_only: false,
            ffmpeg: None,
            proxy: ProxyChoice::System,
            rate_limit: Some(100_000),
        };
        let started = std::time::Instant::now();
        let error = YtDlp::new(&script)
            .download_until(
                &request,
                &TaskControl::new(),
                tokio::time::sleep(std::time::Duration::from_millis(300)),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, YtDlpError::Interrupted);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_run_says_why() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().unwrap();
        let script = directory.path().join("yt-dlp");
        std::fs::write(
            &script,
            "#!/bin/sh\necho 'WARNING: something' >&2\necho 'ERROR: [youtube] x: Video unavailable' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let work = directory.path().join(".ratatosk-f");
        let request = YtDlpRequest {
            url: "https://youtu.be/x",
            work_dir: &work,
            max_height: None,
            audio_only: false,
            ffmpeg: None,
            proxy: ProxyChoice::System,
            rate_limit: None,
        };
        let error = YtDlp::new(&script)
            .download(&request, &TaskControl::new(), |_| {})
            .await
            .unwrap_err();
        assert_eq!(
            error,
            YtDlpError::Failed("ERROR: [youtube] x: Video unavailable".into())
        );
    }

    #[tokio::test]
    async fn a_missing_program_is_reported_not_panicked() {
        let directory = tempdir().unwrap();
        let work = directory.path().join(".ratatosk-m");
        let request = YtDlpRequest {
            url: "https://youtu.be/x",
            work_dir: &work,
            max_height: None,
            audio_only: false,
            ffmpeg: None,
            proxy: ProxyChoice::System,
            rate_limit: None,
        };
        let error = YtDlp::new("/definitely/not/yt-dlp")
            .download(&request, &TaskControl::new(), |_| {})
            .await
            .unwrap_err();
        assert!(matches!(error, YtDlpError::Start(_)));
    }
}
