use std::borrow::Cow;
use std::fmt;
use std::io;

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
    /// A value containing the parsed contents of a synchronised lyrics frame (SYLT).
    SynchronisedLyrics(SynchronisedLyrics),
    /// A value containing the parsed contents of a picture frame (APIC).
    Picture(Picture),
    /// A value containing the parsed contents of a general encapsulated object frame (GEOB).
    EncapsulatedObject(EncapsulatedObject),
    /// A value containing the bytes of a unknown frame.
    Unknown(Vec<u8>),
}

impl Content {
    pub(crate) fn unique(&self) -> impl Eq + '_ {
        match self {
            Self::Text(_) => Vec::new(),
            Self::ExtendedText(extended_text) => vec![Cow::Borrowed(&extended_text.description)],
            Self::Link(_) => Vec::new(),
            Self::ExtendedLink(extended_link) => vec![Cow::Borrowed(&extended_link.description)],
            Self::Comment(comment) => vec![
                Cow::Borrowed(&comment.lang),
                Cow::Borrowed(&comment.description),
            ],
            Self::Lyrics(lyrics) => vec![
                Cow::Borrowed(&lyrics.lang),
                Cow::Borrowed(&lyrics.description),
            ],
            Self::SynchronisedLyrics(synchronised_lyrics) => vec![
                Cow::Borrowed(&synchronised_lyrics.lang),
                Cow::Owned(synchronised_lyrics.content_type.to_string()),
            ],
            Self::Picture(picture) => vec![Cow::Owned(picture.picture_type.to_string())],
            Self::EncapsulatedObject(encapsulated_object) => {
                vec![Cow::Borrowed(&encapsulated_object.description)]
            }
            Self::Unknown(_) => Vec::new(),
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
        match *self {
            Content::Text(ref content) => Some(&*content),
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
    pub fn extended_link(&self) -> Option<&ExtendedLink> {
        match *self {
            Content::ExtendedLink(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `EncapsulatedObject` or None if the value is not `EncapsulatedObject`.
    pub fn encapsulated_object(&self) -> Option<&EncapsulatedObject> {
        match *self {
            Content::EncapsulatedObject(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Comment` or None if the value is not `Comment`.
    pub fn comment(&self) -> Option<&Comment> {
        match *self {
            Content::Comment(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Lyrics` or None if the value is not `Lyrics`.
    pub fn lyrics(&self) -> Option<&Lyrics> {
        match *self {
            Content::Lyrics(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `SynchronisedLyrics` or None if the value is not `SynchronisedLyrics`.
    pub fn synchronised_lyrics(&self) -> Option<&SynchronisedLyrics> {
        match *self {
            Content::SynchronisedLyrics(ref content) => Some(content),
            _ => None,
        }
    }

    /// Returns the `Picture` or None if the value is not `Picture`.
    pub fn picture(&self) -> Option<&Picture> {
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

impl fmt::Display for Content {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Content::Text(s) => write!(f, "{}", s),
            Content::Link(s) => write!(f, "{}", s),
            Content::EncapsulatedObject(enc_obj) => write!(f, "{}", enc_obj),
            Content::ExtendedText(ext_text) => write!(f, "{}", ext_text),
            Content::ExtendedLink(ext_link) => write!(f, "{}", ext_link),
            Content::Comment(comment) => write!(f, "{}", comment),
            Content::Lyrics(lyrics) => write!(f, "{}", lyrics),
            Content::SynchronisedLyrics(sync_lyrics) => write!(f, "{}", sync_lyrics.content_type),
            Content::Picture(picture) => write!(f, "{}", picture),
            Content::Unknown(data) => write!(f, "Unknown, {} bytes", data.len()),
        }
    }
}

/// The parsed contents of an extended text frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// The parsed contents of an extended link frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// The parsed contents of an general encapsulated object frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// The parsed contents of a comment frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// The parsed contents of an unsynchronized lyrics frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// The parsed contents of an unsynchronized lyrics frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct SynchronisedLyrics {
    pub lang: String,
    pub timestamp_format: TimestampFormat,
    pub content_type: SynchronisedLyricsType,
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
    ///```
    ///
    /// # Errors
    ///
    /// This function will return any I/O error reported while formatting.
    pub fn fmt_table(&self, mut writer: impl io::Write) -> io::Result<()> {
        match self.timestamp_format {
            TimestampFormat::MPEG => {
                writeln!(writer, "Frame\t{}", self.content_type)?;

                for (frame, lyric) in self.content.iter() {
                    writeln!(writer, "{}\t{}", frame, lyric)?;
                }
            }
            TimestampFormat::MS => {
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
#[allow(clippy::upper_case_acronyms)] // Breaking change, allow for now.
pub enum TimestampFormat {
    // Absolute time, using MPEG frames as unit.
    MPEG,
    // Absolute time, using milliseconds as unit.
    MS,
}

impl fmt::Display for TimestampFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimestampFormat::MPEG => f.write_str("MPEG frames"),
            TimestampFormat::MS => f.write_str("Milliseconds"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
            timestamp_format: TimestampFormat::MPEG,
            content_type: SynchronisedLyricsType::Lyrics,
            content: vec![
                (1, String::from("first line")),
                (2, String::from("second line")),
            ],
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
        let unknown = Content::Unknown(vec![1, 2, 3]);
        assert_eq!(format!("{}", unknown), "Unknown, 3 bytes");
    }

    #[test]
    fn synchronised_lyrics_format_table() {
        let sync_lyrics_mpeg_lyrics = SynchronisedLyrics {
            lang: String::from("lang value"),
            timestamp_format: TimestampFormat::MPEG,
            content_type: SynchronisedLyricsType::Lyrics,
            content: vec![
                (1, String::from("first line")),
                (2, String::from("second line")),
            ],
        };
        let mut buffer: Vec<u8> = Vec::new();
        assert!(sync_lyrics_mpeg_lyrics.fmt_table(&mut buffer).is_ok());
        assert_eq!(
            std::str::from_utf8(&buffer).unwrap(),
            "Frame\tLyrics\n1\tfirst line\n2\tsecond line\n"
        );

        let sync_lyrics_ms_chord = SynchronisedLyrics {
            lang: String::from("lang value"),
            timestamp_format: TimestampFormat::MS,
            content_type: SynchronisedLyricsType::Chord,
            content: vec![
                (1000, String::from("A")),
                (2000, String::from("B")),
                (12345678, String::from("C")),
            ],
        };
        let mut buffer: Vec<u8> = Vec::new();
        assert!(sync_lyrics_ms_chord.fmt_table(&mut buffer).is_ok());
        assert_eq!(
            std::str::from_utf8(&buffer).unwrap(),
            "Timecode\tChord\n00:00:01.000\tA\n00:00:02.000\tB\n03:25:45.678\tC\n"
        );
    }
}
