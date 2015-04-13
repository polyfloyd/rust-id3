extern crate std;
extern crate byteorder;
extern crate libc;

use std::cmp::min;
use std::path::{Path, PathBuf};
use std::io::{self, Read, Write, Seek, SeekFrom};
use std::fs::{self, File, OpenOptions};
use std::collections::HashMap;

use self::byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use frame::{self, Frame, Encoding, Picture, PictureType};
use frame::Content;

use std::ptr;
use std::ffi;
use self::libc::{c_char, L_tmpnam};
extern {
    pub fn tmpnam(s: *mut c_char) -> *const c_char;
}

static DEFAULT_FILE_DISCARD: [&'static str; 11] = [
    "AENC", "ETCO", "EQUA", "MLLT", "POSS", 
    "SYLT", "SYTC", "RVAD", "TENC", "TLEN", "TSIZ"
];
static PADDING_BYTES: u32 = 2048;

/// An ID3 tag containing metadata frames. 
pub struct Tag {
    /// The path, if any, that this file was loaded from.
    path: Option<PathBuf>,
    /// Indicates if the path that we are writing to is not the same as the path we read from.
    path_changed: bool,
    /// The version of the tag. The first byte represents the major version number, while the
    /// second byte represents the revision number.
    version: [u8; 2],
    /// The size of the tag when read from a file.
    size: u32,
    /// The ID3 header flags.
    flags: Flags,
    /// A vector of frames included in the tag.
    frames: Vec<Frame>,
    /// The offset of the end of the last frame that was read.
    offset: u32,
    /// The offset of the first modified frame.
    modified_offset: u32,
    /// Indicates if when writing, an ID3v1 tag should be removed.
    remove_v1: bool
}

