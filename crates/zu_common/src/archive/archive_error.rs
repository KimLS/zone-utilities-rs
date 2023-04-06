use nom::error::ErrorKind;
use nom::error::ParseError;
use std::str::Utf8Error;
use thiserror::Error;

/// An error indicating errors that can happen with archive access
#[derive(Error, Debug)]
pub enum ArchiveError {
    /// I/O error
    /// Error reading from or writing to a std::io source
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    ///
    ///
    #[error("Wrong version found: {version:?}")]
    WrongVersion { version: u32 },

    ///
    ///
    #[error("Parse Error")]
    Parse(String),

    /// Compression failed
    /// Zlib compression encountered and error
    #[error("Compression failed")]
    Compression,

    /// Decompression failed
    /// Zlib decompression encountered and error
    #[error("Decompression failed")]
    Decompression,

    /// Source file already exists in archive
    /// When trying to put a file into the archive a file with that name already exists
    #[error("Source file already exists in archive")]
    SrcFileAlreadyExists,

    /// Source file doesn't exist in archive
    /// When trying to get a file from the archive a file with that name doesn't exist
    #[error("Source file doesn't exist in archive")]
    SrcFileNotFound,

    /// Destination file already exists in archive
    /// When trying to put a file into the archive a file with that name already exists
    #[error("Destination file already exists in archive")]
    DestFileAlreadyExists,

    /// Bad Regular Expression
    /// Regular expression was malformed
    #[error("Bad Regular Expression")]
    BadRegex(#[from] regex::Error),

    /// Bad UTF8
    /// String data was not valid UTF-8
    #[error("Bad UTF-8")]
    Utf8(#[from] Utf8Error),

    /// Unknown Error
    /// Basically any error that is unexpected
    #[error("Unknown Error")]
    Unknown,
}

impl<I> ParseError<I> for ArchiveError {
    fn from_error_kind(_: I, kind: ErrorKind) -> Self {
        ArchiveError::Parse(format!("Parse error of type: {:?}", kind))
    }

    fn append(_: I, _: ErrorKind, other: Self) -> Self {
        other
    }
}
