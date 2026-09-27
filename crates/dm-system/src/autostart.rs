//! Starting with Windows: a value under the current user's `Run` key, the
//! same place Windows' own "Startup apps" page lists and can turn off. No
//! administrator rights are needed and nothing outside the user's own
//! registry hive is touched.

use std::{io, path::Path};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Ratatosk";
/// Passed when Windows starts the app, so it opens in the tray, not on top.
pub const HIDDEN_ARGUMENT: &str = "--hidden";

/// The command Windows runs at sign-in.
pub fn command_for(application: &Path) -> String {
    format!("\"{}\" {HIDDEN_ARGUMENT}", application.display())
}

/// Whether the app starts with Windows, and from this `application`.
pub fn enabled(application: &Path) -> bool {
    platform::read(RUN_KEY, VALUE_NAME).is_some_and(|command| command == command_for(application))
}

/// Turns starting with Windows on (pointing at `application`) or off.
pub fn set(application: &Path, on: bool) -> io::Result<()> {
    if on {
        platform::write(RUN_KEY, VALUE_NAME, &command_for(application))
    } else {
        platform::delete(RUN_KEY, VALUE_NAME)
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

#[cfg(not(windows))]
mod platform {
    use std::io;

    // Only Windows is supported; elsewhere the app never starts itself.
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

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_it_is_never_on() {
        let app = Path::new("/usr/bin/ratatosk");
        assert!(!enabled(app));
        assert!(set(app, true).is_err());
        assert!(set(app, false).is_ok());
    }
}
