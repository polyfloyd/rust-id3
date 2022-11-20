use crate::frame::Frame;
use crate::stream::encoding::Encoding;
use crate::tag::Version;
use crate::taglike::TagLike;
use std::borrow::Cow;
use std::fmt;
use std::io;

/// The decoded contents of a [`Frame`].
///
/// # Compatibility
///
/// It is important to note that the ID3 spec has a variety of extensions of which not all are
/// implemented by this library. When a new frame content type is added, the signature of this enum
/// changes. Hence, the non_exhaustive attribute is set.
///
/// However, when a new frame type variant is added, frames that would previously decode to
/// [`Unknown`] are now decoded to their new variants. This would break user code, such as custom
/// decoders, that was expecting [`Unknown`].
///
/// In order to prevent breakage when this library adds a new frame type, users must use the
/// [`Content::to_unknown`] method which will return an [`Unknown`] regardlesss of whether the
/// frame content was successfully decoded.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[non_exhaustive]
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
    /// Popularimeter frame content (POPM).
    Popularimeter(Popularimeter),
    /// A value containing the parsed contents of a lyrics frame (USLT).
    Lyrics(Lyrics),
    /// A value containing the parsed contents of a synchronised lyrics frame (SYLT).
    SynchronisedLyrics(SynchronisedLyrics),
    /// A value containing the parsed contents of a picture frame (APIC).
    Picture(Picture),
    /// A value containing the parsed contents of a general encapsulated object frame (GEOB).
    EncapsulatedObject(EncapsulatedObject),
    /// A chapter object containing frames by itself (CHAP).
    Chapter(Chapter),
    /// MPEG location lookup table content (MLLT).
    MpegLocationLookupTable(MpegLocationLookupTable),
    /// A value containing the bytes of a currently unknown frame type.
    ///
    /// Users that wish to write custom decoders must use [`Content::to_unknown`] instead of
    /// matching on this variant. See the compatibility note in the top level enum docs.
    Unknown(Unknown),
}

