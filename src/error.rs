use crate::tag::Tag;
use std::error;
use std::fmt;
use std::io;
use std::str;
use std::string;

/// Type alias for the result of tag operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Kinds of errors that may occur while performing metadata operations.
#[derive(Debug)]
pub enum ErrorKind {
    /// An error kind indicating that an IO error has occurred. Contains the original io::Error.
    Io(io::Error),
    /// An error kind indicating that a string decoding error has occurred. Contains the invalid
    /// bytes.
    StringDecoding(Vec<u8>),
    /// An error kind indicating that the reader does not contain an ID3 tag.
    NoTag,
    /// An error kind indicating that the reader contains an unsupported ID3 tag version. Contains
    /// the major and minor versions that were detected in the tag.
    UnsupportedVersion(u8, u8),
    /// An error kind indicating that parsing error has occurred.
    Parsing,
    /// An error kind indicating that some input was invalid.
    InvalidInput,
    /// An error kind indicating that a feature is not supported.
    UnsupportedFeature,
}

/// A structure able to represent any error that may occur while performing metadata operations.
pub struct Error {
    /// The kind of error.
    pub kind: ErrorKind,
    /// A human readable string describing the error.
    pub description: &'static str,
    /// If any, the part of the tag that was able to be decoded before the error occurred.
    pub partial_tag: Option<Tag>,
}

impl Error {
    /// Creates a new `Error` using the error kind and description.
    pub fn new(kind: ErrorKind, description: &'static str) -> Error {
        Error {
            kind,
            description,
            partial_tag: None,
        }
    }

    /// Creates a new `Error` using the error kind and description.
    pub(crate) fn with_tag(self, tag: Tag) -> Error {
        Error {
            partial_tag: Some(tag),
            ..self
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        if let Some(cause) = self.source() {
            cause.description()
        } else {
            match self.kind {
                ErrorKind::Io(ref err) => error::Error::description(err),
                _ => self.description,
            }
        }
    }

    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self.kind {
            ErrorKind::Io(ref err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error {
            kind: ErrorKind::Io(err),
            description: "",
            partial_tag: None,
        }
    }
}

impl From<string::FromUtf8Error> for Error {
    fn from(err: string::FromUtf8Error) -> Error {
        Error {
            kind: ErrorKind::StringDecoding(err.into_bytes()),
            description: "data is not valid utf-8",
            partial_tag: None,
        }
    }
}

impl From<str::Utf8Error> for Error {
    fn from(_: str::Utf8Error) -> Error {
        Error {
            kind: ErrorKind::StringDecoding(vec![]),
            description: "data is not valid utf-8",
            partial_tag: None,
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        if self.description != "" {
            write!(out, "{:?}: {}", self.kind, error::Error::description(self))
        } else {
            write!(out, "{}", error::Error::description(self))
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        if self.description != "" {
            write!(out, "{:?}: {}", self.kind, error::Error::description(self))
        } else {
            write!(out, "{}", error::Error::description(self))
        }
    }
}
