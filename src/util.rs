extern crate std;

use std::rand;
use std::rand::Rng;

use encoding;

/// Returns a random sequence of 16 bytes, intended to be used as a UUID.
pub fn uuid() -> Vec<u8> {
    rand::task_rng().gen_iter::<u8>().take(16).collect::<Vec<_>>()
}
/// Returns the synchsafe varaiant of a `u32` value.
pub fn synchsafe(n: u32) -> u32 {
    let mut x: u32 = n & 0x7F | (n & 0xFFFFFF80) << 1;
    x = x & 0x7FFF | (x & 0xFFFF8000) << 1;
    x = x & 0x7FFFFF | (x & 0xFF800000) << 1;
    x
}

/// Returns the unsynchsafe varaiant of a `u32` value.
pub fn unsynchsafe(n: u32) -> u32 {
    (n & 0xFF | (n & 0xFF00) >> 1 | (n & 0xFF0000) >> 2 | (n & 0xFF000000) >> 3)
}

/// Returns a vector representation of a `u32` value.
pub fn u32_to_bytes(n: u32) -> Vec<u8> {
    vec!(((n & 0xFF000000) >> 24) as u8, ((n & 0xFF0000) >> 16) as u8, ((n & 0xFF00) >> 8) as u8, (n & 0xFF) as u8)
}

/// Returns a string created from the vector using UTF-8 encoding, removing a trailing null byte
/// if present.
/// Returns `Err` with the original vector if the vector is not a valid UTF-8 string.
pub fn string_from_utf8(data: Vec<u8>) -> Result<String, Vec<u8>> {
    let mut new_data = data;
    if new_data.len() > 0 && new_data.as_slice()[new_data.len() - 1] == 0 {
        new_data = new_data.as_slice().slice_to(new_data.len() - 1).to_vec();
    }
   
    String::from_utf8(new_data)
}

/// Returns a string created from the vector using UTF-16 (with byte order mark) encoding.
/// Returns `Err` with the original vector if the vector is not a valid UTF-16 string.
pub fn string_from_utf16(data: Vec<u8>) -> Result<String, Vec<u8>> {
    if data.len() < 2 || data.len() % 2 != 0 { 
        return Err(data);
    }

    let no_bom = data.slice(2, data.len()).to_vec();

    if data[0] == 0xFF && data[1] == 0xFE { // little endian
        string_from_utf16le(no_bom)
    } else { // big endian
        string_from_utf16be(no_bom)
    }
}

/// Returns a string created from the vector using UTF-16LE encoding.
/// Returns `Err` with the original vector if the vector is not a valid UTF-16LE string.
pub fn string_from_utf16le(data: Vec<u8>) -> Result<String, Vec<u8>> {
    if data.len() % 2 != 0 { 
        return Err(data);
    }

    let mut buf: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut it = std::iter::range_step(0, data.len(), 2);

    for i in it {
        buf.push(data[i] as u16 | data[i + 1] as u16 << 8);
    }

    match String::from_utf16(buf.as_slice()) {
        Some(string) => Ok(string),
        None => Err(data)
    }
}

/// Returns a string created from the vector using UTF-16BE encoding.
/// Returns `Err` with the original vector if the vector is not a valid UTF-16BE string.
pub fn string_from_utf16be(data: Vec<u8>) -> Result<String, Vec<u8>> {
    if data.len() % 2 != 0 { 
        return Err(data);
    }

    let mut buf: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut it = std::iter::range_step(0, data.len(), 2);

    for i in it {
        buf.push(data[i] as u16 << 8 | data[i + 1] as u16);
    }

    match String::from_utf16(buf.as_slice()) {
        Some(string) => Ok(string),
        None => Err(data)
    }
}

/// Returns a UTF-16 (with LE byte order mark) vector representation of the string.
pub fn string_to_utf16(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(2 + text.len() * 2);
    out.push_all([0xFFu8, 0xFEu8]); // add little endian BOM
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

/// Returns a string created from the vector using the specified encoding.
/// Returns `Err` with the original vector if the vector is not a valid string of the specified
/// encoding type.
pub fn string_from_encoding(encoding: encoding::Encoding, data: Vec<u8>) -> Result<String, Vec<u8>> {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => string_from_utf8(data),
        encoding::UTF16 => string_from_utf16(data),
        encoding::UTF16BE => string_from_utf16be(data) 
    }
}

