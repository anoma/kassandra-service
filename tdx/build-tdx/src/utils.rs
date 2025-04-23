use std::fs;
use std::path::{Path, PathBuf};

/// A guard that ensures the current working directory is restored
/// to its original state when the guard goes out of scope.
pub struct DirGuard(PathBuf);

impl DirGuard {
    /// Creates a new `DirGuard` that restores the provided directory
    /// when it goes out of scope.
    ///
    /// # Arguments
    ///
    /// * `original_dir` - The directory to restore when the guard is dropped.
    pub fn new(original_dir: PathBuf) -> Self {
        Self(original_dir)
    }

    /// Creates a new `DirGuard` using the current working directory as the original directory.
    pub fn from_current_dir() -> Self {
        Self::new(std::env::current_dir().unwrap())
    }

    /// Stores the current directory as the original directory and
    /// changes the working directory to the specified `new_dir`.
    ///
    /// # Arguments
    ///
    /// * `new_dir` - The directory to switch to.
    pub fn change_dir(new_dir: impl AsRef<Path>) -> Self {
        let original_dir_guard = DirGuard::from_current_dir();
        std::env::set_current_dir(new_dir.as_ref()).unwrap();
        original_dir_guard
    }
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

/// Attempts to create a hard link from `from` to `to`.
/// If the hard link operation fails (e.g., due to crossing file systems),
/// it falls back to performing a file copy.
///
/// # Arguments
/// - `from`: The source file path.
/// - `to`: The destination file path.
///
/// # Returns
/// - `Ok(0)` if the hard link is successfully created (no data was copied).
/// - `Ok(size)` where `size` is the number of bytes copied if the hard link failed and a copy was performed.
/// - `Err(error)` if an error occurred during the copy operation.
pub fn hard_link_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> std::io::Result<u64> {
    if fs::hard_link(&from, &to).is_err() {
        return fs::copy(from, to);
    }
    Ok(0)
}
