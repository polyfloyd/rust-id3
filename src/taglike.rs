use crate::frame::Content;
use crate::frame::{
    Comment, EncapsulatedObject, ExtendedText, Frame, Lyrics, Picture, PictureType,
    SynchronisedLyrics, Timestamp,
};
use std::borrow::Cow;
use std::mem::swap;

/// TagLike is a trait that provides a set of useful default methods that make manipulation of tag
/// frames easier.
pub trait TagLike: private::Sealed {
    #[doc(hidden)]
    fn frames_vec(&self) -> &Vec<Frame>;
    #[doc(hidden)]
    fn frames_vec_mut(&mut self) -> &mut Vec<Frame>;

    /// Returns the `Content::Text` string for the frame with the specified identifier.
    /// Returns `None` if the frame with the specified ID can't be found or if the content is not
    /// `Content::Text`.
    #[doc(hidden)]
    fn text_for_frame_id(&self, id: &str) -> Option<&str> {
        self.get(id).and_then(|frame| frame.content().text())
    }

    /// Returns the (potential) multiple `Content::Text` strings for the frame with the specified identifier.
    /// Returns `None` if the frame with the specified ID can't be found or if the content is not
    /// `Content::Text`.
    #[doc(hidden)]
    fn text_values_for_frame_id(&self, id: &str) -> Option<Vec<&str>> {
        self.get(id)
            .and_then(|frame| frame.content().text_values())
            .map(Vec::from_iter)
    }

    #[doc(hidden)]
    fn read_timestamp_frame(&self, id: &str) -> Option<Timestamp> {
        self.get(id)
            .and_then(|frame| frame.content().text())
            .and_then(|text| text.parse().ok())
    }

    /// Returns the (disc, total_discs) tuple.
    #[doc(hidden)]
    fn disc_pair(&self) -> Option<(u32, Option<u32>)> {
        self.text_pair("TPOS")
    }

    /// Loads a text frame by its ID and attempt to split it into two parts
    ///
    /// Internally used by track and disc getters and setters.
    #[doc(hidden)]
    fn text_pair(&self, id: &str) -> Option<(u32, Option<u32>)> {
        // The '/' is the preferred character to separate these fields, but the ID3 spec states
        // that frames may separate multple values on zero bytes.
        // Therefore, we try to to split on both '/' and '\0'.
        let text = self.get(id)?.content().text()?;
        let mut split = text.splitn(2, &['\0', '/'][..]);
        let a = split.next()?.parse().ok()?;
        let b = split.next().and_then(|s| s.parse().ok());
        Some((a, b))
    }

    /// Returns a reference to the first frame with the specified identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Frame, Content};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::text("TIT2", "Hello"));
    ///
    /// assert!(tag.get("TIT2").is_some());
    /// assert!(tag.get("TCON").is_none());
    /// ```
    fn get(&self, id: impl AsRef<str>) -> Option<&Frame> {
        self.frames_vec()
            .iter()
            .find(|frame| frame.id() == id.as_ref())
    }

    /// Adds the frame to the tag, replacing and returning any conflicting frame.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Frame, Content};
    /// use id3::frame::ExtendedText;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///
    ///     tag.add_frame(Frame::text("TPE1", "Armin van Buuren"));
    ///     tag.add_frame(ExtendedText{
    ///         description: "hello".to_string(),
    ///         value: "world".to_string(),
    ///     });
    ///
    ///     let removed = tag.add_frame(Frame::text("TPE1", "John 00 Fleming"))
    ///         .ok_or("no such frame")?;
    ///     assert_eq!(removed.content().text(), Some("Armin van Buuren"));
    ///     Ok(())
    /// }
    /// ```
    fn add_frame(&mut self, new_frame: impl Into<Frame>) -> Option<Frame> {
        let new_frame = new_frame.into();
        let removed = self
            .frames_vec()
            .iter()
            .position(|frame| frame.compare(&new_frame))
            .map(|conflict_index| self.frames_vec_mut().remove(conflict_index));
        self.frames_vec_mut().push(new_frame);
        removed
    }

    /// Adds a text frame.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///     tag.set_text("TRCK", "1/13");
    ///     assert_eq!(tag.get("TRCK").ok_or("no such frame")?.content().text(), Some("1/13"));
    ///     Ok(())
    /// }
    /// ```
    fn set_text(&mut self, id: impl AsRef<str>, text: impl Into<String>) {
        self.add_frame(Frame::text(id, text));
    }

