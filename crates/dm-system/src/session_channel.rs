//! Hands a browser session from the native messaging host to the running
//! application without it ever touching the disk or a command line.
//!
//! The native host is a separate, short-lived process started by the browser.
//! A command line is readable by every process of the user and is often
//! recorded by process auditing, and the database is exactly where a session
//! must never be, so the session travels over a local named pipe instead:
//!
//! - the pipe accepts only the current user and only local clients;
//! - the application creates it as the first instance of its name, so no
//!   other process can take the name while the application holds it;
//! - the client checks which program owns the pipe before sending anything,
//!   so a process that took the name first receives nothing;
//! - the client connects at identification level, so the server cannot act
//!   as the user.
//!
//! Outside Windows the channel is unavailable and every call says so; the
//! native host then leaves the download with the browser.

use serde::{Deserialize, Serialize};
use std::{fmt, io};

/// Version of the message format, so an old native host and a new
/// application refuse each other rather than misreading a message.
pub const PROTOCOL_VERSION: u32 = 1;

/// A message is one task id and one cookie header; nothing legitimate comes
/// close to this.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// A session sent from the native host to the application.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandoff {
    pub version: u32,
    pub task_id: String,
    pub cookie: String,
}

impl SessionHandoff {
    pub fn new(task_id: impl Into<String>, cookie: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            task_id: task_id.into(),
            cookie: cookie.into(),
        }
    }
}

impl fmt::Debug for SessionHandoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionHandoff")
            .field("version", &self.version)
            .field("task_id", &self.task_id)
            .field("cookie", &"<redacted>")
            .finish()
    }
}

/// The application's answer. `error` never repeats the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffReply {
    pub accepted: bool,
    pub error: Option<String>,
}

impl HandoffReply {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            error: None,
        }
    }

    pub fn refused(error: impl Into<String>) -> Self {
        Self {
            accepted: false,
            error: Some(error.into()),
        }
    }
}

/// Why a session could not be delivered. None of these carry the session.
#[derive(Debug)]
pub enum DeliveryError {
    /// The platform has no session channel.
    Unsupported,
    /// Nothing answered on the pipe in time.
    Unavailable,
    /// Something answered, but it is not the application; nothing was sent.
    UntrustedServer,
    /// The application received the session and refused it.
    Refused(String),
    Io(io::Error),
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => {
                formatter.write_str("browser sessions cannot be handed over on this system")
            }
            Self::Unavailable => formatter.write_str("Download Manager did not answer in time"),
            Self::UntrustedServer => formatter
                .write_str("the session channel is held by another program, so nothing was sent"),
            Self::Refused(reason) => {
                write!(formatter, "Download Manager refused the session: {reason}")
            }
            Self::Io(error) => write!(formatter, "session channel error: {error}"),
        }
    }
}

impl std::error::Error for DeliveryError {}

impl From<io::Error> for DeliveryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Serializes a message with its little-endian length prefix.
pub fn encode_frame<T: Serialize>(message: &T) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "session message too large",
        ));
    }

    let mut frame = (payload.len() as u32).to_le_bytes().to_vec();
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Checks a declared frame length before any memory is reserved for it.
pub fn checked_frame_length(prefix: [u8; 4]) -> io::Result<usize> {
    let length = u32::from_le_bytes(prefix) as usize;

    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "session message too large",
        ));
    }

    Ok(length)
}

