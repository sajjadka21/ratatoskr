//! Chrome/Edge/Firefox Native Messaging host.
//!
//! The browser only cancels its own download after this host answers
//! `accepted: true`, so that answer must mean the task is safely persisted —
//! not merely that a process was started. The host therefore writes the task
//! into the application's database itself and only then asks the (single
//! instance) application to pick it up. If the application cannot be started
//! the task is still waiting in the list the next time it opens.

use dm_common::{APP_IDENTIFIER, DATABASE_FILE_NAME};
use dm_core::{browser::BrowserHandoff, linkgrabber::extract_links, service::DownloadService};
use dm_storage::Storage;
use serde::{Deserialize, Serialize};
use std::{
    env,
    io::{self, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
};

/// Links forwarded to LinkGrabber in one message. Windows limits a command
/// line to 32 767 characters, so the total is bounded as well.
const MAX_GRAB_LINKS: usize = 500;
const MAX_GRAB_ARGUMENT_CHARS: usize = 24_000;

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
}

impl Environment {
    fn detect() -> Self {
        Self {
            database_path: database_path(),
            application: application_path(),
            launch: launch_application,
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
        return rejected("Download Manager data folder was not found");
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

fn handle_inspect(request: NativeRequest, environment: &Environment) -> NativeResponse {
    let links = extract_links(request.text.as_deref().unwrap_or_default());
    if links.is_empty() {
        return rejected("no HTTP links found");
    }
    let Some(application) = environment.application.as_ref() else {
        return rejected("Download Manager executable was not found");
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
        return rejected("Download Manager could not be started");
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
    use std::{io::Cursor, path::PathBuf};
    use tempfile::tempdir;

    fn launched(_: &PathBuf, _: &[String]) -> bool {
        true
    }

    fn environment(database: Option<PathBuf>) -> Environment {
        Environment {
            database_path: database,
            application: Some(PathBuf::from("app.exe")),
            launch: launched,
        }
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