    // Adds a new text frame with multiple string values.
    //
    /// # Panics
    /// If any of the strings contain a null byte.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///     tag.set_text_values("TCON", ["Synthwave", "Cyber Punk", "Electronic"]);
    ///     let text = tag.get("TCON").ok_or("no such frame")?.content().text();
    ///     assert_eq!(text, Some("Synthwave\u{0}Cyber Punk\u{0}Electronic"));
    ///     Ok(())
    /// }
    /// ```
    fn set_text_values(
        &mut self,
        id: impl AsRef<str>,
        texts: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.add_frame(Frame::with_content(id, Content::new_text_values(texts)));
    }

    /// Remove all frames with the specified identifier and return them.
    ///
    /// # Example
    /// ```
    /// use id3::{Content, Frame, Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::text("TALB", ""));
    /// tag.add_frame(Frame::text("TPE1", ""));
    /// assert_eq!(tag.frames().count(), 2);
    ///
    /// let removed = tag.remove("TALB");
    /// assert_eq!(tag.frames().count(), 1);
    /// assert_eq!(removed.len(), 1);
    ///
    /// let removed = tag.remove("TPE1");
    /// assert_eq!(tag.frames().count(), 0);
    /// assert_eq!(removed.len(), 1);
    /// ```
    fn remove(&mut self, id: impl AsRef<str>) -> Vec<Frame> {
        let mut from = Vec::new();
        swap(&mut from, self.frames_vec_mut());
        let (keep, remove): (Vec<Frame>, Vec<Frame>) = from
            .into_iter()
            .partition(|frame| frame.id() != id.as_ref());
        *self.frames_vec_mut() = keep;
        remove
    }

    /// Returns the year (TYER).
    /// Returns `None` if the year frame could not be found or if it could not be parsed.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Frame};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.year().is_none());
    ///
    /// tag.add_frame(Frame::text("TYER", "2014"));
    /// assert_eq!(tag.year(), Some(2014));
    ///
    /// tag.remove("TYER");
    ///
    /// tag.add_frame(Frame::text("TYER", "nope"));
    /// assert!(tag.year().is_none());
    /// ```
    fn year(&self) -> Option<i32> {
        self.get("TYER")
            .and_then(|frame| frame.content().text())
            .and_then(|text| text.trim_start_matches('0').parse().ok())
    }

    /// Sets the year (TYER).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_year(2014);
    /// assert_eq!(tag.year(), Some(2014));
    /// ```
    fn set_year(&mut self, year: i32) {
        self.set_text("TYER", format!("{year:04}"));
    }

    /// Removes the year (TYER).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_year(2014);
    /// assert!(tag.year().is_some());
    ///
    /// tag.remove_year();
    /// assert!(tag.year().is_none());
    /// ```
    fn remove_year(&mut self) {
        self.remove("TYER");
    }

    /// Return the content of the TDRC frame, if any
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::Timestamp;
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_recorded(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_recorded().map(|t| t.year), Some(2014));
    /// ```
    fn date_recorded(&self) -> Option<Timestamp> {
        self.read_timestamp_frame("TDRC")
    }

    /// Sets the content of the TDRC frame
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_recorded(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_recorded().map(|t| t.year), Some(2014));
    /// ```
    fn set_date_recorded(&mut self, timestamp: Timestamp) {
        let time_string = timestamp.to_string();
        self.set_text("TDRC", time_string);
    }

    /// Remove the content of the TDRC frame
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_recorded(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert!(tag.date_recorded().is_some());
    ///
    /// tag.remove_date_recorded();
    /// assert!(tag.date_recorded().is_none());
    /// ```
    fn remove_date_recorded(&mut self) {
        self.remove("TDRC");
    }

    /// Return the content of the TDRL frame, if any
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_released().map(|t| t.year), Some(2014));
    /// ```
    fn date_released(&self) -> Option<Timestamp> {
        self.read_timestamp_frame("TDRL")
    }

