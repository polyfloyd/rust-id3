extern crate std;

use std::cmp::min;
use std::old_io::{File, Open, Truncate, Write, SeekSet, SeekCur};
use std::collections::HashMap;
use std::borrow::IntoCow;

use audiotag::{AudioTag, TagError, TagResult};
use audiotag::ErrorKind::{InvalidInputError, UnsupportedFeatureError};

use id3v1;
use frame::{self, Frame, Encoding, Picture, PictureType};
use frame::Content::{PictureContent, CommentContent, TextContent, ExtendedTextContent, LyricsContent};
use util;

static DEFAULT_FILE_DISCARD: [&'static str; 11] = [
    "AENC", "ETCO", "EQUA", "MLLT", "POSS", 
    "SYLT", "SYTC", "RVAD", "TENC", "TLEN", "TSIZ"
];
static PADDING_BYTES: u32 = 2048;

/// An ID3 tag containing metadata frames. 
pub struct ID3Tag {
    /// The path, if any, that this file was loaded from.
    path: Option<Path>,
    /// Indicates if the path that we are writing to is not the same as the path we read from.
    path_changed: bool,
    /// The version of the tag. The first byte represents the major version number, while the
    /// second byte represents the revision number.
    version: [u8; 2],
    /// The size of the tag when read from a file.
    size: u32,
    /// The ID3 header flags.
    flags: TagFlags,
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
#[derive(Copy)]
pub struct TagFlags {
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

// TagFlags {{{
impl TagFlags {
    /// Creates a new `TagFlags` with all flags set to false.
    #[inline]
    pub fn new() -> TagFlags {
        TagFlags { 
            unsynchronization: false, extended_header: false, experimental: false, 
            footer: false, compression: false 
        }
    }

    /// Creates a new `TagFlags` using the provided byte.
    pub fn from_byte(byte: u8, version: u8) -> TagFlags {
        let mut flags = TagFlags::new();

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
impl<'a> ID3Tag {
    /// Creates a new ID3v2.3 tag with no frames. 
    #[inline]
    pub fn new() -> ID3Tag {
        ID3Tag { 
            path: None, path_changed: true, version: [3, 0], size: 0, flags: TagFlags::new(), 
            frames: Vec::new(), offset: 0, modified_offset: 0, remove_v1: false
        }
    }

    /// Creates a new ID3 tag with the specified version.
    ///
    /// ID3v2 versions 2 to 4 are supported. Passing any other version will cause a panic.
    #[inline]
    pub fn with_version(version: u8) -> ID3Tag {
        if version < 2 || version > 4 {
            panic!("attempted to set unsupported version");
        }
        let mut tag = ID3Tag::new();
        tag.version = [version, 0];
        tag
    }

    // Frame ID Querying {{{
    #[inline]
    fn artist_id(&self) -> &'static str {
        if self.version[0] == 2 { "TP1" } else { "TPE1" }
    }

    #[inline]
    fn album_artist_id(&self) -> &'static str {
        if self.version[0] == 2 { "TP2" } else { "TPE2" }
    }

    #[inline]
    fn album_id(&self) -> &'static str {
        if self.version[0] == 2 { "TAL" } else { "TALB" }
    }