/// Flags used in the ID3 header.
#[derive(Copy, Clone)]
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
    pub fn from_byte(byte: u8, version: u8) -> Flags {
        let mut flags = Flags::new();

        flags.unsynchronization = byte & 0x80 != 0;

        if version == 2 {
            flags.compression = byte & 0x40 != 0;
        } else {
            flags.extended_header = byte & 0x40 != 0;
            flags.experimental = byte & 0x20 != 0;

            if version == 4 {
                flags.footer = byte & 0x10 != 0;
            }
        }

        flags
    }

    /// Creates a byte representation of the flags suitable for writing to an ID3 tag.
    pub fn to_byte(&self, version: u8) -> u8 {
        let mut byte = 0;

        if self.unsynchronization {
            byte |= 0x80;
        }

        if version == 2 {
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

            if version == 4 {
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
            path: None, path_changed: true, version: [3, 0], size: 0, flags: Flags::new(), 
            frames: Vec::new(), offset: 0, modified_offset: 0, remove_v1: false
        }
    }

    /// Creates a new ID3 tag with the specified version.
    ///
    /// ID3v2 versions 2 to 4 are supported. Passing any other version will cause a panic.
    pub fn with_version(version: u8) -> Tag {
        if version < 2 || version > 4 {
            panic!("attempted to set unsupported version");
        }
        let mut tag = Tag::new();
        tag.version = [version, 0];
        tag
    }

    // Frame ID Querying {{{
    fn artist_id(&self) -> &'static str {
        if self.version[0] == 2 { "TP1" } else { "TPE1" }
    }

    fn album_artist_id(&self) -> &'static str {
        if self.version[0] == 2 { "TP2" } else { "TPE2" }
    }

    fn album_id(&self) -> &'static str {
        if self.version[0] == 2 { "TAL" } else { "TALB" }
    }

    fn title_id(&self) -> &'static str {
        if self.version[0] == 2 { "TT2" } else { "TIT2" }
    }

    fn genre_id(&self) -> &'static str {
        if self.version[0] == 2 { "TCO" } else { "TCON" }
    }

    fn year_id(&self) -> &'static str {
        if self.version[0] == 2 { "TYE" } else { "TYER" }
    }

    fn track_id(&self) -> &'static str {
        if self.version[0] == 2 { "TRK" } else { "TRCK" }
    }

    fn lyrics_id(&self) -> &'static str {
        if self.version[0] == 2 { "ULT" } else { "USLT" }
    }

    fn picture_id(&self) -> &'static str {
        if self.version[0] == 2 { "PIC" } else { "APIC" }
    }

    fn comment_id(&self) -> &'static str {
        if self.version[0] == 2 { "COM" } else { "COMM" }
    }

    fn txxx_id(&self) -> &'static str {
        if self.version[0] == 2 { "TXX" } else { "TXXX" }
    }
    // }}}

    // id3v1 {{{
    /// Returns true if the reader might contain a valid ID3v1 tag.
    pub fn is_candidate_v1<R: Read + Seek>(reader: &mut R) -> bool {
        match ::id3v1::probe_tag(reader) {
            Ok(has_tag) => has_tag,
            Err(_) => false
        }
    }

    /// Attempts to read an ID3v1 tag from the reader. Since the structure of ID3v1 is so different
    /// from ID3v2, the tag will be converted and stored internally as an ID3v2.3 tag.
    pub fn read_from_v1<R: Read + Seek>(reader: &mut R) -> ::Result<Tag> {
        let tag_v1 = try!(::id3v1::read(reader));

        let mut tag = Tag::with_version(3);
        tag.remove_v1 = true;

        if tag_v1.title.is_some() {
            tag.set_title(tag_v1.title.unwrap()); 
        }

        if tag_v1.artist.is_some() {
            tag.set_artist(tag_v1.artist.unwrap());
        }

        if tag_v1.album.is_some() {
            tag.set_album(tag_v1.album.unwrap());
        }

        if tag_v1.year.is_some() {
            let mut frame = Frame::new(tag.year_id());
            frame.content = Content::Text(tag_v1.year.unwrap());
            tag.push(frame);
        }

        if tag_v1.comment.is_some() {
            tag.add_comment(String::new(), tag_v1.comment.unwrap());
        }

        if tag_v1.track.is_some() {
            tag.set_track(tag_v1.track.unwrap() as u32);
        }

        if tag_v1.genre_str.is_some() {
            tag.set_genre(tag_v1.genre_str.unwrap());
        } else if tag_v1.genre.is_some() {
            // TODO maybe convert this from the genre id into a string
            tag.set_genre(format!("{}", tag_v1.genre.unwrap()));
        }

        Ok(tag)
    }

    /// Attempts to read an ID3v1 tag from the file at the specified path. The tag will be
    /// converted into an ID3v2.3 tag upon success.
    pub fn read_from_path_v1<P: AsRef<Path>>(path: P) -> ::Result<Tag> {
        let mut file = try!(File::open(&path));
        let mut tag = try!(Tag::read_from_v1(&mut file));
        tag.path = Some(path.as_ref().to_path_buf());
        tag.path_changed = false;
        Ok(tag)
    }
    // }}}

    /// Returns the version of the tag.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let tag = Tag::with_version(3);
    /// assert_eq!(tag.version(), 3);
    /// ```
    pub fn version(&self) -> u8 {
        self.version[0]
    }

    /// Sets the version of this tag.
    ///
    /// ID3v2 versions 2 to 4 can be set. Trying to set any other version will cause a panic.
    ///
    /// Any frames that could not be converted to the new version will be dropped.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::with_version(4);
    /// assert_eq!(tag.version(), 4);
    ///
    /// tag.set_version(3);
    /// assert_eq!(tag.version(), 3);
    /// ```
    pub fn set_version(&mut self, version: u8) {
        if version < 2 || version > 4 {
            panic!("attempted to set unsupported version");
        }

        if self.version[0] == version {
            return;
        }

        let mut remove_uuid = Vec::new();
        for mut frame in self.frames.iter_mut() {
            if !Tag::convert_frame_version(&mut frame, self.version[0], version) {
                remove_uuid.push(frame.uuid.clone());
            }
        }

        self.frames.retain(|frame: &Frame| !remove_uuid.contains(&frame.uuid));

        self.version = [version, 0];
        self.modified_offset = 0;

    }

    fn convert_frame_version(frame: &mut Frame, old_version: u8, new_version: u8) -> bool {
        if old_version == new_version || (old_version == 3 && new_version == 4) || (old_version == 4 && new_version == 3) {
            return true;
        }

        if (old_version == 3 || old_version == 4) && new_version == 2 {
            // attempt to convert the id
            frame.id = match ::util::convert_id_3_to_2(&frame.id[..]) {
                Some(id) => id.to_string(),
                None => {
                    debug!("no ID3v2.3 to ID3v2.3 mapping for {}", frame.id);
                    return false;
                }
            }
        } else if old_version == 2 && (new_version == 3 || new_version == 4) {
            // attempt to convert the id
            frame.id = match ::util::convert_id_2_to_3(&frame.id[..]) {
                Some(id) => id.to_string(),
                None => {
                    debug!("no ID3v2.2 to ID3v2.3 mapping for {}", frame.id);
                    return false;
                }
            };

            // if the new version is v2.4 and the frame is compressed, we must enable the
            // data_length_indicator flag
            if new_version == 4 && frame.compression() {
                frame.set_compression(true);
            }
        } else {
            // not sure when this would ever occur but lets just say the conversion failed
            return false;
        }

        true
    }

    /// Returns the default unicode text encoding that should be used for this tag.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::{UTF16, UTF8};
    ///
    /// let mut tag_v3 = Tag::with_version(3);
    /// assert_eq!(tag_v3.default_encoding(), UTF16);
    ///
    /// let mut tag_v4 = Tag::with_version(4);
    /// assert_eq!(tag_v4.default_encoding(), UTF8);
    /// ```
    pub fn default_encoding(&self) -> Encoding {
        if self.version[0] >= 4 { Encoding::UTF8 } else { Encoding::UTF16 }
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
        frame.offset = 0;
        self.frames.push(frame);
        true
    }

    /// Adds a text frame using the default text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_text_frame("TCON", "Metal");
    /// assert_eq!(&tag.get("TCON").unwrap().content.text()[..], "Metal");
    /// ```
    pub fn add_text_frame<K: Into<String>, V: Into<String>>(&mut self, id: K, text: V) {
        let encoding = self.default_encoding();
        self.add_text_frame_enc(id, text, encoding);
    }

    /// Adds a text frame using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_text_frame_enc("TRCK", "1/13", UTF16);
    /// assert_eq!(&tag.get("TRCK").unwrap().content.text()[..], "1/13");
    /// ```
    pub fn add_text_frame_enc<K: Into<String>, V: Into<String>>(&mut self, id: K, text: V, encoding: Encoding) {
        let id = id.into();

        self.remove(&id[..]);

        let mut frame = Frame::new(id);
        frame.encoding = encoding;
        frame.content = Content::Text(text.into());

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
        self.modified_offset = {
            let mut modified_offset = self.modified_offset;
            self.frames.retain(|frame| { 
                let keep = &frame.uuid[..] != uuid; 
                if !keep && frame.offset != 0 {
                    modified_offset = min(modified_offset, frame.offset);
                }
                keep
            });
            modified_offset
        };
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
        self.modified_offset = {
            let mut modified_offset = self.modified_offset;
            self.frames.retain(|frame| {
                let keep = &frame.id[..] != id;
                if !keep && frame.offset != 0 {
                    modified_offset = min(modified_offset, frame.offset);
                }
                keep
            });
            modified_offset
        };
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
    /// Returns a vector of the extended text (TXXX) key/value pairs.
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
    ///     key: "key1".to_string(),
    ///     value: "value1".to_string()
    /// });
    /// tag.push(frame);
    ///
    /// let mut frame = Frame::new("TXXX");
    /// frame.content = Content::ExtendedText(frame::ExtendedText { 
    ///     key: "key2".to_string(),
    ///     value: "value2".to_string()
    /// }); 
    /// tag.push(frame);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("key1", "value1")));
    /// assert!(tag.txxx().contains(&("key2", "value2")));
    /// ```
    pub fn txxx(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::new();
        for frame in self.get_all(self.txxx_id()).iter() {
            match frame.content {
                Content::ExtendedText(ref ext) => out.push((&ext.key[..], &ext.value[..])),
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
    pub fn add_txxx<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        let encoding = self.default_encoding();
        self.add_txxx_enc(key, value, encoding);
    }

    /// Adds a user defined text frame (TXXX) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_txxx_enc("key1", "value1", UTF16);
    /// tag.add_txxx_enc("key2", "value2", UTF16);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("key1", "value1")));
    /// assert!(tag.txxx().contains(&("key2", "value2")));
    /// ```
    pub fn add_txxx_enc<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V, encoding: Encoding) {
        let key = key.into();

        self.remove_txxx(Some(&key[..]), None);

        let mut frame = Frame::new(self.txxx_id());
        frame.encoding = encoding;
        frame.content = Content::ExtendedText(frame::ExtendedText { 
            key: key, 
            value: value.into()
        });

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
    pub fn remove_txxx(&mut self, key: Option<&str>, value: Option<&str>) {
        let mut modified_offset = self.modified_offset;

        let id = self.txxx_id();
        self.frames.retain(|frame| {
            let mut key_match = false;
            let mut value_match = false;

            if &frame.id[..] == id {
                match frame.content {
                    Content::ExtendedText(ref ext) => {
                        match key {
                            Some(s) => key_match = s == &ext.key[..],
                            None => key_match = true
                        }

                        match value {
                            Some(s) => value_match = s == &ext.value[..],
                            None => value_match = true 
                        }
                    },
                    _ => { // remove frames that we can't parse
                        key_match = true;
                        value_match = true;
                    }
                }
            }

            if key_match && value_match && frame.offset != 0 {
                modified_offset = min(modified_offset, frame.offset);
            }

            !(key_match && value_match) // true if we want to keep the item
        });

        self.modified_offset = modified_offset;
    }

    /// Returns a vector of references to the pictures in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::{Content, Picture};
    ///
    /// let mut tag = Tag::new();
    /// 
    /// let mut frame = Frame::new("APIC");
    /// frame.content = Content::Picture(Picture::new());
    /// tag.push(frame);
    ///
    /// let mut frame = Frame::new("APIC");
    /// frame.content = Content::Picture(Picture::new());
    /// tag.push(frame);
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
    /// use id3::frame::PictureType::Other;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_picture("image/jpeg", Other, vec!());
    /// tag.add_picture("image/png", Other, vec!());
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(&tag.pictures()[0].mime_type[..], "image/png");
    /// ```
    pub fn add_picture<T: Into<String>>(&mut self, mime_type: T, picture_type: PictureType, data: Vec<u8>) {
        self.add_picture_enc(mime_type, picture_type, "", data, Encoding::Latin1);
    }

    /// Adds a picture frame (APIC) using the specified text encoding.
    /// Any other pictures with the same type will be removed from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::PictureType::Other;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_picture_enc("image/jpeg", Other, "", vec!(), UTF16);
    /// tag.add_picture_enc("image/png", Other, "", vec!(), UTF16);
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(&tag.pictures()[0].mime_type[..], "image/png");
    /// ```
    pub fn add_picture_enc<S: Into<String>, T: Into<String>>(&mut self, mime_type: S, picture_type: PictureType, description: T, data: Vec<u8>, encoding: Encoding) {
        self.remove_picture_type(picture_type);

        let mut frame = Frame::new(self.picture_id());

        frame.encoding = encoding;
        frame.content = Content::Picture(Picture { 
            mime_type: mime_type.into(), 
            picture_type: picture_type, 
            description: description.into(), 
            data: data 
        });

        self.frames.push(frame);
    }

    /// Removes all pictures of the specified type.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::PictureType::{CoverFront, Other};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_picture("image/jpeg", CoverFront, vec!());
    /// tag.add_picture("image/png", Other, vec!());
    /// assert_eq!(tag.pictures().len(), 2);
    ///
    /// tag.remove_picture_type(CoverFront);
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(tag.pictures()[0].picture_type, Other);
    /// ```
    pub fn remove_picture_type(&mut self, picture_type: PictureType) {
        let mut modified_offset = self.modified_offset;

        let id = self.picture_id();
        self.frames.retain(|frame| {
            if &frame.id[..] == id {
                let pic = match frame.content {
                    Content::Picture(ref picture) => picture,
                    _ => return false
                };

                if pic.picture_type == picture_type && frame.offset != 0 {
                    modified_offset = min(modified_offset, frame.offset);
                }

                return pic.picture_type != picture_type
            }

            true
        });

        self.modified_offset = modified_offset;
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
    ///     lang: "eng".to_string(),
    ///     description: "key1".to_string(),
    ///     text: "value1".to_string()
    /// });
    /// tag.push(frame);
    ///
    /// let mut frame = Frame::new("COMM");
    /// frame.content = Content::Comment(Comment { 
    ///     lang: "eng".to_string(),
    ///     description: "key2".to_string(),
    ///     text: "value2".to_string()
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
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_comment("key1", "value1");
    /// tag.add_comment("key2", "value2");
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1", "value1")));
    /// assert!(tag.comments().contains(&("key2", "value2")));
    /// ```
    pub fn add_comment<K: Into<String>, V: Into<String>>(&mut self, description: K, text: V) {
        let encoding = self.default_encoding();
        self.add_comment_enc("eng", description, text, encoding);
    }

    /// Adds a comment (COMM) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_comment_enc("eng", "key1", "value1", UTF16);
    /// tag.add_comment_enc("eng", "key2", "value2", UTF16);
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1", "value1")));
    /// assert!(tag.comments().contains(&("key2", "value2")));
    /// ```
    pub fn add_comment_enc<L: Into<String>, K: Into<String>, V: Into<String>>(&mut self, lang: L, description: K, text: V, encoding: Encoding) {
        let description = description.into();

        self.remove_comment(Some(&description[..]), None);

        let mut frame = Frame::new(self.comment_id());

        frame.encoding = encoding;
        frame.content = Content::Comment(frame::Comment { 
            lang: lang.into(), 
            description: description, 
            text: text.into() 
        });

        self.frames.push(frame);
    }

    /// Removes the comment (COMM) with the specified key and value.
    ///
    /// A key or value may be `None` to specify a wildcard value.
    /// 
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_comment("key1", "value1");
    /// tag.add_comment("key2", "value2");
    /// tag.add_comment("key3", "value2");
    /// tag.add_comment("key4", "value3");
    /// tag.add_comment("key5", "value4");
    /// assert_eq!(tag.comments().len(), 5);
    ///
    /// tag.remove_comment(Some("key1"), None);
    /// assert_eq!(tag.comments().len(), 4);
    ///
    /// tag.remove_comment(None, Some("value2"));
    /// assert_eq!(tag.comments().len(), 2);
    ///
    /// tag.remove_comment(Some("key4"), Some("value3"));
    /// assert_eq!(tag.comments().len(), 1);
    ///
    /// tag.remove_comment(None, None);
    /// assert_eq!(tag.comments().len(), 0);
    /// ```
    pub fn remove_comment(&mut self, description: Option<&str>, text: Option<&str>) {
        let mut modified_offset = self.modified_offset;

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

            if description_match && text_match && frame.offset != 0 {
                modified_offset = frame.offset;
            }

            !(description_match && text_match) // true if we want to keep the item
        });

        self.modified_offset = modified_offset;
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
    /// frame_valid.content = Content::Text("2014".to_string());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.year().unwrap(), 2014);
    ///
    /// tag.remove("TYER");
    ///
    /// let mut frame_invalid = Frame::new("TYER");
    /// frame_invalid.content = Content::Text("nope".to_string());
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
        self.add_text_frame_enc(id, format!("{}", year), Encoding::Latin1);
    }

    /// Sets the year (TYER) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_year_enc(2014, UTF16);
    /// assert_eq!(tag.year().unwrap(), 2014);
    /// ```
    pub fn set_year_enc(&mut self, year: usize, encoding: Encoding) {
        let id = self.year_id();
        self.add_text_frame_enc(id, format!("{}", year), encoding);
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
    /// frame.content = Content::Text("artist".to_string());
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
        let encoding = self.default_encoding();
        self.set_artist_enc(artist, encoding);
    }

    /// Sets the artist (TPE1) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_artist_enc("artist", UTF16);
    /// assert_eq!(tag.artist().unwrap(), "artist");
    /// ```
    pub fn set_artist_enc<T: Into<String>>(&mut self, artist: T, encoding: Encoding) {
        let id = self.artist_id();
        self.add_text_frame_enc(id, artist, encoding);
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
    /// frame.content = Content::Text("artist".to_string());
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
        let encoding = self.default_encoding();
        self.set_album_artist_enc(album_artist, encoding);
    }

    /// Sets the album artist (TPE2) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album_artist_enc("album artist", UTF16);
    /// assert_eq!(tag.album_artist().unwrap(), "album artist");
    /// ```
    pub fn set_album_artist_enc<T: Into<String>>(&mut self, album_artist: T, encoding: Encoding) {
        self.remove("TSOP");
        let id = self.album_artist_id();
        self.add_text_frame_enc(id, album_artist, encoding);
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
    /// frame.content = Content::Text("album".to_string());
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
        let encoding = self.default_encoding();
        self.set_album_enc(album, encoding);
    }

    /// Sets the album (TALB) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album_enc("album", UTF16);
    /// assert_eq!(tag.album().unwrap(), "album");
    /// ```
    pub fn set_album_enc<T: Into<String>>(&mut self, album: T, encoding: Encoding) {
        let id = self.album_id();
        self.add_text_frame_enc(id, album, encoding);
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
    /// frame.content = Content::Text("title".to_string());
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
        let encoding = self.default_encoding();
        self.set_title_enc(title, encoding);
    }

    /// Sets the song title (TIT2) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_title_enc("title", UTF16);
    /// assert_eq!(tag.title().unwrap(), "title");
    /// ```
    pub fn set_title_enc<T: Into<String>>(&mut self, title: T, encoding: Encoding) {
        self.remove("TSOT");
        let id = self.title_id();
        self.add_text_frame_enc(id, title, encoding);
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
    /// frame.content = Content::Text("genre".to_string());
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
        let encoding = self.default_encoding();
        self.set_genre_enc(genre, encoding);
    }

    /// Sets the genre (TCON) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_genre_enc("genre", UTF16);
    /// assert_eq!(tag.genre().unwrap(), "genre");
    /// ```
    pub fn set_genre_enc<T: Into<String>>(&mut self, genre: T, encoding: Encoding) {
        let id = self.genre_id();
        self.add_text_frame_enc(id, genre, encoding);
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

    /// Returns the track number (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.year().is_none());
    ///
    /// let mut frame_valid = Frame::new("TRCK");
    /// frame_valid.content = Content::Text("4".to_string());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.track().unwrap(), 4);
    ///
    /// tag.remove("TRCK");
    ///
    /// let mut frame_invalid = Frame::new("TRCK");
    /// frame_invalid.content = Content::Text("nope".to_string());
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
    /// tag.set_year(2014);
    /// assert_eq!(tag.year().unwrap(), 2014);
    /// ```
    pub fn set_track(&mut self, track: u32) {
        self.set_track_enc(track, Encoding::Latin1);
    }

    /// Sets the track number (TRCK) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_track_enc(5, UTF16);
    /// assert_eq!(tag.track().unwrap(), 5);
    /// ```
    pub fn set_track_enc(&mut self, track: u32, encoding: Encoding) {
        let text = match self.track_pair().and_then(|(_, total_tracks)| total_tracks) {
            Some(n) => format!("{}/{}", track, n),
            None => format!("{}", track)
        };

        let id = self.track_id();
        self.add_text_frame_enc(id, text, encoding);
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
    /// assert!(tag.year().is_none());
    ///
    /// let mut frame_valid = Frame::new("TRCK");
    /// frame_valid.content = Content::Text("4/10".to_string());
    /// tag.push(frame_valid);
    /// assert_eq!(tag.total_tracks().unwrap(), 10);
    ///
    /// tag.remove("TRCK");
    ///
    /// let mut frame_invalid = Frame::new("TRCK");
    /// frame_invalid.content = Content::Text("4/nope".to_string());
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
        self.set_total_tracks_enc(total_tracks, Encoding::Latin1);
    }

    /// Sets the total number of tracks (TRCK) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_tracks_enc(12, UTF16);
    /// assert_eq!(tag.total_tracks().unwrap(), 12);
    /// ```
    pub fn set_total_tracks_enc(&mut self, total_tracks: u32, encoding: Encoding) {
        let text = match self.track_pair() {
            Some((track, _)) => format!("{}/{}", track, total_tracks),
            None => format!("1/{}", total_tracks)
        };

        let id = self.track_id();
        self.add_text_frame_enc(id, text, encoding);
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
    ///     lang: "eng".to_string(),
    ///     description: "description".to_string(),
    ///     text: "lyrics".to_string()
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
    ///
    /// let mut tag = Tag::new();
    /// tag.set_lyrics("lyrics");
    /// assert_eq!(tag.lyrics().unwrap(), "lyrics");
    /// ```
    pub fn set_lyrics<T: Into<String>>(&mut self, text: T) {
        let encoding = self.default_encoding();
        self.set_lyrics_enc("eng", "", text, encoding);
    }

    /// Sets the lyrics text (USLT) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Encoding::UTF16;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_lyrics_enc("eng", "description", "lyrics", UTF16);
    /// assert_eq!(tag.lyrics().unwrap(), "lyrics");
    /// ```
    pub fn set_lyrics_enc<L: Into<String>, K: Into<String>, V: Into<String>>(&mut self, lang: L, description: K, text: V, encoding: Encoding) {
        let id = self.lyrics_id();
        self.remove(id);

        let mut frame = Frame::new(id);

        frame.encoding = encoding;
        frame.content = Content::Lyrics(frame::Lyrics { 
            lang: lang.into(), 
            description: description.into(), 
            text: text.into() 
        });

        self.frames.push(frame);
    }

    /// Removes the lyrics text (USLT) from the tag.
    ///
    /// # Exmaple
    /// ```
    /// use id3::Tag;
    /// 
    /// let mut tag = Tag::new();
    /// tag.set_lyrics("lyrics");
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
    pub fn is_candidate<R: Read + Seek>(reader: &mut R) -> bool {

        macro_rules! try_or_false {
            ($action:expr) => {
                match $action { 
                    Ok(result) => result, 
                    Err(_) => return false 
                }
            }
        }

        let mut ident = [0u8; 3];
        try_or_false!(reader.read(&mut ident));
        let _ = reader.seek(SeekFrom::Current(-3));
        &ident[..] == b"ID3"
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

        try!(reader.read(&mut tag.version));

        debug!("tag version {}", tag.version[0]);

        if tag.version[0] < 2 || tag.version[0] > 4 {
            return Err(::Error::new(::ErrorKind::UnsupportedVersion(tag.version[0]) , "unsupported id3 tag version"));
        }

        tag.flags = Flags::from_byte(try!(reader.read_u8()), tag.version[0]);

        if tag.flags.unsynchronization {
            debug!("unsynchronization is unsupported");
            return Err(::Error::new(::ErrorKind::UnsupportedFeature, "unsynchronization is not supported"))
        } else if tag.flags.compression {
            debug!("id3v2.2 compression is unsupported");
            return Err(::Error::new(::ErrorKind::UnsupportedFeature, "id3v2.2 compression is not supported"));
        }

        tag.size = ::util::unsynchsafe(try!(reader.read_u32::<BigEndian>()));

        let mut offset = 10;

        // TODO actually use the extended header data
        if tag.flags.extended_header {
            let ext_size = ::util::unsynchsafe(try!(reader.read_u32::<BigEndian>()));
            offset += 4;
            let _ = try!(reader.take(ext_size as u64).read_to_end(&mut Vec::with_capacity(ext_size as usize)));
            offset += ext_size;
        }

        while offset < tag.size + 10 {
            let (bytes_read, mut frame) = match Frame::read_from(reader, tag.version[0]) {
                Ok(opt) => match opt {
                    Some(frame) => frame,
                    None => break //padding
                },
                Err(err) => {
                    debug!("{:?}", err);
                    return Err(err);
                }
            };

            frame.offset = offset;
            tag.frames.push(frame);

            offset += bytes_read;
        }

        tag.offset = offset;
        tag.modified_offset = tag.offset;

        Ok(tag)
    }

    /// Attempts to write the ID3 tag to the writer.
    pub fn write_to(&mut self, writer: &mut Write) -> ::Result<()> {
        let path_changed = self.path_changed;

        // remove frames which have the flags indicating they should be removed 
        self.frames.retain(|frame| {
            !(frame.offset != 0 
              && (frame.tag_alter_preservation() 
                  || (path_changed 
                      && (frame.file_alter_preservation() 
                          || DEFAULT_FILE_DISCARD.contains(&&frame.id[..])))))
        });

        let mut data_cache: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        let mut size = 0;

        for frame in self.frames.iter() {
            let mut frame_writer = Vec::new();
            size += try!(frame.write_to(&mut frame_writer, self.version[0]));
            data_cache.insert(frame.uuid.clone(), frame_writer);
        }

        self.size = size + PADDING_BYTES;

        try!(writer.write_all(b"ID3"));
        try!(writer.write_all(&mut self.version)); 
        try!(writer.write_u8(self.flags.to_byte(self.version[0])));
        try!(writer.write_u32::<BigEndian>(::util::synchsafe(self.size)));

        let mut bytes_written = 10;

        for frame in self.frames.iter_mut() {
            debug!("writing {}", frame.id);

            frame.offset = bytes_written;

            bytes_written += match data_cache.get(&frame.uuid) {
                Some(data) => { 
                    try!(writer.write_all(&data[..]));
                    data.len() as u32
                },
                None => try!(frame.write_to(writer, self.version[0]))
            }
        }

        self.offset = bytes_written;
        self.modified_offset = self.offset;

        // write padding
        for _ in 0..PADDING_BYTES {
            try!(writer.write_u8(0));
        }

        Ok(())
    }

    /// Attempts to read an ID3 tag from the file at the indicated path.
    pub fn read_from_path<P: AsRef<Path>>(path: P) -> ::Result<Tag> {
        let mut file = try!(File::open(&path));
        let mut tag: Tag = try!(Tag::read_from(&mut file));
        tag.path = Some(path.as_ref().to_path_buf());
        tag.path_changed = false;
        Ok(tag)
    }

    /// Attempts to write the ID3 tag from the file at the indicated path. If the specified path is
    /// the same path which the tag was read from, then the tag will be written to the padding if
    /// possible.
    pub fn write_to_path<P: AsRef<Path>>(&mut self, path: P) -> ::Result<()> {
        let mut write_new_file = true;
        if self.path.is_some() && path.as_ref() == self.path.as_ref().unwrap().as_path() {
            // remove any old frames that have the tag_alter_presevation flag
            self.modified_offset = {
                let mut modified_offset = self.modified_offset;
                self.frames.retain(|frame| {
                    let keep = frame.offset == 0 || !frame.tag_alter_preservation();
                    if !keep {
                        modified_offset = min(modified_offset, frame.offset);
                    }
                    keep
                });
                modified_offset
            };

            let mut data_cache: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
            let mut size = 0;

            for frame in self.frames.iter() {
                let mut frame_writer = Vec::new();
                size += try!(frame.write_to(&mut frame_writer, self.version[0]));
                data_cache.insert(frame.uuid.clone(), frame_writer);
            }

            debug!("modified offset: {}", self.modified_offset); 

            if size <= self.size && self.modified_offset >= 10 {
                debug!("writing using padding");

                let mut writer = try!(OpenOptions::new().create(true).write(true).open(self.path.as_ref().unwrap()));

                let mut offset = self.modified_offset;
                try!(writer.seek(SeekFrom::Start(offset as u64)));

                for frame in self.frames.iter_mut() {
                    if frame.offset == 0 || frame.offset > self.modified_offset {
                        debug!("writing {}", frame.id);
                        frame.offset = offset;
                        offset += match data_cache.get(&frame.uuid) {
                            Some(data) => { 
                                try!(writer.write_all(&data[..]));
                                data.len() as u32
                            },
                            None => try!(frame.write_to(&mut writer, self.version[0]))
                        }
                    }
                }

                if self.offset > offset {
                    for _ in offset..self.offset {
                        try!(writer.write_u8(0));
                    }
                }

                write_new_file = false;

                self.offset = offset;
                self.modified_offset = self.offset;
            }
        } 

        if write_new_file {
            let data_opt = {
                match File::open(&path) {
                    Ok(mut file) => {
                        // remove the ID3v1 tag if the remove_v1 flag is set
                        let remove_bytes = if self.remove_v1 {
                            if try!(::id3v1::probe_xtag(&mut file)) {
                                Some(::id3v1::TAGPLUS_OFFSET as usize)
                            } else if try!(::id3v1::probe_tag(&mut file)) {
                                Some(::id3v1::TAG_OFFSET as usize)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let mut data = Tag::skip_metadata(&mut file);
                        match remove_bytes {
                            Some(n) => if n <= data.len() {
                                data = data[..data.len() - n].to_vec();
                            },
                            None => {}
                        }
                        Some(data)
                    }
                    Err(_) => None
                }
            };

            self.path_changed = self.path.is_none() || &**self.path.as_ref().unwrap() != path.as_ref();

            let tmp_name = unsafe {
                let mut c_buf: [c_char; L_tmpnam as usize + 1] = [0; L_tmpnam as usize + 1];
                let ret = tmpnam(c_buf.as_mut_ptr());
                if ret == ptr::null() {
                    return Err(::Error::from(io::Error::new(io::ErrorKind::Other, "failed to create temporary file")))
                }
                try!(String::from_utf8(ffi::CStr::from_ptr(c_buf.as_ptr()).to_bytes().to_vec()))
            };
            debug!("writing to temporary file: {}", tmp_name);

            let mut file = try!(OpenOptions::new().write(true).truncate(true).create(true).open(&tmp_name[..]));
            try!(self.write_to(&mut file));

            match data_opt {
                Some(data) => try!(file.write_all(&data[..])),
                None => {}
            }

            try!(fs::rename(tmp_name, &path));
        }

        self.path = Some(path.as_ref().to_path_buf());
        self.path_changed = false;

        Ok(())
    }

    /// Attempts to save the tag back to the file which it was read from. An error with kind
    /// `InvalidInput` will be returned if this is called on a tag which was not read from a file.
    pub fn save(&mut self) -> ::Result<()> {
        if self.path.is_none() {
            return Err(::Error::new(::ErrorKind::InvalidInput, "attempted to save file which was not read from a path"))
        }

        let path = self.path.clone().unwrap();
        self.write_to_path(path)
    }
    //}}}
}

// Tests {{{
#[cfg(test)]
mod tests {
    use tag::Flags;

    #[test]
    fn test_flags_to_bytes() {
        let mut flags = Flags::new();
        assert_eq!(flags.to_byte(4), 0x0);
        flags.unsynchronization = true;
        flags.extended_header = true;
        flags.experimental = true;
        flags.footer = true;
        assert_eq!(flags.to_byte(4), 0xF0);
    }
}
// }}}
