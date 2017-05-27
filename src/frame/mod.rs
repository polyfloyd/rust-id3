use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::str;

pub use self::content::{Content, ExtendedText, ExtendedLink, Comment, Lyrics, Picture, PictureType};
pub use self::timestamp::Timestamp;

#[doc(hidden)]
use ::stream::frame::{self, v2, v3, v4};

use ::tag::{self, Version};

mod content;
mod timestamp;


/// A structure representing an ID3 frame.
///
/// It is imporant to note that the (Partial)Eq and Hash implementations are based on the ID3 spec.
/// This means that text frames with equal ID's are equal but picture frames with both "APIC" as ID
/// are not because their uniqueness is also defined by their content.
#[derive(Clone, Debug, Eq)]
pub struct Frame {
    /// The frame identifier.
    id: [u8; 4],
    /// The parsed content of the frame.
    #[doc(hidden)]
    pub content: Content,

    tag_alter_preservation: bool,
    file_alter_preservation: bool,
}

impl PartialEq for Frame {
    fn eq(&self, other: &Frame) -> bool {
        match self.content {
            Content::Text(_) => self.id == other.id,
            _ => {
                self.id == other.id && self.content == other.content
            },
        }
    }
}

impl Hash for Frame {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        match self.content {
            Content::Text(_) => self.id.hash(state),
            _ => {
                self.id.hash(state);
                self.content.hash(state);
            },
        }
    }
}

impl Frame {
    /// Creates a new ID3v2.3 frame with the specified identifier.
    ///
    /// # Panics
    /// If the id's length is not 3 or 4 bytes long or not known.
//    #[deprecated(note = "Use with_content")]
    pub fn new<T: Into<String>>(id: T) -> Frame {
        Frame::with_content(&id.into(), Content::Unknown(Vec::new()))
    }

    /// Creates a frame with the specified ID and content.
    ///
    /// Both ID3v2.2 and >ID3v2.3 IDs are accepted, although they will be converted to ID3v2.3
    /// format.
    ///
    /// # Panics
    /// If the id's length is not 3 or 4 bytes long or not known.
    pub fn with_content(id: &str, content: Content) -> Frame {
        assert!({
            let l = id.bytes().count();
            l == 3 || l == 4
        });
        Frame {
            id: {
                let idv3 = if id.len() == 3 {
                    // ID3v2.3 supports all ID3v2.2 frames, unwrapping should be safe.
                    ::util::convert_id_2_to_3(id).unwrap()
                } else {
                    id
                };
                let mut b = idv3.bytes();
                [
                    b.next().unwrap(),
                    b.next().unwrap(),
                    b.next().unwrap(),
                    b.next().unwrap(),
                ]
            },
            content: content,
            tag_alter_preservation: false,
            file_alter_preservation: false,
        }
    }

    /// Returns the 4-byte ID of this frame.
    pub fn id(&self) -> &str {
        str::from_utf8(&self.id).unwrap()
    }

    /// Returns the ID that is compatible with specified version or None if no ID is available in
    /// that version.
    pub fn id_for_version(&self, version: Version) -> Option<&str> {
        match version {
            Version::Id3v22 => ::util::convert_id_3_to_2(self.id()),
            Version::Id3v23|Version::Id3v24 => Some(str::from_utf8(&self.id).unwrap()),
        }
    }

    /// Returns the content of the frame.
    pub fn content(&self) -> &Content {
        &self.content
    }

    /// Returns whether the tag_alter_preservation flag is set.
    pub fn tag_alter_preservation(&self) -> bool {
        self.tag_alter_preservation
    }

    /// Sets the tag_alter_preservation flag.
    pub fn set_tag_alter_preservation(&mut self, tag_alter_preservation: bool) {
        self.tag_alter_preservation = tag_alter_preservation;
    }

    /// Returns whether the file_alter_preservation flag is set.
    pub fn file_alter_preservation(&self) -> bool {
        self.file_alter_preservation
    }

    /// Sets the file_alter_preservation flag.
    pub fn set_file_alter_preservation(&mut self, file_alter_preservation: bool) {
        self.file_alter_preservation = file_alter_preservation;
    }

    /// Attempts to read a frame from the reader.
    ///
    /// Returns a tuple containing the number of bytes read and a frame. If pading is encountered
    /// then `None` is returned.
    ///
    /// Only reading from versions 2, 3, and 4 is supported. Attempting to read any other version
    /// will return an error with kind `UnsupportedVersion`.
    pub fn read_from<R>(reader: &mut R, version: tag::Version, unsynchronization: bool) -> ::Result<Option<(usize, Frame)>>
        where R: io::Read {
        frame::decode(reader, version, unsynchronization)
    }

    /// Attempts to write the frame to the writer.
    ///
    /// Returns the number of bytes written.
    ///
    /// Only writing to versions 2, 3, and 4 is supported. Attempting to write using any other
    /// version will return an error with kind `UnsupportedVersion`.
    pub fn write_to(&self, writer: &mut Write, version: tag::Version, unsynchronization: bool) -> ::Result<u32> {
        match version {
            tag::Id3v22 => v2::write(writer, self, unsynchronization),
            tag::Id3v23 => {
                let mut flags = v3::Flags::empty();
                flags.set(v3::TAG_ALTER_PRESERVATION, self.tag_alter_preservation);
                flags.set(v3::FILE_ALTER_PRESERVATION, self.file_alter_preservation);
                v3::write(writer, self, v3::Flags::empty(), unsynchronization)
            },
            tag::Id3v24 => {
                let mut flags = v4::Flags::empty();
                flags.set(v4::UNSYNCHRONISATION, unsynchronization);
                flags.set(v4::TAG_ALTER_PRESERVATION, self.tag_alter_preservation);
                flags.set(v4::FILE_ALTER_PRESERVATION, self.file_alter_preservation);
                v4::write(writer, self, flags)
            },
        }
    }

    /// Returns a string representing the parsed content.
    ///
    /// Returns `None` if the parsed content can not be represented as text.
    ///
    /// # Example
    /// ```
    /// use id3::frame::{self, Frame, Content};
    ///
    /// let title_frame = Frame::with_content("TIT2", Content::Text("title".to_owned()));
    /// assert_eq!(&title_frame.text().unwrap()[..], "title");
    ///
    /// let mut txxx_frame = Frame::with_content("TXXX", Content::ExtendedText(frame::ExtendedText {
    ///     description: "description".to_owned(),
    ///     value: "value".to_owned()
    /// }));
    /// assert_eq!(&txxx_frame.text().unwrap()[..], "description: value");
    /// ```
    #[deprecated(note = "Format using fmt::Display")]
    pub fn text(&self) -> Option<Cow<str>> {
        Some(Cow::Owned(format!("{}", self)))
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.content {
            Content::Text(ref content) => write!(f, "{}", content),
            Content::Link(ref content) => write!(f, "{}", content),
            Content::Lyrics(ref content) => write!(f, "{}", content.text),
            Content::ExtendedText(ref content) => write!(f, "{}: {}", content.description, content.value),
            Content::ExtendedLink(ref content) => write!(f, "{}: {}", content.description, content.link),
            Content::Comment(ref content) => write!(f, "{}: {}", content.description, content.text),
            Content::Picture(ref content) => write!(f, "{}: {:?} ({:?})", content.description, content.picture_type, content.mime_type),
            Content::Unknown(ref content) => write!(f, "unknown, {} bytes", content.len()),
        }
    }
}
