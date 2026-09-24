//! Free space on the drive that holds a folder.

use std::path::Path;

/// Bytes the current user may still write on the drive holding `path`
/// (the nearest folder that exists). `None` where it cannot be read.
pub fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|candidate| candidate.is_dir())?;
    platform::free_space(existing)
}

#[cfg(windows)]
mod platform {
    use std::{os::windows::ffi::OsStrExt, path::Path};
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    pub fn free_space(path: &Path) -> Option<u64> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available: u64 = 0;
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the
        // call, and the output pointer refers to a live u64. The other two
        // outputs are optional and passed as null.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(available)
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    pub fn free_space(_path: &Path) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_folder_is_measured_on_its_nearest_existing_parent() {
        let directory = std::env::temp_dir();
        let deep = directory.join("rud-not-here").join("still-not");
        assert_eq!(free_space(&deep), free_space(&directory));
        if cfg!(windows) {
            assert!(free_space(&directory).unwrap() > 0);
        }
    }
}
