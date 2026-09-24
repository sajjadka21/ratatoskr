//! What happens to a file after it is downloaded: checking it against a
//! published checksum, extracting ZIP archives, a Windows Defender scan, and
//! a command of the user's choosing.
//!
//! Every step is off unless the user turns it on (or gives a checksum), and
//! none of them ever deletes the download or runs it: an archive is kept
//! after extraction, and nothing downloaded is executed.

use std::{
    ffi::OsString,
    fs::File,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use thiserror::Error;

/// Resolves an archive member under the destination directory without
/// allowing absolute paths or `..` traversal. Archive extraction backends can
/// use this guard before writing each member.
pub fn safe_member_path(destination: &Path, member: &str) -> Option<PathBuf> {
    // Archives written on Windows use `\` separators, and a drive prefix
    // (`C:`) is absolute there even when the host platform would not treat it
    // so. Normalise first so the same member is judged the same everywhere.
    let normalized = member.replace('\\', "/");
    let bytes = normalized.as_bytes();
    if normalized.is_empty()
        || normalized.starts_with('/')
        || (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
    {
        return None;
    }
    let relative = Path::new(&normalized);
    if relative.is_absolute() {
        return None;
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(destination.join(relative))
}

// ---------------------------------------------------------------------------
// Checksums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
}

impl HashAlgorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "md5" => Some(Self::Md5),
            "sha1" => Some(Self::Sha1),
            "sha256" => Some(Self::Sha256),
            _ => None,
        }
    }
}

/// A checksum as sites publish it: bare hex, `sha256:hex`, or a line of a
/// `SHA256SUMS` file (`hex  filename`). The algorithm follows from the
/// length: 32 hex digits are MD5, 40 SHA-1, 64 SHA-256.
pub fn parse_expected_checksum(text: &str) -> Option<(HashAlgorithm, String)> {
    let text = text.trim();
    let text = text
        .split_once(':')
        .filter(|(prefix, _)| prefix.len() <= 8)
        .map_or(text, |(_, rest)| rest)
        .trim();
    let hex = text.split_whitespace().next()?.to_ascii_lowercase();
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    let algorithm = match hex.len() {
        32 => HashAlgorithm::Md5,
        40 => HashAlgorithm::Sha1,
        64 => HashAlgorithm::Sha256,
        _ => return None,
    };
    Some((algorithm, hex))
}

/// Hashes a file in 1 MB reads. Blocking: run it off the async runtime.
pub fn hash_file(path: &Path, algorithm: HashAlgorithm) -> io::Result<String> {
    use md5::Digest as _;

    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    macro_rules! digest_with {
        ($hasher:expr) => {{
            let mut hasher = $hasher;
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            to_hex(&hasher.finalize())
        }};
    }
    Ok(match algorithm {
        HashAlgorithm::Md5 => digest_with!(md5::Md5::new()),
        HashAlgorithm::Sha1 => digest_with!(sha1::Sha1::new()),
        HashAlgorithm::Sha256 => digest_with!(sha2::Sha256::new()),
    })
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------
// ZIP extraction
// ---------------------------------------------------------------------------

/// Guards against archives built to fill the disk.
const MAX_EXTRACTED_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_EXPANSION_RATIO: u64 = 200;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("the archive could not be read: {0}")]
    Archive(String),
    #[error("the archive is password-protected")]
    Encrypted,
    #[error("the archive would unpack to far more than its own size, so it was not extracted")]
    TooLarge,
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    pub folder: PathBuf,
    pub files: usize,
    pub bytes: u64,
    /// Members skipped because their path would leave the folder.
    pub skipped: usize,
}

/// Whether a file is a ZIP archive, by its first bytes rather than its name.
pub fn is_zip(path: &Path) -> bool {
    let mut magic = [0_u8; 4];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok()
        && magic == *b"PK\x03\x04"
}

