use std::path::{Component, Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::safe_member_path;
    use std::path::Path;

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
}
