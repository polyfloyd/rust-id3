use crate::tag::Tag;
use std::error;
use std::fmt;
use std::io;
use std::string;

/// Type alias for the result of tag operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Takes a tag result and maps any partial tag to Ok. An Ok result is left untouched. An Err
/// without partial tag is returned as the initial error.
///
/// # Example
/// ```
/// use id3::{Tag, Error, ErrorKind, partial_tag_ok};
///
/// let rs = Err(Error{
///     kind: ErrorKind::Parsing,
///     description: "frame 12 could not be decoded".to_string(),
///     partial_tag: Some(Tag::new()),
/// });
/// assert!(partial_tag_ok(rs).is_ok());
/// ```
pub fn partial_tag_ok(rs: Result<Tag>) -> Result<Tag> {
    match rs {
        Ok(tag) => Ok(tag),
        Err(Error {
            partial_tag: Some(tag),
            ..
        }) => Ok(tag),
        Err(err) => Err(err),
    }
}

/// Takes a tag result and maps the NoTag kind to None. Any other error is returned as Err.
///
/// # Example
/// ```
/// use id3::{Tag, Error, ErrorKind, no_tag_ok};
///
/// let rs = Err(Error{
///     kind: ErrorKind::NoTag,
///     description: "the file contains no ID3 tag".to_string(),
///     partial_tag: None,
/// });
/// assert!(matches!(no_tag_ok(rs), Ok(None)));
///
/// let rs = Err(Error{
///     kind: ErrorKind::Parsing,
///     description: "frame 12 could not be decoded".to_string(),
///     partial_tag: Some(Tag::new()),
/// });
/// assert!(no_tag_ok(rs).is_err());
/// ```
pub fn no_tag_ok(rs: Result<Tag>) -> Result<Option<Tag>> {
    match rs {
        Ok(tag) => Ok(Some(tag)),
        Err(Error {
            kind: ErrorKind::NoTag,
            ..
        }) => Ok(None),
        Err(err) => Err(err),
    }
}

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
    /// An error kind indicating that parsing of some binary data has failed.
    Parsing,
    /// An error kind indicating that some input to a function was invalid.
    InvalidInput,
    /// An error kind indicating that a feature is not supported.
    UnsupportedFeature,
}

/// A structure able to represent any error that may occur while performing metadata operations.
pub struct Error {
    /// The kind of error.
    pub kind: ErrorKind,
    /// A human readable string describing the error.
    pub description: String,
    /// If any, the part of the tag that was able to be decoded before the error occurred.
    pub partial_tag: Option<Tag>,
}

impl Error {
    /// Creates a new `Error` using the error kind and description.
    pub fn new(kind: ErrorKind, description: impl Into<String>) -> Error {
        Error {
            kind,
            description: description.into(),
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
            description: "".to_string(),
            partial_tag: None,
        }
    }
}

impl From<string::FromUtf8Error> for Error {
    fn from(err: string::FromUtf8Error) -> Error {
        Error {
            kind: ErrorKind::StringDecoding(err.into_bytes()),
            description: "data is not valid utf-8".to_string(),
            partial_tag: None,
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.description.is_empty() {
            true => write!(f, "{:?}", self.kind),
            false => write!(f, "{:?}: {}", self.kind, self.description),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.description.is_empty() {
            true => write!(f, "{}", self.kind),
            false => write!(f, "{}: {}", self.kind, self.description),
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ErrorKind::Io(io_error) => write!(f, "IO: {}", io_error),
            ErrorKind::StringDecoding(_) => write!(f, "StringDecoding"),
            ErrorKind::NoTag => write!(f, "NoTag"),
            ErrorKind::Parsing => write!(f, "Parsing"),
            ErrorKind::InvalidInput => write!(f, "InvalidInput"),
            ErrorKind::UnsupportedFeature => write!(f, "UnsupportedFeature"),
        }
    }
}