/// Extracts a ZIP archive into a new folder beside it, named after it. The
/// archive itself is kept. Members that would land outside the folder are
/// skipped, links are not created, and nothing is run. Blocking.
pub fn extract_zip(archive: &Path) -> Result<ExtractReport, ExtractError> {
    let file = File::open(archive)?;
    let archive_size = file.metadata()?.len();
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| ExtractError::Archive(error.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(ExtractError::TooLarge);
    }

    // The declared sizes are checked before anything is written.
    let mut declared: u64 = 0;
    for index in 0..zip.len() {
        let entry = zip
            .by_index_raw(index)
            .map_err(|error| ExtractError::Archive(error.to_string()))?;
        if entry.encrypted() {
            return Err(ExtractError::Encrypted);
        }
        declared = declared.saturating_add(entry.size());
    }
    let limit = MAX_EXTRACTED_BYTES.min(
        archive_size
            .saturating_mul(MAX_EXPANSION_RATIO)
            .max(1024 * 1024),
    );
    if declared > limit {
        return Err(ExtractError::TooLarge);
    }

    let folder = unique_folder(archive)?;
    std::fs::create_dir_all(&folder)?;
    let mut report = ExtractReport {
        folder: folder.clone(),
        files: 0,
        bytes: 0,
        skipped: 0,
    };

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| ExtractError::Archive(error.to_string()))?;
        let Some(target) = safe_member_path(&folder, entry.name()) else {
            report.skipped += 1;
            continue;
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if entry.is_symlink() {
            report.skipped += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        // The real size is limited too: a member may lie about its size.
        let remaining = limit.saturating_sub(report.bytes);
        let copied = io::copy(&mut (&mut entry).take(remaining + 1), &mut output)?;
        if copied > remaining {
            drop(output);
            let _ = std::fs::remove_file(&target);
            return Err(ExtractError::TooLarge);
        }
        report.bytes += copied;
        report.files += 1;
    }
    Ok(report)
}

/// `archive.zip` → `archive`, or `archive (2)` when that exists.
fn unique_folder(archive: &Path) -> io::Result<PathBuf> {
    let parent = archive.parent().unwrap_or(Path::new("."));
    let stem = archive
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "archive".to_owned());
    for attempt in 1..1000 {
        let name = if attempt == 1 {
            stem.clone()
        } else {
            format!("{stem} ({attempt})")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::other(
        "no free folder name for the extracted files",
    ))
}

// ---------------------------------------------------------------------------
// Virus scan and the user's own command
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanResult {
    Clean,
    ThreatFound,
    /// The scanner is missing or could not decide.
    Unavailable(String),
}

/// Windows Defender's command-line scanner, when Windows has one.
pub fn defender_path() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    // The platform folder holds the current engine; the Program Files copy
    // is older but always there.
    let platform = std::env::var_os("ProgramData")
        .map(|base| PathBuf::from(base).join("Microsoft\\Windows Defender\\Platform"))
        .and_then(|folder| std::fs::read_dir(folder).ok())
        .and_then(|entries| {
            let mut versions: Vec<PathBuf> =
                entries.flatten().map(|entry| entry.path()).collect();
            versions.sort();
            versions
                .into_iter()
                .rev()
                .map(|folder| folder.join("MpCmdRun.exe"))
                .find(|path| path.is_file())
        });
    platform.or_else(|| {
        std::env::var_os("ProgramFiles")
            .map(|base| PathBuf::from(base).join("Windows Defender\\MpCmdRun.exe"))
            .filter(|path| path.is_file())
    })
}

/// Scans one file with `scanner` (MpCmdRun). It never removes or
/// quarantines anything itself (`-DisableRemediation`): it reports.
pub async fn scan_file(scanner: &Path, file: &Path) -> ScanResult {
    let mut command = tokio::process::Command::new(scanner);
    command
        .arg("-Scan")
        .arg("-ScanType")
        .arg("3")
        .arg("-File")
        .arg(file)
        .arg("-DisableRemediation")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    hide_window(&mut command);
    match tokio::time::timeout(Duration::from_secs(600), command.status()).await {
        Ok(Ok(status)) => match status.code() {
            Some(0) => ScanResult::Clean,
            Some(2) => ScanResult::ThreatFound,
            other => ScanResult::Unavailable(format!("scanner exit code {other:?}")),
        },
        Ok(Err(error)) => ScanResult::Unavailable(error.to_string()),
        Err(_) => ScanResult::Unavailable("the scan took too long".to_owned()),
    }
}

