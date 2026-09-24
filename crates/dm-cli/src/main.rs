//! `rud`: the Download Manager from the command line.
//!
//! It works on the application's own database and hands anything that
//! changes a running transfer to the application itself, so there is one
//! engine and one record of every download (the specification's "same
//! engine, same state"). Reading commands (`list`, `status`) open the
//! database directly; SQLite's write-ahead log lets them do so while the
//! application runs. Commands that act on downloads start the application,
//! or pass the request to the instance already running.

use dm_common::{DownloadRecord, DownloadStatus};
use dm_storage::Storage;
use serde_json::json;
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "\
rud - Download Manager from the command line

Usage:
  rud add <url>... [--later]      add downloads (start now, or keep for later)
  rud list [--status <status>] [--json]
  rud status <id> [--json]
  rud pause <id>...               pause downloads
  rud resume <id>...              continue paused or failed downloads
  rud retry <id>...               same as resume
  rud cancel <id>...              cancel downloads and delete their partial files
  rud pause-all                   pause everything that is running
  rud queue start <queue>         start a queue (by name or id)
  rud queue stop <queue>          stop a queue
  rud help | version

An <id> may be shortened to its first characters when that is unambiguous.
DOWNLOAD_MANAGER_DATA_DIR and DOWNLOAD_MANAGER_APP_PATH override where the
database and the application are looked for.";

/// Starts the application (or reaches the running one) with arguments.
type Launcher = Box<dyn Fn(&[String]) -> bool>;

/// What the command needs from the outside world, replaceable in tests.
struct Environment {
    database: Option<PathBuf>,
    launch: Launcher,
}

impl Environment {
    fn detect() -> Self {
        Self {
            database: dm_system::locate::database_path(),
            launch: Box::new(|arguments| {
                dm_system::locate::application_path()
                    .is_some_and(|application| dm_system::locate::launch(&application, arguments))
            }),
        }
    }

    fn storage(&self) -> Result<Storage, Failure> {
        let path = self.database.as_ref().ok_or_else(|| {
            Failure::Runtime("the application's data folder was not found".to_owned())
        })?;
        if !path.is_file() {
            return Err(Failure::Runtime(format!(
                "no database at {}; open the application once first",
                path.display()
            )));
        }
        Storage::open(path).map_err(|error| Failure::Runtime(error.to_string()))
    }

