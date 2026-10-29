//! Starting with Windows: a value under the current user's `Run` key, the
//! same place Windows' own "Startup apps" page lists and can turn off. No
//! administrator rights are needed and nothing outside the user's own
//! registry hive is touched.
//!
//! On Linux the same switch is an XDG autostart entry in the user's own
//! `~/.config/autostart`, which GNOME, KDE, Xfce, Cinnamon and MATE all read.

use std::{io, path::Path};

#[cfg(not(target_os = "linux"))]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(not(target_os = "linux"))]
const VALUE_NAME: &str = "Ratatosk";
/// Passed when Windows starts the app, so it opens in the tray, not on top.
pub const HIDDEN_ARGUMENT: &str = "--hidden";

/// The command Windows runs at sign-in.
pub fn command_for(application: &Path) -> String {
    format!("\"{}\" {HIDDEN_ARGUMENT}", application.display())
}

/// Whether the app starts with Windows, and from this `application`.
#[cfg(not(target_os = "linux"))]
pub fn enabled(application: &Path) -> bool {
    platform::read(RUN_KEY, VALUE_NAME).is_some_and(|command| command == command_for(application))
}

/// Turns starting with Windows on (pointing at `application`) or off.
#[cfg(not(target_os = "linux"))]
pub fn set(application: &Path, on: bool) -> io::Result<()> {
    if on {
        platform::write(RUN_KEY, VALUE_NAME, &command_for(application))
    } else {
        platform::delete(RUN_KEY, VALUE_NAME)
    }
}

/// Whether the app starts at sign-in, and from this `application`.
#[cfg(target_os = "linux")]
pub fn enabled(application: &Path) -> bool {
    xdg::config_home().is_some_and(|config| xdg::enabled_in(&config, application))
}

/// Turns starting at sign-in on (pointing at `application`) or off.
#[cfg(target_os = "linux")]
pub fn set(application: &Path, on: bool) -> io::Result<()> {
    let config = xdg::config_home()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?;
    xdg::set_in(&config, application, on)
}

/// The XDG autostart entry. Kept apart from the environment so tests can
/// point it at a temporary folder.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod xdg {
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    const ENTRY: &str = "ratatoskr.desktop";

    pub fn config_home() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }

    /// An AppImage runs from a temporary mount; the file to start later is
    /// the image itself.
    fn launcher(application: &Path) -> PathBuf {
        std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .unwrap_or_else(|| application.to_path_buf())
    }

    /// One `Exec` argument, quoted as the Desktop Entry specification asks.
    fn quote(argument: &str) -> String {
        let mut quoted = String::from('"');
        for character in argument.chars() {
            if matches!(character, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(character);
        }
        quoted.push('"');
        // `%` introduces a field code in Exec and must be doubled.
        quoted.replace('%', "%%")
    }

    pub fn exec_line(application: &Path) -> String {
        format!(
            "{} {}",
            quote(&launcher(application).to_string_lossy()),
            super::HIDDEN_ARGUMENT
        )
    }

    fn entry(application: &Path) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Ratatoskr\nComment=Download manager\n\
             Exec={}\nIcon=ratatoskr\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exec_line(application)
        )
    }

    pub fn enabled_in(config: &Path, application: &Path) -> bool {
        let wanted = format!("Exec={}", exec_line(application));
        fs::read_to_string(config.join("autostart").join(ENTRY)).is_ok_and(|text| {
            text.lines().any(|line| line.trim() == wanted)
                && !text.lines().any(|line| {
                    matches!(
                        line.trim(),
                        "Hidden=true" | "X-GNOME-Autostart-enabled=false"
                    )
                })
        })
    }

    pub fn set_in(config: &Path, application: &Path, on: bool) -> io::Result<()> {
        let folder = config.join("autostart");
        let path = folder.join(ENTRY);
        if !on {
            return match fs::remove_file(&path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        fs::create_dir_all(&folder)?;
        let partial = folder.join(format!("{ENTRY}.partial"));
        fs::write(&partial, entry(application))?;
        fs::rename(&partial, &path)
    }
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn write(key: &str, name: &str, value: &str) -> io::Result<()> {
        let key = wide(key);
        let name = wide(name);
        let value = wide(value);
        let mut handle: HKEY = null_mut();
        // SAFETY: valid NUL-terminated strings and an out pointer for the key.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                null(),
                &mut handle,
                null_mut(),
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        // SAFETY: `handle` is open for writing; the data is UTF-16 text with
        // its terminating NUL, as REG_SZ requires.
        let status = unsafe {
            RegSetValueExW(
                handle,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        // SAFETY: closes the key opened above, once.
        unsafe {
            RegCloseKey(handle);
        }
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(())
    }

    pub fn delete(key: &str, name: &str) -> io::Result<()> {
        let key = wide(key);
        let name = wide(name);
        // SAFETY: valid NUL-terminated strings.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if status != 0 && status != ERROR_FILE_NOT_FOUND {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(())
    }

    pub fn read(key: &str, name: &str) -> Option<String> {
        let key = wide(key);
        let name = wide(name);
        let mut buffer = vec![0_u16; 2048];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer and its size in bytes are passed together.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let length = (size as usize / 2).min(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..length]);
        Some(text.trim_end_matches('\0').to_owned())
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use std::io;

    // Only Windows and Linux are supported; elsewhere the app never starts itself.
    pub fn write(_key: &str, _name: &str, _value: &str) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }

    pub fn delete(_key: &str, _name: &str) -> io::Result<()> {
        Ok(())
    }

    pub fn read(_key: &str, _name: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_quotes_the_path_and_starts_hidden() {
        assert_eq!(
            command_for(Path::new(r"C:\Program Files\Ratatosk\Ratatosk.exe")),
            r#""C:\Program Files\Ratatosk\Ratatosk.exe" --hidden"#
        );
    }

    #[test]
    fn an_autostart_entry_is_written_checked_and_removed() {
        let config = tempfile::tempdir().unwrap();
        let app = Path::new("/opt/Rata toskr/ratatoskr");
        assert!(!xdg::enabled_in(config.path(), app));
        xdg::set_in(config.path(), app, true).unwrap();
        let text =
            std::fs::read_to_string(config.path().join("autostart/ratatoskr.desktop")).unwrap();
        assert!(
            text.contains("Exec=\"/opt/Rata toskr/ratatoskr\" --hidden"),
            "{text}"
        );
        assert!(xdg::enabled_in(config.path(), app));
        assert!(!xdg::enabled_in(config.path(), Path::new("/usr/bin/other")));
        xdg::set_in(config.path(), app, false).unwrap();
        assert!(!xdg::enabled_in(config.path(), app));
        xdg::set_in(config.path(), app, false).unwrap();
    }

    #[test]
    fn exec_arguments_are_escaped() {
        assert_eq!(
            xdg::exec_line(Path::new("/tmp/a$b\"100%\"")),
            r#""/tmp/a\$b\"100%%\"" --hidden"#
        );
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    #[test]
    fn elsewhere_it_is_never_on() {
        let app = Path::new("/usr/bin/ratatosk");
        assert!(!enabled(app));
        assert!(set(app, true).is_err());
        assert!(set(app, false).is_ok());
    }
}