impl Content {
    pub(crate) fn unique(&self, deeper: bool) -> impl Eq + '_ {
        if deeper {
            match self {
                Self::Text(text) => vec![Cow::Borrowed(text.as_bytes())],
                Self::ExtendedText(extended_text) => vec![Cow::Borrowed(extended_text.description.as_bytes())],
                Self::Link(text) => vec![Cow::Borrowed(text.as_bytes())],
                Self::ExtendedLink(extended_link) => vec![Cow::Borrowed(extended_link.description.as_bytes())],
                Self::Popularimeter(popularimeter) => vec![Cow::Borrowed(popularimeter.user.as_bytes())],
                Self::Comment(comment) => vec![
                    Cow::Borrowed(comment.lang.as_bytes()),
                    Cow::Borrowed(comment.description.as_bytes()),
                ],
                Self::Lyrics(lyrics) => vec![
                    Cow::Borrowed(lyrics.lang.as_bytes()),
                    Cow::Borrowed(lyrics.description.as_bytes()),
                ],
                Self::SynchronisedLyrics(synchronised_lyrics) => vec![
                    Cow::Borrowed(synchronised_lyrics.lang.as_bytes()),
                    Cow::Owned(synchronised_lyrics.content_type.to_string().as_bytes().to_owned()),
                ],
                Self::Picture(picture) => vec![Cow::Owned(picture.picture_type.to_string().as_bytes().to_owned())],
                Self::EncapsulatedObject(encapsulated_object) => {
                    vec![Cow::Borrowed(encapsulated_object.description.as_bytes())]
                }
                Self::Chapter(chapter) => vec![Cow::Borrowed(chapter.element_id.as_bytes())],
                Self::MpegLocationLookupTable(_) => Vec::new(),
                Self::Unknown(unknown) => vec![Cow::Borrowed(unknown.data.as_slice())],
            }
        } else {
            match self {
                Self::Text(_) => Vec::new(),
                Self::ExtendedText(extended_text) => vec![Cow::Borrowed(extended_text.description.as_bytes())],
                Self::Link(_) => Vec::new(),
                Self::ExtendedLink(extended_link) => vec![Cow::Borrowed(extended_link.description.as_bytes())],
                Self::Popularimeter(popularimeter) => vec![Cow::Borrowed(popularimeter.user.as_bytes())],
                Self::Comment(comment) => vec![
                    Cow::Borrowed(comment.lang.as_bytes()),
                    Cow::Borrowed(comment.description.as_bytes()),
                ],
                Self::Lyrics(lyrics) => vec![
                    Cow::Borrowed(lyrics.lang.as_bytes()),
                    Cow::Borrowed(lyrics.description.as_bytes()),
                ],
                Self::SynchronisedLyrics(synchronised_lyrics) => vec![
                    Cow::Borrowed(synchronised_lyrics.lang.as_bytes()),
                    Cow::Owned(synchronised_lyrics.content_type.to_string().as_bytes().to_owned()),
                ],
                Self::Picture(picture) => vec![Cow::Owned(picture.picture_type.to_string().as_bytes().to_owned())],
                Self::EncapsulatedObject(encapsulated_object) => {
                    vec![Cow::Borrowed(encapsulated_object.description.as_bytes())]
                }
                Self::Chapter(chapter) => vec![Cow::Borrowed(chapter.element_id.as_bytes())],
                Self::MpegLocationLookupTable(_) => Vec::new(),
                Self::Unknown(unknown) => vec![Cow::Borrowed(unknown.data.as_slice())],
            }
        }
    }

    /// Constructs a new `Text` Content from the specified set of strings.
    ///
    /// # Panics
    /// If any of the strings contain a null byte.
    ///
    /// # Example
    /// ```
    /// use id3::frame::Content;
    ///
    /// let c = Content::new_text_values(["foo", "bar", "baz"]);
    /// assert_eq!(c, Content::Text("foo\u{0}bar\u{0}baz".to_string()))
    /// ```
    pub fn new_text_values(texts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let text = texts
            .into_iter()
            .map(|t| t.into())
            .inspect(|s| assert!(!s.contains('\u{0}')))
            .collect::<Vec<String>>()
            .join("\u{0}");
        Self::Text(text)
    }

    /// Returns the `Text` or None if the value is not `Text`.
    pub fn text(&self) -> Option<&str> {
        match self {
            Content::Text(content) => Some(content),
            _ => None,
        }
    }

    /// Returns split values of the `Text` frame or None if the value is not `Text`. This is only
    /// useful for ID3v2.4 tags, which support text frames containing multiple values separated by
    /// null bytes. This method returns an iterator over the separated values.
    pub fn text_values(&self) -> Option<impl Iterator<Item = &str>> {
        self.text().map(|content| content.split('\0'))
    }

    /// Returns the `ExtendedText` or None if the value is not `ExtendedText`.
    pub fn extended_text(&self) -> Option<&ExtendedText> {
        match self {
            Content::ExtendedText(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Link` or None if the value is not `Link`.
    pub fn link(&self) -> Option<&str> {
        match self {
            Content::Link(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `ExtendedLink` or None if the value is not `ExtendedLink`.
    pub fn extended_link(&self) -> Option<&ExtendedLink> {
        match self {
            Content::ExtendedLink(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `EncapsulatedObject` or None if the value is not `EncapsulatedObject`.
    pub fn encapsulated_object(&self) -> Option<&EncapsulatedObject> {
        match self {
            Content::EncapsulatedObject(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Comment` or None if the value is not `Comment`.
    pub fn comment(&self) -> Option<&Comment> {
        match self {
            Content::Comment(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Lyrics` or None if the value is not `Lyrics`.
    pub fn lyrics(&self) -> Option<&Lyrics> {
        match self {
            Content::Lyrics(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `SynchronisedLyrics` or None if the value is not `SynchronisedLyrics`.
    pub fn synchronised_lyrics(&self) -> Option<&SynchronisedLyrics> {
        match self {
            Content::SynchronisedLyrics(content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Picture` or None if the value is not `Picture`.
    pub fn picture(&self) -> Option<&Picture> {
        match self {
            Content::Picture(picture) => Some(picture),
            _ => None,
        }
    }

    /// Returns the `Chapter` or None if the value is not `Chapter`.
    pub fn chapter(&self) -> Option<&Chapter> {
        match self {
            Content::Chapter(chapter) => Some(chapter),
            _ => None,
        }
    }

    /// Returns the `MpegLocationLookupTable` or None if the value is not
    /// `MpegLocationLookupTable`.
    pub fn mpeg_location_lookup_table(&self) -> Option<&MpegLocationLookupTable> {
        match self {
            Content::MpegLocationLookupTable(mpeg_table) => Some(mpeg_table),
            _ => None,
        }
    }

    /// Returns the `Unknown` or None if the value is not `Unknown`.
    #[deprecated(note = "Use to_unknown")]
    pub fn unknown(&self) -> Option<&[u8]> {
        match self {
            Content::Unknown(unknown) => Some(&unknown.data),
            _ => None,
        }
    }

    /// Returns the `Unknown` variant or an ad-hoc encoding of any other variant.
    ///
    /// See the compatibility note on the docs of `Content` for the reason of why this function
    /// exists.
    pub fn to_unknown(&self) -> crate::Result<Cow<'_, Unknown>> {
        match self {
            Content::Unknown(unknown) => Ok(Cow::Borrowed(unknown)),
            content => {
                let version = Version::default();
                let mut data = Vec::new();
                crate::stream::frame::content::encode(&mut data, content, version, Encoding::UTF8)?;
                Ok(Cow::Owned(Unknown { data, version }))
            }
        }
    }
}

impl fmt::Display for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Content::Text(s) => write!(f, "{}", s),
            Content::Link(s) => write!(f, "{}", s),
            Content::EncapsulatedObject(enc_obj) => write!(f, "{}", enc_obj),
            Content::ExtendedText(ext_text) => write!(f, "{}", ext_text),
            Content::ExtendedLink(ext_link) => write!(f, "{}", ext_link),
            Content::Comment(comment) => write!(f, "{}", comment),
            Content::Popularimeter(popularimeter) => write!(f, "{}", popularimeter),
            Content::Lyrics(lyrics) => write!(f, "{}", lyrics),
            Content::SynchronisedLyrics(sync_lyrics) => write!(f, "{}", sync_lyrics.content_type),
            Content::Picture(picture) => write!(f, "{}", picture),
            Content::Chapter(chapter) => write!(f, "{}", chapter),
            Content::MpegLocationLookupTable(mpeg_table) => write!(f, "{}", mpeg_table),
            Content::Unknown(unknown) => write!(f, "{}", unknown),
        }
    }
}

/// The parsed contents of an extended text frame.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct ExtendedText {
    pub description: String,
    pub value: String,
}