pub fn decode_payload<T: for<'de> Deserialize<'de>>(payload: &[u8]) -> io::Result<T> {
    serde_json::from_slice(payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(windows)]
pub use platform::{deliver, deliver_to, pipe_name, serve, serve_on};

#[cfg(not(windows))]
pub use unsupported::{deliver, deliver_to, pipe_name, serve, serve_on};

#[cfg(windows)]
mod platform {
    use super::{
        DeliveryError, HandoffReply, SessionHandoff, checked_frame_length, decode_payload,
        encode_frame,
    };
    use std::{
        ffi::{OsStr, c_void},
        fs::OpenOptions,
        io::{self, Read, Write},
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, HANDLE, LocalFree},
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::SECURITY_IDENTIFICATION,
        System::{
            Pipes::{GetNamedPipeServerProcessId, WaitNamedPipeW},
            Threading::{
                GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
        },
    };

    /// A client that connects but never finishes its message is dropped
    /// after this long, so it cannot hold a connection open.
    const READ_TIMEOUT: Duration = Duration::from_secs(5);

    /// The pipe name for the current user. Pipes are machine-wide rather than
    /// per session, so the user's SID keeps two signed-in users apart.
    pub fn pipe_name() -> io::Result<String> {
        Ok(format!(
            r"\\.\pipe\{}.browser-session.{}",
            dm_common::APP_IDENTIFIER,
            current_user_sid()?
        ))
    }

    /// Receives sessions on the application's pipe until the process ends.
    pub async fn serve<F>(handler: F) -> io::Result<()>
    where
        F: Fn(SessionHandoff) -> HandoffReply + Send + Sync + 'static,
    {
        serve_on(&pipe_name()?, handler).await
    }

    /// Receives sessions on `name`. The first instance is created with
    /// `first_pipe_instance`, so this fails instead of sharing a name another
    /// process already holds, and a new instance is always created before the
    /// connected one is handed off, so the name is never released.
    pub async fn serve_on<F>(name: &str, handler: F) -> io::Result<()>
    where
        F: Fn(SessionHandoff) -> HandoffReply + Send + Sync + 'static,
    {
        let descriptor = OwnerOnlyDescriptor::for_current_user()?;
        let handler = Arc::new(handler);
        let mut server = descriptor.create(name, true)?;

        loop {
            server.connect().await?;
            let connected = server;
            server = descriptor.create(name, false)?;

            let handler = Arc::clone(&handler);
            tokio::spawn(async move {
                let _ = answer(connected, handler.as_ref()).await;
            });
        }
    }

    async fn answer<F>(mut pipe: NamedPipeServer, handler: &F) -> io::Result<()>
    where
        F: Fn(SessionHandoff) -> HandoffReply,
    {
        let reply = match tokio::time::timeout(READ_TIMEOUT, read_request(&mut pipe)).await {
            Ok(Ok(handoff)) if handoff.version == super::PROTOCOL_VERSION => handler(handoff),
            Ok(Ok(_)) => HandoffReply::refused("unsupported session message version"),
            Ok(Err(_)) => HandoffReply::refused("unreadable session message"),
            Err(_) => return Ok(()),
        };

        pipe.write_all(&encode_frame(&reply)?).await?;

        // Disconnecting discards anything the client has not read yet, which
        // would lose the reply. Wait for the client to close its end instead;
        // the handle closes when `pipe` drops.
        let mut rest = [0_u8; 1];
        let _ = tokio::time::timeout(READ_TIMEOUT, pipe.read(&mut rest)).await;
        Ok(())
    }

    async fn read_request(pipe: &mut NamedPipeServer) -> io::Result<SessionHandoff> {
        let mut prefix = [0_u8; 4];
        pipe.read_exact(&mut prefix).await?;
        let mut payload = vec![0_u8; checked_frame_length(prefix)?];
        pipe.read_exact(&mut payload).await?;
        decode_payload(&payload)
    }

    /// Sends a session to the application's pipe.
    pub fn deliver(
        handoff: &SessionHandoff,
        expected_server: &Path,
        timeout: Duration,
    ) -> Result<(), DeliveryError> {
        deliver_to(&pipe_name()?, handoff, expected_server, timeout)
    }

    /// Sends a session to `name`, but only once the process owning the pipe
    /// has been confirmed to be `expected_server`. Until then nothing is
    /// written, so a program that took the name receives nothing.
    pub fn deliver_to(
        name: &str,
        handoff: &SessionHandoff,
        expected_server: &Path,
        timeout: Duration,
    ) -> Result<(), DeliveryError> {
        let mut pipe = connect(name, timeout)?;

        if !served_by(&pipe, expected_server)? {
            return Err(DeliveryError::UntrustedServer);
        }

        pipe.write_all(&encode_frame(handoff)?)?;
        pipe.flush()?;

        let mut prefix = [0_u8; 4];
        pipe.read_exact(&mut prefix)?;
        let mut payload = vec![0_u8; checked_frame_length(prefix)?];
        pipe.read_exact(&mut payload)?;

        let reply: HandoffReply = decode_payload(&payload)?;

        if reply.accepted {
            Ok(())
        } else {
            Err(DeliveryError::Refused(
                reply.error.unwrap_or_else(|| "no reason given".to_owned()),
            ))
        }
    }

    /// Connects, waiting for an application that is still starting up.
    fn connect(name: &str, timeout: Duration) -> Result<std::fs::File, DeliveryError> {
        let deadline = Instant::now() + timeout;

        loop {
            let attempt = OpenOptions::new()
                .read(true)
                .write(true)
                // Identification level: the server may learn who connected,
                // but cannot act as that user.
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(name);

            match attempt {
                Ok(pipe) => return Ok(pipe),
                Err(error) if Instant::now() >= deadline => {
                    return Err(match error.raw_os_error() {
                        Some(code) if is_waiting_error(code) => DeliveryError::Unavailable,
                        _ => DeliveryError::Io(error),
                    });
                }
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    let wide = wide(name);
                    // SAFETY: `wide` is a NUL-terminated UTF-16 string that
                    // outlives the call.
                    unsafe { WaitNamedPipeW(wide.as_ptr(), 250) };
                }
                Err(error) if error.raw_os_error().is_some_and(is_waiting_error) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(DeliveryError::Io(error)),
            }
        }
    }

    fn is_waiting_error(code: i32) -> bool {
        code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PIPE_BUSY as i32
    }

    /// True when the process on the other end of `pipe` runs `expected`.
    fn served_by(pipe: &std::fs::File, expected: &Path) -> io::Result<bool> {
        let mut process_id = 0_u32;

        // SAFETY: the handle belongs to `pipe`, which outlives the call.
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as HANDLE, &mut process_id) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }

        let actual = process_image(process_id)?;
        Ok(same_file(&actual, expected))
    }

    fn process_image(process_id: u32) -> io::Result<PathBuf> {
        // SAFETY: the handle is checked and closed below.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };

        if process.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut buffer = vec![0_u16; 32_768];
        let mut length = buffer.len() as u32;
        // SAFETY: `buffer` holds `length` UTF-16 units and outlives the call.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut length,
            )
        };
        let error = io::Error::last_os_error();
        // SAFETY: `process` is a valid handle opened above.
        unsafe { CloseHandle(process) };

        if ok == 0 {
            return Err(error);
        }

        Ok(PathBuf::from(String::from_utf16_lossy(
            &buffer[..length as usize],
        )))
    }

    /// Compares two paths as Windows does: after resolving links and without
    /// regard to case.
    fn same_file(left: &Path, right: &Path) -> bool {
        match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
            (Ok(left), Ok(right)) => left.as_os_str().eq_ignore_ascii_case(right.as_os_str()),
            _ => false,
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// The textual SID of the user running this process.
    fn current_user_sid() -> io::Result<String> {
        let mut token: HANDLE = std::ptr::null_mut();

        // SAFETY: `token` receives a handle that is closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }

        let result = token_user_sid(token);
        // SAFETY: `token` was opened above.
        unsafe { CloseHandle(token) };
        result
    }

    fn token_user_sid(token: HANDLE) -> io::Result<String> {
        let mut needed = 0_u32;
        // SAFETY: a null buffer with zero length only asks for the size.
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };

        if needed == 0 {
            return Err(io::Error::last_os_error());
        }

        // u64 storage keeps the buffer aligned for TOKEN_USER.
        let mut buffer = vec![0_u64; (needed as usize).div_ceil(8)];

        // SAFETY: `buffer` is at least `needed` bytes and suitably aligned.
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast::<c_void>(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: GetTokenInformation filled the buffer with a TOKEN_USER.
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let mut text: *mut u16 = std::ptr::null_mut();

        // SAFETY: the SID points into `buffer`, which is alive; `text` is freed below.
        if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: `text` is a NUL-terminated UTF-16 string from the system.
        let sid = unsafe {
            let length = (0..).take_while(|&index| *text.add(index) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
        };
        // SAFETY: `text` was allocated by ConvertSidToStringSidW.
        unsafe { LocalFree(text.cast()) };

        Ok(sid)
    }

    /// A security descriptor granting access to the current user and nobody
    /// else, including other users and administrators' default access.
    struct OwnerOnlyDescriptor {
        descriptor: *mut c_void,
    }

    // SAFETY: the descriptor is created once, never mutated, and freed only
    // on drop, so sharing it between threads is sound.
    unsafe impl Send for OwnerOnlyDescriptor {}
    unsafe impl Sync for OwnerOnlyDescriptor {}

    impl OwnerOnlyDescriptor {
        fn for_current_user() -> io::Result<Self> {
            // P: protected, so no inherited ACE widens it. GA: full access for
            // this user's SID only.
            let sddl = wide(&format!("D:P(A;;GA;;;{})", current_user_sid()?));
            let mut descriptor: *mut c_void = std::ptr::null_mut();

            // SAFETY: `sddl` is NUL-terminated; the descriptor is freed on drop.
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            };

            if ok == 0 {
                return Err(io::Error::last_os_error());
            }

            Ok(Self { descriptor })
        }

        fn create(&self, name: &str, first: bool) -> io::Result<NamedPipeServer> {
            let mut attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.descriptor,
                bInheritHandle: 0,
            };

            // SAFETY: `attributes` and the descriptor it points to are valid
            // for the duration of the call.
            unsafe {
                ServerOptions::new()
                    .first_pipe_instance(first)
                    .reject_remote_clients(true)
                    .pipe_mode(PipeMode::Byte)
                    .create_with_security_attributes_raw(
                        name,
                        (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
                    )
            }
        }
    }

    impl Drop for OwnerOnlyDescriptor {
        fn drop(&mut self) {
            // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
            unsafe { LocalFree(self.descriptor) };
        }
    }
}

