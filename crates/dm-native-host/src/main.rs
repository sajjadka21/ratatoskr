//! Chrome/Edge/Firefox Native Messaging host.
//!
//! The browser only cancels its own download after this host answers
//! `accepted: true`, so that answer must mean the task is safely persisted —
//! not merely that a process was started. The host therefore writes the task
//! into the application's database itself and only then asks the (single
//! instance) application to pick it up. If the application cannot be started
//! the task is still waiting in the list the next time it opens.
//!
//! A browser session (cookie header) is the exception to "persist first": it
//! must never reach the database or a command line. It travels to the running
//! application over its private session channel, and the handoff is accepted
//! only once the application has it. If that fails the task just created is
//! removed again, so the browser keeps its own download and loses nothing.

use dm_common::{APP_IDENTIFIER, DATABASE_FILE_NAME};
use dm_core::{browser::BrowserHandoff, linkgrabber::extract_links, service::DownloadService};
use dm_storage::Storage;
use dm_system::session_channel::{DeliveryError, SessionHandoff};
use serde::{Deserialize, Serialize};
use std::{
    env, fmt,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

/// Links forwarded to LinkGrabber in one message. Windows limits a command
/// line to 32 767 characters, so the total is bounded as well.
const MAX_GRAB_LINKS: usize = 500;
const MAX_GRAB_ARGUMENT_CHARS: usize = 24_000;

/// How long to wait for an application that is already running, and for one
/// that has to be started first.
const SESSION_CONNECT_RUNNING: Duration = Duration::from_millis(750);
const SESSION_CONNECT_STARTING: Duration = Duration::from_secs(20);

/// Command-line switches understood by the desktop application.
const ARG_HANDOFF_TASK: &str = "--handoff-task";
const ARG_GRAB_LINKS: &str = "--grab-links";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeRequest {
    #[serde(rename = "type")]
    message_type: String,
    url: Option<String>,
    text: Option<String>,
    filename_hint: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
    /// The browser's cookie header for the download URL, present only when
    /// the user turned session handover on in the extension.
    cookies: Option<SecretCookie>,
}

/// A cookie header that cannot be printed by accident.
#[derive(Deserialize)]
#[serde(transparent)]
struct SecretCookie(String);

impl SecretCookie {
    fn value(&self) -> Option<&str> {
        let value = self.0.trim();
        (!value.is_empty()).then_some(value)
    }
}

impl fmt::Debug for SecretCookie {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeResponse {
    accepted: bool,
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    link_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_found: Option<bool>,
}

fn main() -> io::Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    loop {
        let Some(payload) = read_message(&mut stdin)? else {
            return Ok(());
        };
        let response = handle_message(&payload, &Environment::detect());
        write_message(&mut stdout, &response)?;
    }
}

/// Where the host finds the database and the application. Injected so tests
/// never touch the real user profile or start real processes.
struct Environment {
    database_path: Option<PathBuf>,
    application: Option<PathBuf>,
    launch: fn(&PathBuf, &[String]) -> bool,
    /// Sends a session to the running application, which must be the program
    /// at the given path.
    deliver_session: fn(&SessionHandoff, &Path, Duration) -> Result<(), DeliveryError>,
}

impl Environment {
    fn detect() -> Self {
        Self {
            database_path: database_path(),
            application: application_path(),
            launch: launch_application,
            deliver_session: dm_system::session_channel::deliver,
        }
    }
}

fn handle_message(payload: &[u8], environment: &Environment) -> NativeResponse {
    let request = match serde_json::from_slice::<NativeRequest>(payload) {
        Ok(request) => request,
        Err(_) => return rejected("invalid native message"),
    };

    match request.message_type.as_str() {
        "ping" => NativeResponse {
            accepted: true,
            app_found: Some(environment.application.is_some()),
            ..NativeResponse::default()
        },
        "download" => handle_download(request, environment),
        "inspect" => handle_inspect(request, environment),
        _ => rejected("unsupported native message"),
    }
}

fn handle_download(request: NativeRequest, environment: &Environment) -> NativeResponse {
    let handoff = BrowserHandoff {
        url: request.url.unwrap_or_default(),
        filename_hint: request.filename_hint,
        referrer: request.referrer,
        user_agent: request.user_agent,
    };
    let Ok(handoff) = handoff.validate() else {
        return rejected("browser handoff validation failed");
    };
    let Some(database_path) = environment.database_path.as_ref() else {
        return rejected("Ratatosk data folder was not found");
    };

    // Persist first. Every failure before this point leaves the browser's
    // own download untouched.
    let task = Storage::open(database_path)
        .ok()
        .map(Arc::new)
        .and_then(|storage| DownloadService::new(storage).ok())
        .and_then(|service| {
            service
                .create_task_with_context(&handoff.url, &handoff.request_context())
                .ok()
        });
    let Some(task) = task else {
        return rejected("the download could not be saved");
    };

    if let Some(cookie) = request.cookies.as_ref().and_then(SecretCookie::value) {
        return hand_over_with_session(database_path, &task.id, cookie, environment);
    }

    // The task exists now, so the handoff is accepted even when the
    // application cannot be started: the row is waiting for it.
    if let Some(application) = environment.application.as_ref() {
        (environment.launch)(application, &[ARG_HANDOFF_TASK.to_owned(), task.id.clone()]);
    }

    NativeResponse {
        accepted: true,
        task_id: Some(task.id),
        ..NativeResponse::default()
    }
}

/// Delivers the session to the running application, which then starts the
/// task. Nothing about the session is written anywhere on the way.
fn hand_over_with_session(
    database_path: &Path,
    task_id: &str,
    cookie: &str,
    environment: &Environment,
) -> NativeResponse {
    // The session goes only to the program this host was installed with; if
    // that program cannot be identified, nothing is sent.
    let Some(application) = environment.application.as_ref() else {
        return withdraw(database_path, task_id, "Ratatosk executable was not found");
    };

    let handoff = SessionHandoff::new(task_id, cookie);

    // Try the running application first; start it only if nothing answers.
    let mut delivered =
        (environment.deliver_session)(&handoff, application, SESSION_CONNECT_RUNNING);

    if matches!(delivered, Err(DeliveryError::Unavailable)) {
        (environment.launch)(application, &[]);
        delivered = (environment.deliver_session)(&handoff, application, SESSION_CONNECT_STARTING);
    }

    match delivered {
        Ok(()) => NativeResponse {
            accepted: true,
            task_id: Some(task_id.to_owned()),
            ..NativeResponse::default()
        },
        Err(error) => withdraw(database_path, task_id, &error.to_string()),
    }
}

/// Removes a task whose session could not be delivered, so the browser keeps
/// its own download. If the application had already started the task, the
/// reply was lost rather than the session, and the handoff stands.
fn withdraw(database_path: &Path, task_id: &str, reason: &str) -> NativeResponse {
    let removed =
        Storage::open(database_path).map(|storage| storage.remove_download_record(task_id));

    match removed {
        Ok(Err(dm_storage::StorageError::DownloadNotRemovable { .. })) => NativeResponse {
            accepted: true,
            task_id: Some(task_id.to_owned()),
            ..NativeResponse::default()
        },
        _ => rejected(reason),
    }
}

fn handle_inspect(request: NativeRequest, environment: &Environment) -> NativeResponse {
    let links = extract_links(request.text.as_deref().unwrap_or_default());
    if links.is_empty() {
        return rejected("no HTTP links found");
    }
    let Some(application) = environment.application.as_ref() else {
        return rejected("Ratatosk executable was not found");
    };

    let mut arguments = vec![ARG_GRAB_LINKS.to_owned()];
    let mut length = 0;
    for link in links.into_iter().take(MAX_GRAB_LINKS) {
        length += link.url.len() + 1;
        if length > MAX_GRAB_ARGUMENT_CHARS {
            break;
        }
        arguments.push(link.url);
    }
    let link_count = arguments.len() - 1;

    // Links go to LinkGrabber for review; nothing starts downloading on its
    // own from a text selection.
    if !(environment.launch)(application, &arguments) {
        return rejected("Ratatosk could not be started");
    }

    NativeResponse {
        accepted: true,
        link_count: Some(link_count),
        ..NativeResponse::default()
    }
}

fn rejected(error: &str) -> NativeResponse {
    NativeResponse {
        accepted: false,
        error: Some(error.to_owned()),
        ..NativeResponse::default()
    }
}

fn launch_application(application: &PathBuf, arguments: &[String]) -> bool {
    // The browser talks to this host over stdin and stdout, and a child
    // inherits both unless told otherwise. Anything the application printed -
    // its log output, for one - would land in the middle of the native
    // messaging stream and corrupt the reply the browser is waiting for.
    Command::new(application)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// The database the desktop application uses: Tauri's application data
/// directory is the platform data directory joined with the bundle
/// identifier. `DOWNLOAD_MANAGER_DATA_DIR` overrides it for portable setups.
fn database_path() -> Option<PathBuf> {
    if let Some(directory) = env::var_os("DOWNLOAD_MANAGER_DATA_DIR") {
        return Some(PathBuf::from(directory).join(DATABASE_FILE_NAME));
    }

    let base = if cfg!(windows) {
        env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    }?;

    Some(base.join(APP_IDENTIFIER).join(DATABASE_FILE_NAME))
}

fn application_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("DOWNLOAD_MANAGER_APP_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = env::current_exe().ok()?.parent()?.to_owned();
    [
        "Ratatosk.exe",
        "download-manager.exe",
        "Download Manager.exe",
        "tauri-app.exe",
    ]
    .into_iter()
    .map(|name| current.join(name))
    .find(|path| path.is_file())
}

fn read_message(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native message too large",
        ));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_message(writer: &mut impl Write, response: &NativeResponse) -> io::Result<()> {
    let payload = serde_json::to_vec(response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "native response too large"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::{handle_message, read_message, write_message, Environment, NativeResponse};
    use dm_storage::Storage;
    use dm_system::session_channel::{DeliveryError, SessionHandoff};
    use std::{
        io::Cursor,
        path::{Path, PathBuf},
        sync::Mutex,
        time::Duration,
    };
    use tempfile::tempdir;

    const COOKIE: &str = "session=s3cr3t-browser-login";

    fn launched(_: &PathBuf, _: &[String]) -> bool {
        true
    }

    /// Sessions the fake application received, as (task id, cookie).
    static DELIVERED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

    fn delivered_ok(handoff: &SessionHandoff, _: &Path, _: Duration) -> Result<(), DeliveryError> {
        DELIVERED
            .lock()
            .unwrap()
            .push((handoff.task_id.clone(), handoff.cookie.clone()));
        Ok(())
    }

    fn nobody_answers(_: &SessionHandoff, _: &Path, _: Duration) -> Result<(), DeliveryError> {
        Err(DeliveryError::Unavailable)
    }

    fn impostor(_: &SessionHandoff, _: &Path, _: Duration) -> Result<(), DeliveryError> {
        Err(DeliveryError::UntrustedServer)
    }

    fn environment(database: Option<PathBuf>) -> Environment {
        Environment {
            database_path: database,
            application: Some(PathBuf::from("app.exe")),
            launch: launched,
            deliver_session: delivered_ok,
        }
    }

    fn download_with_session() -> Vec<u8> {
        format!(
            r#"{{"type":"download","url":"https://example.com/private.zip","cookies":"{COOKIE}"}}"#
        )
        .into_bytes()
    }

    /// True when any database file in `directory` contains `needle`.
    fn stored_anywhere(directory: &Path, needle: &str) -> bool {
        std::fs::read_dir(directory).unwrap().any(|entry| {
            let path = entry.unwrap().path();
            path.is_file()
                && std::fs::read(&path)
                    .unwrap()
                    .windows(needle.len())
                    .any(|window| window == needle.as_bytes())
        })
    }

    #[test]
    fn a_session_is_delivered_and_never_stored() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");

        let response = handle_message(
            &download_with_session(),
            &environment(Some(database.clone())),
        );

        assert!(response.accepted, "{:?}", response.error);
        let task_id = response.task_id.unwrap();
        assert!(DELIVERED
            .lock()
            .unwrap()
            .iter()
            .any(|(id, cookie)| id == &task_id && cookie == COOKIE));

        let storage = Storage::open(&database).unwrap();
        assert!(storage.get_download(&task_id).unwrap().is_some());
        drop(storage);

        assert!(
            !stored_anywhere(directory.path(), "s3cr3t-browser-login"),
            "the session must never reach the database"
        );
    }

    #[test]
    fn an_undelivered_session_leaves_the_download_with_the_browser() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");
        let environment = Environment {
            deliver_session: nobody_answers,
            ..environment(Some(database.clone()))
        };

        let response = handle_message(&download_with_session(), &environment);

        assert!(!response.accepted, "the browser must keep its own download");
        assert!(
            Storage::open(&database)
                .unwrap()
                .list_downloads()
                .unwrap()
                .is_empty(),
            "the task created for the handoff must be withdrawn"
        );
    }

    #[test]
    fn a_session_is_never_sent_to_an_untrusted_program() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");
        let environment = Environment {
            deliver_session: impostor,
            ..environment(Some(database.clone()))
        };

        let response = handle_message(&download_with_session(), &environment);

        assert!(!response.accepted);
        assert!(!response.error.unwrap_or_default().contains("s3cr3t"));
        assert!(Storage::open(&database)
            .unwrap()
            .list_downloads()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_session_needs_a_known_application() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");
        let environment = Environment {
            application: None,
            ..environment(Some(database.clone()))
        };

        let response = handle_message(&download_with_session(), &environment);

        assert!(!response.accepted);
        assert!(Storage::open(&database)
            .unwrap()
            .list_downloads()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_request_never_prints_its_session() {
        let request: super::NativeRequest =
            serde_json::from_slice(&download_with_session()).unwrap();

        assert!(!format!("{request:?}").contains("s3cr3t"));
    }

    #[test]
    fn invalid_messages_are_rejected_without_echoing_sensitive_payloads() {
        let response = handle_message(
            br#"{"type":"download","url":"not-a-url"}"#,
            &environment(None),
        );
        assert!(!response.accepted);
        assert_eq!(
            response.error.as_deref(),
            Some("browser handoff validation failed")
        );
    }

    #[test]
    fn download_is_accepted_only_after_the_task_is_persisted() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");

        let response = handle_message(
            br#"{"type":"download","url":"https://example.com/a.zip","referrer":"https://example.com/page","userAgent":"Mozilla/5.0"}"#,
            &environment(Some(database.clone())),
        );
        assert!(response.accepted);
        let task_id = response.task_id.unwrap();

        let storage = Storage::open(&database).unwrap();
        let task = storage.get_download(&task_id).unwrap().unwrap();
        assert_eq!(task.source_url, "https://example.com/a.zip");
        let context = storage.get_request_context(&task_id).unwrap();
        assert_eq!(
            context.referrer.as_deref(),
            Some("https://example.com/page")
        );
    }

    #[test]
    fn download_is_refused_when_the_database_cannot_be_opened() {
        let directory = tempdir().unwrap();
        // A directory where the database file should be cannot be opened.
        let blocked = directory.path().join("downloads.db");
        std::fs::create_dir_all(&blocked).unwrap();

        let response = handle_message(
            br#"{"type":"download","url":"https://example.com/a.zip"}"#,
            &environment(Some(blocked)),
        );
        assert!(!response.accepted);
    }

    #[test]
    fn selected_text_goes_to_link_grabber_in_one_launch() {
        let response = handle_message(
            br#"{"type":"inspect","text":"see https://a.example/1.zip and https://a.example/2.zip"}"#,
            &environment(None),
        );
        assert!(response.accepted);
        assert_eq!(response.link_count, Some(2));
    }

    #[test]
    fn app_identifier_matches_the_tauri_configuration() {
        let config = include_str!("../../../src-tauri/tauri.conf.json");
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        assert_eq!(config["identifier"], dm_common::APP_IDENTIFIER);
    }

    #[test]
    fn native_messages_use_little_endian_length_prefixes() {
        let payload = br#"{"type":"unknown"}"#;
        let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(payload);
        let decoded = read_message(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(decoded, payload);

        let mut output = Vec::new();
        write_message(
            &mut output,
            &NativeResponse {
                accepted: true,
                ..NativeResponse::default()
            },
        )
        .unwrap();
        assert_eq!(
            u32::from_le_bytes(output[..4].try_into().unwrap()) as usize,
            output.len() - 4
        );
    }
}