impl fmt::Display for ExtendedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            f.write_str(&self.value)
        } else {
            write!(f, "{}: {}", self.description, self.value)
        }
    }
}

impl From<ExtendedText> for Frame {
    fn from(c: ExtendedText) -> Self {
        Self::with_content("TXXX", Content::ExtendedText(c))
    }
}

/// The parsed contents of an extended link frame.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct ExtendedLink {
    pub description: String,
    pub link: String,
}

impl fmt::Display for ExtendedLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            f.write_str(&self.link)
        } else {
            write!(f, "{}: {}", self.description, self.link)
        }
    }
}

impl From<ExtendedLink> for Frame {
    fn from(c: ExtendedLink) -> Self {
        Self::with_content("WXXX", Content::ExtendedLink(c))
    }
}

/// The parsed contents of an general encapsulated object frame.
///
/// `EncapsulatedObject` stores its own encoding, rather than using the same encoding as rest of the tag, because some apps (ex. Serato) tend to write multiple GEOB tags with different encodings.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct EncapsulatedObject {
    pub mime_type: String,
    pub filename: String,
    pub description: String,
    pub data: Vec<u8>,
}

impl fmt::Display for EncapsulatedObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let desc = if self.description.is_empty() {
            "Unknown GEOB"
        } else {
            &self.description
        };
        write!(
            f,
            "{} (\"{}\", \"{}\"), {} bytes",
            desc,
            self.filename,
            self.mime_type,
            self.data.len()
        )
    }
}