#[cfg(not(windows))]
mod unsupported {
    use super::{DeliveryError, HandoffReply, SessionHandoff};
    use std::{io, path::Path, time::Duration};

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "no browser session channel on this system",
        )
    }

    pub fn pipe_name() -> io::Result<String> {
        Err(unsupported())
    }

    pub async fn serve<F>(_handler: F) -> io::Result<()>
    where
        F: Fn(SessionHandoff) -> HandoffReply + Send + Sync + 'static,
    {
        Err(unsupported())
    }

    pub async fn serve_on<F>(_name: &str, _handler: F) -> io::Result<()>
    where
        F: Fn(SessionHandoff) -> HandoffReply + Send + Sync + 'static,
    {
        Err(unsupported())
    }

    pub fn deliver(_: &SessionHandoff, _: &Path, _: Duration) -> Result<(), DeliveryError> {
        Err(DeliveryError::Unsupported)
    }

    pub fn deliver_to(
        _: &str,
        _: &SessionHandoff,
        _: &Path,
        _: Duration,
    ) -> Result<(), DeliveryError> {
        Err(DeliveryError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COOKIE: &str = "session=s3cr3t-browser-login";

    #[test]
    fn frames_round_trip() {
        let handoff = SessionHandoff::new("task-1", COOKIE);
        let frame = encode_frame(&handoff).unwrap();
        let length = checked_frame_length(frame[..4].try_into().unwrap()).unwrap();

        assert_eq!(length, frame.len() - 4);
        assert_eq!(
            decode_payload::<SessionHandoff>(&frame[4..]).unwrap(),
            handoff
        );
    }

    #[test]
    fn an_oversized_frame_is_refused_before_anything_is_allocated() {
        let declared = (MAX_MESSAGE_BYTES as u32 + 1).to_le_bytes();

        assert!(checked_frame_length(declared).is_err());
        assert!(encode_frame(&SessionHandoff::new("t", "x".repeat(MAX_MESSAGE_BYTES))).is_err());
    }

    #[test]
    fn never_prints_the_session() {
        let printed = format!("{:?}", SessionHandoff::new("task-1", COOKIE));

        assert!(!printed.contains("s3cr3t"), "{printed}");
        assert!(printed.contains("task-1"));
    }

    #[cfg(windows)]
    mod windows_pipe {
        use super::*;
        use std::{
            sync::{Arc, Mutex},
            time::Duration,
        };

        fn unique_name() -> String {
            format!(
                r"\\.\pipe\dm-system-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        }

        type Received = Arc<Mutex<Vec<SessionHandoff>>>;

        fn start_server(name: &str, reply: HandoffReply) -> Received {
            let received: Received = Arc::default();
            let sink = Arc::clone(&received);
            let name = name.to_owned();

            tokio::spawn(async move {
                let _ = serve_on(&name, move |handoff| {
                    sink.lock().unwrap().push(handoff);
                    reply.clone()
                })
                .await;
            });

            received
        }

        async fn deliver_blocking(
            name: String,
            handoff: SessionHandoff,
            expected: std::path::PathBuf,
        ) -> Result<(), DeliveryError> {
            tokio::task::spawn_blocking(move || {
                deliver_to(&name, &handoff, &expected, Duration::from_secs(5))
            })
            .await
            .unwrap()
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn delivers_to_the_expected_program() {
            let name = unique_name();
            let received = start_server(&name, HandoffReply::accepted());

            // The test process is the server, so it is the expected program.
            let result = deliver_blocking(
                name,
                SessionHandoff::new("task-1", COOKIE),
                std::env::current_exe().unwrap(),
            )
            .await;

            assert!(result.is_ok(), "{result:?}");
            let received = received.lock().unwrap();
            assert_eq!(received.len(), 1);
            assert_eq!(received[0].task_id, "task-1");
            assert_eq!(received[0].cookie, COOKIE);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn sends_nothing_to_a_program_that_is_not_the_application() {
            let name = unique_name();
            let received = start_server(&name, HandoffReply::accepted());

            // Stand in for another program holding the name: the server is
            // this test process, which is not the program the client expects.
            let impostor_check = std::env::var_os("SystemRoot")
                .map(|root| {
                    std::path::PathBuf::from(root)
                        .join("System32")
                        .join("notepad.exe")
                })
                .unwrap();

            let result =
                deliver_blocking(name, SessionHandoff::new("task-1", COOKIE), impostor_check).await;

            assert!(
                matches!(result, Err(DeliveryError::UntrustedServer)),
                "{result:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                received.lock().unwrap().is_empty(),
                "an untrusted server must never receive the session"
            );
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_refusal_comes_back_without_the_session() {
            let name = unique_name();
            let _received = start_server(&name, HandoffReply::refused("task not found"));

            let result = deliver_blocking(
                name,
                SessionHandoff::new("missing", COOKIE),
                std::env::current_exe().unwrap(),
            )
            .await;

            match result {
                Err(DeliveryError::Refused(reason)) => {
                    assert_eq!(reason, "task not found");
                    assert!(!reason.contains("s3cr3t"));
                }
                other => panic!("expected a refusal, got {other:?}"),
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_second_server_cannot_take_a_name_that_is_in_use() {
            let name = unique_name();
            let _received = start_server(&name, HandoffReply::accepted());
            tokio::time::sleep(Duration::from_millis(100)).await;

            let second = tokio::time::timeout(
                Duration::from_secs(2),
                serve_on(&name, |_| HandoffReply::accepted()),
            )
            .await
            .expect("a second server must fail at once, not wait");

            assert!(second.is_err(), "the name must stay with the first server");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn nothing_listening_is_reported_as_unavailable() {
            let result = deliver_blocking(
                unique_name(),
                SessionHandoff::new("task-1", COOKIE),
                std::env::current_exe().unwrap(),
            );

            let result = tokio::time::timeout(Duration::from_secs(10), result)
                .await
                .unwrap();
            assert!(
                matches!(result, Err(DeliveryError::Unavailable)),
                "{result:?}"
            );
        }

        #[test]
        fn the_pipe_name_is_specific_to_this_user() {
            let name = pipe_name().unwrap();

            assert!(name.starts_with(r"\\.\pipe\"));
            assert!(name.contains(dm_common::APP_IDENTIFIER));
            assert!(name.contains(".S-1-"), "{name}");
        }
    }
}
