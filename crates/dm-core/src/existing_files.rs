//! Bounded, read-only destination checks. Similar names are hints, never download identity.
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingFileMatch {
    pub source_name: String,
    pub name: String,
    pub folder: String,
    pub exact: bool,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInspection {
    pub matches: Vec<ExistingFileMatch>,
    pub partial: bool,
}

fn words(name: &str) -> (String, Vec<String>) {
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let mut stem = stem.to_lowercase();
    if let Some(start) = stem.rfind('(')
        && stem.ends_with(')')
        && stem[start + 1..stem.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        stem.truncate(start);
        stem = stem.trim_end().to_owned();
    }
    let ignored = [
        "web", "dl", "webdl", "webrip", "bluray", "bdrip", "h264", "h265", "x264", "x265", "hevc",
        "avc", "aac", "proper", "repack",
    ];
    let words = stem
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| {
            !w.is_empty()
                && !ignored.contains(w)
                && !(w.ends_with('p') && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit()))
        })
        .take(64)
        .map(str::to_owned)
        .collect();
    (extension.to_lowercase(), words)
}

pub fn similar_names(a: &str, b: &str) -> bool {
    let (ae, aw) = words(a);
    let (be, bw) = words(b);
    if ae.is_empty() || ae != be || aw.is_empty() || bw.is_empty() {
        return false;
    }
    // Preserve all numbers, including season/episode numbers embedded in words.
    let numbers = |words: &[String]| {
        words
            .iter()
            .flat_map(|w| {
                w.split(|c: char| !c.is_ascii_digit())
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>()
    };
    if numbers(&aw) != numbers(&bw) {
        return false;
    }
    let mut row = vec![0usize; bw.len() + 1];
    for a in &aw {
        let mut previous = 0;
        for (j, b) in bw.iter().enumerate() {
            let saved = row[j + 1];
            row[j + 1] = if a == b {
                previous + 1
            } else {
                row[j + 1].max(row[j])
            };
            previous = saved;
        }
    }
    let matched = row[bw.len()];
    let smaller = aw.len().min(bw.len());
    matched * 5 >= aw.len().max(bw.len()) * 4
        || (smaller >= 2 && matched == smaller && aw.len().max(bw.len()) <= smaller + 2)
}

pub fn inspect(names: &[String], roots: &[PathBuf]) -> FolderInspection {
    let mut result = FolderInspection::default();
    let mut seen = HashSet::new();
    let mut pending: VecDeque<_> = roots.iter().cloned().map(|p| (p, 0)).collect();
    let mut entries = 0;
    let started = Instant::now();
    let mut comparisons = 0;
    while let Some((folder, depth)) = pending.pop_front() {
        if !seen.insert(folder.clone()) {
            continue;
        }
        match std::fs::symlink_metadata(&folder) {
            Ok(meta) if meta.file_type().is_symlink() => {
                result.partial = true;
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                result.partial = true;
                continue;
            }
            _ => {}
        }
        let Ok(children) = std::fs::read_dir(&folder) else {
            result.partial = true;
            continue;
        };
        for entry in children {
            entries += 1;
            if entries > 20_000
                || result.matches.len() >= 64
                || started.elapsed() > Duration::from_secs(2)
            {
                result.partial = true;
                return result;
            }
            let Ok(entry) = entry else {
                result.partial = true;
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                result.partial = true;
                continue;
            };
            if kind.is_dir() && depth < 2 {
                pending.push_back((entry.path(), depth + 1));
            } else if kind.is_dir() {
                result.partial = true;
            }
            if !kind.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            for candidate in names {
                comparisons += 1;
                if comparisons > 50_000 || result.matches.len() >= 64 {
                    result.partial = true;
                    return result;
                }
                let exact = candidate.eq_ignore_ascii_case(&name);
                if exact || similar_names(candidate, &name) {
                    result.matches.push(ExistingFileMatch {
                        source_name: candidate.clone(),
                        name: name.clone(),
                        folder: folder.to_string_lossy().into_owned(),
                        exact,
                    });
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_keep_episodes_and_file_types_distinct() {
        assert!(similar_names(
            "The.Mentalist.S04E01.720p.WEB-DL.x265.mkv",
            "The.Mentalist.S04E01.720p.WEB-DL.x265.MovieCottage.mkv"
        ));
        assert!(!similar_names("Show.S04E01.mkv", "Show.S04E02.mkv"));
        assert!(!similar_names("Show.S04E01.mkv", "Show.S04E01.zip"));
    }
    #[test]
    fn checks_real_files_including_files_not_in_history_without_mutating() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Video");
        std::fs::create_dir(&folder).unwrap();
        let file = folder.join("Show.S04E01.site.mkv");
        std::fs::write(&file, b"unchanged").unwrap();
        let result = inspect(&["Show.S04E01.mkv".into()], &[root.path().into()]);
        assert_eq!(result.matches.len(), 1);
        assert!(!result.matches[0].exact);
        let result = inspect(&["Show.S04E01.site.mkv".into()], &[root.path().into()]);
        assert_eq!(result.matches.len(), 1);
        assert!(result.matches[0].exact);
        assert!(!result.partial);
        assert_eq!(std::fs::read(file).unwrap(), b"unchanged");
    }
    #[test]
    fn absent_folders_are_not_created() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        assert!(
            inspect(&["a.zip".into()], std::slice::from_ref(&missing))
                .matches
                .is_empty()
        );
        assert!(!missing.exists());
    }
    #[test]
    fn skipped_deep_folders_are_reported_as_partial() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("a/b/c")).unwrap();
        std::fs::write(root.path().join("a/b/c/video.mp4"), b"untouched").unwrap();
        let result = inspect(&["video.mp4".into()], &[root.path().into()]);
        assert!(result.matches.is_empty());
        assert!(result.partial);
    }
}