/// Returns the index of the first delimiter for the specified encoding.
pub fn find_delim(encoding: encoding::Encoding, data: &[u8], index: uint) -> Option<uint> {
    let mut i = index;
    match encoding {
        encoding::Latin1 | encoding::UTF8 => {
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
        encoding::UTF16 | encoding::UTF16BE => {
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
pub fn delim_len(encoding: encoding::Encoding) -> uint {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => 1,
        encoding::UTF16 | encoding::UTF16BE => 2
    }
}

/// Returns the coresponding ID3v2.3/ID3v2.4 ID given the ID3v2.2 ID. 
pub fn convert_id_2_to_3(id: &str) -> Option<&str> {
    match id {
        "BUF" => Some("RBUF"),

        "CNT" => Some("PCNT"),
        "COM" => Some("COMM"),
        "CRA" => Some("AENC"),

        "ETC" => Some("ETCO"),

        "GEO" => Some("GEOB"),

        "IPL" => Some("IPLS"),

        "LNK" => Some("LINK"),

        "MCI" => Some("MCDI"),
        "MLL" => Some("MLLT"),

        "PIC" => Some("APIC"),
        "POP" => Some("POPM"),

        "REV" => Some("RVRB"),

        "SLT" => Some("SYLT"),
        "STC" => Some("SYTC"),

        "TAL" => Some("TALB"),
        "TBP" => Some("TBPM"),
        "TCM" => Some("TCOM"),
        "TCO" => Some("TCON"),
        "TCR" => Some("TCOP"),
        "TDY" => Some("TDLY"),
        "TEN" => Some("TENC"),
        "TFT" => Some("TFLT"),
        "TKE" => Some("TKEY"),
        "TLA" => Some("TLAN"),
        "TLE" => Some("TLEN"),
        "TMT" => Some("TMED"),
        "TOA" => Some("TOPE"),
        "TOF" => Some("TOFN"),
        "TOL" => Some("TOLY"),
        "TOT" => Some("TOAL"),
        "TP1" => Some("TPE1"),
        "TP2" => Some("TPE2"),
        "TP3" => Some("TPE3"),
        "TP4" => Some("TPE4"),
        "TPA" => Some("TPOS"),
        "TPB" => Some("TPUB"),
        "TRC" => Some("TSRC"),
        "TRK" => Some("TRCK"),
        "TSS" => Some("TSSE"),
        "TT1" => Some("TIT1"),
        "TT2" => Some("TIT2"),
        "TT3" => Some("TIT3"),
        "TXT" => Some("TEXT"),
        "TXX" => Some("TXXX"),
        "TYE" => Some("TYER"),

        "UFI" => Some("UFID"),
        "ULT" => Some("USLT"),

        "WAF" => Some("WOAF"),
        "WAR" => Some("WOAR"),
        "WAS" => Some("WOAS"),
        "WCM" => Some("WCOM"),
        "WCP" => Some("WCOP"),
        "WPB" => Some("WPUB"),
        "WXX" => Some("WXXX"),

        _ => None
    }
}

/// Returns a string describing the frame type.
pub fn frame_description(id: &str) -> &str {
    return match id {
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

        _ => ""
    }
}

// Tests {{{
#[cfg(test)]
mod tests {
    use util;
    use encoding;

    #[test]
    fn test_synchsafe() {
        assert_eq!(681570, util::synchsafe(176994));
        assert_eq!(176994, util::unsynchsafe(681570));
    }

    #[test]
    fn test_strings() {
        let text: &str = "śốмễ śŧŗỉňĝ";

        let mut utf8 = String::from_str(text).into_bytes();
        utf8.push(0);
        assert_eq!(util::string_from_utf8(utf8).unwrap().as_slice(), text);

        // should use little endian BOM
        assert_eq!(util::string_to_utf16(text).as_slice(), b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(util::string_to_utf16be(text).as_slice(), b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D");
        assert_eq!(util::string_to_utf16le(text).as_slice(), b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(util::string_from_encoding(encoding::UTF16BE, b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D".to_vec()).unwrap().as_slice(), text);
        assert_eq!(util::string_from_utf16be(b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D".to_vec()).unwrap().as_slice(), text);

        assert_eq!(util::string_from_utf16le(b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01".to_vec()).unwrap().as_slice(), text);

        // big endian BOM
        assert_eq!(util::string_from_encoding(encoding::UTF16, b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D".to_vec()).unwrap().as_slice(), text);
        assert_eq!(util::string_from_utf16(b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D".to_vec()).unwrap().as_slice(), text);

        // little endian BOM 
        assert_eq!(util::string_from_encoding(encoding::UTF16, b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01".to_vec()).unwrap().as_slice(), text);
        assert_eq!(util::string_from_utf16(b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01".to_vec()).unwrap().as_slice(), text);
    }

    #[test]
    fn test_find_delim() {
        assert_eq!(util::find_delim(encoding::UTF8, [0x0, 0xFF, 0xFF, 0xFF, 0x0], 3).unwrap(), 4);
        assert!(util::find_delim(encoding::UTF8, [0x0, 0xFF, 0xFF, 0xFF, 0xFF], 3).is_none());

        assert_eq!(util::find_delim(encoding::UTF16, [0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF], 2).unwrap(), 4);
        assert!(util::find_delim(encoding::UTF16, [0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF], 2).is_none());

        assert_eq!(util::find_delim(encoding::UTF16BE, [0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF], 2).unwrap(), 4);
        assert!(util::find_delim(encoding::UTF16BE, [0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF], 2).is_none());
    }

    #[test]
    fn test_u32_to_bytes() {
        assert_eq!(util::u32_to_bytes(0x4B92DF71), vec!(0x4B as u8, 0x92 as u8, 0xDF as u8, 0x71 as u8));
    }
}
