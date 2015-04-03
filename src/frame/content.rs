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
    /// Returns the `Text`.
    /// Panics if the value is not `Text`.
    #[inline]
    pub fn text(&self) -> &String {
        match *self {
            Content::Text(ref content) => content,
            _ => panic!("called `Content::text()` on a non `Text` value") 
        }
    }

    /// Returns the `ExtendedText`.
    /// Panics if the value is not `ExtendedText`.
    #[inline]
    pub fn extended_text(&self) -> &super::ExtendedText {
        match *self {
            Content::ExtendedText(ref content) => content,
            _ => panic!("called `Content::extended_text()` on a non `ExtendedText` value") 
        }
    }

    /// Returns the `Link`.
    /// Panics if the value is not `Link`.
    #[inline]
    pub fn link(&self) -> &String {
        match *self {
            Content::Link(ref content) => content,
            _ => panic!("called `Content::link()` on a non `Link` value") 
        }
    }

    /// Returns the `ExtendedLink`.
    /// Panics if the value is not `ExtendedLink`.
    #[inline]
    pub fn extended_link(&self) -> &super::ExtendedLink {
        match *self {
            Content::ExtendedLink(ref content) => content,
            _ => panic!("called `Content::extended_link()` on a non `ExtendedLink` value") 
        }
    }

    /// Returns the `Comment`.
    /// Panics if the value is not `Comment`.
    #[inline]
    pub fn comment(&self) -> &super::Comment {
        match *self {
            Content::Comment(ref content) => content,
            _ => panic!("called `Content::comment()` on a non `Comment` value") 
        }
    }

    /// Returns the `Lyrics`.
    /// Panics if the value is not `Lyrics`.
    #[inline]
    pub fn lyrics(&self) -> &super::Lyrics {
        match *self {
            Content::Lyrics(ref content) => content,
            _ => panic!("called `Content::lyrics()` on a non `Lyrics` value") 
        }
    }

    /// Returns the `Picture`.
    /// Panics if the value is not `Picture`.
    #[inline]
    pub fn picture(&self) -> &super::Picture {
        match *self {
            Content::Picture(ref picture) => picture,
            _ => panic!("called `Content::picture()` on a non `Picture` value") 
        }
    }

    /// Returns the `Unknown`.
    /// Panics if the value is not `Unknown`.
    #[inline]
    pub fn unknown(&self) -> &[u8] {
        match *self {
            Content::Unknown(ref data) => &data[..],
            _ => panic!("called `Content::unknown()` on a non `Unknown` value") 
        }
    }
}