/// Splits the user's command template into a program and arguments,
/// honouring double quotes, and fills `{file}`, `{folder}` and `{name}`.
/// No shell is involved, so a file name can never become a second command.
pub fn build_command(template: &str, file: &Path) -> Option<(OsString, Vec<OsString>)> {
    let folder = file.parent().unwrap_or(Path::new("."));
    let name = file.file_name().unwrap_or_default();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for character in template.trim().chars() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            character if character.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            character => {
                current.push(character);
                started = true;
            }
        }
    }
    if started {
        words.push(current);
    }
    if quoted || words.is_empty() {
        return None;
    }
    let fill = |word: &str| -> OsString {
        // Each placeholder is replaced within its own argument only.
        let mut output = OsString::new();
        let mut rest = word;
        while let Some(start) = rest.find('{') {
            output.push(&rest[..start]);
            let after = &rest[start..];
            let (value, length): (Option<&std::ffi::OsStr>, usize) =
                if after.starts_with("{file}") {
                    (Some(file.as_os_str()), 6)
                } else if after.starts_with("{folder}") {
                    (Some(folder.as_os_str()), 8)
                } else if after.starts_with("{name}") {
                    (Some(name), 6)
                } else {
                    (None, 1)
                };
            match value {
                Some(value) => output.push(value),
                None => output.push("{"),
            }
            rest = &after[length..];
        }
        output.push(rest);
        output
    };
    let program = fill(&words[0]);
    let arguments = words[1..].iter().map(|word| fill(word)).collect();
    Some((program, arguments))
}

/// Runs the user's command for a finished file. Returns an error message
/// when it could not start or ended unsuccessfully.
pub async fn run_command(template: &str, file: &Path) -> Result<(), String> {
    let (program, arguments) = build_command(template, file)
        .ok_or_else(|| "the command is empty or has an unclosed quote".to_owned())?;
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .current_dir(file.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    hide_window(&mut command);
    match tokio::time::timeout(Duration::from_secs(600), command.status()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!("the command ended with {status}")),
        Ok(Err(error)) => Err(format!("the command could not start: {error}")),
        Err(_) => Err("the command ran for more than ten minutes and was stopped".to_owned()),
    }
}

