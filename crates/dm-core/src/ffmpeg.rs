//! FFmpeg, when the user has it: joining a stream's separate picture and
//! sound into one file, and rewrapping transport streams as MP4.
//!
//! FFmpeg is never downloaded or bundled by this application. It is found
//! where the user put it — a path set in Settings, next to the application,
//! or on `PATH` — and run with an argument list (never through a shell) on
//! local files only. Only stream copying is used (`-c copy`), so nothing is
//! re-encoded and a join takes seconds, not minutes.

use crate::control::TaskControl;
use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
};
use thiserror::Error;
use tokio::{io::AsyncReadExt, process::Command};

/// Most of FFmpeg's error output kept for the message.
const MAX_ERROR_CHARS: usize = 600;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FfmpegError {
    #[error("FFmpeg could not be started: {0}")]
    Start(String),
    #[error("FFmpeg could not join the stream: {0}")]
    Failed(String),
    #[error("stopped")]
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ffmpeg {
    path: PathBuf,
}

impl Ffmpeg {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// FFmpeg as configured, or found next to the application or on `PATH`.
    pub fn locate(configured: Option<&Path>) -> Option<Self> {
        if let Some(path) = configured {
            return path.is_file().then(|| Self::new(path));
        }
        let names: &[&str] = if cfg!(windows) {
            &["ffmpeg.exe"]
        } else {
            &["ffmpeg"]
        };
        let beside_app = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let on_path = env::var_os("PATH")
            .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
            .unwrap_or_default();
        beside_app
            .into_iter()
            .chain(on_path)
            .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The first line of `ffmpeg -version`, which names the build.
    pub async fn version(&self) -> Option<String> {
        let output = Command::new(&self.path)
            .arg("-version")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(str::to_owned)
            .filter(|line| line.to_ascii_lowercase().contains("ffmpeg"))
    }

    /// Joins the picture of `video` with the sound of `audio` into an MP4
    /// at `output`, copying both streams as they are.
    pub async fn join(
        &self,
        video: &Path,
        audio: &Path,
        output: &Path,
        control: &TaskControl,
    ) -> Result<(), FfmpegError> {
        let arguments = vec![
            "-i".into(),
            video.as_os_str().to_owned(),
            "-i".into(),
            audio.as_os_str().to_owned(),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "1:a:0".into(),
        ];
        self.run(arguments, output, control).await
    }

    /// Rewraps a stream (a `.ts` file, say) as an MP4 without re-encoding.
    pub async fn remux(
        &self,
        input: &Path,
        output: &Path,
        control: &TaskControl,
    ) -> Result<(), FfmpegError> {
        let arguments = vec![
            "-i".into(),
            input.as_os_str().to_owned(),
            "-map".into(),
            "0".into(),
        ];
        self.run(arguments, output, control).await
    }

    async fn run(
        &self,
        inputs: Vec<std::ffi::OsString>,
        output: &Path,
        control: &TaskControl,
    ) -> Result<(), FfmpegError> {
        let mut command = Command::new(&self.path);
        command
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"])
            .args(inputs)
            .args(["-c", "copy", "-movflags", "+faststart", "-f", "mp4"])
            .arg(output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // No console window flashing up behind the application.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|error| FfmpegError::Start(error.to_string()))?;
        let mut stderr = child.stderr.take();
        let collect = async move {
            let mut text = String::new();
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.read_to_string(&mut text).await;
            }
            text
        };

        let (status, errors) = tokio::select! {
            biased;
            _ = control.stopped() => {
                let _ = child.kill().await;
                return Err(FfmpegError::Stopped);
            }
            result = async { tokio::join!(child.wait(), collect) } => result,
        };
        let status = status.map_err(|error| FfmpegError::Start(error.to_string()))?;
        if status.success() {
            return Ok(());
        }
        let mut message: String = errors
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if message.chars().count() > MAX_ERROR_CHARS {
            message = message.chars().take(MAX_ERROR_CHARS).collect::<String>() + "…";
        }
        if message.is_empty() {
            message = format!("exit status {status}");
        }
        Err(FfmpegError::Failed(message))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The real FFmpeg, when this machine has one. Tests that need it are
    /// skipped (and say so) where it is missing.
    pub(crate) fn real_ffmpeg() -> Option<Ffmpeg> {
        let found = Ffmpeg::locate(None);
        if found.is_none() {
            eprintln!("FFmpeg not found; skipping a test that needs it");
        }
        found
    }

    #[test]
    fn a_configured_path_must_exist() {
        let directory = tempdir().unwrap();
        assert_eq!(
            Ffmpeg::locate(Some(&directory.path().join("nope.exe"))),
            None
        );
        let fake = directory.path().join("ffmpeg.exe");
        std::fs::write(&fake, b"").unwrap();
        assert_eq!(Ffmpeg::locate(Some(&fake)), Some(Ffmpeg::new(&fake)));
    }

    #[tokio::test]
    async fn a_missing_program_is_reported_not_panicked() {
        let ffmpeg = Ffmpeg::new("/definitely/not/ffmpeg");
        let control = TaskControl::new();
        let result = ffmpeg
            .remux(Path::new("in.ts"), Path::new("out.mp4"), &control)
            .await;
        assert!(matches!(result, Err(FfmpegError::Start(_))));
        assert_eq!(ffmpeg.version().await, None);
    }

    #[tokio::test]
    async fn joins_real_picture_and_sound_into_one_file() {
        let Some(ffmpeg) = real_ffmpeg() else {
            return;
        };
        let directory = tempdir().unwrap();
        let video = directory.path().join("video.ts");
        let audio = directory.path().join("audio.ts");
        make_media(
            &ffmpeg,
            &video,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=64x48:rate=10",
                "-c:v",
                "mpeg2video",
            ],
        )
        .await;
        make_media(
            &ffmpeg,
            &audio,
            &["-f", "lavfi", "-i", "sine=duration=1", "-c:a", "aac"],
        )
        .await;

        let output = directory.path().join("joined.part");
        let control = TaskControl::new();
        ffmpeg
            .join(&video, &audio, &output, &control)
            .await
            .unwrap();

        let streams = probe_streams(&output).await;
        assert!(
            streams.contains("video") && streams.contains("audio"),
            "{streams}"
        );
        assert!(ffmpeg.version().await.unwrap().contains("ffmpeg"));
    }

    #[tokio::test]
    async fn a_broken_input_fails_with_ffmpegs_reason() {
        let Some(ffmpeg) = real_ffmpeg() else {
            return;
        };
        let directory = tempdir().unwrap();
        let broken = directory.path().join("broken.ts");
        std::fs::write(&broken, b"not a video").unwrap();
        let control = TaskControl::new();
        let result = ffmpeg
            .remux(&broken, &directory.path().join("out.part"), &control)
            .await;
        assert!(matches!(result, Err(FfmpegError::Failed(message)) if !message.is_empty()));
    }

    pub(crate) async fn make_media(ffmpeg: &Ffmpeg, output: &Path, arguments: &[&str]) {
        let status = Command::new(ffmpeg.path())
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(arguments)
            .args(["-f", "mpegts"])
            .arg(output)
            .status()
            .await
            .unwrap();
        assert!(status.success());
    }

    /// The kinds of stream in a media file, as `ffprobe` reports them.
    pub(crate) async fn probe_streams(path: &Path) -> String {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output()
            .await
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}
