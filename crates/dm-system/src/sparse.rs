//! Sparse partial files.
//!
//! A segmented download writes its last range far into the file before the
//! earlier ranges are filled in. On NTFS, writing past the part of a file
//! that was ever written makes Windows fill the gap with zeros first, so
//! the first write of a late range could stall for as long as it takes to
//! write gigabytes of zeros. Marking the file sparse skips that: gaps take
//! no space and no time. File systems without sparse files (FAT32, exFAT)
//! simply refuse, and the download still works, only with that stall.

use std::fs::File;

/// Marks `file` sparse where the file system supports it. Returns whether
/// it did; failure is never an error.
#[cfg(windows)]
pub fn mark_sparse(file: &File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::{IO::DeviceIoControl, Ioctl::FSCTL_SET_SPARSE};

    let mut returned = 0_u32;
    // SAFETY: the handle is owned by `file` and outlives the call; no input
    // or output buffer is passed, and the call is synchronous.
    let accepted = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    accepted != 0
}

/// Elsewhere files are extended without zero-filling anyway.
#[cfg(not(windows))]
pub fn mark_sparse(_file: &File) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marking_never_fails_the_caller() {
        let directory = std::env::temp_dir().join(format!("dm-sparse-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("file.part");
        let file = File::create(&path).unwrap();
        let _ = mark_sparse(&file);
        file.set_len(1024 * 1024).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 1024 * 1024);
        drop(file);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
