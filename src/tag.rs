use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom, BufReader};
use std::ops;
use std::iter;
use std::path::Path;

use byteorder::{ByteOrder, BigEndian, ReadBytesExt, WriteBytesExt};

use frame::{self, Frame, ExtendedText, ExtendedLink, Comment, Lyrics, Picture, PictureType, Timestamp};
use frame::Content;
use ::storage::{PlainStorage, Storage};

static DEFAULT_FILE_DISCARD: [&'static str; 11] = [
    "AENC", "ETCO", "EQUA", "MLLT", "POSS",
    "SYLT", "SYTC", "RVAD", "TENC", "TLEN", "TSIZ"
];


/// Denotes the version of a tag.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Version {
    /// ID3v2.2
    Id3v22,
    /// ID3v2.3
    Id3v23,
    /// ID3v2.4
    Id3v24,
}
pub use self::Version::*;

impl Version {
    /// Returns the minor version.
    ///
    /// # Example
    /// ```
    /// use id3::Version;
    ///
    /// assert_eq!(Version::Id3v24.minor(), 4);
    /// ```
    pub fn minor(&self) -> u8 {
        match *self {
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
    /// Creates a new ID3v2.4 tag with no frames.
    pub fn new() -> Tag {
        Tag::with_version(Version::Id3v24)
    }

    /// Creates a new ID3 tag with the specified version.
    pub fn with_version(version: Version) -> Tag {
        Tag {
            version: version,
            flags: Flags::new(),
            frames: Vec::new(),
        }
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
    /// tag.add_frame(Frame::new("TPE1"));
    /// tag.add_frame(Frame::new("APIC"));
    ///
    /// assert_eq!(tag.frames().count(), 2);
    /// ```
    pub fn frames(&'a self) -> Box<iter::Iterator<Item=&'a Frame> + 'a> {
        Box::new(self.frames.iter())
    }

    /// Returns an iterator over the extended texts in the tag.
    pub fn extended_texts(&'a self) -> Box<iter::Iterator<Item=&'a ExtendedText> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content.extended_text());
        Box::new(iter)
    }

    /// Returns an iterator over the extended links in the tag.
    pub fn extended_links(&'a self) -> Box<iter::Iterator<Item=&'a ExtendedLink> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content.extended_link());
        Box::new(iter)
    }

    /// Returns an iterator over the comments in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::{Content, Comment};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let frame = Frame::with_content("COMM", Content::Comment(Comment {
    ///     lang: "eng".to_owned(),
    ///     description: "key1".to_owned(),
    ///     text: "value1".to_owned()
    /// }));
    /// tag.add_frame(frame);
    ///
    /// let frame = Frame::with_content("COMM", Content::Comment(Comment {
    ///     lang: "eng".to_owned(),
    ///     description: "key2".to_owned(),
    ///     text: "value2".to_owned()
    /// }));
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.comments().count(), 2);
    /// ```
    pub fn comments(&'a self) -> Box<iter::Iterator<Item=&'a Comment> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content.comment());
        Box::new(iter)
    }

    /// Returns an iterator over the extended links in the tag.
    pub fn lyrics(&'a self) -> Box<iter::Iterator<Item=&'a Lyrics> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content.lyrics());
        Box::new(iter)
    }

    /// Returns an iterator over the pictures in the tag.
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
    /// tag.add_frame(Frame::with_content("APIC", Content::Picture(picture.clone())));
    /// tag.add_frame(Frame::with_content("APIC", Content::Picture(picture.clone())));
    ///
    /// assert_eq!(tag.pictures().count(), 1);
    /// ```
    pub fn pictures(&'a self) -> Box<iter::Iterator<Item=&'a Picture> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content.picture());
        Box::new(iter)
    }

    /// Returns a reference to the first frame with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::new("TIT2"));
    ///
    /// assert!(tag.get("TIT2").is_some());
    /// assert!(tag.get("TCON").is_none());
    /// ```
    pub fn get(&self, id: &str) -> Option<&Frame> {
        self.frames.iter()
            .find(|frame| frame.id == id)
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
    /// tag.push(Frame::new("TALB"));
    ///
    /// assert_eq!(tag.get_all("TXXX").len(), 1);
    /// assert_eq!(tag.get_all("TALB").len(), 1);
    /// ```
    #[deprecated(note = "Combine frames() with Iterator::filter")]
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
    /// assert_eq!(&tag.frames().nth(0).unwrap().id[..], "TALB");
    /// ```
    #[deprecated(note = "Use add_frame")]
    pub fn push(&mut self, new_frame: Frame) -> bool {
        self.add_frame(new_frame)
    }

    /// Adds the frame to the tag, replacing any conflicting frame.
    ///
    /// The frame identifier will attempt to be converted into the corresponding identifier for the
    /// tag version.
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
    /// tag.push(Frame::new("TALB"));
    /// assert_eq!(&tag.frames().nth(0).unwrap().id[..], "TALB");
    /// ```
    pub fn add_frame(&mut self, new_frame: Frame) -> bool {
        if let Some(conflict_index) = self.frames.iter().position(|frame| *frame == new_frame) {
            self.frames.remove(conflict_index);
        }
        self.frames.push(new_frame);
        true // TODO
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
    #[deprecated(note = "Use set_text()")]
    pub fn add_text_frame<K: Into<String>, V: Into<String>>(&mut self, id: K, text: V) {
        self.set_text(id, text);
    }

    /// Adds a text frame.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_text("TRCK", "1/13");
    /// assert_eq!(tag.get("TRCK").unwrap().content.text().unwrap(), "1/13");
    /// ```
    pub fn set_text<K: Into<String>, V: Into<String>>(&mut self, id: K, text: V) {
        self.add_frame(Frame::with_content(id, Content::Text(text.into())));
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
    /// assert_eq!(tag.frames().count(), 1);
    ///
    /// let uuid = tag.frames().nth(0).unwrap().uuid.clone();
    /// tag.remove_uuid(&uuid[..]);
    /// assert_eq!(tag.frames().count(), 0);
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
    /// tag.push(Frame::new("USLT"));
    ///
    /// assert_eq!(tag.frames().count(), 2);
    ///
    /// tag.remove("TXXX");
    /// assert_eq!(tag.frames().count(), 1);
    ///
    /// tag.remove("USLT");
    /// assert_eq!(tag.frames().count(), 0);
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
        self.get(id)
            .and_then(|frame| frame.content.text())
    }

    fn read_timestamp_frame(&self, id: &str) -> Option<Timestamp> {
        self.get(id)
            .and_then(|frame| frame.content.text())
            .and_then(|text| text.parse().ok())
    }

    /// Loads a text frame by its ID and attempt to split it into two parts
    ///
    /// Internally used by track and disc getters and setters.
    fn text_pair(&self, id: &str) -> Option<(u32, Option<u32>)> {
        self.get(id)
            .and_then(|frame| frame.content.text())
            .and_then(|text| {
                let mut split = text.splitn(2, '/');
                if let Some(num) = split.next().unwrap().parse().ok() {
                    Some((num, split.next().and_then(|s| s.parse().ok())))
                } else {
                    None
                }
            })
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
    /// tag.add_frame(frame);
    ///
    /// let mut frame = Frame::new("TXXX");
    /// frame.content = Content::ExtendedText(frame::ExtendedText {
    ///     description: "description2".to_owned(),
    ///     value: "value2".to_owned()
    /// });
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("description1", "value1")));
    /// assert!(tag.txxx().contains(&("description2", "value2")));
    /// ```
    pub fn txxx(&self) -> Vec<(&str, &str)> {
        self.frames()
            .filter(|frame| frame.id == self.txxx_id())
            .filter_map(|frame| frame.content.extended_text())
            .map(|ext| (ext.description.as_str(), ext.value.as_str()))
            .collect()
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
        let frame = Frame::with_content(self.txxx_id(), Content::ExtendedText(frame::ExtendedText {
            description: description.into(),
            value: value.into(),
        }));
        self.add_frame(frame);
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
    /// assert_eq!(tag.pictures().count(), 1);
    /// assert_eq!(&tag.pictures().nth(0).unwrap().mime_type[..], "image/png");
    /// ```
    pub fn add_picture(&mut self, picture: Picture) {
        let frame = Frame::with_content(self.picture_id(), Content::Picture(picture));
        self.add_frame(frame);
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
    /// assert_eq!(tag.pictures().count(), 2);
    /// tag.remove_picture_by_type(PictureType::CoverFront);
    /// assert_eq!(tag.pictures().count(), 1);
    /// assert_eq!(tag.pictures().nth(0).unwrap().picture_type, PictureType::Other);
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

    /// Adds a comment (COMM).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Comment;
    ///
    /// let mut tag = Tag::new();
    ///
    /// let com1 = Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key1".to_string(),
    ///     text: "value1".to_string(),
    /// };
    /// let com2 = Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key2".to_string(),
    ///     text: "value2".to_string(),
    /// };
    /// tag.add_comment(com1.clone());
    /// tag.add_comment(com2.clone());
    ///
    /// assert_eq!(tag.comments().count(), 2);
    /// assert_ne!(None, tag.comments().position(|c| *c == com1));
    /// assert_ne!(None, tag.comments().position(|c| *c == com2));
    /// ```
    pub fn add_comment(&mut self, comment: Comment) {
        let frame = Frame::with_content(self.comment_id(), Content::Comment(comment));
        self.add_frame(frame);
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
    /// assert_eq!(tag.comments().count(), 2);
    ///
    /// tag.remove_comment(Some("key1"), None);
    /// assert_eq!(tag.comments().count(), 1);
    ///
    /// tag.remove_comment(None, Some("value2"));
    /// assert_eq!(tag.comments().count(), 0);
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
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.year().unwrap(), 2014);
    ///
    /// tag.remove("TYER");
    ///
    /// let mut frame_invalid = Frame::new("TYER");
    /// frame_invalid.content = Content::Text("nope".to_owned());
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.year().is_none());
    /// ```
    pub fn year(&self) -> Option<i32> {
        let id = self.year_id();
        self.get(id)
            .and_then(|frame| frame.content.text())
            .and_then(|text| text.trim_left_matches("0").parse().ok())
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
    pub fn set_year(&mut self, year: i32) {
        let id = self.year_id();
        self.set_text(id, format!("{:04}", year));
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
        self.set_text("TDRC", time_string);
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
        self.set_text("TDRL", time_string);
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
    /// tag.add_frame(frame);
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
        self.set_text(id, artist);
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
    /// tag.add_frame(frame);
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
        let id = self.album_artist_id();
        self.set_text(id, album_artist);
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
    /// tag.add_frame(frame);
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
        self.set_text(id, album);
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
    /// tag.add_frame(frame);
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
        let id = self.title_id();
        self.set_text(id, title);
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
    /// tag.add_frame(frame);
    /// assert_eq!(tag.duration().unwrap(), 350);
    /// ```
    pub fn duration(&self) -> Option<u32> {
        self.text_for_frame_id("TLEN")
            .and_then(|t| t.parse().ok())
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
        self.set_text("TLEN", duration.to_string());
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
    /// tag.add_frame(frame);
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
        self.set_text(id, genre);
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
        self.text_pair(self.disc_id())
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
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.disc().unwrap(), 4);
    ///
    /// tag.remove("TPOS");
    ///
    /// let mut frame_invalid = Frame::new("TPOS");
    /// frame_invalid.content = Content::Text("nope".to_owned());
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.disc().is_none());
    /// ```
    pub fn disc(&self) -> Option<u32> {
        self.disc_pair()
            .map(|(disc, _)| disc)
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
        let text = match self.text_pair(self.disc_id()).and_then(|(_, total_discs)| total_discs) {
            Some(n) => format!("{}/{}", disc, n),
            None => format!("{}", disc),
        };
        let id = self.disc_id();
        self.set_text(id, text);
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
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.total_discs().unwrap(), 10);
    ///
    /// tag.remove("TPOS");
    ///
    /// let mut frame_invalid = Frame::new("TPOS");
    /// frame_invalid.content = Content::Text("4/nope".to_owned());
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.total_discs().is_none());
    /// ```
    pub fn total_discs(&self) -> Option<u32> {
        self.text_pair(self.disc_id())
            .and_then(|(_, total_discs)| total_discs)
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
        let text = match self.text_pair(self.disc_id()) {
            Some((disc, _)) => format!("{}/{}", disc, total_discs),
            None => format!("1/{}", total_discs)
        };
        let id = self.disc_id();
        self.set_text(id, text);
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
        if let Some((disc, _)) = self.text_pair(self.disc_id()) {
            let id = self.disc_id();
            self.set_text(id, format!("{}", disc));
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
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.track().unwrap(), 4);
    ///
    /// tag.remove("TRCK");
    ///
    /// let mut frame_invalid = Frame::new("TRCK");
    /// frame_invalid.content = Content::Text("nope".to_owned());
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.track().is_none());
    /// ```
    pub fn track(&self) -> Option<u32> {
        self.text_pair(self.track_id())
            .map(|(track, _)| track)
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
        let text = match self.text_pair(self.track_id()).and_then(|(_, total_tracks)| total_tracks) {
            Some(n) => format!("{}/{}", track, n),
            None => format!("{}", track)
        };
        let id = self.track_id();
        self.set_text(id, text);
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
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.total_tracks().unwrap(), 10);
    ///
    /// tag.remove("TRCK");
    ///
    /// let mut frame_invalid = Frame::new("TRCK");
    /// frame_invalid.content = Content::Text("4/nope".to_owned());
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.total_tracks().is_none());
    /// ```
    pub fn total_tracks(&self) -> Option<u32> {
        self.text_pair(self.track_id())
            .and_then(|(_, total_tracks)| total_tracks)
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
        let text = match self.text_pair(self.track_id()) {
            Some((track, _)) => format!("{}/{}", track, total_tracks),
            None => format!("1/{}", total_tracks)
        };
        let id = self.track_id();
        self.set_text(id, text);
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
        if let Some((track, _)) = self.text_pair(self.track_id()) {
            let id = self.track_id();
            self.set_text(id, format!("{}", track));
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
    /// assert_eq!(tag.lyrics().nth(0).unwrap().text, "The lyrics");
    /// ```
    #[deprecated]
    pub fn set_lyrics(&mut self, lyrics: Lyrics) {
        let id = self.lyrics_id();
        let frame = Frame::with_content(id, Content::Lyrics(lyrics));
        self.add_frame(frame);
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
    /// assert_eq!(1, tag.lyrics().count());
    /// tag.remove_lyrics();
    /// assert_eq!(0, tag.lyrics().count());
    /// ```
    #[deprecated]
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
        try!(writer.write_all(&[self.version.minor() as u8, 2]));
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
            tag.set_text(id, tag_v1.year.clone());
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
        assert_eq!(PictureType::CoverFront, tag.pictures().nth(0).unwrap().picture_type);
    }

    #[test]
    fn read_id3v24() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let tag = Tag::read_from(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        assert_eq!(PictureType::CoverFront, tag.pictures().nth(0).unwrap().picture_type);
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
