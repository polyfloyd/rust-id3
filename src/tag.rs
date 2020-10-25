use crate::frame::Content;
use crate::frame::{
    Comment, ExtendedLink, ExtendedText, Frame, Lyrics, Picture, PictureType, SynchronisedLyrics,
    Timestamp,
};
use crate::storage::{self, PlainStorage, Storage};
use crate::stream;
use crate::v1;
use std::fs::{self, File};
use std::io::{self, BufReader, Write};
use std::iter::Iterator;
use std::path::Path;

/// Denotes the version of a tag.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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
    pub fn minor(self) -> u8 {
        match self {
            Id3v22 => 2,
            Id3v23 => 3,
            Id3v24 => 4,
        }
    }
}

/// An ID3 tag containing metadata frames.
#[derive(Clone, Debug, Default, Eq)]
pub struct Tag {
    /// A vector of frames included in the tag.
    frames: Vec<Frame>,
}

impl<'a> Tag {
    /// Creates a new ID3v2.4 tag with no frames.
    pub fn new() -> Tag {
        Tag::default()
    }

    /// Returns an iterator over the all frames in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame, Content};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::with_content("TPE1", Content::Text("".to_string())));
    /// tag.add_frame(Frame::with_content("APIC", Content::Text("".to_string())));
    ///
    /// assert_eq!(tag.frames().count(), 2);
    /// ```
    pub fn frames(&'a self) -> impl Iterator<Item = &'a Frame> + 'a {
        self.frames.iter()
    }

    /// Returns an iterator over the extended texts in the tag.
    pub fn extended_texts(&'a self) -> impl Iterator<Item = &'a ExtendedText> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().extended_text())
    }

    /// Returns an iterator over the extended links in the tag.
    pub fn extended_links(&'a self) -> impl Iterator<Item = &'a ExtendedLink> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().extended_link())
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
    pub fn comments(&'a self) -> impl Iterator<Item = &'a Comment> + 'a {
        self.frames().filter_map(|frame| frame.content().comment())
    }

    /// Returns an iterator over the lyrics frames in the tag.
    pub fn lyrics(&'a self) -> impl Iterator<Item = &'a Lyrics> + 'a {
        self.frames().filter_map(|frame| frame.content().lyrics())
    }

    /// Returns an iterator over the synchronised lyrics frames in the tag.
    pub fn synchronised_lyrics(&'a self) -> impl Iterator<Item = &'a SynchronisedLyrics> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().synchronised_lyrics())
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
    pub fn pictures(&'a self) -> impl Iterator<Item = &'a Picture> + 'a {
        self.frames().filter_map(|frame| frame.content().picture())
    }

    /// Returns a reference to the first frame with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame, Content};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::with_content("TIT2", Content::Text("".to_string())));
    ///
    /// assert!(tag.get("TIT2").is_some());
    /// assert!(tag.get("TCON").is_none());
    /// ```
    pub fn get(&self, id: impl AsRef<str>) -> Option<&Frame> {
        self.frames().find(|frame| frame.id() == id.as_ref())
    }

    /// Adds the frame to the tag, replacing and returning any conflicting frame.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame, Content};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::with_content("TALB", Content::Text("".to_string())));
    /// tag.add_frame(Frame::with_content("TALB", Content::Text("".to_string())));
    /// assert_eq!(tag.frames().nth(0).unwrap().id(), "TALB");
    /// ```
    pub fn add_frame(&mut self, new_frame: Frame) -> Option<Frame> {
        let removed = self
            .frames
            .iter()
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
    /// tag.set_text("TRCK", "1/13");
    /// assert_eq!(tag.get("TRCK").unwrap().content().text().unwrap(), "1/13");
    /// ```
    pub fn set_text(&mut self, id: impl AsRef<str>, text: impl Into<String>) {
        self.add_frame(Frame::with_content(id, Content::Text(text.into())));
    }

    /// Removes all frames with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, Frame, Content};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::with_content("TALB", Content::Text("".to_string())));
    /// tag.add_frame(Frame::with_content("TPE1", Content::Text("".to_string())));
    ///
    /// assert_eq!(tag.frames().count(), 2);
    ///
    /// tag.remove("TALB");
    /// assert_eq!(tag.frames().count(), 1);
    ///
    /// tag.remove("TPE1");
    /// assert_eq!(tag.frames().count(), 0);
    /// ```
    pub fn remove(&mut self, id: impl AsRef<str>) {
        self.frames.retain(|frame| frame.id() != id.as_ref());
    }

    /// Adds a user defined text frame (TXXX).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_extended_text("key1", "value1");
    /// tag.add_extended_text("key2", "value2");
    ///
    /// assert_eq!(tag.extended_texts().count(), 2);
    /// assert!(tag.extended_texts().any(|t| t.description == "key1" && t.value == "value1"));
    /// assert!(tag.extended_texts().any(|t| t.description == "key2" && t.value == "value2"));
    /// ```
    pub fn add_extended_text(&mut self, description: impl Into<String>, value: impl Into<String>) {
        let frame = Frame::with_content(
            "TXXX",
            Content::ExtendedText(ExtendedText {
                description: description.into(),
                value: value.into(),
            }),
        );
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
    /// tag.add_extended_text("key1", "value1");
    /// tag.add_extended_text("key2", "value2");
    /// tag.add_extended_text("key3", "value2");
    /// tag.add_extended_text("key4", "value3");
    /// tag.add_extended_text("key5", "value4");
    /// assert_eq!(tag.extended_texts().count(), 5);
    ///
    /// tag.remove_extended_text(Some("key1"), None);
    /// assert_eq!(tag.extended_texts().count(), 4);
    ///
    /// tag.remove_extended_text(None, Some("value2"));
    /// assert_eq!(tag.extended_texts().count(), 2);
    ///
    /// tag.remove_extended_text(Some("key4"), Some("value3"));
    /// assert_eq!(tag.extended_texts().count(), 1);
    ///
    /// tag.remove_extended_text(None, None);
    /// assert_eq!(tag.extended_texts().count(), 0);
    /// ```
    pub fn remove_extended_text(&mut self, description: Option<&str>, value: Option<&str>) {
        self.frames.retain(|frame| {
            if frame.id() == "TXXX" {
                match *frame.content() {
                    Content::ExtendedText(ref ext) => {
                        let descr_match = description.map(|v| v == ext.description).unwrap_or(true);
                        let value_match = value.map(|v| v == ext.value).unwrap_or(true);
                        // True if we want to keep the frame.
                        !(descr_match && value_match)
                    }
                    _ => {
                        // A TXXX frame must always have content of the ExtendedText type. Remove
                        // frames that do not fit this requirement.
                        false
                    }
                }
            } else {
                true
            }
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
                    _ => return false,
                };
                return pic.picture_type != picture_type;
            }

            true
        });
    }

    /// Removes all pictures.
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
    /// tag.remove_all_pictures();
    /// assert_eq!(tag.pictures().count(), 0);
    /// ```
    pub fn remove_all_pictures(&mut self) {
        self.frames.retain(|frame| frame.id() != "APIC");
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
            if frame.id() == "COMM" {
                match *frame.content() {
                    Content::Comment(ref com) => {
                        let descr_match = description.map(|v| v == com.description).unwrap_or(true);
                        let text_match = text.map(|v| v == com.text).unwrap_or(true);
                        // True if we want to keep the frame.
                        !(descr_match && text_match)
                    }
                    _ => {
                        // A COMM frame must always have content of the Comment type. Remove frames
                        // that do not fit this requirement.
                        false
                    }
                }
            } else {
                true
            }
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
            .and_then(|text| text.trim_start_matches('0').parse().ok())
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
    pub fn set_artist(&mut self, artist: impl Into<String>) {
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
    pub fn set_album_artist(&mut self, album_artist: impl Into<String>) {
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
    pub fn set_album(&mut self, album: impl Into<String>) {
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
    pub fn set_title(&mut self, title: impl Into<String>) {
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
        self.text_for_frame_id("TLEN").and_then(|t| t.parse().ok())
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
    pub fn set_genre(&mut self, genre: impl Into<String>) {
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
        self.disc_pair().map(|(disc, _)| disc)
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
        let text = match self
            .text_pair("TPOS")
            .and_then(|(_, total_discs)| total_discs)
        {
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
            None => format!("1/{}", total_discs),
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
        self.text_pair("TRCK").map(|(track, _)| track)
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
        let text = match self
            .text_pair("TRCK")
            .and_then(|(_, total_tracks)| total_tracks)
        {
            Some(n) => format!("{}/{}", track, n),
            None => format!("{}", track),
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
            None => format!("1/{}", total_tracks),
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
    /// tag.add_lyrics(Lyrics {
    ///     lang: "eng".to_string(),
    ///     description: "".to_string(),
    ///     text: "The lyrics".to_string(),
    /// });
    /// assert_eq!(tag.lyrics().nth(0).unwrap().text, "The lyrics");
    /// ```
    pub fn add_lyrics(&mut self, lyrics: Lyrics) {
        let frame = Frame::with_content("USLT", Content::Lyrics(lyrics));
        self.add_frame(frame);
    }

    /// Removes the lyrics text (USLT) from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::Lyrics;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_lyrics(Lyrics {
    ///     lang: "eng".to_string(),
    ///     description: "".to_string(),
    ///     text: "The lyrics".to_string(),
    /// });
    /// assert_eq!(1, tag.lyrics().count());
    /// tag.remove_all_lyrics();
    /// assert_eq!(0, tag.lyrics().count());
    /// ```
    pub fn remove_all_lyrics(&mut self) {
        self.remove("USLT");
    }

    /// Adds a synchronised lyrics frame (SYLT).
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::{SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_synchronised_lyrics(SynchronisedLyrics {
    ///     lang: "eng".to_string(),
    ///     timestamp_format: TimestampFormat::MS,
    ///     content_type: SynchronisedLyricsType::Lyrics,
    ///     content: vec![
    ///         (1000, "he".to_string()),
    ///         (1100, "llo".to_string()),
    ///         (1200, "world".to_string()),
    ///     ],
    /// });
    /// assert_eq!(1, tag.synchronised_lyrics().count());
    /// ```
    pub fn add_synchronised_lyrics(&mut self, lyrics: SynchronisedLyrics) {
        let frame = Frame::with_content("SYLT", Content::SynchronisedLyrics(lyrics));
        self.add_frame(frame);
    }

    /// Removes all synchronised lyrics (SYLT) frames from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::Tag;
    /// use id3::frame::{SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_synchronised_lyrics(SynchronisedLyrics {
    ///     lang: "eng".to_string(),
    ///     timestamp_format: TimestampFormat::MS,
    ///     content_type: SynchronisedLyricsType::Lyrics,
    ///     content: vec![
    ///         (1000, "he".to_string()),
    ///         (1100, "llo".to_string()),
    ///         (1200, "world".to_string()),
    ///     ],
    /// });
    /// assert_eq!(1, tag.synchronised_lyrics().count());
    /// tag.remove_all_synchronised_lyrics();
    /// assert_eq!(0, tag.synchronised_lyrics().count());
    /// ```
    pub fn remove_all_synchronised_lyrics(&mut self) {
        self.remove("SYLT");
    }

    /// Will return true if the reader is a candidate for an ID3 tag. The reader position will be
    /// reset back to the previous position before returning.
    pub fn is_candidate(mut reader: impl io::Read + io::Seek) -> crate::Result<bool> {
        let initial_position = reader.seek(io::SeekFrom::Current(0))?;
        let rs = storage::locate_id3v2(&mut reader);
        reader.seek(io::SeekFrom::Start(initial_position))?;
        Ok(rs?.is_some())
    }

    /// Detects the presense of an ID3v2 tag at the current position of the reader and skips it if
    /// it if found. Returns true if a tag was found.
    pub fn skip(mut reader: impl io::Read + io::Seek) -> crate::Result<bool> {
        let initial_position = reader.seek(io::SeekFrom::Current(0))?;
        let range = storage::locate_id3v2(&mut reader)?;
        let end = range.as_ref().map(|r| r.end).unwrap_or(0);
        reader.seek(io::SeekFrom::Start(initial_position + end))?;
        Ok(range.is_some())
    }

    /// Attempts to read an ID3 tag from the reader.
    pub fn read_from(reader: impl io::Read) -> crate::Result<Tag> {
        stream::tag::decode(reader)
    }

    /// Attempts to read an ID3 tag from the file at the indicated path.
    pub fn read_from_path(path: impl AsRef<Path>) -> crate::Result<Tag> {
        let file = BufReader::new(File::open(path)?);
        Tag::read_from(file)
    }

    /// Attempts to write the ID3 tag to the writer using the specified version.
    pub fn write_to(&self, writer: impl io::Write, version: Version) -> crate::Result<()> {
        stream::tag::Encoder::new()
            .version(version)
            .encode(self, writer)
    }

    /// Attempts to write the ID3 tag from the file at the indicated path. If the specified path is
    /// the same path which the tag was read from, then the tag will be written to the padding if
    /// possible.
    pub fn write_to_path(&self, path: impl AsRef<Path>, version: Version) -> crate::Result<()> {
        let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        #[allow(clippy::reversed_empty_ranges)]
        let location = storage::locate_id3v2(&mut file)?.unwrap_or(0..0); // Create a new tag if none could be located.

        let mut storage = PlainStorage::new(file, location);
        let mut w = storage.writer()?;
        self.write_to(&mut w, version)?;
        w.flush()?;
        Ok(())
    }

    /// Removes an ID3v2 tag from the specified file.
    ///
    /// Returns true if the file initially contained a tag.
    pub fn remove_from(mut file: &mut fs::File) -> crate::Result<bool> {
        let location = match storage::locate_id3v2(&mut file)? {
            Some(l) => l,
            None => return Ok(false),
        };
        // Open the ID3 region for writing with write nothing. With the padding set to zero, this
        // removes the region in its entirety.
        let mut storage = PlainStorage::with_padding(file, location, 0, Some(0));
        storage.writer()?.flush()?;
        Ok(true)
    }

    /// Returns the `Content::Text` string for the frame with the specified identifier.
    /// Returns `None` if the frame with the specified ID can't be found or if the content is not
    /// `Content::Text`.
    fn text_for_frame_id(&self, id: &str) -> Option<&str> {
        self.get(id).and_then(|frame| frame.content().text())
    }

    fn read_timestamp_frame(&self, id: &str) -> Option<Timestamp> {
        self.get(id)
            .and_then(|frame| frame.content().text())
            .and_then(|text| text.parse().ok())
    }

    /// Returns the (disc, total_discs) tuple.
    fn disc_pair(&self) -> Option<(u32, Option<u32>)> {
        self.text_pair("TPOS")
    }

    /// Loads a text frame by its ID and attempt to split it into two parts
    ///
    /// Internally used by track and disc getters and setters.
    fn text_pair(&self, id: &str) -> Option<(u32, Option<u32>)> {
        self.get(id)
            .and_then(|frame| frame.content().text())
            .and_then(|text| {
                let mut split = text.splitn(2, '/');
                if let Ok(num) = split.next().unwrap().parse() {
                    Some((num, split.next().and_then(|s| s.parse().ok())))
                } else {
                    None
                }
            })
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Tag) -> bool {
        self.frames.len() == other.frames.len()
            && self.frames().all(|frame| other.frames.contains(frame))
    }
}

impl From<v1::Tag> for Tag {
    fn from(tag_v1: v1::Tag) -> Tag {
        let mut tag = Tag::new();
        if let Some(genre) = tag_v1.genre() {
            tag.set_genre(genre.to_string());
        }
        if !tag_v1.title.is_empty() {
            tag.set_title(tag_v1.title);
        }
        if !tag_v1.artist.is_empty() {
            tag.set_artist(tag_v1.artist);
        }
        if !tag_v1.album.is_empty() {
            tag.set_album(tag_v1.album);
        }
        if !tag_v1.year.is_empty() {
            tag.set_text("TYER", tag_v1.year);
        }
        if !tag_v1.comment.is_empty() {
            tag.add_comment(Comment {
                lang: "eng".to_string(),
                description: "".to_string(),
                text: tag_v1.comment,
            });
        }
        if let Some(track) = tag_v1.track {
            tag.set_track(u32::from(track));
        }
        tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Seek;
    use tempfile::tempdir;

    #[test]
    fn remove_id3v2() {
        let tmp = tempdir().unwrap();
        let tmp_name = tmp.path().join("remove_id3v2_tag");
        {
            let mut tag_file = fs::File::create(&tmp_name).unwrap();
            let mut original = fs::File::open("testdata/id3v24.id3").unwrap();
            io::copy(&mut original, &mut tag_file).unwrap();
        }
        let mut tag_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&tmp_name)
            .unwrap();
        tag_file.seek(io::SeekFrom::Start(0)).unwrap();
        assert!(Tag::remove_from(&mut tag_file).unwrap());
        tag_file.seek(io::SeekFrom::Start(0)).unwrap();
        assert!(!Tag::remove_from(&mut tag_file).unwrap());
    }
}