    #[inline]
    fn title_id(&self) -> &'static str {
        if self.version[0] == 2 { "TT2" } else { "TIT2" }
    }

    #[inline]
    fn genre_id(&self) -> &'static str {
        if self.version[0] == 2 { "TCO" } else { "TCON" }
    }

    #[inline]
    fn year_id(&self) -> &'static str {
        if self.version[0] == 2 { "TYE" } else { "TYER" }
    }

    #[inline]
    fn track_id(&self) -> &'static str {
        if self.version[0] == 2 { "TRK" } else { "TRCK" }
    }

    #[inline]
    fn lyrics_id(&self) -> &'static str {
        if self.version[0] == 2 { "ULT" } else { "USLT" }
    }

    #[inline]
    fn picture_id(&self) -> &'static str {
        if self.version[0] == 2 { "PIC" } else { "APIC" }
    }

    #[inline]
    fn comment_id(&self) -> &'static str {
        if self.version[0] == 2 { "COM" } else { "COMM" }
    }

    #[inline]
    fn txxx_id(&self) -> &'static str {
        if self.version[0] == 2 { "TXX" } else { "TXXX" }
    }
    // }}}

    /// Returns true if the reader might contain a valid ID3v1 tag. This method is different than
    /// AudioTag::is_candidate() since this methods requires the Seek trait.
    pub fn is_candidate_v1<R: Reader + Seek>(reader: &mut R) -> bool {
        match id3v1::probe_tag(reader) {
            Ok(has_tag) => has_tag,
            Err(_) => false
        }
    }

    /// Attempts to read an ID3v1 tag from the reader. Since the structure of ID3v1 is so different
    /// from ID3v2, the tag will be converted and stored internally as an ID3v2.3 tag.
    pub fn read_from_v1<R: Reader + Seek>(reader: &mut R) -> TagResult<ID3Tag> {
        let tag_v1 = try!(id3v1::read(reader));

        let mut tag = ID3Tag::with_version(3);
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
            let mut frame = Frame::with_version(tag.year_id(), tag.version());
            frame.content = TextContent(tag_v1.year.unwrap());
            tag.add_frame(frame);
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

    /// Attempts to read an ID3v1 tag from the data as the specified path. The tag will be
    /// converted into an ID3v2.3 tag upon success.
    pub fn read_from_path_v1(path: &Path) -> TagResult<ID3Tag> {
        let mut file = try!(File::open(path));
        let mut tag = try!(ID3Tag::read_from_v1(&mut file));
        tag.path = Some(path.clone());
        tag.path_changed = false;
        Ok(tag)
    }

    /// Returns the version of the tag.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    ///
    /// let tag = ID3Tag::with_version(3);
    /// assert_eq!(tag.version(), 3);
    /// ```
    #[inline]
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
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::with_version(4);
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

        self.version = [version, 0];
        
        let mut remove_uuid = Vec::new();
        for frame in self.frames.iter_mut() {
            if !frame.set_version(version) {
                remove_uuid.push(frame.uuid.clone());
            }
        }

        self.modified_offset = 0;
            
        self.frames.retain(|frame: &Frame| !remove_uuid.contains(&frame.uuid));
    }

    /// Returns the default unicode encoding that should be used for this tag.
    ///
    /// For ID3 versions greater than v2.4 this returns UTF8. For versions less than v2.4 this
    /// returns UTF16.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::Encoding::{UTF16, UTF8};
    ///
    /// let mut tag_v3 = ID3Tag::with_version(3);
    /// assert_eq!(tag_v3.default_encoding(), UTF16);
    ///
    /// let mut tag_v4 = ID3Tag::with_version(4);
    /// assert_eq!(tag_v4.default_encoding(), UTF8);
    /// ```
    #[inline]
    pub fn default_encoding(&self) -> Encoding {
        if self.version[0] >= 4 {
            Encoding::UTF8
        } else {
            Encoding::UTF16
        }
    }

    /// Returns a vector of references to all frames in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_frame(Frame::new("TPE1"));
    /// tag.add_frame(Frame::new("APIC"));
    ///
    /// assert_eq!(tag.get_frames().len(), 2);
    /// ```
    #[inline]
    pub fn get_frames(&'a self) -> &'a Vec<Frame> {
        &self.frames
    }

    /// Returns a reference to the first frame with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_frame(Frame::new("TIT2"));
    ///
    /// assert!(tag.get_frame_by_id("TIT2").is_some());
    /// assert!(tag.get_frame_by_id("TCON").is_none());
    /// ```
    pub fn get_frame_by_id(&'a self, id: &str) -> Option<&'a Frame> {
        for frame in self.frames.iter() {
            if frame.id.as_slice() == id {
                return Some(frame);
            }
        }

        None
    }

    /// Returns a vector of references to frames with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_frame(Frame::new("TXXX"));
    /// tag.add_frame(Frame::new("TXXX"));
    /// tag.add_frame(Frame::new("TALB"));
    ///
    /// assert_eq!(tag.get_frames_by_id("TXXX").len(), 2);
    /// assert_eq!(tag.get_frames_by_id("TALB").len(), 1);
    /// ```
    pub fn get_frames_by_id(&'a self, id: &str) -> Vec<&'a Frame> {
        let mut matches = Vec::new();
        for frame in self.frames.iter() {
            if frame.id.as_slice() == id {
                matches.push(frame);
            }
        }

        matches
    }

    /// Adds a frame to the tag. The frame identifier will attempt to be converted into the
    /// corresponding identifier for the tag version.
    ///
    /// Returns whether the frame was added to the tag. The only reason the frame would not be
    /// added to the tag is if the frame identifier could not be converted from the frame version
    /// to the tag version.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.add_frame(Frame::new("TALB"));
    /// assert_eq!(tag.get_frames()[0].id.as_slice(), "TALB");
    /// ```
    pub fn add_frame(&mut self, mut frame: Frame) -> bool {
        frame.generate_uuid();
        frame.offset = 0;
        if !frame.set_version(self.version[0]) {
            return false;
        }
        self.frames.push(frame);
        true
    }

    /// Adds a text frame using the default text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.add_text_frame("TCON", "Metal");
    /// assert_eq!(tag.get_frame_by_id("TCON").unwrap().content.text().as_slice(), "Metal");
    /// ```
    #[inline]
    pub fn add_text_frame<K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, id: K, text: V) {
        let encoding = self.default_encoding();
        self.add_text_frame_enc(id, text, encoding);
    }

    /// Adds a text frame using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.add_text_frame_enc("TRCK", "1/13", UTF16);
    /// assert_eq!(tag.get_frame_by_id("TRCK").unwrap().content.text().as_slice(), "1/13");
    /// ```
    pub fn add_text_frame_enc<K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, id: K, text: V, encoding: Encoding) {
        let id = id.into_cow().into_owned();

        self.remove_frames_by_id(id.as_slice());
       
        let mut frame = Frame::with_version(id, self.version[0]);
        frame.set_encoding(encoding);
        frame.content = TextContent(text.into_cow().into_owned());

        self.frames.push(frame);
    }

    /// Removes the frame with the specified uuid.
    /// 
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_frame(Frame::new("TPE2"));
    /// assert_eq!(tag.get_frames().len(), 1);
    ///
    /// let uuid = tag.get_frames()[0].uuid.clone();
    /// tag.remove_frame_by_uuid(uuid.as_slice());
    /// assert_eq!(tag.get_frames().len(), 0);
    /// ```
    pub fn remove_frame_by_uuid(&mut self, uuid: &[u8]) {
        let mut modified_offset = self.modified_offset;
        {
            let mut set_modified_offset = |&mut: offset: u32| {
                if offset != 0 {
                    modified_offset = min(modified_offset, offset);
                }
                false
            };
            self.frames.retain(|frame| {
                frame.uuid.as_slice() != uuid || set_modified_offset(frame.offset)
            });
        }
        self.modified_offset = modified_offset;
    }

    /// Removes all frames with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_frame(Frame::new("TXXX"));
    /// tag.add_frame(Frame::new("TXXX"));
    /// tag.add_frame(Frame::new("USLT"));
    ///
    /// assert_eq!(tag.get_frames().len(), 3);
    ///
    /// tag.remove_frames_by_id("TXXX");
    /// assert_eq!(tag.get_frames().len(), 1);
    ///
    /// tag.remove_frames_by_id("USLT");
    /// assert_eq!(tag.get_frames().len(), 0);
    /// ```   
    pub fn remove_frames_by_id(&mut self, id: &str) {
        let mut modified_offset = self.modified_offset;
        {
            let mut set_modified_offset = |&mut: offset: u32| {
                if offset != 0 {
                    modified_offset = min(modified_offset, offset);
                }
                false
            };
            self.frames.retain(|frame| {
                frame.id.as_slice() != id || set_modified_offset(frame.offset)
            });
        }
        self.modified_offset = modified_offset;
    }

    /// Returns the `TextContent` string for the frame with the specified identifier.
    /// Returns `None` if the frame with the specified ID can't be found or if the content is not
    /// `TextContent`.
    fn text_for_frame_id(&self, id: &str) -> Option<String> {
        match self.get_frame_by_id(id) {
            Some(frame) => match frame.content {
                TextContent(ref text) => Some(text.clone()),
                _ => None
            },
            None => None
        }
    }

    // Getters/Setters {{{
    /// Returns a vector of the user defined text frames' (TXXX) key/value pairs.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    /// use id3::frame;
    /// use id3::Content::ExtendedTextContent;
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// let mut frame = Frame::new("TXXX");
    /// frame.content = ExtendedTextContent(frame::ExtendedText { 
    ///     key: "key1".to_string(),
    ///     value: "value1".to_string()
    /// });
    /// tag.add_frame(frame);
    ///
    /// let mut frame = Frame::new("TXXX");
    /// frame.content = ExtendedTextContent(frame::ExtendedText { 
    ///     key: "key2".to_string(),
    ///     value: "value2".to_string()
    /// }); 
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("key1".to_string(), "value1".to_string())));
    /// assert!(tag.txxx().contains(&("key2".to_string(), "value2".to_string())));
    /// ```
    pub fn txxx(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for frame in self.get_frames_by_id(self.txxx_id()).iter() {
            match frame.content {
                ExtendedTextContent(ref ext) => out.push((ext.key.clone(), ext.value.clone())),
                _ => { }
            }
        }

        out
    }

    /// Adds a user defined text frame (TXXX).
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_txxx("key1", "value1");
    /// tag.add_txxx("key2", "value2");
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("key1".to_string(), "value1".to_string())));
    /// assert!(tag.txxx().contains(&("key2".to_string(), "value2".to_string())));
    /// ```
    #[inline]
    pub fn add_txxx<K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, key: K, value: V) {
        let encoding = self.default_encoding();
        self.add_txxx_enc(key, value, encoding);
    }

    /// Adds a user defined text frame (TXXX) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_txxx_enc("key1", "value1", UTF16);
    /// tag.add_txxx_enc("key2", "value2", UTF16);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("key1".to_string(), "value1".to_string())));
    /// assert!(tag.txxx().contains(&("key2".to_string(), "value2".to_string())));
    /// ```
    pub fn add_txxx_enc<K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, key: K, value: V, encoding: Encoding) {
        let key = key.into_cow().into_owned();

        self.remove_txxx(Some(key.as_slice()), None);

        let mut frame = Frame::with_version(self.txxx_id(), self.version[0]);
        frame.set_encoding(encoding);
        frame.content = ExtendedTextContent(frame::ExtendedText { 
            key: key, 
            value: value.into_cow().into_owned()
        });
        
        self.frames.push(frame);
    }

    /// Removes the user defined text frame (TXXX) with the specified key and value.
    /// A key or value may be `None` to specify a wildcard value.
    /// 
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::new();
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

            if frame.id.as_slice() == id {
                match frame.content {
                    ExtendedTextContent(ref ext) => {
                        match key {
                            Some(s) => key_match = s == ext.key.as_slice(),
                            None => key_match = true
                        }

                        match value {
                            Some(s) => value_match = s == ext.value.as_slice(),
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
    /// use id3::{ID3Tag, Frame};
    /// use id3::frame::Picture;
    /// use id3::Content::PictureContent;
    ///
    /// let mut tag = ID3Tag::new();
    /// 
    /// let mut frame = Frame::new("APIC");
    /// frame.content = PictureContent(Picture::new());
    /// tag.add_frame(frame);
    ///
    /// let mut frame = Frame::new("APIC");
    /// frame.content = PictureContent(Picture::new());
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.pictures().len(), 2);
    /// ```
    pub fn pictures(&self) -> Vec<&Picture> {
        let mut pictures = Vec::new();
        for frame in self.get_frames_by_id(self.picture_id()).iter() {
            match frame.content {
                PictureContent(ref picture) => pictures.push(picture),
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
    /// use id3::ID3Tag;
    /// use id3::frame::PictureType::Other;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.add_picture("image/jpeg", Other, vec!());
    /// tag.add_picture("image/png", Other, vec!());
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(tag.pictures()[0].mime_type.as_slice(), "image/png");
    /// ```
    #[inline]
    pub fn add_picture<T: IntoCow<'a, String, str>>(&mut self, mime_type: T, picture_type: PictureType, data: Vec<u8>) {
        self.add_picture_enc(mime_type, picture_type, "", data, Encoding::Latin1);
    }

    /// Adds a picture frame (APIC) using the specified text encoding.
    /// Any other pictures with the same type will be removed from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::frame::PictureType::Other;
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.add_picture_enc("image/jpeg", Other, "", vec!(), UTF16);
    /// tag.add_picture_enc("image/png", Other, "", vec!(), UTF16);
    /// assert_eq!(tag.pictures().len(), 1);
    /// assert_eq!(tag.pictures()[0].mime_type.as_slice(), "image/png");
    /// ```
    pub fn add_picture_enc<S: IntoCow<'a, String, str>, T: IntoCow<'a, String, str>>(&mut self, mime_type: S, picture_type: PictureType, description: T, data: Vec<u8>, encoding: Encoding) {
        self.remove_picture_type(picture_type);

        let mut frame = Frame::with_version(self.picture_id(), self.version[0]);

        frame.set_encoding(encoding);
        frame.content = PictureContent(Picture { 
            mime_type: mime_type.into_cow().into_owned(), 
            picture_type: picture_type, 
            description: description.into_cow().into_owned(), 
            data: data 
        });

        self.frames.push(frame);
    }

    /// Removes all pictures of the specified type.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::frame::PictureType::{CoverFront, Other};
    ///
    /// let mut tag = ID3Tag::new();
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
            if frame.id.as_slice() == id {
                let pic = match frame.content {
                    PictureContent(ref picture) => picture,
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

    /// Returns a vector of the user comment frames' (COMM) key/value pairs.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    /// use id3::frame;
    /// use id3::Content::CommentContent;
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// let mut frame = Frame::new("COMM");
    /// frame.content = CommentContent(frame::Comment {
    ///     lang: "eng".to_string(),
    ///     description: "key1".to_string(),
    ///     text: "value1".to_string()
    /// });
    /// tag.add_frame(frame);
    ///
    /// let mut frame = Frame::new("COMM");
    /// frame.content = CommentContent(frame::Comment { 
    ///     lang: "eng".to_string(),
    ///     description: "key2".to_string(),
    ///     text: "value2".to_string()
    /// });
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1".to_string(), "value1".to_string())));
    /// assert!(tag.comments().contains(&("key2".to_string(), "value2".to_string())));
    /// ```
    pub fn comments(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for frame in self.get_frames_by_id(self.comment_id()).iter() {
            match frame.content {
                CommentContent(ref comment) => out.push((comment.description.clone(), 
                                                         comment.text.clone())),
                _ => { }
            }
        }

        out
    }
 
    /// Adds a user comment frame (COMM).
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_comment("key1", "value1");
    /// tag.add_comment("key2", "value2");
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1".to_string(), "value1".to_string())));
    /// assert!(tag.comments().contains(&("key2".to_string(), "value2".to_string())));
    /// ```
    #[inline]
    pub fn add_comment<K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, description: K, text: V) {
        let encoding = self.default_encoding();
        self.add_comment_enc("eng", description, text, encoding);
    }

    /// Adds a user comment frame (COMM) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    ///
    /// tag.add_comment_enc("eng", "key1", "value1", UTF16);
    /// tag.add_comment_enc("eng", "key2", "value2", UTF16);
    ///
    /// assert_eq!(tag.comments().len(), 2);
    /// assert!(tag.comments().contains(&("key1".to_string(), "value1".to_string())));
    /// assert!(tag.comments().contains(&("key2".to_string(), "value2".to_string())));
    /// ```
    pub fn add_comment_enc<L: IntoCow<'a, String, str>, K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, lang: L, description: K, text: V, encoding: Encoding) {
        let description = description.into_cow().into_owned();

        self.remove_comment(Some(description.as_slice()), None);

        let mut frame = Frame::with_version(self.comment_id(), self.version[0]);

        frame.set_encoding(encoding);
        frame.content = CommentContent(frame::Comment { 
            lang: lang.into_cow().into_owned(), 
            description: description, 
            text: text.into_cow().into_owned() 
        });
       
        self.frames.push(frame);
    }

    /// Removes the user comment frame (COMM) with the specified key and value.
    /// A key or value may be `None` to specify a wildcard value.
    /// 
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::new();
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

            if frame.id.as_slice() == id {
                match frame.content {
                    CommentContent(ref comment) =>  {
                        match description {
                            Some(s) => description_match = s == comment.description.as_slice(),
                            None => description_match = true
                        }

                        match text {
                            Some(s) => text_match = s == comment.text.as_slice(),
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

    /// Sets the artist (TPE1) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_artist_enc("artist", UTF16);
    /// assert_eq!(tag.artist().unwrap().as_slice(), "artist");
    /// ```
    #[inline]
    pub fn set_artist_enc<T: IntoCow<'a, String, str>>(&mut self, artist: T, encoding: Encoding) {
        let id = self.artist_id();
        self.add_text_frame_enc(id, artist, encoding);
    }

    /// Sets the album artist (TPE2) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_album_artist_enc("album artist", UTF16);
    /// assert_eq!(tag.album_artist().unwrap().as_slice(), "album artist");
    /// ```
    #[inline]
    pub fn set_album_artist_enc<T: IntoCow<'a, String, str>>(&mut self, album_artist: T, encoding: Encoding) {
        self.remove_frames_by_id("TSOP");
        let id = self.album_artist_id();
        self.add_text_frame_enc(id, album_artist, encoding);
    }

    /// Sets the album (TALB) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_album_enc("album", UTF16);
    /// assert_eq!(tag.album().unwrap().as_slice(), "album");
    /// ```
    #[inline]
    pub fn set_album_enc<T: IntoCow<'a, String, str>>(&mut self, album: T, encoding: Encoding) {
        let id = self.album_id();
        self.add_text_frame_enc(id, album, encoding);
    }

    /// Sets the song title (TIT2) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_title_enc("title", UTF16);
    /// assert_eq!(tag.title().unwrap().as_slice(), "title");
    /// ```
    #[inline]
    pub fn set_title_enc<T: IntoCow<'a, String, str>>(&mut self, title: T, encoding: Encoding) {
        self.remove_frames_by_id("TSOT");
        let id = self.title_id();
        self.add_text_frame_enc(id, title, encoding);
    }

    /// Sets the genre (TCON) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_genre_enc("genre", UTF16);
    /// assert_eq!(tag.genre().unwrap().as_slice(), "genre");
    /// ```
    #[inline]
    pub fn set_genre_enc<T: IntoCow<'a, String, str>>(&mut self, genre: T, encoding: Encoding) {
        let id = self.genre_id();
        self.add_text_frame_enc(id, genre, encoding);
    }

    /// Returns the year (TYER).
    /// Returns `None` if the year frame could not be found or if it could not be parsed.
    ///
    /// # Example
    /// ```
    /// use id3::{ID3Tag, Frame};
    /// use id3::Content::TextContent;
    ///
    /// let mut tag = ID3Tag::new();
    /// assert!(tag.year().is_none());
    ///
    /// let mut frame_valid = Frame::new("TYER");
    /// frame_valid.content = TextContent("2014".to_string());
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.year().unwrap(), 2014);
    ///
    /// tag.remove_frames_by_id("TYER");
    ///
    /// let mut frame_invalid = Frame::new("TYER");
    /// frame_invalid.content = TextContent("nope".to_string());
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.year().is_none());
    /// ```
    pub fn year(&self) -> Option<usize> {
        let id = self.year_id();
        match self.get_frame_by_id(id) {
            Some(frame) => {
                match frame.content {
                    TextContent(ref text) => text.as_slice().parse::<usize>().ok(),
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
    /// use id3::ID3Tag;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_year(2014);
    /// assert_eq!(tag.year().unwrap(), 2014);
    /// ```
    #[inline]
    pub fn set_year(&mut self, year: usize) {
        let id = self.year_id();
        self.add_text_frame_enc(id, format!("{}", year), Encoding::Latin1);
    }

    /// Sets the year (TYER) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::ID3Tag;
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_year_enc(2014, UTF16);
    /// assert_eq!(tag.year().unwrap(), 2014);
    /// ```
    #[inline]
    pub fn set_year_enc(&mut self, year: usize, encoding: Encoding) {
        let id = self.year_id();
        self.add_text_frame_enc(id, format!("{}", year), encoding);
    }

    /// Returns the (track, total_tracks) tuple.
    fn track_pair(&self) -> Option<(u32, Option<u32>)> {
        match self.get_frame_by_id(self.track_id()) {
            Some(frame) => {
                match frame.content {
                    TextContent(ref text) => {
                        let split: Vec<&str> = text.as_slice().splitn(2, '/').collect();

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

    /// Sets the track number (TRCK) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
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


    /// Sets the total number of tracks (TRCK) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
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


    /// Sets the lyrics text (USLT) using the specified text encoding.
    ///
    /// # Example
    /// ```
    /// use id3::{AudioTag, ID3Tag};
    /// use id3::Encoding::UTF16;
    ///
    /// let mut tag = ID3Tag::new();
    /// tag.set_lyrics_enc("eng", "description", "lyrics", UTF16);
    /// assert_eq!(tag.lyrics().unwrap().as_slice(), "lyrics");
    /// ```
    pub fn set_lyrics_enc<L: IntoCow<'a, String, str>, K: IntoCow<'a, String, str>, V: IntoCow<'a, String, str>>(&mut self, lang: L, description: K, text: V, encoding: Encoding) {
        let id = self.lyrics_id();
        self.remove_frames_by_id(id);

        let mut frame = Frame::with_version(id, self.version[0]);

        frame.set_encoding(encoding);
        frame.content = LyricsContent(frame::Lyrics { 
            lang: lang.into_cow().into_owned(), 
            description: description.into_cow().into_owned(), 
            text: text.into_cow().into_owned() 
        });
        
        self.frames.push(frame);
    }
    //}}}
}
impl<'a> AudioTag<'a> for ID3Tag {
    // Reading/Writing {{{
    fn skip_metadata<R: Reader + Seek>(reader: &mut R, _: Option<ID3Tag>) -> Vec<u8> {
        macro_rules! try_io {
            ($reader:ident, $action:expr) => {
                match $action { 
                    Ok(bytes) => bytes, 
                    Err(_) => {
                        match $reader.seek(0, SeekSet) {
                            Ok(_) => {
                                match $reader.read_to_end() {
                                    Ok(bytes) => return bytes,
                                    Err(_) => return Vec::new()
                                }
                            },
                            Err(_) => return Vec::new()
                        }
                    }
                }
            }
        }

        let ident = try_io!(reader, reader.read_exact(3));
        if ident.as_slice() == b"ID3" {
            try_io!(reader, reader.seek(3, SeekCur));
            let offset = 10 + util::unsynchsafe(try_io!(reader, reader.read_be_u32()));   
            try_io!(reader, reader.seek(offset as i64, SeekSet));
        } else {
            try_io!(reader, reader.seek(0, SeekSet));
        }

        try_io!(reader, reader.read_to_end())
    }

    fn is_candidate(reader: &mut Reader, _: Option<ID3Tag>) -> bool {
        macro_rules! try_or_false {
            ($action:expr) => {
                match $action { 
                    Ok(result) => result, 
                    Err(_) => return false 
                }
            }
        }

        (try_or_false!(reader.read_exact(3))).as_slice() == b"ID3"
    }

    fn read_from(reader: &mut Reader) -> TagResult<ID3Tag> {
        let mut tag = ID3Tag::new();

        let identifier = try!(reader.read_exact(3));
        if identifier.as_slice() != b"ID3" {
            debug!("no id3 tag found");
            return Err(TagError::new(InvalidInputError, "buffer does not contain an id3 tag"))
        }

        try!(reader.read(&mut tag.version));

        debug!("tag version {}", tag.version[0]);

        if tag.version[0] < 2 || tag.version[0] > 4 {
            return Err(TagError::new(InvalidInputError, "unsupported id3 tag version"));
        }

        tag.flags = TagFlags::from_byte(try!(reader.read_byte()), tag.version[0]);

        if tag.flags.unsynchronization {
            debug!("unsynchronization is unsupported");
            return Err(TagError::new(UnsupportedFeatureError, "unsynchronization is not supported"))
        } else if tag.flags.compression {
            debug!("id3v2.2 compression is unsupported");
            return Err(TagError::new(UnsupportedFeatureError, "id3v2.2 compression is not supported"));
        }

        tag.size = util::unsynchsafe(try!(reader.read_be_u32()));
        
        let mut offset = 10;

        // TODO actually use the extended header data
        if tag.flags.extended_header {
            let ext_size = util::unsynchsafe(try!(reader.read_be_u32()));
            offset += 4;
            let _ = try!(reader.read_exact(ext_size as usize));
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

    fn write_to(&mut self, writer: &mut Writer) -> TagResult<()> {
        let path_changed = self.path_changed;
        
        // remove frames which have the flags indicating they should be removed 
        self.frames.retain(|frame| {
            !(frame.offset != 0 
              && (frame.tag_alter_preservation() 
                  || (path_changed 
                      && (frame.file_alter_preservation() 
                          || DEFAULT_FILE_DISCARD.contains(&frame.id.as_slice())))))
        });
            
        let mut data_cache: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        let mut size = 0;

        for frame in self.frames.iter() {
            let mut frame_writer = Vec::new();
            size += try!(frame.write_to(&mut frame_writer));
            data_cache.insert(frame.uuid.clone(), frame_writer);
        }

        self.size = size + PADDING_BYTES;

        try!(writer.write_all(b"ID3"));
        try!(writer.write_all(&mut self.version)); 
        try!(writer.write_u8(self.flags.to_byte(self.version[0])));
        try!(writer.write_be_u32(util::synchsafe(self.size)));

        let mut bytes_written = 10;

        for frame in self.frames.iter_mut() {
            debug!("writing {}", frame.id);

            frame.offset = bytes_written;

            bytes_written += match data_cache.get(&frame.uuid) {
                Some(data) => { 
                    try!(writer.write_all(&data[]));
                    data.len() as u32
                },
                None => try!(frame.write_to(writer))
            }
        }

        self.offset = bytes_written;
        self.modified_offset = self.offset;

        // write padding
        for _ in range(0, PADDING_BYTES) {
            try!(writer.write_u8(0));
        }

        Ok(())
    }

    fn read_from_path(path: &Path) -> TagResult<ID3Tag> {
        let mut file = try!(File::open(path));
        let mut tag: ID3Tag = try!(AudioTag::read_from(&mut file));
        tag.path = Some(path.clone());
        tag.path_changed = false;
        Ok(tag)
    }

    fn write_to_path(&mut self, path: &Path) -> TagResult<()> {
        let data_opt = {
            match File::open(path) {
                Ok(mut file) => {
                    // remove the ID3v1 tag if the remove_v1 flag is set
                    let remove_bytes = if self.remove_v1 {
                        if try!(id3v1::probe_xtag(&mut file)) {
                            Some(id3v1::TAGPLUS_OFFSET as usize)
                        } else if try!(id3v1::probe_tag(&mut file)) {
                            Some(id3v1::TAG_OFFSET as usize)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let mut data = AudioTag::skip_metadata(&mut file, None::<ID3Tag>);
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

        self.path_changed = self.path.is_none() || self.path.as_ref().unwrap() != path;

        let mut file = try!(File::open_mode(path, Truncate, Write));
        self.write_to(&mut file).unwrap();
        
        match data_opt {
            Some(data) => file.write_all(&data[]).unwrap(),
            None => {}
        }

        self.path = Some(path.clone());
        self.path_changed = false;

        Ok(())
    }

    fn save(&mut self) -> TagResult<()> {
        if self.path.is_none() {
            panic!("attempted to save file which was not read from a path");
        }

        // remove any old frames that have the tag_alter_presevation flag
        let mut modified_offset = self.modified_offset;
        {
            let mut set_modified_offset = |&mut: offset: u32| {
                if offset != 0 {
                    modified_offset = min(modified_offset, offset);
                }
                false
            };       
            self.frames.retain(|frame| {
                frame.offset == 0 || !frame.tag_alter_preservation() 
                    || set_modified_offset(frame.offset)
            });
        }
        self.modified_offset = modified_offset;

        let mut data_cache: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        let mut size = 0;

        for frame in self.frames.iter() {
            let mut frame_writer = Vec::new();
            size += try!(frame.write_to(&mut frame_writer));
            data_cache.insert(frame.uuid.clone(), frame_writer);
        }

        debug!("modified offset: {}", self.modified_offset); 
       
        if size <= self.size && self.modified_offset >= 10 {
            debug!("writing using padding");

            let mut writer = try!(File::open_mode(self.path.as_ref().unwrap(), Open, Write));

            let mut offset = self.modified_offset;
            try!(writer.seek(offset as i64, SeekSet));

            for frame in self.frames.iter_mut() {
                if frame.offset == 0 || frame.offset > self.modified_offset {
                    debug!("writing {}", frame.id);
                    frame.offset = offset;
                    offset += match data_cache.get(&frame.uuid) {
                        Some(data) => { 
                            try!(writer.write_all(&data[]));
                            data.len() as u32
                        },
                        None => try!(frame.write_to(&mut writer))
                    }
                }
            }

            if self.offset > offset {
                for _ in range(offset, self.offset) {
                    try!(writer.write_u8(0));
                }
            }

            self.offset = offset;
            self.modified_offset = self.offset;

            Ok(())
        } else {
            debug!("rewriting file");
            let path = self.path.clone().unwrap();
            self.write_to_path(&path)
        }
    }
    //}}}
    
    #[inline]
    fn artist(&self) -> Option<String> {
        self.text_for_frame_id(self.artist_id())
    }

    #[inline]
    fn set_artist<T: IntoCow<'a, String, str>>(&mut self, artist: T) {
        let encoding = self.default_encoding();
        self.set_artist_enc(artist, encoding);
    }

    #[inline]
    fn remove_artist(&mut self) {
        let id = self.artist_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn album_artist(&self) -> Option<String> {
        self.text_for_frame_id(self.album_artist_id())
    }

    #[inline]
    fn set_album_artist<T: IntoCow<'a, String, str>>(&mut self, album_artist: T) {
        let encoding = self.default_encoding();
        self.set_album_artist_enc(album_artist, encoding);
    }

    #[inline]
    fn remove_album_artist(&mut self) {
        let id = self.album_artist_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn album(&self) -> Option<String> {
        self.text_for_frame_id(self.album_id())
    }

    fn set_album<T: IntoCow<'a, String, str>>(&mut self, album: T) {
        let encoding = self.default_encoding();
        self.set_album_enc(album, encoding);
    }

    #[inline]
    fn remove_album(&mut self) {
        self.remove_frames_by_id("TSOP");
        let id = self.album_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn title(&self) -> Option<String> {
        self.text_for_frame_id(self.title_id())
    }

    #[inline]
    fn set_title<T: IntoCow<'a, String, str>>(&mut self, title: T) {
        let encoding = self.default_encoding();
        self.set_title_enc(title, encoding);
    }

    #[inline]
    fn remove_title(&mut self) {
        let id = self.title_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn genre(&self) -> Option<String> {
        self.text_for_frame_id(self.genre_id())
    }

    #[inline]
    fn set_genre<T: IntoCow<'a, String, str>>(&mut self, genre: T) {
        let encoding = self.default_encoding();
        self.set_genre_enc(genre, encoding);
    }

    #[inline]
    fn remove_genre(&mut self) {
        let id = self.genre_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn track(&self) -> Option<u32> {
        self.track_pair().and_then(|(track, _)| Some(track))
    }

    #[inline]
    fn set_track(&mut self, track: u32) {
        self.set_track_enc(track, Encoding::Latin1);
    }

    #[inline]
    fn remove_track(&mut self) {
        let id = self.track_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn total_tracks(&self) -> Option<u32> {
        self.track_pair().and_then(|(_, total_tracks)| total_tracks)
    }

    #[inline]
    fn set_total_tracks(&mut self, total_tracks: u32) {
        self.set_total_tracks_enc(total_tracks, Encoding::Latin1);
    }

    fn remove_total_tracks(&mut self) {
        let id = self.track_id();
        match self.track_pair() {
            Some((track, _)) => self.add_text_frame(id, format!("{}", track)),
            None => {}
        }
    }

    fn lyrics(&self) -> Option<String> {
        match self.get_frame_by_id(self.lyrics_id()) {
            Some(frame) => match frame.content {
                LyricsContent(ref lyrics) => Some(lyrics.text.clone()),
                _ => None
            },
            None => None
        }
    }

    #[inline]
    fn set_lyrics<T: IntoCow<'a, String, str>>(&mut self, text: T) {
        let encoding = self.default_encoding();
        self.set_lyrics_enc("eng", text, "", encoding);
    }

    #[inline]
    fn remove_lyrics(&mut self) {
        let id = self.lyrics_id();
        self.remove_frames_by_id(id);
    }

    #[inline]
    fn set_picture<T: IntoCow<'a, String, str>>(&mut self, mime_type: T, data: Vec<u8>) {
        self.remove_picture();
        self.add_picture(mime_type, PictureType::Other, data);
    }

    #[inline]
    fn remove_picture(&mut self) {
        let id = self.picture_id();
        self.remove_frames_by_id(id);
    }

    fn all_metadata(&self) -> Vec<(String, String)> {
        let mut metadata = Vec::new();
        for frame in self.frames.iter() {
            match frame.text() {
                Some(text) => metadata.push((frame.id.clone(), text)),
                None => {}
            }
        }
        metadata
    }
}
// }}}

// Tests {{{
#[cfg(test)]
mod tests {
    use tag::TagFlags;

    #[test]
    fn test_flags_to_bytes() {
        let mut flags = TagFlags::new();
        assert_eq!(flags.to_byte(4), 0x0);
        flags.unsynchronization = true;
        flags.extended_header = true;
        flags.experimental = true;
        flags.footer = true;
        assert_eq!(flags.to_byte(4), 0xF0);
    }
}
// }}}
