extern crate std;

use phf;

use std::rand;
use std::rand::Rng;

use frame::Encoding;

/// Returns a random sequence of 16 bytes, intended to be used as a UUID.
#[inline]
pub fn uuid() -> Vec<u8> {
    rand::task_rng().gen_iter::<u8>().take(16).collect::<Vec<_>>()
}
/// Returns the synchsafe varaiant of a `u32` value.
#[inline]
pub fn synchsafe(n: u32) -> u32 {
    let mut x: u32 = n & 0x7F | (n & 0xFFFFFF80) << 1;
    x = x & 0x7FFF | (x & 0xFFFF8000) << 1;
    x = x & 0x7FFFFF | (x & 0xFF800000) << 1;
    x
}

/// Returns the unsynchsafe varaiant of a `u32` value.
#[inline]
pub fn unsynchsafe(n: u32) -> u32 {
    (n & 0xFF | (n & 0xFF00) >> 1 | (n & 0xFF0000) >> 2 | (n & 0xFF000000) >> 3)
}

/// Returns a vector representation of a `u32` value.
#[inline]
pub fn u32_to_bytes(n: u32) -> Vec<u8> {
    vec!(((n & 0xFF000000) >> 24) as u8, ((n & 0xFF0000) >> 16) as u8, ((n & 0xFF00) >> 8) as u8, (n & 0xFF) as u8)
}

/// Returns a string created from the vector using the specified encoding.
/// Returns `None` if the vector is not a valid string of the specified
/// encoding type.
#[inline]
pub fn string_from_encoding(encoding: Encoding, data: &[u8]) -> Option<String> {
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => string_from_utf8(data),
        Encoding::UTF16 => string_from_utf16(data),
        Encoding::UTF16BE => string_from_utf16be(data) 
    }
}

/// Returns a string created from the vector using UTF-8 encoding, removing a trailing null byte
/// if present.
/// Returns `None` if the vector is not a valid UTF-8 string.
pub fn string_from_utf8(data: &[u8]) -> Option<String> {
    if data.len() > 0 && data[data.len() - 1] == 0 {
        String::from_utf8(data.slice_to(data.len() - 1).to_vec()).ok()
    } else {
        String::from_utf8(data.to_vec()).ok()
    }
}

/// Returns a string created from the vector using UTF-16 (with byte order mark) encoding.
/// Returns `None` if the vector is not a valid UTF-16 string.
pub fn string_from_utf16(data: &[u8]) -> Option<String> {
    if data.len() < 2 || data.len() % 2 != 0 { 
        return None;
    }

    let no_bom = data.slice(2, data.len());

    if data[0] == 0xFF && data[1] == 0xFE { // little endian
        string_from_utf16le(no_bom)
    } else { // big endian
        string_from_utf16be(no_bom)
    }
}

/// Returns a string created from the vector using UTF-16LE encoding.
/// Returns `None` if the vector is not a valid UTF-16LE string.
pub fn string_from_utf16le(data: &[u8]) -> Option<String> {
    if data.len() % 2 != 0 { 
        return None;
    }

    let mut buf: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut it = std::iter::range_step(0, data.len(), 2);

    for i in it {
        buf.push(data[i] as u16 | data[i + 1] as u16 << 8);
    }

    String::from_utf16(buf.as_slice())
}

/// Returns a string created from the vector using UTF-16BE encoding.
/// Returns `None` if the vector is not a valid UTF-16BE string.
pub fn string_from_utf16be(data: &[u8]) -> Option<String> {
    if data.len() % 2 != 0 { 
        return None;
    }

    let mut buf: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut it = std::iter::range_step(0, data.len(), 2);

    for i in it {
        buf.push(data[i] as u16 << 8 | data[i + 1] as u16);
    }

    String::from_utf16(buf.as_slice())
}

/// Returns a UTF-16 (with LE byte order mark) vector representation of the string.
pub fn string_to_utf16(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(2 + text.len() * 2);
    out.push_all(&[0xFFu8, 0xFEu8]); // add little endian BOM
    out.extend(string_to_utf16le(text).into_iter());
    out
}

/// Returns a UTF-16BE vector representation of the string.
pub fn string_to_utf16be(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(text.len() * 2);
    for c in text.as_slice().utf16_units() {
        out.push(((c & 0xFF00) >> 8) as u8);
        out.push((c & 0x00FF) as u8);
    }

    out
}

/// Returns a UTF-16LE vector representation of the string.
pub fn string_to_utf16le(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(text.len() * 2);
    for c in text.as_slice().utf16_units() {
        out.push((c & 0x00FF) as u8);
        out.push(((c & 0xFF00) >> 8) as u8);
    }

    out
}

