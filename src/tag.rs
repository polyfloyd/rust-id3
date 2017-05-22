use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom, BufReader};
use std::ops;
use std::path::Path;

use byteorder::{ByteOrder, BigEndian, ReadBytesExt, WriteBytesExt};

use frame::{self, Frame, Comment, Lyrics, Picture, PictureType, Timestamp};
use frame::Content;
use ::storage::{PlainStorage, Storage};

static DEFAULT_FILE_DISCARD: [&'static str; 11] = [
    "AENC", "ETCO", "EQUA", "MLLT", "POSS",
    "SYLT", "SYTC", "RVAD", "TENC", "TLEN", "TSIZ"
];


/// Denotes the version of a tag.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Version {
    /// ID3v1
    Id3v1,
    /// ID3v2.2
    Id3v22,
    /// ID3v2.3
    Id3v23,
    /// ID3v2.4
    Id3v24,
}
pub use self::Version::*;

impl Version {
    /// Returns the major version.
    ///
    /// # Example
    /// ```
    /// use id3::Version;
    ///
    /// assert_eq!(Version::Id3v1.major(), 1);
    /// assert_eq!(Version::Id3v24.major(), 2);
    /// ```
    pub fn major(&self) -> u32 {
        match *self {
            Id3v1 => 1,
            Id3v22 => 2,
            Id3v23 => 2,
            Id3v24 => 2,
        }
    }

    /// Returns the minor version. For ID3v1, this will be 0.
    ///
    /// # Example
    /// ```
    /// use id3::Version;
    ///
    /// assert_eq!(Version::Id3v1.minor(), 0);
    /// assert_eq!(Version::Id3v24.minor(), 4);
    /// ```
    pub fn minor(&self) -> u8 {
        match *self {
            Id3v1 => 0,
            Id3v22 => 2,
            Id3v23 => 3,
            Id3v24 => 4,
        }
    }
}


/// An ID3 tag containing metadata frames.
#[derive(Clone, Debug)]
pub struct Tag {
    version: Version,
    /// The ID3 header flags.
    flags: Flags,
    /// A vector of frames included in the tag.
    frames: Vec<Frame>,
}

/// Flags used in the ID3 header.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Flags {
    /// Indicates whether or not unsynchronization is used.
    pub unsynchronization: bool,
    /// Indicates whether or not the header is followed by an extended header.
    pub extended_header: bool,
    /// Indicates whether the tag is in an experimental stage.
    pub experimental: bool,
    /// Indicates whether a footer is present.
    pub footer: bool,
    /// Indicates whether or not compression is used. This flag is only used in ID3v2.2.
    pub compression: bool // v2.2 only
}

// Flags {{{
impl Flags {
    /// Creates a new `Flags` with all flags set to false.
    pub fn new() -> Flags {
        Flags {
            unsynchronization: false, extended_header: false, experimental: false,
            footer: false, compression: false
        }
    }

    /// Creates a new `Flags` using the provided byte.
    pub fn from_byte(byte: u8, version: Version) -> Flags {
        let mut flags = Flags::new();

        flags.unsynchronization = byte & 0x80 != 0;

        if version == Version::Id3v22 {
            flags.compression = byte & 0x40 != 0;
        } else {
            flags.extended_header = byte & 0x40 != 0;
            flags.experimental = byte & 0x20 != 0;

            if version == Version::Id3v24 {
                flags.footer = byte & 0x10 != 0;
            }
        }

        flags
    }

    /// Creates a byte representation of the flags suitable for writing to an ID3 tag.
    pub fn to_byte(&self, version: Version) -> u8 {
        let mut byte = 0;

        if self.unsynchronization {
            byte |= 0x80;
        }

        if version == Version::Id3v22 {
            if self.compression {
                byte |= 0x40;
            }
        } else {
            if self.extended_header {
                byte |= 0x40;
            }

            if self.experimental {
                byte |= 0x20
            }

            if version == Version::Id3v24 {
                if self.footer {
                    byte |= 0x10;
                }
            }
        }

        byte
    }
}
// }}}

