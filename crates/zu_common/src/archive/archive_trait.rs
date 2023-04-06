//! Archive traits
//!
//! Currently used by the various PFS archives to implement a common interface

use super::archive_error::ArchiveError;

/// All archives implement this
/// Indicates an archive that can be created and closed
pub trait IArchive {
    /// Create a new archive in an empty state
    fn new() -> Self;
    /// Put the archive into an empty state
    fn close(&mut self);
}

/// Provides read access to an archive
pub trait IReadableArchive {
    /// Open an archive by parsing it from a block of bytes
    fn open_from_bytes<T>(&mut self, input: T) -> Result<(), ArchiveError>
    where
        T: AsRef<[u8]>;
    /// Open an archive by parsing it from a file on the file system
    fn open_file(&mut self, filename: &str) -> Result<(), ArchiveError>;
    /// Extract a file from the archive into a Vec<u8>
    fn get(&self, in_archive_path: &str) -> Result<Vec<u8>, ArchiveError>;
    /// Check to see if a file exists in the archive
    fn exists(&self, in_archive_path: &str) -> Result<bool, ArchiveError>;
    /// Search for files in the archive by passing a regex string
    fn search(&self, search_regex: &str) -> Result<Vec<String>, ArchiveError>;
}

/// Provides write access to an archive
pub trait IWritableArchive {
    /// Save the contents of an archive to a block of bytes
    fn save_to_bytes(&self) -> Result<Vec<u8>, ArchiveError>;
    /// Save the contents of an archive to a file on the file system
    fn save_to_file(&self, filename: &str) -> Result<(), ArchiveError>;
    /// Sets a file in the archive to a specific block of bytes
    fn set<T>(&mut self, in_archive_path: &str, input: T) -> Result<(), ArchiveError>
    where
        T: AsRef<[u8]>;
    /// Removes a file in the archive
    fn remove(&mut self, in_archive_path: &str) -> Result<(), ArchiveError>;
    /// Renames a file in the archive
    fn rename(
        &mut self,
        in_archive_path: &str,
        new_in_archive_path: &str,
    ) -> Result<(), ArchiveError>;
    /// Copies a file in the archive
    fn copy(
        &mut self,
        in_archive_path: &str,
        new_in_archive_path: &str,
    ) -> Result<(), ArchiveError>;
}