impl From<EncapsulatedObject> for Frame {
    fn from(c: EncapsulatedObject) -> Self {
        Self::with_content("GEOB", Content::EncapsulatedObject(c))
    }
}

/// The parsed contents of a comment frame.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct Comment {
    pub lang: String,
    pub description: String,
    pub text: String,
}

impl fmt::Display for Comment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            f.write_str(&self.text)
        } else {
            write!(f, "{}: {}", self.description, self.text)
        }
    }
}

impl From<Comment> for Frame {
    fn from(c: Comment) -> Self {
        Self::with_content("COMM", Content::Comment(c))
    }
}

/// The parsed contents of a popularimeter frame.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Popularimeter {
    /// An identifier for the user which performed the rating. Typically an email address.
    pub user: String,
    /// The rating is 1-255 where 1 is worst and 255 is best. 0 is unknown.
    pub rating: u8,
    /// The play count for this user. It is intended to be incremented for every time the file is
    /// played.
    pub counter: u64,
}

impl fmt::Display for Popularimeter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: *{}* ({})", self.user, self.rating, self.counter)
    }
}

impl From<Popularimeter> for Frame {
    fn from(c: Popularimeter) -> Self {
        Self::with_content("POPM", Content::Popularimeter(c))
    }
}

/// The parsed contents of an unsynchronized lyrics frame.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct Lyrics {
    pub lang: String,
    pub description: String,
    pub text: String,
}

impl fmt::Display for Lyrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            f.write_str(&self.text)
        } else {
            write!(f, "{}: {}", self.description, self.text)
        }
    }
}

impl From<Lyrics> for Frame {
    fn from(c: Lyrics) -> Self {
        Self::with_content("USLT", Content::Lyrics(c))
    }
}

/// The parsed contents of an synchronized lyrics frame.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct SynchronisedLyrics {
    pub lang: String,
    pub timestamp_format: TimestampFormat,
    pub content_type: SynchronisedLyricsType,
    pub description: String,
    // The content of a synchronised lyrics consists of the text segments mapped to a timestamp as
    // specified by the `timestamp_format` field.
    pub content: Vec<(u32, String)>,
}

const MILLISECONDS_PER_HOUR: u32 = 3600000;
const MILLISECONDS_PER_MINUTE: u32 = 60000;
const MILLISECONDS_PER_SECOND: u32 = 1000;

impl SynchronisedLyrics {
    /// Write the lyrics to the provided `writer` as a plain text table.
    ///
    /// A typical table might look like:
    ///
    /// ```text
    /// Timecode        Lyrics
    /// 00:00:12.123    Song line one
    /// 00:00:22.456    Song line two
    /// …
    /// ```
    ///
    /// # Errors
    ///
    /// This function will return any I/O error reported while formatting.
    pub fn fmt_table(&self, mut writer: impl io::Write) -> io::Result<()> {
        match self.timestamp_format {
            TimestampFormat::Mpeg => {
                writeln!(writer, "Frame\t{}", self.content_type)?;

                for (frame, lyric) in self.content.iter() {
                    writeln!(writer, "{}\t{}", frame, lyric)?;
                }
            }
            TimestampFormat::Ms => {
                writeln!(writer, "Timecode\t{}", self.content_type)?;

                for (total_ms, lyric) in self.content.iter() {
                    let hours = total_ms / MILLISECONDS_PER_HOUR;
                    let mins = (total_ms % MILLISECONDS_PER_HOUR) / MILLISECONDS_PER_MINUTE;
                    let secs = (total_ms % MILLISECONDS_PER_MINUTE) / MILLISECONDS_PER_SECOND;
                    let ms = total_ms % MILLISECONDS_PER_SECOND;

                    writeln!(
                        writer,
                        "{:02}:{:02}:{:02}.{:03}\t{}",
                        hours, mins, secs, ms, lyric
                    )?;
                }
            }
        }

        Ok(())
    }
}

