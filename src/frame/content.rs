use std::hash::{Hash, Hasher};


/// The decoded contents of a frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    /// A value containing the parsed contents of a text frame.
    Text(String),
    /// A value containing the parsed contents of a user defined text frame (TXXX).
    ExtendedText(ExtendedText),
    /// A value containing the parsed contents of a web link frame.
    Link(String),
    /// A value containing the parsed contents of a user defined web link frame (WXXX).
    ExtendedLink(ExtendedLink),
    /// A value containing the parsed contents of a comment frame (COMM).
    Comment(Comment),
    /// A value containing the parsed contents of a lyrics frame (USLT).
    Lyrics(Lyrics),
    /// A value containing the parsed contents of a picture frame (APIC).
    Picture(Picture),
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


/// The parsed contents of an extended text frame.
#[derive(Clone, Debug, Eq)]
#[allow(missing_docs)]
pub struct ExtendedText {
    pub description: String,
    pub value: String,
}

impl PartialEq for ExtendedText {
    fn eq(&self, other: &Self) -> bool {
        self.description == other.description
    }
}

impl Hash for ExtendedText {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.description.hash(state);
    }
}


/// The parsed contents of an extended link frame.
#[derive(Clone, Debug, Eq)]
#[allow(missing_docs)]
pub struct ExtendedLink {
    pub description: String,
    pub link: String,
}

impl PartialEq for ExtendedLink {
    fn eq(&self, other: &Self) -> bool {
        self.description == other.description
    }
}

impl Hash for ExtendedLink {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.description.hash(state);
    }
}


/// The parsed contents of a comment frame.
#[derive(Clone, Debug, Eq)]
#[allow(missing_docs)]
pub struct Comment {
    pub lang: String,
    pub description: String,
    pub text: String
}

impl PartialEq for Comment {
    fn eq(&self, other: &Self) -> bool {
        self.lang == other.lang && self.description == other.description
    }
}

impl Hash for Comment {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.lang.hash(state);
        self.description.hash(state);
    }
}


/// The parsed contents of an unsynchronized lyrics frame.
#[derive(Clone, Debug, Eq)]
#[allow(missing_docs)]
pub struct Lyrics {
    pub lang: String,
    pub description: String,
    pub text: String
}

impl PartialEq for Lyrics {
    fn eq(&self, other: &Self) -> bool {
        self.lang == other.lang && self.description == other.description
    }
}

impl Hash for Lyrics {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.lang.hash(state);
        self.description.hash(state);
    }
}


/// Types of pictures used in APIC frames.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum PictureType {
    Other,
    Icon,
    OtherIcon,
    CoverFront,
    CoverBack,
    Leaflet,
    Media,
    LeadArtist,
    Artist,
    Conductor,
    Band,
    Composer,
    Lyricist,
    RecordingLocation,
    DuringRecording,
    DuringPerformance,
    ScreenCapture,
    BrightFish,
    Illustration,
    BandLogo,
    PublisherLogo
}


/// A structure representing an ID3 picture frame's contents.
#[derive(Clone, Eq, Debug)]
pub struct Picture {
    /// The picture's MIME type.
    pub mime_type: String,
    /// The type of picture.
    pub picture_type: PictureType,
    /// A description of the picture's contents.
    pub description: String,
    /// The image data.
    pub data: Vec<u8>
}

impl Picture {
    /// Creates a new `Picture` with empty values.
    #[deprecated]
    pub fn new() -> Picture {
        Picture {
            mime_type: String::new(),
            picture_type: PictureType::Other,
            description: String::new(),
            data: Vec::new(),
        }
    }
}

impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        self.picture_type == other.picture_type
    }
}

impl Hash for Picture {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.picture_type.hash(state);
    }
}