/// Returns the index of the first delimiter for the specified encoding.
pub fn find_delim(encoding: Encoding, data: &[u8], index: uint) -> Option<uint> {
    let mut i = index;
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            if i >= data.len() {
                return None;
            }

            for c in data.slice_from(i).iter() {
                if *c == 0 {
                    break;
                }
                i += 1;
            }

            if i == data.len() { // delimiter was not found
                return None;
            }

            Some(i)
        },
        Encoding::UTF16 | Encoding::UTF16BE => {
            while i + 1 < data.len() 
                && (data[i] != 0 || data[i + 1] != 0) {
                    i += 2;
                }

            if i + 1 >= data.len() { // delimiter was not found
                return None;
            }

            Some(i)
        }
    } 
}

/// Returns the delimiter length for the specified encoding.
#[inline]
pub fn delim_len(encoding: Encoding) -> uint {
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => 1,
        Encoding::UTF16 | Encoding::UTF16BE => 2
    }
}

static ID_2_TO_3: phf::Map<&'static str, &'static str> = phf_map! {
    "BUF" => "RBUF",

    "CNT" => "PCNT",
    "COM" => "COMM",
    "CRA" => "AENC",

    "ETC" => "ETCO",

    "GEO" => "GEOB",

    "IPL" => "IPLS",

    "LNK" => "LINK",

    "MCI" => "MCDI",
    "MLL" => "MLLT",

    "PIC" => "APIC",
    "POP" => "POPM",

    "REV" => "RVRB",

    "SLT" => "SYLT",
    "STC" => "SYTC",

    "TAL" => "TALB",
    "TBP" => "TBPM",
    "TCM" => "TCOM",
    "TCO" => "TCON",
    "TCR" => "TCOP",
    "TDY" => "TDLY",
    "TEN" => "TENC",
    "TFT" => "TFLT",
    "TKE" => "TKEY",
    "TLA" => "TLAN",
    "TLE" => "TLEN",
    "TMT" => "TMED",
    "TOA" => "TOPE",
    "TOF" => "TOFN",
    "TOL" => "TOLY",
    "TOT" => "TOAL",
    "TP1" => "TPE1",
    "TP2" => "TPE2",
    "TP3" => "TPE3",
    "TP4" => "TPE4",
    "TPA" => "TPOS",
    "TPB" => "TPUB",
    "TRC" => "TSRC",
    "TRK" => "TRCK",
    "TSS" => "TSSE",
    "TT1" => "TIT1",
    "TT2" => "TIT2",
    "TT3" => "TIT3",
    "TXT" => "TEXT",
    "TXX" => "TXXX",
    "TYE" => "TYER",

    "UFI" => "UFID",
    "ULT" => "USLT",

    "WAF" => "WOAF",
    "WAR" => "WOAR",
    "WAS" => "WOAS",
    "WCM" => "WCOM",
    "WCP" => "WCOP",
    "WPB" => "WPUB",
    "WXX" => "WXXX",
};

/// Returns the coresponding ID3v2.3/ID3v2.4 ID given the ID3v2.2 ID. 
#[inline]
pub fn convert_id_2_to_3(id: &str) -> Option<&str> {
    ID_2_TO_3.get_equiv(id).map(|t| t.clone())
}

static ID_3_TO_2: phf::Map<&'static str, &'static str> = phf_map! {
    "RBUF" => "BUF",
              
    "PCNT" => "CNT",
    "COMM" => "COM",
    "AENC" => "CRA",
              
    "ETCO" => "ETC",
              
    "GEOB" => "GEO",
              
    "IPLS" => "IPL",
              
    "LINK" => "LNK",
              
    "MCDI" => "MCI",
    "MLLT" => "MLL",
              
    "APIC" => "PIC",
    "POPM" => "POP",
              
    "RVRB" => "REV",
              
    "SYLT" => "SLT",
    "SYTC" => "STC",
              
    "TALB" => "TAL",
    "TBPM" => "TBP",
    "TCOM" => "TCM",
    "TCON" => "TCO",
    "TCOP" => "TCR",
    "TDLY" => "TDY",
    "TENC" => "TEN",
    "TFLT" => "TFT",
    "TKEY" => "TKE",
    "TLAN" => "TLA",
    "TLEN" => "TLE",
    "TMED" => "TMT",
    "TOPE" => "TOA",
    "TOFN" => "TOF",
    "TOLY" => "TOL",
    "TOAL" => "TOT",
    "TPE1" => "TP1",
    "TPE2" => "TP2",
    "TPE3" => "TP3",
    "TPE4" => "TP4",
    "TPOS" => "TPA",
    "TPUB" => "TPB",
    "TSRC" => "TRC",
    "TRCK" => "TRK",
    "TSSE" => "TSS",
    "TIT1" => "TT1",
    "TIT2" => "TT2",
    "TIT3" => "TT3",
    "TEXT" => "TXT",
    "TXXX" => "TXX",
    "TYER" => "TYE",
              
    "UFID" => "UFI",
    "USLT" => "ULT",
              
    "WOAF" => "WAF",
    "WOAR" => "WAR",
    "WOAS" => "WAS",
    "WCOM" => "WCM",
    "WCOP" => "WCP",
    "WPUB" => "WPB",
    "WXXX" => "WXX",
};