impl From<SynchronisedLyrics> for Frame {
    fn from(c: SynchronisedLyrics) -> Self {
        Self::with_content("SYLT", Content::SynchronisedLyrics(c))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub enum TimestampFormat {
    // Absolute time, using MPEG frames as unit.
    Mpeg,
    // Absolute time, using milliseconds as unit.
    Ms,
}

impl fmt::Display for TimestampFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimestampFormat::Mpeg => f.write_str("MPEG frames"),
            TimestampFormat::Ms => f.write_str("Milliseconds"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub enum SynchronisedLyricsType {
    // Is other.
    Other,
    // Is lyrics.
    Lyrics,
    // Is text transcription.
    Transcription,
    // Is movement/part name (e.g. "Adagio").
    PartName,
    // Is events (e.g. "Don Quijote enters the stage").
    Event,
    // Is chord (e.g. "Bb F Fsus").
    Chord,
    // Is trivia/'pop up' information.
    Trivia,
}

impl fmt::Display for SynchronisedLyricsType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynchronisedLyricsType::Other => f.write_str("Other"),
            SynchronisedLyricsType::Lyrics => f.write_str("Lyrics"),
            SynchronisedLyricsType::Transcription => f.write_str("Transcription"),
            SynchronisedLyricsType::PartName => f.write_str("Part name"),
            SynchronisedLyricsType::Event => f.write_str("Event"),
            SynchronisedLyricsType::Chord => f.write_str("Chord"),
            SynchronisedLyricsType::Trivia => f.write_str("Trivia"),
        }
    }
}

/// Types of pictures used in APIC frames.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
    PublisherLogo,
    Undefined(u8),
}

impl From<PictureType> for u8 {
    fn from(pt: PictureType) -> Self {
        match pt {
            PictureType::Other => 0,
            PictureType::Icon => 1,
            PictureType::OtherIcon => 2,
            PictureType::CoverFront => 3,
            PictureType::CoverBack => 4,
            PictureType::Leaflet => 5,
            PictureType::Media => 6,
            PictureType::LeadArtist => 7,
            PictureType::Artist => 8,
            PictureType::Conductor => 9,
            PictureType::Band => 10,
            PictureType::Composer => 11,
            PictureType::Lyricist => 12,
            PictureType::RecordingLocation => 13,
            PictureType::DuringRecording => 14,
            PictureType::DuringPerformance => 15,
            PictureType::ScreenCapture => 16,
            PictureType::BrightFish => 17,
            PictureType::Illustration => 18,
            PictureType::BandLogo => 19,
            PictureType::PublisherLogo => 20,
            PictureType::Undefined(b) => b,
        }
    }
}

impl fmt::Display for PictureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PictureType::Other => f.write_str("Other"),
            PictureType::Icon => f.write_str("Icon"),
            PictureType::OtherIcon => f.write_str("Other icon"),
            PictureType::CoverFront => f.write_str("Front cover"),
            PictureType::CoverBack => f.write_str("Back cover"),
            PictureType::Leaflet => f.write_str("Leaflet"),
            PictureType::Media => f.write_str("Media"),
            PictureType::LeadArtist => f.write_str("Lead artist"),
            PictureType::Artist => f.write_str("Artist"),
            PictureType::Conductor => f.write_str("Conductor"),
            PictureType::Band => f.write_str("Band"),
            PictureType::Composer => f.write_str("Composer"),
            PictureType::Lyricist => f.write_str("Lyricist"),
            PictureType::RecordingLocation => f.write_str("Recording location"),
            PictureType::DuringRecording => f.write_str("During recording"),
            PictureType::DuringPerformance => f.write_str("During performance"),
            PictureType::ScreenCapture => f.write_str("Screen capture"),
            PictureType::BrightFish => f.write_str("Bright fish"),
            PictureType::Illustration => f.write_str("Illustration"),
            PictureType::BandLogo => f.write_str("Band logo"),
            PictureType::PublisherLogo => f.write_str("Publisher logo"),
            PictureType::Undefined(b) => write!(f, "Undefined type {}", b),
        }
    }
}

