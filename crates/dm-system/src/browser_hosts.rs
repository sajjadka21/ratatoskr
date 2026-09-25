//! Connecting the browser extension to this computer's Ratatosk.
//!
//! Browsers talk to a local program only through a "native messaging host"
//! they find in the registry. Registering it by hand was the hard part of
//! installing the extension; the application does it for the current user
//! (no administrator rights, nothing outside the user's own registry hive)
//! every time it starts, pointing at the host program next to it.
//!
//! The extension's IDs are fixed (the Chromium one by the key in its
//! manifest, the Firefox one by `browser_specific_settings`), so only that
//! extension may start the host.

use serde_json::json;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const HOST_NAME: &str = "com.download_manager.native";
/// The extension's ID in Chrome, Edge, Brave and other Chromium browsers.
pub const CHROMIUM_EXTENSION_ID: &str = "ocefplbhcgfmihahfkaknodbdidflhle";
/// IDs the stores give the published extension (a store assigns its own).
/// Add each one here once the extension is published there.
pub const STORE_EXTENSION_IDS: &[&str] = &[];
/// The extension's ID in Firefox.
pub const FIREFOX_EXTENSION_ID: &str = "browser@ratatosk.app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
    Brave,
    Chromium,
    Firefox,
}

impl Browser {
    pub const ALL: [Browser; 5] = [
        Browser::Chrome,
        Browser::Edge,
        Browser::Brave,
        Browser::Chromium,
        Browser::Firefox,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Brave => "brave",
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
        }
    }

    /// The registry key (under HKEY_CURRENT_USER) the browser reads.
    pub fn registry_key(self) -> String {
        let base = match self {
            Self::Chrome => r"Software\Google\Chrome\NativeMessagingHosts",
            Self::Edge => r"Software\Microsoft\Edge\NativeMessagingHosts",
            Self::Brave => r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
            Self::Chromium => r"Software\Chromium\NativeMessagingHosts",
            Self::Firefox => r"Software\Mozilla\NativeMessagingHosts",
        };
        format!(r"{base}\{HOST_NAME}")
    }

    const fn is_firefox(self) -> bool {
        matches!(self, Self::Firefox)
    }
}

/// The host manifest for Chromium browsers or for Firefox.
pub fn manifest(host: &Path, firefox: bool) -> serde_json::Value {
    let mut manifest = json!({
        "name": HOST_NAME,
        "description": "Ratatosk native messaging host",
        "path": host.to_string_lossy(),
        "type": "stdio",
    });
    if firefox {
        manifest["allowed_extensions"] = json!([FIREFOX_EXTENSION_ID]);
    } else {
        manifest["allowed_origins"] = json!(
            std::iter::once(CHROMIUM_EXTENSION_ID)
                .chain(STORE_EXTENSION_IDS.iter().copied())
                .map(|id| format!("chrome-extension://{id}/"))
                .collect::<Vec<_>>()
        );
    }
    manifest
}

/// Writes both host manifests into `folder` and points every browser's
/// registry key at them. Returns the browsers now registered.
pub fn register(host: &Path, folder: &Path) -> io::Result<Vec<Browser>> {
    if !host.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the browser connector program (dm-native-host) is missing",
        ));
    }
    fs::create_dir_all(folder)?;
    let chromium = folder.join("chromium-host.json");
    let firefox = folder.join("firefox-host.json");
    write_if_changed(&chromium, &manifest(host, false))?;
    write_if_changed(&firefox, &manifest(host, true))?;

    let mut registered = Vec::new();
    for browser in Browser::ALL {
        let path = if browser.is_firefox() {
            &firefox
        } else {
            &chromium
        };
        if platform::set_default_value(&browser.registry_key(), &path.to_string_lossy()).is_ok() {
            registered.push(browser);
        }
    }
    Ok(registered)
}

/// The browsers whose registry key points at a manifest that exists.
pub fn registered() -> Vec<Browser> {
    Browser::ALL
        .into_iter()
        .filter(|browser| {
            platform::default_value(&browser.registry_key())
                .map(PathBuf::from)
                .is_some_and(|path| path.is_file())
        })
        .collect()
}

/// The host program next to the application, if it is there.
pub fn host_beside(application: &Path) -> Option<PathBuf> {
    let directory = application.parent()?;
    let name = if cfg!(windows) {
        "dm-native-host.exe"
    } else {
        "dm-native-host"
    };
    Some(directory.join(name)).filter(|path| path.is_file())
}

fn write_if_changed(path: &Path, manifest: &serde_json::Value) -> io::Result<()> {
    let text = serde_json::to_string_pretty(manifest).map_err(io::Error::other)?;
    if fs::read_to_string(path).is_ok_and(|current| current == text) {
        return Ok(());
    }
    fs::write(path, text)
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn set_default_value(key: &str, value: &str) -> io::Result<()> {
        let key = wide(key);
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
        // SAFETY: `handle` is open for writing; the data is the UTF-16 text
        // including its terminating NUL, as REG_SZ requires.
        let status = unsafe {
            RegSetValueExW(
                handle,
                null(),
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

    pub fn default_value(key: &str) -> Option<String> {
        let key = wide(key);
        let mut buffer = vec![0_u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer and its size in bytes are passed together.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                null(),
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
        let text = text.trim_end_matches('\0').to_owned();
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    // Only Windows is supported; elsewhere nothing is registered.
    pub fn set_default_value(_key: &str, _value: &str) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }

    pub fn default_value(_key: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_browser_family_gets_its_own_allow_list() {
        let host = Path::new(r"C:\Program Files\Ratatosk\dm-native-host.exe");
        let chromium = manifest(host, false);
        assert_eq!(chromium["name"], HOST_NAME);
        assert_eq!(
            chromium["path"],
            r"C:\Program Files\Ratatosk\dm-native-host.exe"
        );
        assert_eq!(
            chromium["allowed_origins"][0],
            "chrome-extension://ocefplbhcgfmihahfkaknodbdidflhle/"
        );
        assert!(chromium.get("allowed_extensions").is_none());

        let firefox = manifest(host, true);
        assert_eq!(firefox["allowed_extensions"][0], FIREFOX_EXTENSION_ID);
        assert!(firefox.get("allowed_origins").is_none());
    }

    #[test]
    fn registry_keys_name_the_host() {
        assert_eq!(
            Browser::Edge.registry_key(),
            r"Software\Microsoft\Edge\NativeMessagingHosts\com.download_manager.native"
        );
        assert_eq!(
            Browser::Firefox.registry_key(),
            r"Software\Mozilla\NativeMessagingHosts\com.download_manager.native"
        );
    }

    #[test]
    fn manifests_are_written_and_a_missing_host_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("dm-native-host.exe");
        assert!(register(&missing, directory.path()).is_err());

        std::fs::write(&missing, b"").unwrap();
        let folder = directory.path().join("hosts");
        let _ = register(&missing, &folder).unwrap();
        let written: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(folder.join("firefox-host.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(written["allowed_extensions"][0], FIREFOX_EXTENSION_ID);
        assert!(folder.join("chromium-host.json").is_file());
        assert_eq!(
            host_beside(&directory.path().join("Ratatosk.exe")).is_some(),
            cfg!(windows)
        );
    }
}
