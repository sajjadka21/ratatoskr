//! Operating-system integration the engine needs but must not depend on:
//! keeping the computer awake while downloads run, and the power actions a
//! queue can take when it finishes.
//!
//! Every function is safe to call on any platform; outside Windows the power
//! actions report `Unsupported` and keep-awake does nothing.

pub mod locate;
pub mod session_channel;
pub mod sparse;

use std::{fmt, io, str::FromStr, sync::mpsc, thread};

/// What to do with the computer after a queue finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Sleep,
    Hibernate,
    Shutdown,
}

impl PowerAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Hibernate => "hibernate",
            Self::Shutdown => "shutdown",
        }
    }
}

impl fmt::Display for PowerAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PowerAction {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "sleep" => Ok(Self::Sleep),
            "hibernate" => Ok(Self::Hibernate),
            "shutdown" => Ok(Self::Shutdown),
            _ => Err(()),
        }
    }
}

/// Puts the computer to sleep, hibernates it or shuts it down. The caller is
/// responsible for asking the user first; this acts immediately.
pub fn perform(action: PowerAction) -> io::Result<()> {
    platform::perform(action)
}

/// Keeps Windows from sleeping while downloads are running.
///
/// `SetThreadExecutionState` applies to the thread that calls it, and async
/// tasks move between threads, so one dedicated thread owns the request and
/// is told when to hold or release it.
pub struct KeepAwake {
    sender: mpsc::Sender<bool>,
}

impl KeepAwake {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<bool>();

        let spawned = thread::Builder::new()
            .name("keep-awake".to_owned())
            .spawn(move || {
                let mut holding = false;
                // Ends when every sender is dropped.
                while let Ok(wanted) = receiver.recv() {
                    if wanted != holding {
                        platform::set_keep_awake(wanted);
                        holding = wanted;
                    }
                }
                if holding {
                    platform::set_keep_awake(false);
                }
            });

        if spawned.is_err() {
            // Without the thread the requests below are simply dropped; the
            // computer then sleeps as it normally would.
            let (sender, _) = mpsc::channel();
            return Self { sender };
        }

        Self { sender }
    }

    /// Requests (`true`) or releases (`false`) the keep-awake hold.
    pub fn set(&self, keep_awake: bool) {
        let _ = self.sender.send(keep_awake);
    }
}

impl Default for KeepAwake {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
mod platform {
    use super::PowerAction;
    use std::{io, os::windows::process::CommandExt, process::Command};
    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetSuspendState, SetThreadExecutionState,
    };

    /// Keeps a console window from flashing when `shutdown.exe` runs.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn perform(action: PowerAction) -> io::Result<()> {
        match action {
            PowerAction::Shutdown => {
                let status = Command::new("shutdown")
                    .args(["/s", "/t", "0"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .status()?;
                if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other("shutdown was refused"))
                }
            }
            PowerAction::Sleep | PowerAction::Hibernate => {
                let hibernate = action == PowerAction::Hibernate;
                // SAFETY: plain Win32 call with value arguments.
                let accepted = unsafe { SetSuspendState(hibernate, false, false) };
                if accepted {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            }
        }
    }

    pub fn set_keep_awake(keep_awake: bool) {
        let flags = if keep_awake {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // SAFETY: plain Win32 call; it only changes this thread's request.
        unsafe {
            SetThreadExecutionState(flags);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::PowerAction;
    use std::io;

    pub fn perform(action: PowerAction) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{action} is only supported on Windows"),
        ))
    }

    pub fn set_keep_awake(_keep_awake: bool) {}
}

#[cfg(test)]
mod tests {
    use super::{KeepAwake, PowerAction};

    #[test]
    fn power_actions_round_trip_through_their_names() {
        for action in [
            PowerAction::Sleep,
            PowerAction::Hibernate,
            PowerAction::Shutdown,
        ] {
            assert_eq!(action.as_str().parse::<PowerAction>(), Ok(action));
        }
        assert!("reboot".parse::<PowerAction>().is_err());
    }

    #[test]
    fn keep_awake_accepts_requests_and_shuts_down_cleanly() {
        let keep_awake = KeepAwake::new();
        keep_awake.set(true);
        keep_awake.set(true);
        keep_awake.set(false);
        drop(keep_awake);
    }
}