// Tag {{{
impl<'a> Tag {
    /// Creates a new ID3v2.3 tag with no frames.
    pub fn new() -> Tag {
        Tag {
            version: Version::Id3v24, flags: Flags::new(),
            frames: Vec::new()
        }
    }

    /// Creates a new ID3 tag with the specified version.
    pub fn with_version(version: Version) -> Tag {
        let mut tag = Tag::new();
        tag.version = version;
        tag
    }

    // Frame ID Querying {{{
    fn artist_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TP1" } else { "TPE1" }
    }

    fn album_artist_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TP2" } else { "TPE2" }
    }

    fn album_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TAL" } else { "TALB" }
    }

    fn title_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TT2" } else { "TIT2" }
    }

    fn genre_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TCO" } else { "TCON" }
    }

    fn year_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TYE" } else { "TYER" }
    }

    fn track_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TRK" } else { "TRCK" }
    }

    fn lyrics_id(&self) -> &'static str {
        if self.version.minor() == 2 { "ULT" } else { "USLT" }
    }

    fn picture_id(&self) -> &'static str {
        if self.version.minor() == 2 { "PIC" } else { "APIC" }
    }

    fn comment_id(&self) -> &'static str {
        if self.version.minor() == 2 { "COM" } else { "COMM" }
    }

    fn txxx_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TXX" } else { "TXXX" }
    }

    fn disc_id(&self) -> &'static str {
        if self.version.minor() == 2 { "TPA" } else { "TPOS" }
    }
    // }}}

    // id3v1 {{{
    /// Returns true if the reader might contain a valid ID3v1 tag.
    #[deprecated(note = "Use v1::Tag::is_candidate")]
    pub fn is_candidate_v1<R: Read + Seek>(reader: &mut R) -> bool {
        ::v1::Tag::is_candidate(reader)
            .unwrap_or(false)
    }

    /// Attempts to read an ID3v1 tag from the reader. Since the structure of ID3v1 is so different
    /// from ID3v2, the tag will be converted and stored internally as an ID3v2.3 tag.
    #[deprecated(note = "Use tag_v1.into()")]
    pub fn read_from_v1<R: Read + Seek>(reader: &mut R) -> ::Result<Tag> {
        let tag_v1 = ::v1::Tag::read_from(reader)?;
        Ok(tag_v1.into())
    }

    /// Attempts to read an ID3v1 tag from the file at the specified path. The tag will be
    /// converted into an ID3v2.3 tag upon success.
    #[deprecated(note = "Use tag_v1.into()")]
    pub fn read_from_path_v1<P: AsRef<Path>>(path: P) -> ::Result<Tag> {
        let mut file = File::open(&path)?;
        let tag_v1 = ::v1::Tag::read_from(&mut file)?;
        Ok(tag_v1.into())
    }
    // }}}

    /// Returns the version of the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Version};
    ///
    /// let tag = Tag::with_version(Version::Id3v23);
    /// assert_eq!(tag.version(), Version::Id3v23);
    /// ```
    pub fn version(&self) -> Version {
        self.version
    }

    /// Sets the version of this tag.
    ///
    /// ID3v2 versions 2 to 4 can be set. Trying to set any other version will cause a panic.
    ///
    /// Any frames that could not be converted to the new version will be dropped.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Version};
    ///
    /// let mut tag = Tag::with_version(Version::Id3v24);
    /// assert_eq!(tag.version(), Version::Id3v24);
    ///
    /// tag.set_version(Version::Id3v23);
    /// assert_eq!(tag.version(), Version::Id3v23);
    /// ```
    pub fn set_version(&mut self, version: Version) {
        if self.version == version {
            return;
        }

        let mut remove_uuid = Vec::new();
        for mut frame in self.frames.iter_mut() {
            if !Tag::convert_frame_version(&mut frame, self.version, version) {
                remove_uuid.push(frame.uuid.clone());
            }
        }

        self.frames.retain(|frame: &Frame| !remove_uuid.contains(&frame.uuid));

        self.version = version;
    }

    fn convert_frame_version(frame: &mut Frame, old_version: Version, new_version: Version) -> bool {
        if old_version == new_version {
            return true;
        }
        if old_version == Id3v23 && new_version == Id3v24 {
            return true;
        }
        if old_version == Id3v24 && new_version == Id3v23 {
            return true;
        }

        if (old_version == Id3v23 || old_version == Id3v24) && new_version == Id3v22 {
            // attempt to convert the id
            frame.id = match ::util::convert_id_3_to_2(&frame.id[..]) {
                Some(id) => id.to_owned(),
                None => {
                    debug!("no ID3v2.3 to ID3v2.3 mapping for {}", frame.id);
                    return false;
                }
            }
        } else if old_version == Id3v22 && (new_version == Id3v23 || new_version == Id3v24) {
            // attempt to convert the id
            frame.id = match ::util::convert_id_2_to_3(&frame.id[..]) {
                Some(id) => id.to_owned(),
                None => {
                    debug!("no ID3v2.2 to ID3v2.3 mapping for {}", frame.id);
                    return false;
                }
            };

            // if the new version is v2.4 and the frame is compressed, we must enable the
            // data_length_indicator flag
            if new_version == Id3v24 && frame.compression() {
                frame.set_compression(true);
            }
        } else {
            // not sure when this would ever occur but lets just say the conversion failed
            return false;
        }

        true
    }

    /// Returns a vector of references to all frames in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.push(Frame::new("TPE1"));
    /// tag.push(Frame::new("APIC"));
    ///
    /// assert_eq!(tag.frames().len(), 2);
    /// ```
    pub fn frames(&'a self) -> &'a Vec<Frame> {
        &self.frames
    }

    /// Returns a reference to the first frame with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.push(Frame::new("TIT2"));
    ///
    /// assert!(tag.get("TIT2").is_some());
    /// assert!(tag.get("TCON").is_none());
    /// ```
    pub fn get(&'a self, id: &str) -> Option<&'a Frame> {
        for frame in self.frames.iter() {
            if &frame.id[..] == id {
                return Some(frame);
            }
        }

        None
    }

    /// Returns a vector of references to frames with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.push(Frame::new("TXXX"));
    /// tag.push(Frame::new("TXXX"));
    /// tag.push(Frame::new("TALB"));
    ///
    /// assert_eq!(tag.get_all("TXXX").len(), 2);
    /// assert_eq!(tag.get_all("TALB").len(), 1);
    /// ```
    pub fn get_all(&'a self, id: &str) -> Vec<&'a Frame> {
        let mut matches = Vec::new();
        for frame in self.frames.iter() {
            if &frame.id[..] == id {
                matches.push(frame);
            }
        }

        matches
    }

    /// Adds the frame to the tag. The frame identifier will attempt to be converted into the
    /// corresponding identifier for the tag version.
    ///
    /// Returns whether the frame was added to the tag. The only reason the frame would not be
    /// added to the tag is if the frame identifier could not be converted from the frame version
    /// to the tag version.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    /// tag.push(Frame::new("TALB"));
    /// assert_eq!(&tag.frames()[0].id[..], "TALB");
    /// ```
    pub fn push(&mut self, mut frame: Frame) -> bool {
        frame.generate_uuid();
        self.frames.push(frame);
        true
    }

    /// Adds a text frame.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_text_frame("TRCK", "1/13");
    /// assert_eq!(tag.get("TRCK").unwrap().content.text().unwrap(), "1/13");
    /// ```
    pub fn add_text_frame<K: Into<String>, V: Into<String>>(&mut self, id: K, text: V) {
        let id = id.into();
        self.remove(&id[..]);
        let frame = Frame::with_content(id, Content::Text(text.into()));
        self.frames.push(frame);
    }

    /// Removes the frame with the specified uuid.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.push(Frame::new("TPE2"));
    /// assert_eq!(tag.frames().len(), 1);
    ///
    /// let uuid = tag.frames()[0].uuid.clone();
    /// tag.remove_uuid(&uuid[..]);
    /// assert_eq!(tag.frames().len(), 0);
    /// ```
    pub fn remove_uuid(&mut self, uuid: &[u8]) {
        self.frames.retain(|frame| {
            &frame.uuid[..] != uuid
        });
    }

    /// Removes all frames with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.push(Frame::new("TXXX"));
    /// tag.push(Frame::new("TXXX"));
    /// tag.push(Frame::new("USLT"));
    ///
    /// assert_eq!(tag.frames().len(), 3);
    ///
    /// tag.remove("TXXX");
    /// assert_eq!(tag.frames().len(), 1);
    ///
    /// tag.remove("USLT");
    /// assert_eq!(tag.frames().len(), 0);
    /// ```
    pub fn remove(&mut self, id: &str) {
        self.frames.retain(|frame| {
            &frame.id[..] != id
        });
    }

    /// Returns the `Content::Text` string for the frame with the specified identifier.
    /// Returns `None` if the frame with the specified ID can't be found or if the content is not
    /// `Content::Text`.
    fn text_for_frame_id(&self, id: &str) -> Option<&str> {
        match self.get(id) {
            Some(frame) => match frame.content {
                Content::Text(ref text) => Some(&text[..]),
                _ => None
            },
            None => None
        }
    }

    // Getters/Setters {{{
    /// Returns a vector of the extended text (TXXX) description/value pairs.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::{self, Content};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TXXX");
    /// frame.content = Content::ExtendedText(frame::ExtendedText {
    ///     description: "description1".to_owned(),
    ///     value: "value1".to_owned()
    /// });
    /// tag.push(frame);
    ///
    /// let mut frame = Frame::new("TXXX");
    /// frame.content = Content::ExtendedText(frame::ExtendedText {
    ///     description: "description2".to_owned(),
    ///     value: "value2".to_owned()
    /// });
    /// tag.push(frame);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("description1", "value1")));
    /// assert!(tag.txxx().contains(&("description2", "value2")));
    /// ```
    pub fn txxx(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::new();
        for frame in self.get_all(self.txxx_id()).iter() {
            match frame.content {
                Content::ExtendedText(ref ext) => out.push((&ext.description[..], &ext.value[..])),
                _ => { }
            }
        }

        out
    }

    /// Adds a user defined text frame (TXXX).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_txxx("key1", "value1");
    /// tag.add_txxx("key2", "value2");
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("key1", "value1")));
    /// assert!(tag.txxx().contains(&("key2", "value2")));
    /// ```
    pub fn add_txxx<K: Into<String>, V: Into<String>>(&mut self, description: K, value: V) {
        let description = description.into();
        self.remove_txxx(Some(&description[..]), None);

        let frame = Frame::with_content(self.txxx_id(), Content::ExtendedText(frame::ExtendedText {
            description: description,
            value: value.into()
        }));
        self.frames.push(frame);
    }

    /// Removes the user defined text frame (TXXX) with the specified key and value.
    ///
    /// A key or value may be `None` to specify a wildcard value.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_txxx("key1", "value1");
    /// tag.add_txxx("key2", "value2");
    /// tag.add_txxx("key3", "value2");
    /// tag.add_txxx("key4", "value3");
    /// tag.add_txxx("key5", "value4");
    /// assert_eq!(tag.txxx().len(), 5);
    ///
    /// tag.remove_txxx(Some("key1"), None);
    /// assert_eq!(tag.txxx().len(), 4);
    ///
    /// tag.remove_txxx(None, Some("value2"));
    /// assert_eq!(tag.txxx().len(), 2);
    ///
    /// tag.remove_txxx(Some("key4"), Some("value3"));
    /// assert_eq!(tag.txxx().len(), 1);
    ///
    /// tag.remove_txxx(None, None);
    /// assert_eq!(tag.txxx().len(), 0);
    /// ```
    pub fn remove_txxx(&mut self, description: Option<&str>, value: Option<&str>) {
        let id = self.txxx_id();
        self.frames.retain(|frame| {
            let mut description_match = false;
            let mut value_match = false;

            if &frame.id[..] == id {
                match frame.content {
                    Content::ExtendedText(ref ext) => {
                        match description {
                            Some(s) => description_match = s == &ext.description[..],
                            None => description_match = true
                        }

                        match value {
                            Some(s) => value_match = s == &ext.value[..],
                            None => value_match = true
                        }
                    },
                    _ => { // remove frames that we can't parse
                        description_match = true;
                        value_match = true;
                    }
                }
            }

            !(description_match && value_match) // true if we want to keep the item
        });
    }

    /// Returns a vector of references to the pictures in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::{Content, Picture, PictureType};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let picture = Picture {
    ///     mime_type: String::new(),
    ///     picture_type: PictureType::Other,
    ///     description: String::new(),
    ///     data: Vec::new(),
    /// };
    /// tag.push(Frame::with_content("APIC", Content::Picture(picture.clone())));
    /// tag.push(Frame::with_content("APIC", Content::Picture(picture.clone())));
    ///
    /// assert_eq!(tag.pictures().len(), 2);
    /// ```
    pub fn pictures(&self) -> Vec<&Picture> {
        let mut pictures = Vec::new();
        for frame in self.get_all(self.picture_id()).iter() {
            match frame.content {
                Content::Picture(ref picture) => pictures.push(picture),
                _ => { }
            }
        }
        pictures
    }

    /// Adds a picture frame (APIC).
    /// Any other pictures with the same type will be removed from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::{Picture, PictureType};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_picture(Picture {
    ///     mime_type: "image/jpeg".to_string(),
    ///     picture_type: PictureType::Other,
    ///     description: "some image".to_string(),
    ///     data: vec![],
    /// });
    /// tag.add_picture(Picture {
    ///     mime_type: "image/png".to_string(),
    ///     picture_type: PictureType::Other,
    ///     description: "some other image".to_string(),
    ///     data: vec![],
    /// });
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(&tag.pictures()[0].mime_type[..], "image/png");
    /// ```
    pub fn add_picture(&mut self, picture: Picture) {
        self.remove_picture_by_type(picture.picture_type);
        let frame = Frame::with_content(self.picture_id(), Content::Picture(picture));
        self.frames.push(frame);
    }

    /// Removes all pictures of the specified type.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::{Picture, PictureType};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_picture(Picture {
    ///     mime_type: "image/jpeg".to_string(),
    ///     picture_type: PictureType::Other,
    ///     description: "some image".to_string(),
    ///     data: vec![],
    /// });
    /// tag.add_picture(Picture {
    ///     mime_type: "image/png".to_string(),
    ///     picture_type: PictureType::CoverFront,
    ///     description: "some other image".to_string(),
    ///     data: vec![],
    /// });
    ///
    /// assert_eq!(tag.pictures().len(), 2);
    /// tag.remove_picture_by_type(PictureType::CoverFront);
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(tag.pictures()[0].picture_type, PictureType::Other);
    /// ```
    pub fn remove_picture_by_type(&mut self, picture_type: PictureType) {
        let id = self.picture_id();
        self.frames.retain(|frame| {
            if &frame.id[..] == id {
                let pic = match frame.content {
                    Content::Picture(ref picture) => picture,
                    _ => return false
                };
                return pic.picture_type != picture_type
            }

            true
        });
    }

    /// Returns a vector of comment (COMM) key/value pairs.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::{Content, Comment};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("COMM");
    /// frame.content = Content::Comment(Comment {
    ///     lang: "eng".to_owned(),
    ///     description: "key1".to_owned(),
    ///     text: "value1".to_owned()
    /// });
    /// tag.push(frame);
    ///
    /// let mut frame = Frame::new("COMM");
    /// frame.content = Content::Comment(Comment {
    ///     lang: "eng".to_owned(),
    ///     description: "key2".to_owned(),
    ///     text: "value2".to_owned()
    /// });
    /// tag.push(frame);
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1", "value1")));
    /// assert!(tag.comments().contains(&("key2", "value2")));
    /// ```
    pub fn comments(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::new();
        for frame in self.get_all(self.comment_id()).iter() {
            match frame.content {
                Content::Comment(ref comment) => {
                    out.push((&comment.description[..], &comment.text[..]));
                },
                _ => { }
            }
        }

        out
    }

    /// Adds a comment (COMM).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Comment;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_comment(Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key1".to_string(),
    ///     text: "value1".to_string(),
    /// });
    /// tag.add_comment(Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key2".to_string(),
    ///     text: "value2".to_string(),
    /// });
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1", "value1")));
    /// assert!(tag.comments().contains(&("key2", "value2")));
    /// ```
    pub fn add_comment(&mut self, comment: Comment) {
        self.remove_comment(Some(&comment.description[..]), None);
        let frame = Frame::with_content(self.comment_id(), Content::Comment(comment));
        self.frames.push(frame);
    }

    /// Removes the comment (COMM) with the specified key and value.
    ///
    /// A key or value may be `None` to specify a wildcard value.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Comment;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_comment(Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key1".to_string(),
    ///     text: "value1".to_string(),
    /// });
    /// tag.add_comment(Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key2".to_string(),
    ///     text: "value2".to_string(),
    /// });
    /// assert_eq!(tag.comments().len(), 2);
    ///
    /// tag.remove_comment(Some("key1"), None);
    /// assert_eq!(tag.comments().len(), 1);
    ///
    /// tag.remove_comment(None, Some("value2"));
    /// assert_eq!(tag.comments().len(), 0);
    /// ```
    pub fn remove_comment(&mut self, description: Option<&str>, text: Option<&str>) {
        let id = self.comment_id();
        self.frames.retain(|frame| {
            let mut description_match = false;
            let mut text_match = false;

            if &frame.id[..] == id {
                match frame.content {
                    Content::Comment(ref comment) =>  {
                        match description {
                            Some(s) => description_match = s == &comment.description[..],
                            None => description_match = true
                        }

                        match text {
                            Some(s) => text_match = s == &comment.text[..],
                            None => text_match = true
                        }
                    },
                    _ => { // remove frames that we can't parse
                        description_match = true;
                        text_match = true;
                    }
                }
            }
            !(description_match && text_match) // true if we want to keep the item
        });
    }

    /// Returns the year (TYER).
    /// Returns `None` if the year frame could not be found or if it could not be parsed.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.year().is_none());
    ///
    /// let mut frame_valid = Frame::new("TYER");
    /// frame_valid.content = Content::Text("2014".to_owned());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.year().unwrap(), 2014);
    ///
    /// tag.remove("TYER");
    ///
    /// let mut frame_invalid = Frame::new("TYER");
    /// frame_invalid.content = Content::Text("nope".to_owned());
    /// tag.push(frame_invalid);
    /// assert!(tag.year().is_none());
    /// ```
    pub fn year(&self) -> Option<usize> {
        let id = self.year_id();
        match self.get(id) {
            Some(frame) => {
                match frame.content {
                    Content::Text(ref text) => text[..].parse::<usize>().ok(),
                    _ => None
                }
            },
            None => None
        }
    }

    /// Sets the year (TYER).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_year(2014);
    /// assert_eq!(tag.year().unwrap(), 2014);
    /// ```
    pub fn set_year(&mut self, year: usize) {
        let id = self.year_id();
        self.add_text_frame(id, format!("{}", year));
    }

    fn read_timestamp_frame(&self, id: &str) -> Option<Timestamp> {
        match self.get(id) {
            None => None,
            Some(frame) => {
                match frame.content {
                    Content::Text(ref text) => text.parse().ok(),
                    _ => None
                }
            }
        }
    }

    /// Return the content of the TRDC frame, if any
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::Timestamp;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_recorded(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_recorded().unwrap().year, 2014);
    /// ```
    pub fn date_recorded(&self) -> Option<Timestamp> {
        self.read_timestamp_frame("TDRC")
    }

    /// Sets the content of the TDRC frame
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::Timestamp;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_recorded(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_recorded().unwrap().year, 2014);
    /// ```
    pub fn set_date_recorded(&mut self, timestamp: Timestamp) {
        let time_string = timestamp.to_string();
        self.add_text_frame("TDRC", time_string);
    }

    /// Return the content of the TDRL frame, if any
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::Timestamp;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_released().unwrap().year, 2014);
    /// ```
    pub fn date_released(&self) -> Option<Timestamp> {
        self.read_timestamp_frame("TDRL")
    }

    /// Sets the content of the TDRL frame
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::Timestamp;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_released().unwrap().year, 2014);
    /// ```
    pub fn set_date_released(&mut self, timestamp: Timestamp) {
        let time_string = timestamp.to_string();
        self.add_text_frame("TDRL", time_string);
    }

    /// Returns the artist (TPE1).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TPE1");
    /// frame.content = Content::Text("artist".to_owned());
    /// tag.push(frame);
    /// assert_eq!(tag.artist().unwrap(), "artist");
    /// ```
    pub fn artist(&self) -> Option<&str> {
        self.text_for_frame_id(self.artist_id())
    }

    /// Sets the artist (TPE1).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_artist("artist");
    /// assert_eq!(tag.artist().unwrap(), "artist");
    /// ```
    pub fn set_artist<T: Into<String>>(&mut self, artist: T) {
        let id = self.artist_id();
        self.add_text_frame(id, artist);
    }

    /// Removes the artist (TPE1).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_artist("artist");
    /// assert!(tag.artist().is_some());
    ///
    /// tag.remove_artist();
    /// assert!(tag.artist().is_none());
    /// ```
    pub fn remove_artist(&mut self) {
        let id = self.artist_id();
        self.remove(id);
    }

    /// Sets the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TPE2");
    /// frame.content = Content::Text("artist".to_owned());
    /// tag.push(frame);
    /// assert_eq!(tag.album_artist().unwrap(), "artist");
    /// ```
    pub fn album_artist(&self) -> Option<&str> {
        self.text_for_frame_id(self.album_artist_id())
    }

    /// Sets the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album_artist("artist");
    /// assert_eq!(tag.album_artist().unwrap(), "artist");
    /// ```
    pub fn set_album_artist<T: Into<String>>(&mut self, album_artist: T) {
        self.remove("TSOP");
        let id = self.album_artist_id();
        self.add_text_frame(id, album_artist);
    }

    /// Removes the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album_artist("artist");
    /// assert!(tag.album_artist().is_some());
    ///
    /// tag.remove_album_artist();
    /// assert!(tag.album_artist().is_none());
    /// ```
    pub fn remove_album_artist(&mut self) {
        let id = self.album_artist_id();
        self.remove(id);
    }

    /// Returns the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TALB");
    /// frame.content = Content::Text("album".to_owned());
    /// tag.push(frame);
    /// assert_eq!(tag.album().unwrap(), "album");
    /// ```
    pub fn album(&self) -> Option<&str> {
        self.text_for_frame_id(self.album_id())
    }

    /// Sets the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album("album");
    /// assert_eq!(tag.album().unwrap(), "album");
    /// ```
    pub fn set_album<T: Into<String>>(&mut self, album: T) {
        let id = self.album_id();
        self.add_text_frame(id, album);
    }

    /// Removes the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album("album");
    /// assert!(tag.album().is_some());
    ///
    /// tag.remove_album();
    /// assert!(tag.album().is_none());
    /// ```
    pub fn remove_album(&mut self) {
        self.remove("TSOP");
        let id = self.album_id();
        self.remove(id);
    }

    /// Returns the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TIT2");
    /// frame.content = Content::Text("title".to_owned());
    /// tag.push(frame);
    /// assert_eq!(tag.title().unwrap(), "title");
    /// ```
    pub fn title(&self) -> Option<&str> {
        self.text_for_frame_id(self.title_id())
    }

    /// Sets the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_title("title");
    /// assert_eq!(tag.title().unwrap(), "title");
    /// ```
    pub fn set_title<T: Into<String>>(&mut self, title: T) {
        self.remove("TSOT");
        let id = self.title_id();
        self.add_text_frame(id, title);
    }

    /// Removes the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_title("title");
    /// assert!(tag.title().is_some());
    ///
    /// tag.remove_title();
    /// assert!(tag.title().is_none());
    /// ```
    pub fn remove_title(&mut self) {
        let id = self.title_id();
        self.remove(id);
    }

    /// Returns the duration (TLEN).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TLEN");
    /// frame.content = Content::Text("350".to_owned());
    /// tag.push(frame);
    /// assert_eq!(tag.duration().unwrap(), 350);
    /// ```
    pub fn duration(&self) -> Option<u32> {
        self.text_for_frame_id("TLEN").and_then(|t| t[..].parse::<u32>().ok())
    }

    /// Sets the duration (TLEN).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_duration(350);
    /// assert_eq!(tag.duration().unwrap(), 350);
    /// ```
    pub fn set_duration(&mut self, duration: u32) {
        self.add_text_frame("TLEN", duration.to_string());
    }

    /// Removes the duration (TLEN).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_duration(350);
    /// assert!(tag.duration().is_some());
    ///
    /// tag.remove_duration();
    /// assert!(tag.duration().is_none());
    /// ```
    pub fn remove_duration(&mut self) {
       self.remove("TLEN");
    }

    /// Returns the genre (TCON).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("TCON");
    /// frame.content = Content::Text("genre".to_owned());
    /// tag.push(frame);
    /// assert_eq!(tag.genre().unwrap(), "genre");
    /// ```
    pub fn genre(&self) -> Option<&str> {
        self.text_for_frame_id(self.genre_id())
    }

    /// Sets the genre (TCON).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_genre("genre");
    /// assert_eq!(tag.genre().unwrap(), "genre");
    /// ```
    pub fn set_genre<T: Into<String>>(&mut self, genre: T) {
        let id = self.genre_id();
        self.add_text_frame(id, genre);
    }

    /// Removes the genre (TCON).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_genre("genre");
    /// assert!(tag.genre().is_some());
    ///
    /// tag.remove_genre();
    /// assert!(tag.genre().is_none());
    /// ```
    pub fn remove_genre(&mut self) {
        let id = self.genre_id();
        self.remove(id);
    }

    /// Returns the (disc, total_discs) tuple.
    fn disc_pair(&self) -> Option<(u32, Option<u32>)> {
        match self.get(self.disc_id()) {
            Some(frame) => {
                match frame.content {
                    Content::Text(ref text) => {
                        let split: Vec<&str> = text[..].splitn(2, '/').collect();

                        let total_discs = if split.len() == 2 {
                            match split[1].parse::<u32>() {
                                Ok(total_discs) => Some(total_discs),
                                Err(_) => return None
                            }
                        } else {
                            None
                        };

                        match split[0].parse::<u32>() {
                            Ok(disc) => Some((disc, total_discs)),
                            Err(_) => None
                        }
                    },
                    _ => None
                }
            },
            None => None
        }
    }

    /// Returns the disc number (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.disc().is_none());
    ///
    /// let mut frame_valid = Frame::new("TPOS");
    /// frame_valid.content = Content::Text("4".to_owned());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.disc().unwrap(), 4);
    ///
    /// tag.remove("TPOS");
    ///
    /// let mut frame_invalid = Frame::new("TPOS");
    /// frame_invalid.content = Content::Text("nope".to_owned());
    /// tag.push(frame_invalid);
    /// assert!(tag.disc().is_none());
    /// ```
    pub fn disc(&self) -> Option<u32> {
        self.disc_pair().and_then(|(disc, _)| Some(disc))
    }

    /// Sets the disc (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_disc(2);
    /// assert_eq!(tag.disc().unwrap(), 2);
    /// ```
    pub fn set_disc(&mut self, disc: u32) {
        let text = match self.disc_pair().and_then(|(_, total_discs)| total_discs) {
            Some(n) => format!("{}/{}", disc, n),
            None => format!("{}", disc)
        };
        let id = self.disc_id();
        self.add_text_frame(id, text);
    }

    /// Removes the disc number (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_disc(3);
    /// assert!(tag.disc().is_some());
    ///
    /// tag.remove_disc();
    /// assert!(tag.disc().is_none());
    /// ```
    pub fn remove_disc(&mut self) {
        let id = self.disc_id();
        self.remove(id);
    }

    /// Returns the total number of discs (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.disc().is_none());
    ///
    /// let mut frame_valid = Frame::new("TPOS");
    /// frame_valid.content = Content::Text("4/10".to_owned());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.total_discs().unwrap(), 10);
    ///
    /// tag.remove("TPOS");
    ///
    /// let mut frame_invalid = Frame::new("TPOS");
    /// frame_invalid.content = Content::Text("4/nope".to_owned());
    /// tag.push(frame_invalid);
    /// assert!(tag.total_discs().is_none());
    /// ```
    pub fn total_discs(&self) -> Option<u32> {
        self.disc_pair().and_then(|(_, total_discs)| total_discs)
    }

    /// Sets the total number of discs (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_discs(10);
    /// assert_eq!(tag.total_discs().unwrap(), 10);
    /// ```
    pub fn set_total_discs(&mut self, total_discs: u32) {
        let text = match self.disc_pair() {
            Some((disc, _)) => format!("{}/{}", disc, total_discs),
            None => format!("1/{}", total_discs)
        };
        let id = self.disc_id();
        self.add_text_frame(id, text);
    }

    /// Removes the total number of discs (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_discs(10);
    /// assert!(tag.total_discs().is_some());
    ///
    /// tag.remove_total_discs();
    /// assert!(tag.total_discs().is_none());
    /// ```
    pub fn remove_total_discs(&mut self) {
        let id = self.disc_id();
        match self.disc_pair() {
            Some((disc, _)) => self.add_text_frame(id, format!("{}", disc)),
            None => {}
        }
    }

    /// Returns the (track, total_tracks) tuple.
    fn track_pair(&self) -> Option<(u32, Option<u32>)> {
        match self.get(self.track_id()) {
            Some(frame) => {
                match frame.content {
                    Content::Text(ref text) => {
                        let split: Vec<&str> = text[..].splitn(2, '/').collect();

                        let total_tracks = if split.len() == 2 {
                            match split[1].parse::<u32>() {
                                Ok(total_tracks) => Some(total_tracks),
                                Err(_) => return None
                            }
                        } else {
                            None
                        };

                        match split[0].parse::<u32>() {
                            Ok(track) => Some((track, total_tracks)),
                            Err(_) => None
                        }
                    },
                    _ => None
                }
            },
            None => None
        }
    }

    /// Returns the track number (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.track().is_none());
    ///
    /// let mut frame_valid = Frame::new("TRCK");
    /// frame_valid.content = Content::Text("4".to_owned());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.track().unwrap(), 4);
    ///
    /// tag.remove("TRCK");
    ///
    /// let mut frame_invalid = Frame::new("TRCK");
    /// frame_invalid.content = Content::Text("nope".to_owned());
    /// tag.push(frame_invalid);
    /// assert!(tag.track().is_none());
    /// ```
    pub fn track(&self) -> Option<u32> {
        self.track_pair().and_then(|(track, _)| Some(track))
    }

    /// Sets the track (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_track(10);
    /// assert_eq!(tag.track().unwrap(), 10);
    /// ```
    pub fn set_track(&mut self, track: u32) {
        let text = match self.track_pair().and_then(|(_, total_tracks)| total_tracks) {
            Some(n) => format!("{}/{}", track, n),
            None => format!("{}", track)
        };
        let id = self.track_id();
        self.add_text_frame(id, text);
    }

    /// Removes the track number (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_track(10);
    /// assert!(tag.track().is_some());
    ///
    /// tag.remove_track();
    /// assert!(tag.track().is_none());
    /// ```
    pub fn remove_track(&mut self) {
        let id = self.track_id();
        self.remove(id);
    }

    /// Returns the total number of tracks (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.total_tracks().is_none());
    ///
    /// let mut frame_valid = Frame::new("TRCK");
    /// frame_valid.content = Content::Text("4/10".to_owned());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.total_tracks().unwrap(), 10);
    ///
    /// tag.remove("TRCK");
    ///
    /// let mut frame_invalid = Frame::new("TRCK");
    /// frame_invalid.content = Content::Text("4/nope".to_owned());
    /// tag.push(frame_invalid);
    /// assert!(tag.total_tracks().is_none());
    /// ```
    pub fn total_tracks(&self) -> Option<u32> {
        self.track_pair().and_then(|(_, total_tracks)| total_tracks)
    }

    /// Sets the total number of tracks (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_tracks(10);
    /// assert_eq!(tag.total_tracks().unwrap(), 10);
    /// ```
    pub fn set_total_tracks(&mut self, total_tracks: u32) {
        let text = match self.track_pair() {
            Some((track, _)) => format!("{}/{}", track, total_tracks),
            None => format!("1/{}", total_tracks)
        };
        let id = self.track_id();
        self.add_text_frame(id, text);
    }

    /// Removes the total number of tracks (TCON).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_tracks(10);
    /// assert!(tag.total_tracks().is_some());
    ///
    /// tag.remove_total_tracks();
    /// assert!(tag.total_tracks().is_none());
    /// ```
    pub fn remove_total_tracks(&mut self) {
        let id = self.track_id();
        match self.track_pair() {
            Some((track, _)) => self.add_text_frame(id, format!("{}", track)),
            None => {}
        }
    }

    /// Returns the lyrics (USLT).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    /// use id3::frame::Lyrics;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let mut frame = Frame::new("USLT");
    /// frame.content = Content::Lyrics(Lyrics {
    ///     lang: "eng".to_owned(),
    ///     description: "description".to_owned(),
    ///     text: "lyrics".to_owned()
    /// });
    /// tag.push(frame);
    /// assert_eq!(tag.lyrics().unwrap(), "lyrics");
    /// ```
    pub fn lyrics(&self) -> Option<&str> {
        match self.get(self.lyrics_id()) {
            Some(frame) => match frame.content {
                Content::Lyrics(ref lyrics) => Some(&lyrics.text[..]),
                _ => None
            },
            None => None
        }
    }

    /// Sets the lyrics (USLT).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Lyrics;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_lyrics(Lyrics {
    ///     lang: "eng".to_string(),
    ///     description: "".to_string(),
    ///     text: "The lyrics".to_string(),
    /// });
    /// assert_eq!(tag.lyrics().unwrap(), "The lyrics");
    /// ```
    pub fn set_lyrics(&mut self, lyrics: Lyrics) {
        let id = self.lyrics_id();
        self.remove(id);
        let frame = Frame::with_content(id, Content::Lyrics(lyrics));
        self.frames.push(frame);
    }

    /// Removes the lyrics text (USLT) from the tag.
    ///
    /// # Exmaple
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Lyrics;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_lyrics(Lyrics {
    ///     lang: "eng".to_string(),
    ///     description: "".to_string(),
    ///     text: "The lyrics".to_string(),
    /// });
    /// assert!(tag.lyrics().is_some());
    /// tag.remove_lyrics();
    /// assert!(tag.lyrics().is_none());
    /// ```
    pub fn remove_lyrics(&mut self) {
        let id = self.lyrics_id();
        self.remove(id);
    }
    //}}}

    // Reading/Writing {{{
    /// Returns the contents of the reader without any ID3 metadata.
    pub fn skip_metadata<R: Read + Seek>(reader: &mut R) -> Vec<u8> {
        macro_rules! try_io {
            ($reader:ident, $action:expr) => {
                match $action {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        match $reader.seek(SeekFrom::Start(0)) {
                            Ok(_) => {
                                let mut bytes = Vec::<u8>::new();
                                match $reader.read_to_end(&mut bytes) {
                                    Ok(_) => return bytes,
                                    Err(_) => return Vec::new()
                                }
                            },
                            Err(_) => return Vec::new()
                        }
                    }
                }
            }
        }

        let mut ident = [0u8; 3];
        try_io!(reader, reader.read(&mut ident));
        if &ident[..] == b"ID3" {
            try_io!(reader, reader.seek(SeekFrom::Current(3)));
            let offset = 10 + ::util::unsynchsafe(try_io!(reader, reader.read_u32::<BigEndian>()));
            try_io!(reader, reader.seek(SeekFrom::Start(offset as u64)));
        } else {
            try_io!(reader, reader.seek(SeekFrom::Start(0)));
        }

        let mut bytes = Vec::<u8>::new();
        try_io!(reader, reader.read_to_end(&mut bytes));
        bytes
    }

    /// Will return true if the reader is a candidate for an ID3 tag. The reader position will be
    /// reset back to the previous position before returning.
    pub fn is_candidate<R: Read + Seek>(reader: &mut R) -> ::Result<bool> {
        let initial_position = reader.seek(io::SeekFrom::Current(0))?;
        let rs = locate_id3v2(reader);
        reader.seek(io::SeekFrom::Start(initial_position))?;
        Ok(rs?.is_some())
    }

    /// Attempts to read an ID3 tag from the reader.
    pub fn read_from(reader: &mut Read) -> ::Result<Tag> {
        let mut tag = Tag::new();

        let mut identifier = [0u8; 3];
        try!(reader.read(&mut identifier));
        if &identifier[..] != b"ID3" {
            debug!("no id3 tag found");
            return Err(::Error::new(::ErrorKind::NoTag, "reader does not contain an id3 tag"))
        }

        let mut version_buf = [0; 2];
        reader.read_exact(&mut version_buf)?;
        tag.version = match version_buf[0] {
            2 => Version::Id3v22,
            3 => Version::Id3v23,
            4 => Version::Id3v24,
            _ => return Err(::Error::new(::ErrorKind::UnsupportedVersion(version_buf[0]) , "unsupported id3 tag version")),
        };

        tag.flags = Flags::from_byte(try!(reader.read_u8()), tag.version);

        if tag.flags.compression {
            debug!("id3v2.2 compression is unsupported");
            return Err(::Error::new(::ErrorKind::UnsupportedFeature, "id3v2.2 compression is not supported"));
        }

        let tag_size = ::util::unsynchsafe(try!(reader.read_u32::<BigEndian>()));

        let mut offset = 10;

        // TODO actually use the extended header data
        if tag.flags.extended_header {
            let ext_size = ::util::unsynchsafe(try!(reader.read_u32::<BigEndian>()));
            offset += 4;
            let mut extended_header_data = Vec::with_capacity(ext_size as usize);
            try!(reader.take(ext_size as u64).read_to_end(&mut extended_header_data));
            if tag.flags.unsynchronization {
                ::util::resynchronize(&mut extended_header_data);
            }
            offset += ext_size;
        }

        while offset < tag_size + 10 {
            let (bytes_read, frame) = match Frame::read_from(reader, tag.version, tag.flags.unsynchronization) {
                Ok(opt) => match opt {
                    Some(frame) => frame,
                    None => break //padding
                },
                Err(err) => {
                    debug!("{:?}", err);
                    return Err(err);
                }
            };

            tag.frames.push(frame);

            offset += bytes_read;
        }

        Ok(tag)
    }

    /// Attempts to write the ID3 tag to the writer.
    pub fn write_to(&mut self, writer: &mut Write) -> ::Result<()> {
        // remove frames which have the flags indicating they should be removed
        self.frames.retain(|frame| {
            !(frame.tag_alter_preservation()
                  || (frame.file_alter_preservation()
                          || DEFAULT_FILE_DISCARD.contains(&&frame.id[..])))
        });

        let mut data_cache: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        let mut size = 0;

        for frame in self.frames.iter() {
            let mut frame_writer = Vec::new();
            size += try!(frame.write_to(&mut frame_writer, self.version, self.flags.unsynchronization));
            data_cache.insert(frame.uuid.clone(), frame_writer);
        }

        try!(writer.write_all(b"ID3"));
        try!(writer.write_all(&[self.version.minor() as u8, self.version.major() as u8]));
        try!(writer.write_u8(self.flags.to_byte(self.version)));
        try!(writer.write_u32::<BigEndian>(::util::synchsafe(size)));

        for frame in self.frames.iter_mut() {
            debug!("writing {}", frame.id);
            match data_cache.get(&frame.uuid) {
                Some(data) => {
                    try!(writer.write_all(&data[..]));
                    data.len() as u32
                },
                None => try!(frame.write_to(writer, self.version, self.flags.unsynchronization))
            };
        }

        Ok(())
    }

    /// Attempts to read an ID3 tag from the file at the indicated path.
    pub fn read_from_path<P: AsRef<Path>>(path: P) -> ::Result<Tag> {
        let mut file = BufReader::new(File::open(&path)?);
        Tag::read_from(&mut file)
    }

    /// Attempts to write the ID3 tag from the file at the indicated path. If the specified path is
    /// the same path which the tag was read from, then the tag will be written to the padding if
    /// possible.
    pub fn write_to_path<P: AsRef<Path>>(&mut self, path: P) -> ::Result<()> {
        let mut file = fs::File::open(path)?;
        let location = locate_id3v2(&mut file)?
            .unwrap_or(0..0); // Create a new tag if none could be located.

        let mut storage = PlainStorage::new(file, location);
        let mut w = storage.writer()?;
        self.write_to(&mut w)?;
        w.flush()?;

        Ok(())
    }
    //}}}
}

