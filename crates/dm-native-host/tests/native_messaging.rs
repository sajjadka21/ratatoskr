//! Runs the real native host binary the way a browser does.
//!
//! Unit tests call the host's functions directly, which cannot show what the
//! process as a whole writes to stdout. The browser reads that stream as the
//! protocol, so a single stray byte breaks every handoff; these tests read it
//! exactly as the browser would.

#![cfg(windows)]

use dm_storage::Storage;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use tempfile::tempdir;

const COOKIE: &str = "session=native-host-s3cr3t";

/// A program that writes to stdout and exits at once. Standing in for the
/// application, it shows whether anything a launched program prints can
/// reach the browser's stream, and it never opens the session channel.
fn chatty_stand_in() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32")
        .join("hostname.exe")
}

/// Sends one native message and returns everything the host wrote to stdout.
fn exchange(message: &serde_json::Value, data_dir: &std::path::Path) -> Vec<u8> {
    let payload = serde_json::to_vec(message).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dm-native-host"))
        .env("DOWNLOAD_MANAGER_DATA_DIR", data_dir)
        .env("DOWNLOAD_MANAGER_APP_PATH", chatty_stand_in())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    {
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(&(payload.len() as u32).to_le_bytes())
            .unwrap();
        stdin.write_all(&payload).unwrap();
    }

    child.wait_with_output().unwrap().stdout
}

/// Parses the stream as exactly one native message, failing on anything
/// before, inside or after it.
fn single_reply(stdout: &[u8]) -> serde_json::Value {
    assert!(stdout.len() >= 4, "no reply at all: {stdout:?}");

    let length = u32::from_le_bytes(stdout[..4].try_into().unwrap()) as usize;

    assert_eq!(
        stdout.len(),
        4 + length,
        "the stream must hold exactly one framed reply, found {:?}",
        String::from_utf8_lossy(stdout)
    );

    serde_json::from_slice(&stdout[4..]).unwrap()
}

#[test]
fn an_undeliverable_session_is_refused_cleanly_and_leaves_nothing_behind() {
    let data = tempdir().unwrap();

    let stdout = exchange(
        &serde_json::json!({
            "type": "download",
            "url": "https://example.com/private.zip",
            "referrer": "https://example.com/page",
            "cookies": COOKIE,
        }),
        data.path(),
    );

    let reply = single_reply(&stdout);

    assert_eq!(
        reply["accepted"], false,
        "nothing answered the session channel, so the browser must keep its download"
    );
    assert!(!reply["error"]
        .as_str()
        .unwrap_or_default()
        .contains("s3cr3t"));
    assert!(
        !String::from_utf8_lossy(&stdout).contains("s3cr3t"),
        "the session must never be echoed"
    );

    let storage = Storage::open(data.path().join("downloads.db")).unwrap();
    assert!(
        storage.list_downloads().unwrap().is_empty(),
        "the task created for the handoff must be withdrawn"
    );
    drop(storage);

    for entry in std::fs::read_dir(data.path()).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes
                .windows(COOKIE.len())
                .any(|window| window == COOKIE.as_bytes()),
            "{} holds the session",
            path.display()
        );
    }
}

#[test]
fn a_ping_is_answered_with_one_clean_frame() {
    let data = tempdir().unwrap();

    let reply = single_reply(&exchange(
        &serde_json::json!({ "type": "ping" }),
        data.path(),
    ));

    assert_eq!(reply["accepted"], true);
    assert_eq!(reply["appFound"], true);
}