    fn send(&self, arguments: Vec<String>) -> Result<(), Failure> {
        if (self.launch)(&arguments) {
            Ok(())
        } else {
            Err(Failure::Runtime(
                "the application was not found next to rud; set DOWNLOAD_MANAGER_APP_PATH"
                    .to_owned(),
            ))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Failure {
    Usage(String),
    Runtime(String),
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let environment = Environment::detect();
    let mut output = io::stdout().lock();
    match run(&arguments, &environment, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("rud: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Runtime(message)) => {
            eprintln!("rud: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    arguments: &[String],
    environment: &Environment,
    output: &mut impl Write,
) -> Result<(), Failure> {
    let (command, rest) = arguments
        .split_first()
        .ok_or_else(|| Failure::Usage("no command given".to_owned()))?;
    let write = |output: &mut dyn Write, text: &str| {
        writeln!(output, "{text}").map_err(|error| Failure::Runtime(error.to_string()))
    };

    match command.as_str() {
        "help" | "--help" | "-h" => write(output, USAGE),
        "version" | "--version" | "-V" => write(output, concat!("rud ", env!("CARGO_PKG_VERSION"))),
        "add" => {
            let later = rest.iter().any(|argument| argument == "--later");
            let urls: Vec<&String> = rest
                .iter()
                .filter(|argument| !argument.starts_with("--"))
                .collect();
            if urls.is_empty() {
                return Err(Failure::Usage("add needs at least one link".to_owned()));
            }
            for url in &urls {
                validate_url(url)?;
            }
            let storage = environment.storage()?;
            let now = unix_now();
            let mut ids = Vec::new();
            for url in urls {
                let task = storage
                    .create_download(url, now)
                    .map_err(|error| Failure::Runtime(error.to_string()))?;
                write(output, &format!("added {}  {url}", short_id(&task.id)))?;
                ids.push(task.id);
            }
            let flag = if later { "--refresh" } else { "--handoff-task" };
            environment.send(std::iter::once(flag.to_owned()).chain(ids).collect())
        }
        "list" => {
            let json_output = rest.iter().any(|argument| argument == "--json");
            let status_filter = option_value(rest, "--status")?
                .map(|value| {
                    value
                        .parse::<DownloadStatus>()
                        .map_err(|_| Failure::Usage(format!("unknown status {value:?}")))
                })
                .transpose()?;
            let downloads: Vec<DownloadRecord> = environment
                .storage()?
                .list_downloads()
                .map_err(|error| Failure::Runtime(error.to_string()))?
                .into_iter()
                .filter(|task| status_filter.is_none_or(|status| task.status == status))
                .collect();
            if json_output {
                let items: Vec<_> = downloads.iter().map(record_json).collect();
                write(output, &serde_json::Value::Array(items).to_string())
            } else if downloads.is_empty() {
                write(output, "no downloads")
            } else {
                write(
                    output,
                    &format!(
                        "{:<10} {:<12} {:>6} {:>10}  NAME",
                        "ID", "STATUS", "DONE", "SIZE"
                    ),
                )?;
                for task in &downloads {
                    write(
                        output,
                        &format!(
                            "{:<10} {:<12} {:>6} {:>10}  {}",
                            short_id(&task.id),
                            task.status.as_str(),
                            percent(task),
                            task.total_bytes.map_or_else(|| "?".to_owned(), human_bytes),
                            display_name(task)
                        ),
                    )?;
                }
                Ok(())
            }
        }
        "status" => {
            let id = rest
                .iter()
                .find(|argument| !argument.starts_with("--"))
                .ok_or_else(|| Failure::Usage("status needs an id".to_owned()))?;
            let storage = environment.storage()?;
            let task = resolve(&storage, id)?;
            if rest.iter().any(|argument| argument == "--json") {
                return write(output, &record_json(&task).to_string());
            }
            let lines = [
                format!("id        {}", task.id),
                format!("name      {}", display_name(&task)),
                format!("status    {}", task.status.as_str()),
                format!(
                    "progress  {} of {} ({})",
                    human_bytes(task.downloaded_bytes),
                    task.total_bytes.map_or_else(|| "?".to_owned(), human_bytes),
                    percent(&task)
                ),
                format!("link      {}", task.source_url),
                format!(
                    "saved to  {}",
                    task.destination_path.as_deref().unwrap_or("-")
                ),
            ];
            for line in lines {
                write(output, &line)?;
            }
            if let Some(message) = &task.error_message {
                write(output, &format!("note      {message}"))?;
            }
            Ok(())
        }
        "pause" | "resume" | "retry" | "cancel" => {
            let ids: Vec<&String> = rest
                .iter()
                .filter(|argument| !argument.starts_with("--"))
                .collect();
            if ids.is_empty() {
                return Err(Failure::Usage(format!("{command} needs at least one id")));
            }
            let storage = environment.storage()?;
            let resolved = ids
                .iter()
                .map(|id| resolve(&storage, id).map(|task| task.id))
                .collect::<Result<Vec<_>, _>>()?;
            let action = if command == "retry" {
                "resume"
            } else {
                command.as_str()
            };
            environment.send(
                ["--control".to_owned(), action.to_owned()]
                    .into_iter()
                    .chain(resolved.iter().cloned())
                    .collect(),
            )?;
            write(
                output,
                &format!(
                    "{action}: {}",
                    resolved
                        .iter()
                        .map(|id| short_id(id))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            )
        }
        "pause-all" => {
            environment.send(vec!["--control".to_owned(), "pause-all".to_owned()])?;
            write(output, "pause-all: sent")
        }
        "queue" => {
            let (action, name) = match rest {
                [action, name, ..] if action == "start" || action == "stop" => (action, name),
                _ => {
                    return Err(Failure::Usage(
                        "use: rud queue start|stop <queue>".to_owned(),
                    ));
                }
            };
            let queues = environment
                .storage()?
                .list_queues()
                .map_err(|error| Failure::Runtime(error.to_string()))?;
            let queue = queues
                .iter()
                .find(|queue| queue.id == *name || queue.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| Failure::Runtime(format!("no queue named {name:?}")))?;
            environment.send(vec![
                "--control".to_owned(),
                format!("queue-{action}"),
                queue.id.clone(),
            ])?;
            write(output, &format!("queue {action}: {}", queue.name))
        }
        other => Err(Failure::Usage(format!("unknown command {other:?}"))),
    }
}

fn option_value<'a>(arguments: &'a [String], name: &str) -> Result<Option<&'a String>, Failure> {
    match arguments.iter().position(|argument| argument == name) {
        None => Ok(None),
        Some(index) => arguments
            .get(index + 1)
            .map(Some)
            .ok_or_else(|| Failure::Usage(format!("{name} needs a value"))),
    }
}

fn validate_url(url: &str) -> Result<(), Failure> {
    let lower = url.to_ascii_lowercase();
    let web = (lower.starts_with("http://") || lower.starts_with("https://"))
        && url.len() > "http://x".len()
        && !url
            .chars()
            .any(|character| character.is_whitespace() || character.is_control());
    if web {
        Ok(())
    } else {
        Err(Failure::Usage(format!(
            "{url:?} is not an http or https link"
        )))
    }
}

/// A download by full id or by an unambiguous prefix of at least 4 characters.
fn resolve(storage: &Storage, id: &str) -> Result<DownloadRecord, Failure> {
    if let Some(task) = storage
        .get_download(id)
        .map_err(|error| Failure::Runtime(error.to_string()))?
    {
        return Ok(task);
    }
    if id.len() < 4 {
        return Err(Failure::Runtime(format!(
            "no download {id:?} (use at least 4 characters)"
        )));
    }
    let matches: Vec<DownloadRecord> = storage
        .list_downloads()
        .map_err(|error| Failure::Runtime(error.to_string()))?
        .into_iter()
        .filter(|task| task.id.starts_with(id))
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next().expect("one match")),
        0 => Err(Failure::Runtime(format!("no download {id:?}"))),
        count => Err(Failure::Runtime(format!(
            "{id:?} matches {count} downloads; use more characters"
        ))),
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn display_name(task: &DownloadRecord) -> String {
    task.filename.clone().unwrap_or_else(|| {
        task.source_url
            .split(['?', '#'])
            .next()
            .and_then(|path| path.rsplit('/').find(|part| !part.is_empty()))
            .unwrap_or(&task.source_url)
            .to_owned()
    })
}

fn percent(task: &DownloadRecord) -> String {
    match task.total_bytes {
        Some(total) if total > 0 => format!(
            "{:.0}%",
            task.downloaded_bytes as f64 * 100.0 / total as f64
        ),
        _ if task.status == DownloadStatus::Completed => "100%".to_owned(),
        _ => "-".to_owned(),
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn record_json(task: &DownloadRecord) -> serde_json::Value {
    json!({
        "id": task.id,
        "status": task.status.as_str(),
        "name": display_name(task),
        "sourceUrl": task.source_url,
        "downloadedBytes": task.downloaded_bytes,
        "totalBytes": task.total_bytes,
        "destinationPath": task.destination_path,
        "queueId": task.queue_id,
        "errorCode": task.error_code,
        "errorMessage": task.error_message,
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use tempfile::tempdir;

    struct Fixture {
        _directory: tempfile::TempDir,
        environment: Environment,
        sent: Rc<RefCell<Vec<Vec<String>>>>,
        storage: Storage,
    }

    fn fixture() -> Fixture {
        let directory = tempdir().unwrap();
        let database = directory.path().join("downloads.db");
        let storage = Storage::open(&database).unwrap();
        let sent = Rc::new(RefCell::new(Vec::new()));
        let recorder = Rc::clone(&sent);
        Fixture {
            environment: Environment {
                database: Some(database),
                launch: Box::new(move |arguments| {
                    recorder.borrow_mut().push(arguments.to_vec());
                    true
                }),
            },
            _directory: directory,
            sent,
            storage,
        }
    }

    fn run_text(fixture: &Fixture, arguments: &[&str]) -> Result<String, Failure> {
        let arguments: Vec<String> = arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect();
        let mut output = Vec::new();
        run(&arguments, &fixture.environment, &mut output)?;
        Ok(String::from_utf8(output).unwrap())
    }

    #[test]
    fn adding_stores_the_download_and_hands_it_to_the_application() {
        let fixture = fixture();
        let printed = run_text(&fixture, &["add", "https://example.com/a.iso"]).unwrap();

        let downloads = fixture.storage.list_downloads().unwrap();
        assert_eq!(downloads.len(), 1);
        assert!(printed.contains(&short_id(&downloads[0].id)));
        assert_eq!(
            fixture.sent.borrow().as_slice(),
            &[vec!["--handoff-task".to_owned(), downloads[0].id.clone()]]
        );
    }

    #[test]
    fn adding_for_later_only_refreshes_the_window() {
        let fixture = fixture();
        run_text(&fixture, &["add", "--later", "https://example.com/a.iso"]).unwrap();
        assert_eq!(fixture.sent.borrow()[0][0], "--refresh");
    }

    #[test]
    fn links_that_are_not_web_addresses_are_refused_before_anything_is_stored() {
        let fixture = fixture();
        let error = run_text(
            &fixture,
            &["add", "https://example.com/ok.iso", "file:///etc/passwd"],
        )
        .unwrap_err();
        assert!(matches!(error, Failure::Usage(_)));
        assert!(fixture.storage.list_downloads().unwrap().is_empty());
        assert!(fixture.sent.borrow().is_empty());
    }

    #[test]
    fn listing_prints_a_table_or_json_and_filters_by_status() {
        let fixture = fixture();
        fixture
            .storage
            .create_download("https://example.com/one.zip", 1)
            .unwrap();

        let table = run_text(&fixture, &["list"]).unwrap();
        assert!(table.contains("one.zip"));
        assert!(table.contains("created"));

        let json_text = run_text(&fixture, &["list", "--json"]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json_text.trim()).unwrap();
        assert_eq!(parsed[0]["name"], "one.zip");

        let none = run_text(&fixture, &["list", "--status", "completed"]).unwrap();
        assert!(none.contains("no downloads"));
        assert!(matches!(
            run_text(&fixture, &["list", "--status", "sleeping"]),
            Err(Failure::Usage(_))
        ));
    }

    #[test]
    fn controls_resolve_short_ids_and_go_to_the_application() {
        let fixture = fixture();
        let task = fixture
            .storage
            .create_download("https://example.com/one.zip", 1)
            .unwrap();
        let prefix: String = task.id.chars().take(6).collect();

        run_text(&fixture, &["retry", &prefix]).unwrap();
        assert_eq!(
            fixture.sent.borrow()[0],
            vec!["--control".to_owned(), "resume".to_owned(), task.id.clone()]
        );

        assert!(matches!(
            run_text(&fixture, &["pause", "ab"]),
            Err(Failure::Runtime(_))
        ));
        assert!(matches!(
            run_text(&fixture, &["pause"]),
            Err(Failure::Usage(_))
        ));
    }

    #[test]
    fn queues_are_found_by_name() {
        let fixture = fixture();
        let printed = run_text(&fixture, &["queue", "start", "default queue"]).unwrap();
        assert!(printed.contains("Default Queue"));
        assert_eq!(fixture.sent.borrow()[0][1], "queue-start");
        assert!(matches!(
            run_text(&fixture, &["queue", "start", "nope"]),
            Err(Failure::Runtime(_))
        ));
    }

    #[test]
    fn a_missing_database_is_explained() {
        let environment = Environment {
            database: Some(PathBuf::from("/definitely/not/here/downloads.db")),
            launch: Box::new(|_| true),
        };
        let mut output = Vec::new();
        let error = run(&["list".to_owned()], &environment, &mut output).unwrap_err();
        assert!(
            matches!(error, Failure::Runtime(message) if message.contains("open the application once"))
        );
    }

    #[test]
    fn sizes_and_progress_read_naturally() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