impl From<::v1::Tag> for Tag {
    fn from(tag_v1: ::v1::Tag) -> Tag {
        let mut tag = Tag::with_version(Version::Id3v24);
        if tag_v1.title.len() > 0 {
            tag.set_title(tag_v1.title.clone());
        }
        if tag_v1.artist.len() > 0 {
            tag.set_artist(tag_v1.artist.clone());
        }
        if tag_v1.album.len() > 0 {
            tag.set_album(tag_v1.album.clone());
        }
        if tag_v1.year.len() > 0 {
            let id = tag.year_id();
            tag.add_text_frame(id, tag_v1.year.clone());
        }
        if tag_v1.comment.len() > 0 {
            tag.add_comment(Comment {
                lang: "eng".to_string(),
                description: "".to_string(),
                text: tag_v1.comment.clone(),
            });
        }
        if let Some(track) = tag_v1.track {
            tag.set_track(track as u32);
        }
        if let Some(genre) = tag_v1.genre() {
            tag.set_genre(genre.to_string());
        }
        tag
    }
}


fn locate_id3v2<R>(reader: &mut R) -> ::Result<Option<ops::Range<u64>>>
    where R: io::Read + io::Seek {
    reader.seek(io::SeekFrom::Start(0))?;
    let mut header = [0u8; 10];
    let nread = reader.read(&mut header)?;
    if nread < header.len() || &header[..3] != b"ID3" {
        return Ok(None);
    }
    match header[3] {
        2|3|4 => (),
        _ => return Err(::Error::new(::ErrorKind::UnsupportedVersion(header[3]) , "unsupported id3 tag version")),
    };

    let size = ::util::unsynchsafe(BigEndian::read_u32(&header[6..10]));
    reader.seek(io::SeekFrom::Start(size as u64))?;
    let num_padding = reader.bytes()
        .take_while(|rs| rs.as_ref().map(|b| *b == 0x00).unwrap_or(false))
        .count();
    Ok(Some(0..size as u64 + num_padding as u64))
}


// Tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io;
    use tag::Flags;

    #[test]
    fn test_flags_to_bytes() {
        let mut flags = Flags::new();
        assert_eq!(flags.to_byte(Id3v24), 0x0);
        flags.unsynchronization = true;
        flags.extended_header = true;
        flags.experimental = true;
        flags.footer = true;
        assert_eq!(flags.to_byte(Id3v24), 0xF0);
    }

    #[test]
    fn test_locate_id3v2() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let location = locate_id3v2(&mut file).unwrap();
        assert!(location.is_some());
    }

    #[test]
    fn read_id3v23() {
        let mut file = fs::File::open("testdata/id3v23.id3").unwrap();
        let tag = Tag::read_from(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!("Genre", tag.genre().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        assert_eq!(PictureType::CoverFront, tag.pictures().get(0).unwrap().picture_type);
    }

    #[test]
    fn read_id3v24() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let tag = Tag::read_from(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        assert_eq!(PictureType::CoverFront, tag.pictures().get(0).unwrap().picture_type);
    }

    #[test]
    fn write_id3v24() {
        let mut tag = Tag::new();
        tag.set_title("Title");
        tag.set_artist("Artist");
        tag.set_genre("Genre");

        let mut buffer = Vec::new();
        tag.write_to(&mut buffer).unwrap();

        let tag_read = Tag::read_from(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag.title(), tag_read.title());
        assert_eq!(tag.artist(), tag_read.artist());
        assert_eq!(tag.genre(), tag_read.genre());
    }
}
// }}}