/// Returns the coresponding ID3v2.2 ID given the ID3v2.3/ID3v2.3 ID. 
#[inline]
pub fn convert_id_3_to_2(id: &str) -> Option<&str> {
    ID_3_TO_2.get_equiv(id).map(|t| t.clone())
}

static FRAME_DESCRIPTIONS: phf::Map<&'static str, &'static str> = phf_map! {
    "AENC" => "Audio encryption",
    "APIC" => "Attached picture",
    "ASPI" => "Audio seek point index",

    "COMM" => "Comments",
    "COMR" => "Commercial frame",

    "ENCR" => "Encryption method registration",
    "EQU2" => "Equalisation (2)",
    "EQUA" => "Equalization",
    "ETCO" => "Event timing codes",

    "IPLS" => "Involved people list",

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
    "RVAD" => "Relative volume adjustment",
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
    "TDAT" => "Date",
    "TDEN" => "Encoding time",
    "TDLY" => "Playlist delay",
    "TDOR" => "Original release time",
    "TDRC" => "Recording time",
    "TDRL" => "Release time",
    "TDTG" => "Tagging time",
    "TENC" => "Encoded by",
    "TEXT" => "Lyricist/Text writer",
    "TFLT" => "File type",
    "TIME" => "Time",
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
    "TORY" => "Original release year",
    "TOWN" => "File owner/licensee",
    "TPE1" => "Lead performer(s)/Soloist(s)",
    "TPE2" => "Band/orchestra/accompaniment",
    "TPE3" => "Conductor/performer refinement",
    "TPE4" => "Interpreted, remixed, or otherwise modified by",
    "TPOS" => "Part of a set",
    "TPRO" => "Produced notice",
    "TPUB" => "Publisher",
    "TRCK" => "Track number/Position in set",
    "TRDA" => "Recording dates",
    "TRSN" => "Internet radio station name",
    "TRSO" => "Internet radio station owner",
    "TSIZ" => "Size",
    "TSO2" => "Album artist sort order",
    "TSOA" => "Album sort order",
    "TSOC" => "Composer sort order",
    "TSOP" => "Performer sort order",
    "TSOT" => "Title sort order",
    "TSRC" => "ISRC (international standard recording code)",
    "TSSE" => "Software/Hardware and settings used for encoding",
    "TYER" => "Year",
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
};

/// Returns a string describing the frame type.
#[inline]
pub fn frame_description(id: &str) -> &str {
    match FRAME_DESCRIPTIONS.get_equiv(id).map(|t| t.clone()) {
        Some(desc) => desc,
        None => ""
    }
}

// Tests {{{
#[cfg(test)]
mod tests {
    use util;
    use frame::Encoding;

    #[test]
    fn test_synchsafe() {
        assert_eq!(681570, util::synchsafe(176994));
        assert_eq!(176994, util::unsynchsafe(681570));
    }

    #[test]
    fn test_strings() {
        let text: &str = "śốмễ śŧŗỉňĝ";

        let mut utf8 = text.as_bytes().to_vec();
        utf8.push(0);
        assert_eq!(util::string_from_utf8(utf8.as_slice()).unwrap().as_slice(), text);

        // should use little endian BOM
        assert_eq!(util::string_to_utf16(text).as_slice(), b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(util::string_to_utf16be(text).as_slice(), b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D");
        assert_eq!(util::string_to_utf16le(text).as_slice(), b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(util::string_from_encoding(Encoding::UTF16BE, b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap().as_slice(), text);
        assert_eq!(util::string_from_utf16be(b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap().as_slice(), text);

        assert_eq!(util::string_from_utf16le(b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap().as_slice(), text);

        // big endian BOM
        assert_eq!(util::string_from_encoding(Encoding::UTF16, b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap().as_slice(), text);
        assert_eq!(util::string_from_utf16(b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap().as_slice(), text);

        // little endian BOM 
        assert_eq!(util::string_from_encoding(Encoding::UTF16, b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap().as_slice(), text);
        assert_eq!(util::string_from_utf16(b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap().as_slice(), text);
    }

    #[test]
    fn test_find_delim() {
        assert_eq!(util::find_delim(Encoding::UTF8, &[0x0, 0xFF, 0xFF, 0xFF, 0x0], 3).unwrap(), 4);
        assert!(util::find_delim(Encoding::UTF8, &[0x0, 0xFF, 0xFF, 0xFF, 0xFF], 3).is_none());

        assert_eq!(util::find_delim(Encoding::UTF16, &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF], 2).unwrap(), 4);
        assert!(util::find_delim(Encoding::UTF16, &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF], 2).is_none());

        assert_eq!(util::find_delim(Encoding::UTF16BE, &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF], 2).unwrap(), 4);
        assert!(util::find_delim(Encoding::UTF16BE, &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF], 2).is_none());
    }

    #[test]
    fn test_u32_to_bytes() {
        assert_eq!(util::u32_to_bytes(0x4B92DF71), vec!(0x4B as u8, 0x92 as u8, 0xDF as u8, 0x71 as u8));
    }
}