    /// Sets the content of the TDRL frame
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.date_released().map(|t| t.year), Some(2014));
    /// ```
    fn set_date_released(&mut self, timestamp: Timestamp) {
        let time_string = timestamp.to_string();
        self.set_text("TDRL", time_string);
    }

    /// Remove the content of the TDRL frame
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert!(tag.date_released().is_some());
    ///
    /// tag.remove_date_released();
    /// assert!(tag.date_released().is_none());
    /// ```
    fn remove_date_released(&mut self) {
        self.remove("TDRL");
    }

    /// Return the content of the TDOR frame, if any
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_original_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.original_date_released().map(|t| t.year), Some(2014));
    /// ```
    fn original_date_released(&self) -> Option<Timestamp> {
        self.read_timestamp_frame("TDOR")
    }

    /// Sets the content of the TDOR frame
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_original_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert_eq!(tag.original_date_released().map(|t| t.year), Some(2014));
    /// ```
    fn set_original_date_released(&mut self, timestamp: Timestamp) {
        let time_string = timestamp.to_string();
        self.set_text("TDOR", time_string);
    }

    /// Remove the content of the TDOR frame
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike, Timestamp};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_original_date_released(Timestamp{ year: 2014, month: None, day: None, hour: None, minute: None, second: None });
    /// assert!(tag.original_date_released().is_some());
    ///
    /// tag.remove_original_date_released();
    /// assert!(tag.original_date_released().is_none());
    /// ```
    fn remove_original_date_released(&mut self) {
        self.remove("TDOR");
    }

    /// Returns the artist (TPE1).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::text("TPE1", "artist"));
    /// assert_eq!(tag.artist(), Some("artist"));
    /// ```
    fn artist(&self) -> Option<&str> {
        self.text_for_frame_id("TPE1")
    }

    /// Returns the (potential) multiple artists (TPE1).
    fn artists(&self) -> Option<Vec<&str>> {
        self.text_values_for_frame_id("TPE1")
    }

    /// Sets the artist (TPE1).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_artist("artist");
    /// assert_eq!(tag.artist(), Some("artist"));
    /// ```
    fn set_artist(&mut self, artist: impl Into<String>) {
        self.set_text("TPE1", artist);
    }

    /// Removes the artist (TPE1).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_artist("artist");
    /// assert!(tag.artist().is_some());
    ///
    /// tag.remove_artist();
    /// assert!(tag.artist().is_none());
    /// ```
    fn remove_artist(&mut self) {
        self.remove("TPE1");
    }

    /// Sets the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::text("TPE2", "artist"));
    /// assert_eq!(tag.album_artist(), Some("artist"));
    /// ```
    fn album_artist(&self) -> Option<&str> {
        self.text_for_frame_id("TPE2")
    }

    /// Sets the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album_artist("artist");
    /// assert_eq!(tag.album_artist(), Some("artist"));
    /// ```
    fn set_album_artist(&mut self, album_artist: impl Into<String>) {
        self.set_text("TPE2", album_artist);
    }

    /// Removes the album artist (TPE2).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album_artist("artist");
    /// assert!(tag.album_artist().is_some());
    ///
    /// tag.remove_album_artist();
    /// assert!(tag.album_artist().is_none());
    /// ```
    fn remove_album_artist(&mut self) {
        self.remove("TPE2");
    }

    /// Returns the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::text("TALB", "album"));
    /// assert_eq!(tag.album(), Some("album"));
    /// ```
    fn album(&self) -> Option<&str> {
        self.text_for_frame_id("TALB")
    }

    /// Sets the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album("album");
    /// assert_eq!(tag.album(), Some("album"));
    /// ```
    fn set_album(&mut self, album: impl Into<String>) {
        self.set_text("TALB", album);
    }

    /// Removes the album (TALB).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_album("album");
    /// assert!(tag.album().is_some());
    ///
    /// tag.remove_album();
    /// assert!(tag.album().is_none());
    /// ```
    fn remove_album(&mut self) {
        self.remove("TALB");
    }

    /// Returns the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::text("TIT2", "title"));
    /// assert_eq!(tag.title(), Some("title"));
    /// ```
    fn title(&self) -> Option<&str> {
        self.text_for_frame_id("TIT2")
    }

    /// Sets the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_title("title");
    /// assert_eq!(tag.title(), Some("title"));
    /// ```
    fn set_title(&mut self, title: impl Into<String>) {
        self.set_text("TIT2", title);
    }

    /// Removes the title (TIT2).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_title("title");
    /// assert!(tag.title().is_some());
    ///
    /// tag.remove_title();
    /// assert!(tag.title().is_none());
    /// ```
    fn remove_title(&mut self) {
        self.remove("TIT2");
    }

    /// Returns the duration (TLEN).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::text("TLEN", "350"));
    /// assert_eq!(tag.duration(), Some(350));
    /// ```
    fn duration(&self) -> Option<u32> {
        self.text_for_frame_id("TLEN").and_then(|t| t.parse().ok())
    }

    /// Sets the duration (TLEN).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_duration(350);
    /// assert_eq!(tag.duration(), Some(350));
    /// ```
    fn set_duration(&mut self, duration: u32) {
        self.set_text("TLEN", duration.to_string());
    }

    /// Removes the duration (TLEN).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_duration(350);
    /// assert!(tag.duration().is_some());
    ///
    /// tag.remove_duration();
    /// assert!(tag.duration().is_none());
    /// ```
    fn remove_duration(&mut self) {
        self.remove("TLEN");
    }

    /// Returns the plain genre (TCON) text.
    ///
    /// Please be aware that ID3v2 specifies that this frame is permitted to refer to a
    /// predetermined set of ID3v1 genres by index. To handle such frames, use `genre_parsed`
    /// instead.
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::text("TCON", "genre"));
    /// assert_eq!(tag.genre(), Some("genre"));
    /// tag.set_genre("(31)");
    /// assert_eq!(tag.genre(), Some("(31)"));
    /// ```
    fn genre(&self) -> Option<&str> {
        self.text_for_frame_id("TCON")
    }

    /// Returns the genre (TCON) with ID3v1 genre indices resolved.
    ///
    /// # Example
    /// ```
    /// use id3::frame::Content;
    /// use id3::{Frame, Tag, TagLike};
    /// use std::borrow::Cow;
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Frame::text("TCON", "genre"));
    /// assert_eq!(tag.genre_parsed(), Some(Cow::Borrowed("genre")));
    /// tag.set_genre("(31)");
    /// assert_eq!(tag.genre_parsed(), Some(Cow::Owned("Trance".to_string())));
    /// ```
    fn genre_parsed(&self) -> Option<Cow<str>> {
        let tcon = self.text_for_frame_id("TCON")?;
        Some(crate::tcon::Parser::parse_tcon(tcon))
    }

    /// Returns the (potential) multiple plain genres (TCON).
    fn genres(&self) -> Option<Vec<&str>> {
        self.text_values_for_frame_id("TCON")
    }

    /// Sets the plain genre (TCON).
    ///
    /// No attempt is made to interpret and convert ID3v1 indices.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_genre("genre");
    /// assert_eq!(tag.genre(), Some("genre"));
    /// ```
    fn set_genre(&mut self, genre: impl Into<String>) {
        self.set_text("TCON", genre);
    }

    /// Removes the genre (TCON).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_genre("genre");
    /// assert!(tag.genre().is_some());
    ///
    /// tag.remove_genre();
    /// assert!(tag.genre().is_none());
    /// ```
    fn remove_genre(&mut self) {
        self.remove("TCON");
    }

    /// Returns the disc number (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.disc().is_none());
    ///
    /// tag.add_frame(Frame::text("TPOS", "4"));
    /// assert_eq!(tag.disc(), Some(4));
    ///
    /// tag.remove("TPOS");
    ///
    /// tag.add_frame(Frame::text("TPOS", "nope"));
    /// assert!(tag.disc().is_none());
    /// ```
    fn disc(&self) -> Option<u32> {
        self.disc_pair().map(|(disc, _)| disc)
    }

    /// Sets the disc (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_disc(2);
    /// assert_eq!(tag.disc(), Some(2));
    /// ```
    fn set_disc(&mut self, disc: u32) {
        let text = match self
            .text_pair("TPOS")
            .and_then(|(_, total_discs)| total_discs)
        {
            Some(n) => format!("{disc}/{n}"),
            None => format!("{disc}"),
        };
        self.set_text("TPOS", text);
    }

    /// Removes the disc number (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_disc(3);
    /// assert!(tag.disc().is_some());
    ///
    /// tag.remove_disc();
    /// assert!(tag.disc().is_none());
    /// ```
    fn remove_disc(&mut self) {
        self.remove("TPOS");
    }

    /// Returns the total number of discs (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.disc().is_none());
    ///
    /// tag.add_frame(Frame::text("TPOS", "4/10"));
    /// assert_eq!(tag.total_discs(), Some(10));
    ///
    /// tag.remove("TPOS");
    ///
    /// tag.add_frame(Frame::text("TPOS", "4/nope"));
    /// assert!(tag.total_discs().is_none());
    /// ```
    fn total_discs(&self) -> Option<u32> {
        self.text_pair("TPOS")
            .and_then(|(_, total_discs)| total_discs)
    }

    /// Sets the total number of discs (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_discs(10);
    /// assert_eq!(tag.total_discs(), Some(10));
    /// ```
    fn set_total_discs(&mut self, total_discs: u32) {
        let text = match self.text_pair("TPOS") {
            Some((disc, _)) => format!("{disc}/{total_discs}"),
            None => format!("1/{total_discs}",),
        };
        self.set_text("TPOS", text);
    }

    /// Removes the total number of discs (TPOS).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_discs(10);
    /// assert!(tag.total_discs().is_some());
    ///
    /// tag.remove_total_discs();
    /// assert!(tag.total_discs().is_none());
    /// ```
    fn remove_total_discs(&mut self) {
        if let Some((disc, _)) = self.text_pair("TPOS") {
            self.set_text("TPOS", format!("{disc}"));
        }
    }

    /// Returns the track number (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.track().is_none());
    ///
    /// tag.add_frame(Frame::text("TRCK", "4"));
    /// assert_eq!(tag.track(), Some(4));
    ///
    /// tag.remove("TRCK");
    ///
    /// tag.add_frame(Frame::text("TRCK", "nope"));
    /// assert!(tag.track().is_none());
    /// ```
    fn track(&self) -> Option<u32> {
        self.text_pair("TRCK").map(|(track, _)| track)
    }

    /// Sets the track (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_track(10);
    /// assert_eq!(tag.track(), Some(10));
    /// ```
    fn set_track(&mut self, track: u32) {
        let text = match self
            .text_pair("TRCK")
            .and_then(|(_, total_tracks)| total_tracks)
        {
            Some(n) => format!("{track}/{n}"),
            None => format!("{track}"),
        };
        self.set_text("TRCK", text);
    }

    /// Removes the track number (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_track(10);
    /// assert!(tag.track().is_some());
    ///
    /// tag.remove_track();
    /// assert!(tag.track().is_none());
    /// ```
    fn remove_track(&mut self) {
        self.remove("TRCK");
    }

    /// Returns the total number of tracks (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::Content;
    ///
    /// let mut tag = Tag::new();
    /// assert!(tag.total_tracks().is_none());
    ///
    /// tag.add_frame(Frame::text("TRCK", "4/10"));
    /// assert_eq!(tag.total_tracks(), Some(10));
    ///
    /// tag.remove("TRCK");
    ///
    /// tag.add_frame(Frame::text("TRCK", "4/nope"));
    /// assert!(tag.total_tracks().is_none());
    /// ```
    fn total_tracks(&self) -> Option<u32> {
        self.text_pair("TRCK")
            .and_then(|(_, total_tracks)| total_tracks)
    }

    /// Sets the total number of tracks (TRCK).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_tracks(10);
    /// assert_eq!(tag.total_tracks(), Some(10));
    /// ```
    fn set_total_tracks(&mut self, total_tracks: u32) {
        let text = match self.text_pair("TRCK") {
            Some((track, _)) => format!("{track}/{total_tracks}"),
            None => format!("1/{total_tracks}"),
        };
        self.set_text("TRCK", text);
    }

    /// Removes the total number of tracks (TCON).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    /// tag.set_total_tracks(10);
    /// assert!(tag.total_tracks().is_some());
    ///
    /// tag.remove_total_tracks();
    /// assert!(tag.total_tracks().is_none());
    /// ```
    fn remove_total_tracks(&mut self) {
        if let Some((track, _)) = self.text_pair("TRCK") {
            self.set_text("TRCK", format!("{track}"));
        }
    }

    /// Adds a user defined text frame (TXXX).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
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
    #[deprecated(note = "Use add_frame(frame::ExtendedText{ .. })")]
    fn add_extended_text(&mut self, description: impl Into<String>, value: impl Into<String>) {
        self.add_frame(ExtendedText {
            description: description.into(),
            value: value.into(),
        });
    }

    /// Removes the user defined text frame (TXXX) with the specified key and value.
    ///
    /// A key or value may be `None` to specify a wildcard value.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
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
    fn remove_extended_text(&mut self, description: Option<&str>, value: Option<&str>) {
        self.frames_vec_mut().retain(|frame| {
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
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{Picture, PictureType};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///     tag.add_picture(Picture {
    ///         mime_type: "image/jpeg".to_string(),
    ///         picture_type: PictureType::Other,
    ///         description: "some image".to_string(),
    ///         data: vec![],
    ///     });
    ///     tag.add_picture(Picture {
    ///         mime_type: "image/png".to_string(),
    ///         picture_type: PictureType::Other,
    ///         description: "some other image".to_string(),
    ///         data: vec![],
    ///     });
    ///     assert_eq!(tag.pictures().count(), 1);
    ///     assert_eq!(&tag.pictures().nth(0).ok_or("no such picture")?.mime_type[..], "image/png");
    ///     Ok(())
    /// }
    /// ```
    #[deprecated(note = "Use add_frame(frame::Picture{ .. })")]
    fn add_picture(&mut self, picture: Picture) {
        self.add_frame(picture);
    }

    /// Removes all pictures of the specified type.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{Picture, PictureType};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///     tag.add_picture(Picture {
    ///         mime_type: "image/jpeg".to_string(),
    ///         picture_type: PictureType::Other,
    ///         description: "some image".to_string(),
    ///         data: vec![],
    ///     });
    ///     tag.add_picture(Picture {
    ///         mime_type: "image/png".to_string(),
    ///         picture_type: PictureType::CoverFront,
    ///         description: "some other image".to_string(),
    ///         data: vec![],
    ///     });
    ///
    ///     assert_eq!(tag.pictures().count(), 2);
    ///     tag.remove_picture_by_type(PictureType::CoverFront);
    ///     assert_eq!(tag.pictures().count(), 1);
    ///     assert_eq!(tag.pictures().nth(0).ok_or("no such picture")?.picture_type, PictureType::Other);
    ///     Ok(())
    /// }
    /// ```
    fn remove_picture_by_type(&mut self, picture_type: PictureType) {
        self.frames_vec_mut().retain(|frame| {
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
    /// use id3::{Tag, TagLike};
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
    fn remove_all_pictures(&mut self) {
        self.frames_vec_mut().retain(|frame| frame.id() != "APIC");
    }

    /// Adds a comment (COMM).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
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
    #[deprecated(note = "Use add_frame(frame::Comment{ .. })")]
    fn add_comment(&mut self, comment: Comment) {
        self.add_frame(comment);
    }

    /// Removes the comment (COMM) with the specified key and value.
    ///
    /// A key or value may be `None` to specify a wildcard value.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
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
    fn remove_comment(&mut self, description: Option<&str>, text: Option<&str>) {
        self.frames_vec_mut().retain(|frame| {
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

    /// Adds an encapsulated object frame (GEOB).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_encapsulated_object("key1", "application/octet-stream", "", &b"\x00\x01\xAB"[..]);
    /// tag.add_encapsulated_object("key2", "application/json", "foo.json", &b"{ \"value\" }"[..]);
    ///
    /// assert_eq!(tag.encapsulated_objects().count(), 2);
    /// assert!(tag.encapsulated_objects().any(|t| t.description == "key1" && t.mime_type == "application/octet-stream" && t.filename == "" && t.data == b"\x00\x01\xAB"));
    /// assert!(tag.encapsulated_objects().any(|t| t.description == "key2" && t.mime_type == "application/json" && t.filename == "foo.json" && t.data == b"{ \"value\" }"));
    /// ```
    #[deprecated(note = "Use add_frame(frame::EncapsulatedObject{ .. })")]
    fn add_encapsulated_object(
        &mut self,
        description: impl Into<String>,
        mime_type: impl Into<String>,
        filename: impl Into<String>,
        data: impl Into<Vec<u8>>,
    ) {
        self.add_frame(EncapsulatedObject {
            description: description.into(),
            mime_type: mime_type.into(),
            filename: filename.into(),
            data: data.into(),
        });
    }

    /// Removes the encapsulated object frame (GEOB) with the specified key, MIME type, filename
    /// and
    /// data.
    ///
    /// A key or value may be `None` to specify a wildcard value.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_encapsulated_object("key1", "application/octet-stream", "filename1", &b"value1"[..]);
    /// tag.add_encapsulated_object("key2", "text/plain", "filename2", &b"value2"[..]);
    /// tag.add_encapsulated_object("key3", "text/plain", "filename3", &b"value2"[..]);
    /// tag.add_encapsulated_object("key4", "application/octet-stream", "filename4", &b"value3"[..]);
    /// tag.add_encapsulated_object("key5", "application/octet-stream", "filename4", &b"value4"[..]);
    /// tag.add_encapsulated_object("key6", "application/octet-stream", "filename5", &b"value5"[..]);
    /// tag.add_encapsulated_object("key7", "application/octet-stream", "filename6", &b"value6"[..]);
    /// tag.add_encapsulated_object("key8", "application/octet-stream", "filename7", &b"value7"[..]);
    /// assert_eq!(tag.encapsulated_objects().count(), 8);
    ///
    /// tag.remove_encapsulated_object(Some("key1"), None, None, None);
    /// assert_eq!(tag.encapsulated_objects().count(), 7);
    ///
    /// tag.remove_encapsulated_object(None, Some("text/plain"), None, None);
    /// assert_eq!(tag.encapsulated_objects().count(), 5);
    ///
    /// tag.remove_encapsulated_object(None, None, Some("filename4"), None);
    /// assert_eq!(tag.encapsulated_objects().count(), 3);
    ///
    /// tag.remove_encapsulated_object(None, None, None, Some(&b"value5"[..]));
    /// assert_eq!(tag.encapsulated_objects().count(), 2);
    ///
    /// tag.remove_encapsulated_object(Some("key7"), None, Some("filename6"), None);
    /// assert_eq!(tag.encapsulated_objects().count(), 1);
    ///
    /// tag.remove_encapsulated_object(None, None, None, None);
    /// assert_eq!(tag.encapsulated_objects().count(), 0);
    /// ```
    fn remove_encapsulated_object(
        &mut self,
        description: Option<&str>,
        mime_type: Option<&str>,
        filename: Option<&str>,
        data: Option<&[u8]>,
    ) {
        self.frames_vec_mut().retain(|frame| {
            if frame.id() == "GEOB" {
                match *frame.content() {
                    Content::EncapsulatedObject(ref ext) => {
                        let descr_match = description.map(|v| v == ext.description).unwrap_or(true);
                        let mime_match = mime_type.map(|v| v == ext.mime_type).unwrap_or(true);
                        let filename_match = filename.map(|v| v == ext.filename).unwrap_or(true);
                        let data_match = data.map(|v| v == ext.data).unwrap_or(true);
                        // True if we want to keep the frame.
                        !(descr_match && mime_match && filename_match && data_match)
                    }
                    _ => {
                        // A GEOB frame must always have content of the EncapsulatedObject type.
                        // Remove frames that do not fit this requirement.
                        false
                    }
                }
            } else {
                true
            }
        });
    }

    /// Sets the lyrics (USLT).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::Lyrics;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///     tag.add_lyrics(Lyrics {
    ///         lang: "eng".to_string(),
    ///         description: "".to_string(),
    ///         text: "The lyrics".to_string(),
    ///     });
    ///     assert_eq!(tag.lyrics().nth(0).ok_or("no such lyrics")?.text, "The lyrics");
    ///     Ok(())
    /// }
    /// ```
    #[deprecated(note = "Use add_frame(frame::Lyrics{ .. })")]
    fn add_lyrics(&mut self, lyrics: Lyrics) {
        self.add_frame(lyrics);
    }

    /// Removes the lyrics text (USLT) from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
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
    fn remove_all_lyrics(&mut self) {
        self.remove("USLT");
    }

    /// Adds a synchronised lyrics frame (SYLT).
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_synchronised_lyrics(SynchronisedLyrics {
    ///     lang: "eng".to_string(),
    ///     timestamp_format: TimestampFormat::Ms,
    ///     content_type: SynchronisedLyricsType::Lyrics,
    ///     content: vec![
    ///         (1000, "he".to_string()),
    ///         (1100, "llo".to_string()),
    ///         (1200, "world".to_string()),
    ///     ],
    ///     description: "description".to_string()
    /// });
    /// assert_eq!(1, tag.synchronised_lyrics().count());
    /// ```
    #[deprecated(note = "Use add_frame(frame::SynchronisedLyrics{ .. })")]
    fn add_synchronised_lyrics(&mut self, lyrics: SynchronisedLyrics) {
        self.add_frame(lyrics);
    }

    /// Removes all synchronised lyrics (SYLT) frames from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_synchronised_lyrics(SynchronisedLyrics {
    ///     lang: "eng".to_string(),
    ///     timestamp_format: TimestampFormat::Ms,
    ///     content_type: SynchronisedLyricsType::Lyrics,
    ///     content: vec![
    ///         (1000, "he".to_string()),
    ///         (1100, "llo".to_string()),
    ///         (1200, "world".to_string()),
    ///     ],
    ///     description: "description".to_string()
    /// });
    /// assert_eq!(1, tag.synchronised_lyrics().count());
    /// tag.remove_all_synchronised_lyrics();
    /// assert_eq!(0, tag.synchronised_lyrics().count());
    /// ```
    fn remove_all_synchronised_lyrics(&mut self) {
        self.remove("SYLT");
    }

    /// /// Removes all chapters (CHAP) frames from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{Chapter, Content, Frame};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Chapter{
    ///     element_id: "01".to_string(),
    ///     start_time: 1000,
    ///     end_time: 2000,
    ///     start_offset: 0xff,
    ///     end_offset: 0xff,
    ///     frames: Vec::new(),
    /// });
    /// assert_eq!(1, tag.chapters().count());
    /// tag.remove_all_chapters();
    /// assert_eq!(0, tag.chapters().count());
    /// ```
    fn remove_all_chapters(&mut self) {
        self.remove("CHAP");
    }

    /// /// Removes all tables of contents (CTOC) frames from the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{Chapter, TableOfContents, Content, Frame};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Chapter{
    ///     element_id: "chap01".to_string(),
    ///     start_time: 1000,
    ///     end_time: 2000,
    ///     start_offset: 0xff,
    ///     end_offset: 0xff,
    ///     frames: Vec::new(),
    /// });
    /// tag.add_frame(TableOfContents{
    ///     element_id: "01".to_string(),
    ///     top_level: true,
    ///     ordered: true,
    ///     elements: vec!["chap01".to_string()],
    ///     frames: Vec::new(),
    /// });
    /// assert_eq!(1, tag.tables_of_contents().count());
    /// tag.remove_all_tables_of_contents();
    /// assert_eq!(0, tag.tables_of_contents().count());
    /// ```
    fn remove_all_tables_of_contents(&mut self) {
        self.remove("CTOC");
    }

    /// Removes all Unique File Identifiers with the specified owner_identifier.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{UniqueFileIdentifier};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut tag = Tag::new();
    ///     tag.add_frame(UniqueFileIdentifier {
    ///         owner_identifier: "https://example.com".to_string(),
    ///         identifier: "09FxXfNTQsCgzkPmCeFwlr".into(),
    ///     });
    ///     tag.add_frame(UniqueFileIdentifier {
    ///         owner_identifier: "http://www.id3.org/dummy/ufid.html".to_string(),
    ///         identifier: "7FZo5fMqyG5Ys1dm8F1FHa".into(),
    ///     });
    ///
    ///     assert_eq!(tag.unique_file_identifiers().count(), 2);
    ///     tag.remove_unique_file_identifier_by_owner_identifier("http://www.id3.org/dummy/ufid.html");
    ///     assert_eq!(tag.unique_file_identifiers().count(), 1);
    ///     assert_eq!(tag.unique_file_identifiers().nth(0).ok_or("no such ufid owner")?.owner_identifier, "https://example.com");
    ///     Ok(())
    /// }
    /// ```
    fn remove_unique_file_identifier_by_owner_identifier(&mut self, owner_identifier: &str) {
        self.frames_vec_mut().retain(|frame| {
            if frame.id() == "UFID" {
                let uf = match *frame.content() {
                    Content::UniqueFileIdentifier(ref unique_file_identifier) => {
                        unique_file_identifier
                    }
                    _ => return false,
                };
                return uf.owner_identifier != owner_identifier;
            }

            true
        });
    }

    /// Removes all unique file identifiers.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{UniqueFileIdentifier};
    ///
    /// let mut tag = Tag::new();
    ///     tag.add_frame(UniqueFileIdentifier {
    ///         owner_identifier: "https://example.com".to_string(),
    ///         identifier: "09FxXfNTQsCgzkPmCeFwlr".into(),
    ///     });
    ///     tag.add_frame(UniqueFileIdentifier {
    ///         owner_identifier: "http://www.id3.org/dummy/ufid.html".to_string(),
    ///         identifier: "7FZo5fMqyG5Ys1dm8F1FHa".into(),
    ///     });
    ///
    /// assert_eq!(tag.unique_file_identifiers().count(), 2);
    /// tag.remove_all_unique_file_identifiers();
    /// assert_eq!(tag.unique_file_identifiers().count(), 0);
    /// ```
    fn remove_all_unique_file_identifiers(&mut self) {
        self.frames_vec_mut().retain(|frame| frame.id() != "UFID");
    }
}

// https://rust-lang.github.io/api-guidelines/future-proofing.html#c-sealed
mod private {
    use crate::frame::Chapter;
    use crate::frame::TableOfContents;
    use crate::tag::Tag;

    pub trait Sealed {}

    impl Sealed for Tag {}
    impl Sealed for Chapter {}
    impl Sealed for TableOfContents {}
}
