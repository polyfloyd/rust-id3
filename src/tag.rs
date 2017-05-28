use std::fs::{self, File};
use std::io::{self, Read, Write, Seek, SeekFrom, BufReader};
use std::iter;
use std::ops;
use std::path::Path;
use byteorder::{ByteOrder, BigEndian, ReadBytesExt};
use ::frame::Content;
use ::frame::{Frame, ExtendedText, ExtendedLink, Comment, Lyrics, Picture, PictureType, Timestamp};
use ::storage::{PlainStorage, Storage};
use ::stream::{self, unsynch};


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
    /// A vector of frames included in the tag.
    frames: Vec<Frame>,
}

// Tag {{{
impl<'a> Tag {
    /// Creates a new ID3v2.4 tag with no frames.
    pub fn new() -> Tag {
        Tag { frames: Vec::new() }
    }

    /// Creates a new ID3 tag with the specified version.
    #[deprecated(note = "Tags now use ID3v2.4 for internal storage")]
    pub fn with_version(_: Version) -> Tag {
        Tag { frames: Vec::new() }
    }

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
    #[deprecated(note = "Tags now use ID3v2.4 for internal storage")]
    pub fn version(&self) -> Version {
        Version::Id3v24
    }

    /// Sets the version of this tag.
    #[deprecated(note = "Tags now use ID3v2.4 for internal storage")]
    pub fn set_version(&mut self, _: Version) { }

    /// Returns an iterator over the all frames in the tag.
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
            .filter_map(|frame| frame.content().extended_text());
        Box::new(iter)
    }

    /// Returns an iterator over the extended links in the tag.
    pub fn extended_links(&'a self) -> Box<iter::Iterator<Item=&'a ExtendedLink> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content().extended_link());
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
            .filter_map(|frame| frame.content().comment());
        Box::new(iter)
    }

    /// Returns an iterator over the extended links in the tag.
    pub fn lyrics(&'a self) -> Box<iter::Iterator<Item=&'a Lyrics> + 'a> {
        let iter = self.frames.iter()
            .filter_map(|frame| frame.content().lyrics());
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
            .filter_map(|frame| frame.content().picture());
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
            .find(|frame| frame.id() == id)
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
            if frame.id() == id {
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
    /// assert_eq!(tag.frames().nth(0).unwrap().id(), "TALB");
    /// ```
    #[deprecated(note = "Use add_frame")]
    pub fn push(&mut self, new_frame: Frame) -> bool {
        self.add_frame(new_frame);
        true
    }

    /// Adds the frame to the tag, replacing and returning any conflicting frame.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::new("TALB"));
    /// tag.add_frame(Frame::new("TALB"));
    /// assert_eq!(tag.frames().nth(0).unwrap().id(), "TALB");
    /// ```
    pub fn add_frame(&mut self, new_frame: Frame) -> Option<Frame> {
        let removed = self.frames.iter()
            .position(|frame| *frame == new_frame)
            .map(|conflict_index| self.frames.remove(conflict_index));
        self.frames.push(new_frame);
        removed
    }

    /// Adds a text frame.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_text_frame("TRCK", "1/13");
    /// assert_eq!(tag.get("TRCK").unwrap().content().text().unwrap(), "1/13");
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
    /// assert_eq!(tag.get("TRCK").unwrap().content().text().unwrap(), "1/13");
    /// ```
    pub fn set_text<K: Into<String>, V: Into<String>>(&mut self, id: K, text: V) {
        self.add_frame(Frame::with_content(&id.into(), Content::Text(text.into())));
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
            frame.id() != id
        });
    }

    /// Returns the `Content::Text` string for the frame with the specified identifier.
    /// Returns `None` if the frame with the specified ID can't be found or if the content is not
    /// `Content::Text`.
    fn text_for_frame_id(&self, id: &str) -> Option<&str> {
        self.get(id)
            .and_then(|frame| frame.content().text())
    }

    fn read_timestamp_frame(&self, id: &str) -> Option<Timestamp> {
        self.get(id)
            .and_then(|frame| frame.content().text())
            .and_then(|text| text.parse().ok())
    }

    /// Loads a text frame by its ID and attempt to split it into two parts
    ///
    /// Internally used by track and disc getters and setters.
    fn text_pair(&self, id: &str) -> Option<(u32, Option<u32>)> {
        self.get(id)
            .and_then(|frame| frame.content().text())
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
    /// let frame = Frame::with_content("TXXX", Content::ExtendedText(frame::ExtendedText {
    ///     description: "description1".to_owned(),
    ///     value: "value1".to_owned()
    /// }));
    /// tag.add_frame(frame);
    ///
    /// let frame = Frame::with_content("TXXX", Content::ExtendedText(frame::ExtendedText {
    ///     description: "description2".to_owned(),
    ///     value: "value2".to_owned()
    /// }));
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.txxx().len(), 2);
    /// assert!(tag.txxx().contains(&("description1", "value1")));
    /// assert!(tag.txxx().contains(&("description2", "value2")));
    /// ```
    #[deprecated(note = "Use extended_texts()")]
    pub fn txxx(&self) -> Vec<(&str, &str)> {
        self.frames()
            .filter(|frame| frame.id() == "TXXX")
            .filter_map(|frame| frame.content().extended_text())
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
        let frame = Frame::with_content("TXXX", Content::ExtendedText(ExtendedText {
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
        self.frames.retain(|frame| {
            let mut description_match = false;
            let mut value_match = false;

            if frame.id() == "TXXX" {
                match *frame.content() {
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
        let frame = Frame::with_content("APIC", Content::Picture(picture));
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
        self.frames.retain(|frame| {
            if frame.id() == "APIC" {
                let pic = match *frame.content() {
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
        let frame = Frame::with_content("COMM", Content::Comment(comment));
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
        self.frames.retain(|frame| {
            let mut description_match = false;
            let mut text_match = false;

            if frame.id() == "COMM" {
                match *frame.content() {
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
    /// let frame_valid = Frame::with_content("TYER", Content::Text("2014".to_owned()));
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.year().unwrap(), 2014);
    ///
    /// tag.remove("TYER");
    ///
    /// let frame_invalid = Frame::with_content("TYER", Content::Text("nope".to_owned()));
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.year().is_none());
    /// ```
    pub fn year(&self) -> Option<i32> {
        self.get("TYER")
            .and_then(|frame| frame.content().text())
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
        self.set_text("TYER", format!("{:04}", year));
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
    /// let frame = Frame::with_content("TPE1", Content::Text("artist".to_owned()));
    /// tag.add_frame(frame);
    /// assert_eq!(tag.artist().unwrap(), "artist");
    /// ```
    pub fn artist(&self) -> Option<&str> {
        self.text_for_frame_id("TPE1")
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
        self.set_text("TPE1", artist);
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
        self.remove("TPE1");
    }

    /// Sets the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// let frame = Frame::with_content("TPE2", Content::Text("artist".to_owned()));
    /// tag.add_frame(frame);
    /// assert_eq!(tag.album_artist().unwrap(), "artist");
    /// ```
    pub fn album_artist(&self) -> Option<&str> {
        self.text_for_frame_id("TPE2")
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
        self.set_text("TPE2", album_artist);
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
        self.remove("TPE2");
    }

    /// Returns the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// let frame = Frame::with_content("TALB", Content::Text("album".to_owned()));
    /// tag.add_frame(frame);
    /// assert_eq!(tag.album().unwrap(), "album");
    /// ```
    pub fn album(&self) -> Option<&str> {
        self.text_for_frame_id("TALB")
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
        self.set_text("TALB", album);
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
        self.remove("TALB");
    }

    /// Returns the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// let frame = Frame::with_content("TIT2", Content::Text("title".to_owned()));
    /// tag.add_frame(frame);
    /// assert_eq!(tag.title().unwrap(), "title");
    /// ```
    pub fn title(&self) -> Option<&str> {
        self.text_for_frame_id("TIT2")
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
        self.set_text("TIT2", title);
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
        self.remove("TIT2");
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
    /// let frame = Frame::with_content("TLEN", Content::Text("350".to_owned()));
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
    /// let frame = Frame::with_content("TCON", Content::Text("genre".to_owned()));
    /// tag.add_frame(frame);
    /// assert_eq!(tag.genre().unwrap(), "genre");
    /// ```
    pub fn genre(&self) -> Option<&str> {
        self.text_for_frame_id("TCON")
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
        self.set_text("TCON", genre);
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
        self.remove("TCON");
    }

    /// Returns the (disc, total_discs) tuple.
    fn disc_pair(&self) -> Option<(u32, Option<u32>)> {
        self.text_pair("TPOS")
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
    /// let mut frame_valid = Frame::with_content("TPOS", Content::Text("4".to_owned()));
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.disc().unwrap(), 4);
    ///
    /// tag.remove("TPOS");
    ///
    /// let mut frame_invalid = Frame::with_content("TPOS", Content::Text("nope".to_owned()));
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
        let text = match self.text_pair("TPOS").and_then(|(_, total_discs)| total_discs) {
            Some(n) => format!("{}/{}", disc, n),
            None => format!("{}", disc),
        };
        self.set_text("TPOS", text);
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
        self.remove("TPOS");
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
    /// let frame_valid = Frame::with_content("TPOS", Content::Text("4/10".to_owned()));
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.total_discs().unwrap(), 10);
    ///
    /// tag.remove("TPOS");
    ///
    /// let frame_invalid = Frame::with_content("TPOS", Content::Text("4/nope".to_owned()));
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.total_discs().is_none());
    /// ```
    pub fn total_discs(&self) -> Option<u32> {
        self.text_pair("TPOS")
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
        let text = match self.text_pair("TPOS") {
            Some((disc, _)) => format!("{}/{}", disc, total_discs),
            None => format!("1/{}", total_discs)
        };
        self.set_text("TPOS", text);
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
        if let Some((disc, _)) = self.text_pair("TPOS") {
            self.set_text("TPOS", format!("{}", disc));
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
    /// let frame_valid = Frame::with_content("TRCK", Content::Text("4".to_owned()));
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.track().unwrap(), 4);
    ///
    /// tag.remove("TRCK");
    ///
    /// let frame_invalid = Frame::with_content("TRCK", Content::Text("nope".to_owned()));
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.track().is_none());
    /// ```
    pub fn track(&self) -> Option<u32> {
        self.text_pair("TRCK")
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
        let text = match self.text_pair("TRCK").and_then(|(_, total_tracks)| total_tracks) {
            Some(n) => format!("{}/{}", track, n),
            None => format!("{}", track)
        };
        self.set_text("TRCK", text);
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
        self.remove("TRCK");
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
    /// let frame_valid = Frame::with_content("TRCK", Content::Text("4/10".to_owned()));
    /// tag.add_frame(frame_valid);
    /// assert_eq!(tag.total_tracks().unwrap(), 10);
    ///
    /// tag.remove("TRCK");
    ///
    /// let frame_invalid = Frame::with_content("TRCK", Content::Text("4/nope".to_owned()));
    /// tag.add_frame(frame_invalid);
    /// assert!(tag.total_tracks().is_none());
    /// ```
    pub fn total_tracks(&self) -> Option<u32> {
        self.text_pair("TRCK")
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
        let text = match self.text_pair("TRCK") {
            Some((track, _)) => format!("{}/{}", track, total_tracks),
            None => format!("1/{}", total_tracks)
        };
        self.set_text("TRCK", text);
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
        if let Some((track, _)) = self.text_pair("TRCK") {
            self.set_text("TRCK", format!("{}", track));
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
    #[deprecated(note = "There can be more than one lyrics frame")]
    pub fn set_lyrics(&mut self, lyrics: Lyrics) {
        let frame = Frame::with_content("USLT", Content::Lyrics(lyrics));
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
    #[deprecated(note = "There can be more than one lyrics frame")]
    pub fn remove_lyrics(&mut self) {
        self.remove("USLT");
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
            let offset = 10 + unsynch::decode_u32(try_io!(reader, reader.read_u32::<BigEndian>()));
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
    pub fn read_from<R>(reader: &mut R) -> ::Result<Tag>
        where R: io::Read {
        stream::tag::decode(reader)
    }

    /// Attempts to read an ID3 tag from the file at the indicated path.
    pub fn read_from_path<P: AsRef<Path>>(path: P) -> ::Result<Tag> {
        let mut file = BufReader::new(File::open(&path)?);
        Tag::read_from(&mut file)
    }

    /// Attempts to write the ID3 tag to the writer using the specified version.
    pub fn write_to<W>(&self, writer: &mut W, version: Version) -> ::Result<()>
        where W: io::Write {
        stream::tag::EncoderBuilder::default()
            .version(version)
            .build()
            .unwrap()
            .encode(self, writer)
    }

    /// Attempts to write the ID3 tag from the file at the indicated path. If the specified path is
    /// the same path which the tag was read from, then the tag will be written to the padding if
    /// possible.
    pub fn write_to_path<P: AsRef<Path>>(&self, path: P, version: Version) -> ::Result<()> {
        let mut file = fs::File::open(path)?;
        let location = locate_id3v2(&mut file)?
            .unwrap_or(0..0); // Create a new tag if none could be located.

        let mut storage = PlainStorage::new(file, location);
        let mut w = storage.writer()?;
        self.write_to(&mut w, version)?;
        w.flush()?;
        Ok(())
    }
    //}}}
}

impl From<::v1::Tag> for Tag {
    fn from(tag_v1: ::v1::Tag) -> Tag {
        let mut tag = Tag::new();
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
            tag.set_text("TYER", tag_v1.year.clone());
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
        _ => return Err(::Error::new(::ErrorKind::UnsupportedVersion(header[4], header[3]) , "unsupported id3 tag version")),
    };

    let size = unsynch::decode_u32(BigEndian::read_u32(&header[6..10]));
    reader.seek(io::SeekFrom::Start(size as u64))?;
    let num_padding = reader.bytes()
        .take_while(|rs| rs.as_ref().map(|b| *b == 0x00).unwrap_or(false))
        .count();
    Ok(Some(0..size as u64 + num_padding as u64))
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_locate_id3v2() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let location = locate_id3v2(&mut file).unwrap();
        assert!(location.is_some());
    }
}
