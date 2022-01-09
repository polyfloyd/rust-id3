use crate::tag::Version;
use std::fmt;
use std::str;

pub use self::content::{
    Chapter, Comment, Content, EncapsulatedObject, ExtendedLink, ExtendedText, Lyrics, Picture,
    PictureType, SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat, Unknown,
};
pub use self::timestamp::Timestamp;

mod content;
mod timestamp;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ID {
    /// A valid 4-byte frame ID.
    Valid(String),
    /// If an ID3v2.2 ID could not be mapped to its ID3v2.4 counterpart, it is stored as is. This
    /// allows invalid ID3v2.2 frames to be retained.
    Invalid(String),
}

/// A structure representing an ID3 frame.
///
/// It is imporant to note that the (Partial)Eq and Hash implementations are based on the ID3 spec.
/// This means that text frames with equal ID's are equal but picture frames with both "APIC" as ID
/// are not because their uniqueness is also defined by their content.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Frame {
    id: ID,
    content: Content,
    tag_alter_preservation: bool,
    file_alter_preservation: bool,
}

impl Frame {
    pub(crate) fn unique(&self) -> impl Eq + '_ {
        (&self.id, self.content.unique())
    }

    /// Creates a frame with the specified ID and content.
    ///
    /// Both ID3v2.2 and >ID3v2.3 IDs are accepted, although they will be converted to ID3v2.3
    /// format. If an ID3v2.2 ID is supplied but could not be remapped, it is stored as-is.
    ///
    /// # Panics
    /// If the id's length is not 3 or 4 bytes long.
    pub fn with_content(id: impl AsRef<str>, content: Content) -> Frame {
        assert!({
            let l = id.as_ref().bytes().count();
            l == 3 || l == 4
        });
        Frame {
            id: if id.as_ref().len() == 3 {
                match convert_id_2_to_3(id.as_ref()) {
                    Some(translated) => ID::Valid(translated.to_string()),
                    None => ID::Invalid(id.as_ref().to_string()),
                }
            } else {
                ID::Valid(id.as_ref().to_string())
            },
            content,
            tag_alter_preservation: false,
            file_alter_preservation: false,
        }
    }

    /// Returns the ID of this frame.
    ///
    /// The string returned us usually 4 bytes long except when the frame was read from an ID3v2.2
    /// tag and the ID could not be mapped to an ID3v2.3 ID.
    pub fn id(&self) -> &str {
        match self.id {
            ID::Valid(ref id) | ID::Invalid(ref id) => id,
        }
    }

    /// Returns the ID that is compatible with specified version or None if no ID is available in
    /// that version.
    pub fn id_for_version(&self, version: Version) -> Option<&str> {
        match (version, &self.id) {
            (Version::Id3v22, &ID::Valid(ref id)) => convert_id_3_to_2(id),
            (Version::Id3v23, &ID::Valid(ref id))
            | (Version::Id3v24, &ID::Valid(ref id))
            | (Version::Id3v22, &ID::Invalid(ref id)) => Some(id),
            (_, &ID::Invalid(_)) => None,
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

    /// Returns the name of the frame.
    ///
    /// The name is the _human-readable_ representation of a frame
    /// id. For example, the id `"TCOM"` corresponds to the name
    /// `"Composer"`. The names are taken from the
    /// [ID3v2.4](http://id3.org/id3v2.4.0-frames),
    /// [ID3v2.3](http://id3.org/d3v2.3.0) and
    /// [ID3v2.2](http://id3.org/d3v2-00) standards.
    pub fn name(&self) -> &str {
        match self.id() {
            // Ids and names defined in section 4 of http://id3.org/id3v2.4.0-frames
            "AENC" => "Audio encryption",
            "APIC" => "Attached picture",
            "ASPI" => "Audio seek point index",
            "COMM" => "Comments",
            "COMR" => "Commercial frame",
            "ENCR" => "Encryption method registration",
            "EQU2" => "Equalisation (2)",
            "ETCO" => "Event timing codes",
            "GEOB" => "General encapsulated object",
            "GRID" => "Group identification registration",
            "LINK" => "Linked information",
            "MCDI" => "Music CD identifier",
            "MLLT" => "MPEG location lookup table",
            "OWNE" => "Ownership frame",
            "PRIV" => "Private frame",
            "PCNT" => "Play counter",
            "POPM" => "Popularimeter",
            "POSS" => "Position synchronisation frame",
            "RBUF" => "Recommended buffer size",
            "RVA2" => "Relative volume adjustment (2)",
            "RVRB" => "Reverb",
            "SEEK" => "Seek frame",
            "SIGN" => "Signature frame",
            "SYLT" => "Synchronised lyric/text",
            "SYTC" => "Synchronised tempo codes",
            "TALB" => "Album/Movie/Show title",
            "TBPM" => "BPM (beats per minute)",
            "TCOM" => "Composer",
            "TCON" => "Content type",
            "TCOP" => "Copyright message",
            "TDEN" => "Encoding time",
            "TDLY" => "Playlist delay",
            "TDOR" => "Original release time",
            "TDRC" => "Recording time",
            "TDRL" => "Release time",
            "TDTG" => "Tagging time",
            "TENC" => "Encoded by",
            "TEXT" => "Lyricist/Text writer",
            "TFLT" => "File type",
            "TIPL" => "Involved people list",
            "TIT1" => "Content group description",
            "TIT2" => "Title/songname/content description",
            "TIT3" => "Subtitle/Description refinement",
            "TKEY" => "Initial key",
            "TLAN" => "Language(s)",
            "TLEN" => "Length",
            "TMCL" => "Musician credits list",
            "TMED" => "Media type",
            "TMOO" => "Mood",
            "TOAL" => "Original album/movie/show title",
            "TOFN" => "Original filename",
            "TOLY" => "Original lyricist(s)/text writer(s)",
            "TOPE" => "Original artist(s)/performer(s)",
            "TOWN" => "File owner/licensee",
            "TPE1" => "Lead performer(s)/Soloist(s)",
            "TPE2" => "Band/orchestra/accompaniment",
            "TPE3" => "Conductor/performer refinement",
            "TPE4" => "Interpreted, remixed, or otherwise modified by",
            "TPOS" => "Part of a set",
            "TPRO" => "Produced notice",
            "TPUB" => "Publisher",
            "TRCK" => "Track number/Position in set",
            "TRSN" => "Internet radio station name",
            "TRSO" => "Internet radio station owner",
            "TSOA" => "Album sort order",
            "TSOP" => "Performer sort order",
            "TSOT" => "Title sort order",
            "TSRC" => "ISRC (international standard recording code)",
            "TSSE" => "Software/Hardware and settings used for encoding",
            "TSST" => "Set subtitle",
            "TXXX" => "User defined text information frame",
            "UFID" => "Unique file identifier",
            "USER" => "Terms of use",
            "USLT" => "Unsynchronised lyric/text transcription",
            "WCOM" => "Commercial information",
            "WCOP" => "Copyright/Legal information",
            "WOAF" => "Official audio file webpage",
            "WOAR" => "Official artist/performer webpage",
            "WOAS" => "Official audio source webpage",
            "WORS" => "Official Internet radio station homepage",
            "WPAY" => "Payment",
            "WPUB" => "Publishers official webpage",
            "WXXX" => "User defined URL link frame",

            // Ids and names defined in section 4 of
            // http://id3.org/d3v2.3.0 which have not been previously
            // defined above
            "EQUA" => "Equalization",
            "IPLS" => "Involved people list",
            "RVAD" => "Relative volume adjustment",
            "TDAT" => "Date",
            "TIME" => "Time",
            "TORY" => "Original release year",
            "TRDA" => "Recording dates",
            "TSIZ" => "Size",
            "TYER" => "Year",

            // Ids and names defined in section 4 of
            // http://id3.org/d3v2-00 which have not been previously
            // defined above
            "BUF" => "Recommended buffer size",
            "CNT" => "Play counter",
            "COM" => "Comments",
            "CRA" => "Audio encryption",
            "CRM" => "Encrypted meta frame",
            "ETC" => "Event timing codes",
            "EQU" => "Equalization",
            "GEO" => "General encapsulated object",
            "IPL" => "Involved people list",
            "LNK" => "Linked information",
            "MCI" => "Music CD Identifier",
            "MLL" => "MPEG location lookup table",
            "PIC" => "Attached picture",
            "POP" => "Popularimeter",
            "REV" => "Reverb",
            "RVA" => "Relative volume adjustment",
            "SLT" => "Synchronized lyric/text",
            "STC" => "Synced tempo codes",
            "TAL" => "Album/Movie/Show title",
            "TBP" => "BPM (Beats Per Minute)",
            "TCM" => "Composer",
            "TCO" => "Content type",
            "TCR" => "Copyright message",
            "TDA" => "Date",
            "TDY" => "Playlist delay",
            "TEN" => "Encoded by",
            "TFT" => "File type",
            "TIM" => "Time",
            "TKE" => "Initial key",
            "TLA" => "Language(s)",
            "TLE" => "Length",
            "TMT" => "Media type",
            "TOA" => "Original artist(s)/performer(s)",
            "TOF" => "Original filename",
            "TOL" => "Original Lyricist(s)/text writer(s)",
            "TOR" => "Original release year",
            "TOT" => "Original album/Movie/Show title",
            "TP1" => "Lead artist(s)/Lead performer(s)/Soloist(s)/Performing group",
            "TP2" => "Band/Orchestra/Accompaniment",
            "TP3" => "Conductor/Performer refinement",
            "TP4" => "Interpreted, remixed, or otherwise modified by",
            "TPA" => "Part of a set",
            "TPB" => "Publisher",
            "TRC" => "ISRC (International Standard Recording Code)",
            "TRD" => "Recording dates",
            "TRK" => "Track number/Position in set",
            "TSI" => "Size",
            "TSS" => "Software/hardware and settings used for encoding",
            "TT1" => "Content group description",
            "TT2" => "Title/Songname/Content description",
            "TT3" => "Subtitle/Description refinement",
            "TXT" => "Lyricist/text writer",
            "TXX" => "User defined text information frame",
            "TYE" => "Year",
            "UFI" => "Unique file identifier",
            "ULT" => "Unsychronized lyric/text transcription",
            "WAF" => "Official audio file webpage",
            "WAR" => "Official artist/performer webpage",
            "WAS" => "Official audio source webpage",
            "WCM" => "Commercial information",
            "WCP" => "Copyright/Legal information",
            "WPB" => "Publishers official webpage",
            "WXX" => "User defined URL link frame",

            v => v,
        }
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{} = {}", self.name(), self.content)
    }
}

macro_rules! convert_2_to_3_and_back {
    ( $( $id2:expr, $id3:expr ),* ) => {
        fn convert_id_2_to_3(id: impl AsRef<str>) -> Option<&'static str> {
            match id.as_ref() {
                $($id2 => Some($id3),)*
                _ => None,
            }
        }

        fn convert_id_3_to_2(id: impl AsRef<str>) -> Option<&'static str> {
            match id.as_ref() {
                $($id3 => Some($id2),)*
                _ => None,
            }
        }
    }
}

