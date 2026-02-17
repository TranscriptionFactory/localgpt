// Path resolution and directory scoping utilities for tool security.

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

/// Resolve a user-provided path to its real filesystem path.
///
/// Expands `~` via shellexpand, then canonicalizes. For new files where the
/// path doesn't exist yet, canonicalizes the parent and appends the filename.
pub fn resolve_real_path(path: &str) -> Result<PathBuf> {
    let expanded = shellexpand::tilde(path).to_string();
    let p = PathBuf::from(&expanded);

    // Try canonicalize directly (works for existing paths)
    if let Ok(canonical) = fs::canonicalize(&p) {
        return Ok(canonical);
    }

    // For new files: canonicalize parent, append filename
    if let (Some(parent), Some(filename)) = (p.parent(), p.file_name())
        && let Ok(canonical_parent) = fs::canonicalize(parent)
    {
        return Ok(canonical_parent.join(filename));
    }

    // Fallback: return the expanded path as-is
    Ok(p)
}

/// Check whether a resolved path is within one of the allowed directories.
/// If `allowed_dirs` is empty, all paths are allowed (unrestricted mode).
pub fn check_path_allowed(real_path: &std::path::Path, allowed_dirs: &[PathBuf]) -> Result<()> {
    if allowed_dirs.is_empty() {
        return Ok(());
    }

    for dir in allowed_dirs {
        if real_path.starts_with(dir) {
            return Ok(());
        }
    }

    Err(anyhow::anyhow!(
        "Path denied: {} is outside allowed directories",
        real_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn resolve_existing_path() {
        // /tmp always exists
        let result = resolve_real_path("/tmp").unwrap();
        // On macOS /tmp -> /private/tmp
        assert!(result.exists() || result.to_str().unwrap().contains("tmp"));
    }

    #[test]
    fn resolve_nonexistent_file_in_existing_dir() {
        let result = resolve_real_path("/tmp/nonexistent_test_file_xyz.txt").unwrap();
        assert!(
            result
                .to_str()
                .unwrap()
                .contains("nonexistent_test_file_xyz.txt")
        );
    }

    #[test]
    fn resolve_tilde_expansion() {
        let result = resolve_real_path("~/some_file.txt").unwrap();
        // Should not start with ~
        assert!(!result.to_str().unwrap().starts_with('~'));
    }

    #[test]
    fn check_path_allowed_empty_permits_all() {
        let dirs: Vec<PathBuf> = vec![];
        assert!(check_path_allowed(Path::new("/etc/passwd"), &dirs).is_ok());
    }

    #[test]
    fn check_path_allowed_within_dir() {
        let dirs = vec![PathBuf::from("/tmp"), PathBuf::from("/home")];
        assert!(check_path_allowed(Path::new("/tmp/foo.txt"), &dirs).is_ok());
        assert!(check_path_allowed(Path::new("/home/user/file"), &dirs).is_ok());
    }

    #[test]
    fn check_path_denied_outside_dir() {
        let dirs = vec![PathBuf::from("/tmp")];
        assert!(check_path_allowed(Path::new("/etc/passwd"), &dirs).is_err());
    }
}
