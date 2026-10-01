//! Read text and CF_HTML under one clipboard lock. Clipboard contents remain
//! private and transient; readers must not log or persist this snapshot.
use std::io;

#[derive(Default, Hash)]
pub struct Snapshot {
    pub text: Option<String>,
    pub html: Option<String>,
}

#[cfg(windows)]
pub fn read() -> io::Result<Snapshot> {
    use clipboard_win::{Clipboard, formats, raw};
    const LIMIT: usize = 1024 * 1024;
    let html_format = raw::register_format("HTML Format");
    let _guard = Clipboard::new_attempts(3).map_err(|_| io::Error::other("Clipboard is busy"))?;
    fn bytes(format: u32) -> Option<Vec<u8>> {
        let mut buffer = vec![0; LIMIT + 2];
        let size = raw::get(format, &mut buffer).ok()?;
        if size > LIMIT {
            return None;
        }
        buffer.truncate(size);
        Some(buffer)
    }
    let text = bytes(formats::CF_UNICODETEXT).and_then(|bytes| {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16(&units).ok()
    });
    let html = html_format
        .and_then(|format| bytes(format.get()))
        .and_then(|mut bytes| {
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            String::from_utf8(bytes).ok()
        });
    Ok(Snapshot { text, html })
}

#[cfg(not(windows))]
pub fn read() -> io::Result<Snapshot> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Rich clipboard is available on Windows",
    ))
}