#[rustfmt::skip]
convert_2_to_3_and_back!(
    "BUF", "RBUF",

    "CNT", "PCNT",
    "COM", "COMM",
    "CRA", "AENC",
    // "CRM" does not exist in ID3v2.3

    "ETC", "ETCO",
    "EQU", "EQUA",

    "GEO", "GEOB",

    "IPL", "IPLS",

    "LNK", "LINK",

    "MCI", "MCDI",
    "MLL", "MLLT",

    "PIC", "APIC",
    "POP", "POPM",

    "REV", "RVRB",
    "RVA", "RVA2",

    "SLT", "SYLT",
    "STC", "SYTC",

    "TAL", "TALB",
    "TBP", "TBPM",
    "TCM", "TCOM",
    "TCO", "TCON",
    "TCR", "TCOP",
    "TDA", "TDAT",
    "TDY", "TDLY",
    "TEN", "TENC",
    "TFT", "TFLT",
    "TIM", "TIME",
    "TKE", "TKEY",
    "TLA", "TLAN",
    "TLE", "TLEN",
    "TMT", "TMED",
    "TOA", "TOPE",
    "TOF", "TOFN",
    "TOL", "TOLY",
    "TOT", "TOAL",
    "TOR", "TORY",
    "TP1", "TPE1",
    "TP2", "TPE2",
    "TP3", "TPE3",
    "TP4", "TPE4",
    "TPA", "TPOS",
    "TPB", "TPUB",
    "TRC", "TSRC",
    "TRD", "TRDA",
    "TRK", "TRCK",
    "TSI", "TSIZ",
    "TSS", "TSSE",
    "TT1", "TIT1",
    "TT2", "TIT2",
    "TT3", "TIT3",
    "TXT", "TEXT",
    "TXX", "TXXX",
    "TYE", "TYER",

    "UFI", "UFID",
    "ULT", "USLT",

    "WAF", "WOAF",
    "WAR", "WOAR",
    "WAS", "WOAS",
    "WCM", "WCOM",
    "WCP", "WCOP",
    "WPB", "WPUB",
    "WXX", "WXXX"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        let title_frame = Frame::with_content("TIT2", Content::Text("title".to_owned()));
        assert_eq!(
            format!("{}", title_frame),
            "Title/songname/content description = title"
        );

        let txxx_frame = Frame::with_content(
            "TXXX",
            Content::ExtendedText(ExtendedText {
                description: "description".to_owned(),
                value: "value".to_owned(),
            }),
        );
        assert_eq!(
            format!("{}", txxx_frame),
            "User defined text information frame = description: value"
        );
    }
}
