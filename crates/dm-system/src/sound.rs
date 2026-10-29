//! A short sound when a download ends, using the sounds the user chose in
//! Windows for "Asterisk" (finished) and "Critical Stop" (failed), so it
//! fits the rest of their system. On Linux the desktop's sound theme names
//! the same two events. Plays in the background; nothing waits.

/// Which sound to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chime {
    Finished,
    Failed,
}

impl Chime {
    /// The Windows sound event it uses.
    pub const fn alias(self) -> &'static str {
        match self {
            Self::Finished => "SystemAsterisk",
            Self::Failed => "SystemHand",
        }
    }

    /// The freedesktop sound-theme event it uses.
    pub const fn theme_event(self) -> &'static str {
        match self {
            Self::Finished => "complete",
            Self::Failed => "dialog-error",
        }
    }
}

/// Plays `chime` without waiting for it to finish. Does nothing where the
/// system has no sound for it, or when the user turned that sound off.
pub fn play(chime: Chime) {
    #[cfg(target_os = "linux")]
    platform::play(chime.theme_event());
    #[cfg(not(target_os = "linux"))]
    platform::play(chime.alias());
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};

    pub fn play(alias: &str) {
        let name: Vec<u16> = alias.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `name` is a NUL-terminated UTF-16 string; SND_ASYNC copies
        // what it needs before returning.
        unsafe {
            PlaySoundW(
                name.as_ptr(),
                std::ptr::null_mut(),
                SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::process::{Command, Stdio};

    /// `canberra-gtk-play` follows the desktop's sound theme and its
    /// "event sounds" switch. Where it is not installed nothing plays.
    pub fn play(event: &str) {
        let started = Command::new("canberra-gtk-play")
            .args(["--id", event, "--description", "Ratatoskr"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = started {
            // Reaped on a thread of its own so no zombie is left behind.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    pub fn play(_alias: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_ending_has_its_windows_sound() {
        assert_eq!(Chime::Finished.alias(), "SystemAsterisk");
        assert_eq!(Chime::Failed.alias(), "SystemHand");
        assert_eq!(Chime::Finished.theme_event(), "complete");
        assert_eq!(Chime::Failed.theme_event(), "dialog-error");
        play(Chime::Finished);
    }
}