/// A structure representing an ID3 picture frame's contents.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Picture {
    /// The picture's MIME type.
    pub mime_type: String,
    /// The type of picture.
    pub picture_type: PictureType,
    /// A description of the picture's contents.
    pub description: String,
    /// The image data.
    pub data: Vec<u8>,
}

impl fmt::Display for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.description.is_empty() {
            write!(f, "{} ({})", self.picture_type, self.mime_type)
        } else {
            write!(
                f,
                "{}: {} ({}, {} bytes)",
                self.description,
                self.picture_type,
                self.mime_type,
                self.data.len()
            )
        }
    }
}

impl From<Picture> for Frame {
    fn from(c: Picture) -> Self {
        Self::with_content("APIC", Content::Picture(c))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct Chapter {
    pub element_id: String,
    pub start_time: u32,
    pub end_time: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub frames: Vec<Frame>,
}

impl fmt::Display for Chapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (start, end, unit) = match (self.start_offset, self.end_offset) {
            (0xffffffff, 0xffffffff) => (self.start_time, self.end_time, "ms"),
            (_, _) => (self.start_offset, self.end_offset, "b"),
        };
        let frames: Vec<&str> = self.frames.iter().map(|f| f.id()).collect();
        write!(
            f,
            "{start}{unit}-{end}{unit}: {frames}",
            start = start,
            end = end,
            unit = unit,
            frames = frames.join(", "),
        )
    }
}

impl Extend<Frame> for Chapter {
    fn extend<I: IntoIterator<Item = Frame>>(&mut self, iter: I) {
        self.frames.extend(iter)
    }
}

impl TagLike for Chapter {
    fn frames_vec(&self) -> &Vec<Frame> {
        &self.frames
    }

    fn frames_vec_mut(&mut self) -> &mut Vec<Frame> {
        &mut self.frames
    }
}

impl From<Chapter> for Frame {
    fn from(c: Chapter) -> Self {
        Self::with_content("CHAP", Content::Chapter(c))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct MpegLocationLookupTable {
    pub frames_between_reference: u16,
    /// Truncated to 24 bits.
    pub bytes_between_reference: u32,
    /// Truncated to 24 bits.
    pub millis_between_reference: u32,
    /// The number of bits in [`MpegLocationLookupTableReference::deviate_bytes`] to retain.
    /// Must be a multiple of 4.
    ///
    /// The sum of bits_for_bytes and bits_for_millis may not exceed 64.
    pub bits_for_bytes: u8,
    /// The number of bits in [`MpegLocationLookupTableReference::deviate_millis`] to retain.
    /// Must be a multiple of 4.
    pub bits_for_millis: u8,
    pub references: Vec<MpegLocationLookupTableReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(missing_docs)]
pub struct MpegLocationLookupTableReference {
    pub deviate_bytes: u32,
    pub deviate_millis: u32,
}

impl fmt::Display for MpegLocationLookupTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mpeg Lookup Table, {} references", self.references.len())
    }
}

impl From<MpegLocationLookupTable> for Frame {
    fn from(c: MpegLocationLookupTable) -> Self {
        Self::with_content("MLLT", Content::MpegLocationLookupTable(c))
    }
}

/// The contents of a frame for which no decoder is currently implemented.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Unknown {
    /// The binary contents of the frame, excluding the frame header. No compression or
    /// unsynchronization is applied.
    pub data: Vec<u8>,
    /// The version of the tag which contained this frame.
    pub version: Version,
}