#[cfg(windows)]
fn hide_window(command: &mut tokio::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn rejects_archive_traversal_and_absolute_members() {
        let destination = Path::new("C:\\Downloads");
        assert!(safe_member_path(destination, "folder/file.txt").is_some());
        assert!(safe_member_path(destination, "..\\outside.txt").is_none());
        assert!(safe_member_path(destination, "C:\\outside.txt").is_none());
        assert!(safe_member_path(destination, "C:relative.txt").is_none());
        assert!(safe_member_path(destination, "/etc/passwd").is_none());
        assert!(safe_member_path(destination, "a\\..\\..\\b.txt").is_none());
        assert!(safe_member_path(destination, "").is_none());
    }

    #[test]
    fn checksums_are_read_in_every_common_shape() {
        let sha = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        assert_eq!(
            parse_expected_checksum(sha),
            Some((HashAlgorithm::Sha256, sha.to_ascii_lowercase()))
        );
        assert_eq!(
            parse_expected_checksum(&format!("sha256:{sha}")).map(|(algorithm, _)| algorithm),
            Some(HashAlgorithm::Sha256)
        );
        assert_eq!(
            parse_expected_checksum(&format!("{sha}  ubuntu-26.04.iso"))
                .map(|(algorithm, _)| algorithm),
            Some(HashAlgorithm::Sha256)
        );
        assert_eq!(
            parse_expected_checksum("d41d8cd98f00b204e9800998ecf8427e")
                .map(|(algorithm, _)| algorithm),
            Some(HashAlgorithm::Md5)
        );
        assert_eq!(
            parse_expected_checksum("da39a3ee5e6b4b0d3255bfef95601890afd80709")
                .map(|(algorithm, _)| algorithm),
            Some(HashAlgorithm::Sha1)
        );
        assert_eq!(parse_expected_checksum("not a checksum"), None);
        assert_eq!(parse_expected_checksum("abc123"), None);
    }

    #[test]
    fn files_hash_to_the_published_values() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("hello.txt");
        std::fs::write(&path, b"hello\n").unwrap();
        assert_eq!(
            hash_file(&path, HashAlgorithm::Sha256).unwrap(),
            "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"
        );
        assert_eq!(
            hash_file(&path, HashAlgorithm::Sha1).unwrap(),
            "f572d396fae9206628714fb2ce00f72e94f2258f"
        );
        assert_eq!(
            hash_file(&path, HashAlgorithm::Md5).unwrap(),
            "b1946ac92492d2347c6235b4d2611184"
        );
    }

    pub(crate) fn zip_with(path: &Path, members: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in members {
            writer.start_file(*name, options).unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn archives_unpack_beside_themselves_and_are_kept() {
        let directory = tempdir().unwrap();
        let archive = directory.path().join("photos.zip");
        zip_with(&archive, &[("a.txt", b"first"), ("inner/b.txt", b"second")]);
        assert!(is_zip(&archive));

        let report = extract_zip(&archive).unwrap();
        assert_eq!(report.folder, directory.path().join("photos"));
        assert_eq!(report.files, 2);
        assert_eq!(
            std::fs::read(report.folder.join("inner/b.txt")).unwrap(),
            b"second"
        );
        assert!(archive.exists(), "the archive is never deleted");

        // A second extraction gets its own folder instead of overwriting.
        let again = extract_zip(&archive).unwrap();
        assert_eq!(again.folder, directory.path().join("photos (2)"));
    }

    #[test]
    fn members_that_escape_the_folder_are_skipped() {
        let directory = tempdir().unwrap();
        let archive = directory.path().join("evil.zip");
        zip_with(&archive, &[("../escaped.txt", b"x"), ("ok.txt", b"fine")]);

        let report = extract_zip(&archive).unwrap();
        assert_eq!(report.files, 1);
        assert_eq!(report.skipped, 1);
        assert!(!directory.path().join("escaped.txt").exists());
    }

    #[test]
    fn archives_that_expand_absurdly_are_refused() {
        let directory = tempdir().unwrap();
        let archive = directory.path().join("bomb.zip");
        // 64 MB of zeros compresses to about 64 KB: far past the 200x ratio.
        let zeros = vec![0_u8; 64 * 1024 * 1024];
        zip_with(&archive, &[("zeros.bin", &zeros)]);

        match extract_zip(&archive) {
            Err(ExtractError::TooLarge) => {}
            other => panic!("expected TooLarge, got {other:?}"),
        }
        assert!(!directory.path().join("bomb").join("zeros.bin").exists());
        assert!(!is_zip(&directory.path().join("missing.zip")));
    }

    #[test]
    fn commands_are_split_without_a_shell_and_filled_per_argument() {
        let file = Path::new("/downloads/My File; rm -rf.iso");
        let (program, arguments) = build_command(
            r#""C:\Tools\check.exe" --file {file} --in "{folder}" -n {name}"#,
            file,
        )
        .unwrap();
        assert_eq!(program, OsString::from(r"C:\Tools\check.exe"));
        assert_eq!(
            arguments,
            vec![
                OsString::from("--file"),
                OsString::from("/downloads/My File; rm -rf.iso"),
                OsString::from("--in"),
                OsString::from("/downloads"),
                OsString::from("-n"),
                OsString::from("My File; rm -rf.iso"),
            ]
        );
        assert_eq!(build_command("   ", file), None);
        assert_eq!(build_command(r#""unclosed {file}"#, file), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_users_command_and_a_scanner_report_their_outcome() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().unwrap();
        let file = directory.path().join("done.bin");
        std::fs::write(&file, b"x").unwrap();
        let marker = directory.path().join("ran.txt");
        let executable = |name: &str, body: &str| {
            let path = directory.path().join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };

        let script = executable("record.sh", "#!/bin/sh\necho \"$1\" > \"$2\"\n");
        let template = format!("{} {{file}} {}", script.display(), marker.display());
        run_command(&template, &file).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap().trim(),
            file.to_string_lossy()
        );
        assert!(
            run_command("/definitely/not/a/program {file}", &file)
                .await
                .is_err()
        );

        let threat = executable("threat.sh", "#!/bin/sh\nexit 2\n");
        assert_eq!(scan_file(&threat, &file).await, ScanResult::ThreatFound);
        let clean = executable("clean.sh", "#!/bin/sh\nexit 0\n");
        assert_eq!(scan_file(&clean, &file).await, ScanResult::Clean);
        assert!(defender_path().is_none());
    }
}
