use self::Content::{
    TextContent, ExtendedTextContent, LinkContent, ExtendedLinkContent, CommentContent,
    LyricsContent, PictureContent, UnknownContent
};

/// The decoded contents of a frame.
pub enum Content {
    /// A value containing the parsed contents of a text frame.
    TextContent(super::Text),
    /// A value containing the parsed contents of a user defined text frame (TXXX).
    ExtendedTextContent(super::ExtendedText),
    /// A value containing the parsed contents of a web link frame.
    LinkContent(super::Link),
    /// A value containing the parsed contents of a user defined web link frame (WXXX).
    ExtendedLinkContent(super::ExtendedLink),
    /// A value containing the parsed contents of a comment frame (COMM).
    CommentContent(super::Comment),
    /// A value containing the parsed contents of a lyrics frame (USLT).
    LyricsContent(super::Lyrics),
    /// A value containing the parsed contents of a picture frame (APIC).
    PictureContent(super::Picture),
    /// A value containing the bytes of a unknown frame.
    UnknownContent(Vec<u8>),
}

impl Content {
    /// Returns the `TextContent`.
    /// Panics if the value is not `TextContent`.
    #[inline]
    pub fn text(&self) -> &super::Text {
        match *self {
            TextContent(ref content) => content,
            _ => panic!("called `Content::text()` on a non `TextContent` value") 
        }
    }

    /// Returns the `ExtendedTextContent`.
    /// Panics if the value is not `ExtendedTextContent`.
    #[inline]
    pub fn extended_text(&self) -> &super::ExtendedText {
        match *self {
            ExtendedTextContent(ref content) => content,
            _ => panic!("called `Content::extended_text()` on a non `ExtendedTextContent` value") 
        }
    }

    /// Returns the `LinkContent`.
    /// Panics if the value is not `LinkContent`.
    #[inline]
    pub fn link(&self) -> &super::Link {
        match *self {
            LinkContent(ref content) => content,
            _ => panic!("called `Content::link()` on a non `LinkContent` value") 
        }
    }

    /// Returns the `ExtendedLinkContent`.
    /// Panics if the value is not `ExtendedLinkContent`.
    #[inline]
    pub fn extended_link(&self) -> &super::ExtendedLink {
        match *self {
            ExtendedLinkContent(ref content) => content,
            _ => panic!("called `Content::extended_link()` on a non `ExtendedLinkContent` value") 
        }
    }

    /// Returns the `CommentContent`.
    /// Panics if the value is not `CommentContent`.
    #[inline]
    pub fn comment(&self) -> &super::Comment {
        match *self {
            CommentContent(ref content) => content,
            _ => panic!("called `Content::comment()` on a non `CommentContent` value") 
        }
    }

    /// Returns the `LyricsContent`.
    /// Panics if the value is not `LyricsContent`.
    #[inline]
    pub fn lyrics(&self) -> &super::Lyrics {
        match *self {
            LyricsContent(ref content) => content,
            _ => panic!("called `Content::lyrics()` on a non `LyricsContent` value") 
        }
    }

    /// Returns the `PictureContent`.
    /// Panics if the value is not `PictureContent`.
    #[inline]
    pub fn picture(&self) -> &super::Picture {
        match *self {
            PictureContent(ref picture) => picture,
            _ => panic!("called `Content::picture()` on a non `PictureContent` value") 
        }
    }

    /// Returns the `UnknownContent`.
    /// Panics if the value is not `UnknownContent`.
    #[inline]
    pub fn unknown(&self) -> &[u8] {
        match *self {
            UnknownContent(ref data) => data.as_slice(),
            _ => panic!("called `Content::unknown()` on a non `UnknownContent` value") 
        }
    }
}

