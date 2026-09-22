use std::path::{Component, Path, PathBuf};

/// Resolves an archive member under the destination directory without
/// allowing absolute paths or `..` traversal. Archive extraction backends can
/// use this guard before writing each member.
pub fn safe_member_path(destination: &Path, member: &str) -> Option<PathBuf> {
    let relative = Path::new(member);
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
    }
}