impl fmt::Display for Unknown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {} bytes", self.version, self.data.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_text_display() {
        let text = Content::Text(String::from("text value"));
        assert_eq!(format!("{}", text), "text value");
    }

    #[test]
    fn content_extended_text_display() {
        let ext_text = Content::ExtendedText(ExtendedText {
            description: String::from("description value"),
            value: String::from("value value"),
        });
        assert_eq!(format!("{}", ext_text), "description value: value value");
    }

    #[test]
    fn content_link_display() {
        let link = Content::Link(String::from("link value"));
        assert_eq!(format!("{}", link), "link value");
    }

    #[test]
    fn content_extended_link_display() {
        let ext_link = Content::ExtendedLink(ExtendedLink {
            description: String::from("description value"),
            link: String::from("link value"),
        });
        assert_eq!(format!("{}", ext_link), "description value: link value");
    }

    #[test]
    fn content_comment_display() {
        let comment = Content::Comment(Comment {
            lang: String::from("lang value"),
            description: String::from("description value"),
            text: String::from("text value"),
        });
        assert_eq!(format!("{}", comment), "description value: text value");
    }

    #[test]
    fn content_lyrics_display() {
        let lyrics = Content::Lyrics(Lyrics {
            lang: String::from("lang value"),
            description: String::from("description value"),
            text: String::from("text value"),
        });
        assert_eq!(format!("{}", lyrics), "description value: text value");
    }

    #[test]
    fn content_synchronised_lyrics_display() {
        let sync_lyrics = Content::SynchronisedLyrics(SynchronisedLyrics {
            lang: String::from("lang value"),
            timestamp_format: TimestampFormat::Mpeg,
            content_type: SynchronisedLyricsType::Lyrics,
            content: vec![
                (1, String::from("first line")),
                (2, String::from("second line")),
            ],
            description: String::from("description"),
        });
        assert_eq!(format!("{}", sync_lyrics), "Lyrics");
    }

    #[test]
    fn content_picture_display() {
        let picture = Content::Picture(Picture {
            mime_type: String::from("MIME type"),
            picture_type: PictureType::Artist,
            description: String::from("description value"),
            data: vec![1, 2, 3],
        });
        assert_eq!(
            format!("{}", picture),
            "description value: Artist (MIME type, 3 bytes)"
        );
    }

    #[test]
    fn content_unknown_display() {
        let unknown = Content::Unknown(Unknown {
            version: Version::Id3v24,
            data: vec![1, 2, 3],
        });
        assert_eq!(format!("{}", unknown), "ID3v2.4, 3 bytes");
    }

    #[test]
    fn synchronised_lyrics_format_table() {
        let sync_lyrics_mpeg_lyrics = SynchronisedLyrics {
            lang: String::from("lang value"),
            timestamp_format: TimestampFormat::Mpeg,
            content_type: SynchronisedLyricsType::Lyrics,
            content: vec![
                (1, String::from("first line")),
                (2, String::from("second line")),
            ],
            description: String::from("description"),
        };
        let mut buffer: Vec<u8> = Vec::new();
        assert!(sync_lyrics_mpeg_lyrics.fmt_table(&mut buffer).is_ok());
        assert_eq!(
            std::str::from_utf8(&buffer).unwrap(),
            "Frame\tLyrics\n1\tfirst line\n2\tsecond line\n"
        );

        let sync_lyrics_ms_chord = SynchronisedLyrics {
            lang: String::from("lang value"),
            timestamp_format: TimestampFormat::Ms,
            content_type: SynchronisedLyricsType::Chord,
            content: vec![
                (1000, String::from("A")),
                (2000, String::from("B")),
                (12345678, String::from("C")),
            ],
            description: String::from("description"),
        };
        let mut buffer: Vec<u8> = Vec::new();
        assert!(sync_lyrics_ms_chord.fmt_table(&mut buffer).is_ok());
        assert_eq!(
            std::str::from_utf8(&buffer).unwrap(),
            "Timecode\tChord\n00:00:01.000\tA\n00:00:02.000\tB\n03:25:45.678\tC\n"
        );
    }

    #[test]
    fn unknown_to_unknown() {
        let unknown = Unknown {
            version: Version::Id3v22,
            data: vec![1, 2, 3, 4],
        };
        let content = Content::Unknown(unknown.clone());
        assert_eq!(*content.to_unknown().unwrap(), unknown);
    }

    #[test]
    fn link_to_unknown() {
        let content = Content::Text("https://polyfloyd.net".to_string());
        let mut data = vec![3]; // Encoding byte.
        data.extend("https://polyfloyd.net".bytes());
        let unknown = Unknown {
            version: Version::Id3v24,
            data,
        };
        assert_eq!(*content.to_unknown().unwrap(), unknown);
    }
}
