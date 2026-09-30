//! A short sound when a download ends, using the sounds the user chose in
//! Windows for "Asterisk" (finished) and "Critical Stop" (failed), so it
//! fits the rest of their system. Plays in the background; nothing waits.

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
}

/// Plays `chime` without waiting for it to finish. Does nothing outside
/// Windows or when the user turned that Windows sound off.
pub fn play(chime: Chime) {
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

#[cfg(not(windows))]
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
        play(Chime::Finished);
    }
}
