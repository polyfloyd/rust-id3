#[derive(Debug)]
/// The decoded contents of a frame.
pub enum Content {
    /// A value containing the parsed contents of a text frame.
    Text(String),
    /// A value containing the parsed contents of a user defined text frame (TXXX).
    ExtendedText(super::ExtendedText),
    /// A value containing the parsed contents of a web link frame.
    Link(String),
    /// A value containing the parsed contents of a user defined web link frame (WXXX).
    ExtendedLink(super::ExtendedLink),
    /// A value containing the parsed contents of a comment frame (COMM).
    Comment(super::Comment),
    /// A value containing the parsed contents of a lyrics frame (USLT).
    Lyrics(super::Lyrics),
    /// A value containing the parsed contents of a picture frame (APIC).
    Picture(super::Picture),
    /// A value containing the bytes of a unknown frame.
    Unknown(Vec<u8>),
}

impl Content {
    /// Returns the `Text` or None if the value is not `Text`.
    pub fn text(&self) -> Option<&str> {
        match *self {
            Content::Text(ref content) => Some(&*content),
            _ => None,
        }
    }

    /// Returns the `ExtendedText` or None if the value is not `ExtendedText`.
    pub fn extended_text(&self) -> Option<&super::ExtendedText> {
        match *self {
            Content::ExtendedText(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Link` or None if the value is not `Link`.
    pub fn link(&self) -> Option<&str> {
        match *self {
            Content::Link(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `ExtendedLink` or None if the value is not `ExtendedLink`.
    pub fn extended_link(&self) -> Option<&super::ExtendedLink> {
        match *self {
            Content::ExtendedLink(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Comment` or None if the value is not `Comment`.
    pub fn comment(&self) -> Option<&super::Comment> {
        match *self {
            Content::Comment(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Lyrics` or None if the value is not `Lyrics`.
    pub fn lyrics(&self) -> Option<&super::Lyrics> {
        match *self {
            Content::Lyrics(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Picture` or None if the value is not `Picture`.
    pub fn picture(&self) -> Option<&super::Picture> {
        match *self {
            Content::Picture(ref picture) => Some(picture),
            _ => None,
        }
    }

    /// Returns the `Unknown` or None if the value is not `Unknown`.
    pub fn unknown(&self) -> Option<&[u8]> {
        match *self {
            Content::Unknown(ref data) => Some(&data[..]),
            _ => None,
        }
    }
}

