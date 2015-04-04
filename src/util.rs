extern crate std;
extern crate rand;

use self::rand::Rng;
use frame::Encoding;
use std::mem::transmute;
use std::collections::HashMap;

/// Returns a random sequence of 16 bytes, intended to be used as a UUID.
#[inline]
pub fn uuid() -> Vec<u8> {
    rand::thread_rng().gen_iter::<u8>().take(16).collect()
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

/// Returns a string created from the vector using UTF-8 encoding, removing any trailing null
/// bytes.
/// Returns `None` if the vector is not a valid UTF-8 string.
pub fn string_from_utf8(data: &[u8]) -> Option<String> {
    let data: Vec<u8> = data.iter().take_while(|&c| *c != 0).map(|c| *c).collect();
    String::from_utf8(data).ok()
}

/// Returns a string created from the vector using UTF-16 (with byte order mark) encoding.
/// Returns `None` if the vector is not a valid UTF-16 string.
pub fn string_from_utf16(data: &[u8]) -> Option<String> {
    if data.len() < 2 || data.len() % 2 != 0 { 
        return None;
    }

    if data[0] == 0xFF && data[1] == 0xFE { // little endian
        string_from_utf16le(&data[2..])
    } else { // big endian
        string_from_utf16be(&data[2..])
    }
}

/// Returns a string created from the vector using UTF-16LE encoding.
/// Returns `None` if the vector is not a valid UTF-16LE string.
pub fn string_from_utf16le(data: &[u8]) -> Option<String> {
    if data.len() % 2 != 0 { 
        return None;
    }

    if cfg!(target_endian = "little") {
        let buf = unsafe { transmute::<_, &[u16]>(&data[..]) };
        String::from_utf16(&buf[..data.len() / 2]).ok()
    } else {
        let mut buf: Vec<u16> = Vec::with_capacity(data.len() / 2);

        for i in (0..data.len()).step_by(2) {
            buf.push(data[i] as u16 | (data[i + 1] as u16) << 8);
        }

        String::from_utf16(&buf[..]).ok()
    }
}

/// Returns a string created from the vector using UTF-16BE encoding.
/// Returns `None` if the vector is not a valid UTF-16BE string.
pub fn string_from_utf16be(data: &[u8]) -> Option<String> {
    if data.len() % 2 != 0 { 
        return None;
    }
    if cfg!(target_endian = "big") {
        let buf = unsafe { transmute::<_, &[u16]>(&data[..]) };
        String::from_utf16(&buf[..data.len() / 2]).ok()
    } else {
        let mut buf: Vec<u16> = Vec::with_capacity(data.len() / 2);

        for i in (0..data.len()).step_by(2) {
            buf.push((data[i] as u16) << 8 | data[i + 1] as u16);
        }

        String::from_utf16(&buf[..]).ok()
    }
}

/// Returns a UTF-16 (with native byte order) vector representation of the string.
pub fn string_to_utf16(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(2 + text.len() * 2);

    if cfg!(target_endian = "little") {
        out.push_all(&[0xFF, 0xFE]); // add little endian BOM
        out.extend(string_to_utf16le(text).into_iter());
    } else {
        out.push_all(&[0xFE, 0xFF]); // add big endian BOM
        out.extend(string_to_utf16be(text).into_iter());
    }
    out
}

/// Returns a UTF-16BE vector representation of the string.
pub fn string_to_utf16be(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(text.len() * 2);
    for c in text[..].utf16_units() {
        out.push(((c & 0xFF00) >> 8) as u8);
        out.push((c & 0x00FF) as u8);
    }

    out
}

/// Returns a UTF-16LE vector representation of the string.
pub fn string_to_utf16le(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(text.len() * 2);
    for c in text.utf16_units() {
        out.push((c & 0x00FF) as u8);
        out.push(((c & 0xFF00) >> 8) as u8);
    }

    out
}

/// Returns the index of the first delimiter for the specified encoding.
pub fn find_delim(encoding: Encoding, data: &[u8], index: usize) -> Option<usize> {
    let mut i = index;
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            if i >= data.len() {
                return None;
            }

            for c in data[i..].iter() {
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
pub fn delim_len(encoding: Encoding) -> usize {
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => 1,
        Encoding::UTF16 | Encoding::UTF16BE => 2
    }
}

lazy_static! {
    static ref ID_2_TO_3: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("BUF", "RBUF");

        m.insert("CNT", "PCNT");
        m.insert("COM", "COMM");
        m.insert("CRA", "AENC");

        m.insert("ETC", "ETCO");

        m.insert("GEO", "GEOB");

        m.insert("IPL", "IPLS");

        m.insert("LNK", "LINK");

        m.insert("MCI", "MCDI");
        m.insert("MLL", "MLLT");

        m.insert("PIC", "APIC");
        m.insert("POP", "POPM");

        m.insert("REV", "RVRB");

        m.insert("SLT", "SYLT");
        m.insert("STC", "SYTC");

        m.insert("TAL", "TALB");
        m.insert("TBP", "TBPM");
        m.insert("TCM", "TCOM");
        m.insert("TCO", "TCON");
        m.insert("TCR", "TCOP");
        m.insert("TDY", "TDLY");
        m.insert("TEN", "TENC");
        m.insert("TFT", "TFLT");
        m.insert("TKE", "TKEY");
        m.insert("TLA", "TLAN");
        m.insert("TLE", "TLEN");
        m.insert("TMT", "TMED");
        m.insert("TOA", "TOPE");
        m.insert("TOF", "TOFN");
        m.insert("TOL", "TOLY");
        m.insert("TOT", "TOAL");
        m.insert("TP1", "TPE1");
        m.insert("TP2", "TPE2");
        m.insert("TP3", "TPE3");
        m.insert("TP4", "TPE4");
        m.insert("TPA", "TPOS");
        m.insert("TPB", "TPUB");
        m.insert("TRC", "TSRC");
        m.insert("TRK", "TRCK");
        m.insert("TSS", "TSSE");
        m.insert("TT1", "TIT1");
        m.insert("TT2", "TIT2");
        m.insert("TT3", "TIT3");
        m.insert("TXT", "TEXT");
        m.insert("TXX", "TXXX");
        m.insert("TYE", "TYER");

        m.insert("UFI", "UFID");
        m.insert("ULT", "USLT");

        m.insert("WAF", "WOAF");
        m.insert("WAR", "WOAR");
        m.insert("WAS", "WOAS");
        m.insert("WCM", "WCOM");
        m.insert("WCP", "WCOP");
        m.insert("WPB", "WPUB");
        m.insert("WXX", "WXXX");
        m
    };
}

/// Returns the coresponding ID3v2.3/ID3v2.4 ID given the ID3v2.2 ID. 
#[inline]
pub fn convert_id_2_to_3(id: &str) -> Option<&'static str> {
    ID_2_TO_3.get(id).map(|t| *t)
}

lazy_static! {
    static ref ID_3_TO_2: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("RBUF", "BUF");

        m.insert("PCNT", "CNT");
        m.insert("COMM", "COM");
        m.insert("AENC", "CRA");

        m.insert("ETCO", "ETC");

        m.insert("GEOB", "GEO");

        m.insert("IPLS", "IPL");

        m.insert("LINK", "LNK");

        m.insert("MCDI", "MCI");
        m.insert("MLLT", "MLL");

        m.insert("APIC", "PIC");
        m.insert("POPM", "POP");

        m.insert("RVRB", "REV");

        m.insert("SYLT", "SLT");
        m.insert("SYTC", "STC");

        m.insert("TALB", "TAL");
        m.insert("TBPM", "TBP");
        m.insert("TCOM", "TCM");
        m.insert("TCON", "TCO");
        m.insert("TCOP", "TCR");
        m.insert("TDLY", "TDY");
        m.insert("TENC", "TEN");
        m.insert("TFLT", "TFT");
        m.insert("TKEY", "TKE");
        m.insert("TLAN", "TLA");
        m.insert("TLEN", "TLE");
        m.insert("TMED", "TMT");
        m.insert("TOPE", "TOA");
        m.insert("TOFN", "TOF");
        m.insert("TOLY", "TOL");
        m.insert("TOAL", "TOT");
        m.insert("TPE1", "TP1");
        m.insert("TPE2", "TP2");
        m.insert("TPE3", "TP3");
        m.insert("TPE4", "TP4");
        m.insert("TPOS", "TPA");
        m.insert("TPUB", "TPB");
        m.insert("TSRC", "TRC");
        m.insert("TRCK", "TRK");
        m.insert("TSSE", "TSS");
        m.insert("TIT1", "TT1");
        m.insert("TIT2", "TT2");
        m.insert("TIT3", "TT3");
        m.insert("TEXT", "TXT");
        m.insert("TXXX", "TXX");
        m.insert("TYER", "TYE");

        m.insert("UFID", "UFI");
        m.insert("USLT", "ULT");

        m.insert("WOAF", "WAF");
        m.insert("WOAR", "WAR");
        m.insert("WOAS", "WAS");
        m.insert("WCOM", "WCM");
        m.insert("WCOP", "WCP");
        m.insert("WPUB", "WPB");
        m.insert("WXXX", "WXX");
        m
    };
}

/// Returns the coresponding ID3v2.2 ID given the ID3v2.3/ID3v2.3 ID. 
#[inline]
pub fn convert_id_3_to_2(id: &str) -> Option<&'static str> {
    ID_3_TO_2.get(id).map(|t| *t)
}

lazy_static! {
    static ref FRAME_DESCRIPTIONS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("AENC", "Audio encryption");
        m.insert("APIC", "Attached picture");
        m.insert("ASPI", "Audio seek point index");

        m.insert("COMM", "Comments");
        m.insert("COMR", "Commercial frame");

        m.insert("ENCR", "Encryption method registration");
        m.insert("EQU2", "Equalisation (2)");
        m.insert("EQUA", "Equalization");
        m.insert("ETCO", "Event timing codes");

        m.insert("IPLS", "Involved people list");

        m.insert("GEOB", "General encapsulated object");
        m.insert("GRID", "Group identification registration");

        m.insert("LINK", "Linked information");

        m.insert("MCDI", "Music CD identifier");
        m.insert("MLLT", "MPEG location lookup table");

        m.insert("OWNE", "Ownership frame");

        m.insert("PRIV", "Private frame");
        m.insert("PCNT", "Play counter");
        m.insert("POPM", "Popularimeter");
        m.insert("POSS", "Position synchronisation frame");

        m.insert("RBUF", "Recommended buffer size");
        m.insert("RVA2", "Relative volume adjustment (2)");
        m.insert("RVAD", "Relative volume adjustment");
        m.insert("RVRB", "Reverb");

        m.insert("SEEK", "Seek frame");
        m.insert("SIGN", "Signature frame");
        m.insert("SYLT", "Synchronised lyric/text");
        m.insert("SYTC", "Synchronised tempo codes");

        m.insert("TALB", "Album/Movie/Show title");
        m.insert("TBPM", "BPM (beats per minute)");
        m.insert("TCOM", "Composer");
        m.insert("TCON", "Content type");
        m.insert("TCOP", "Copyright message");
        m.insert("TDAT", "Date");
        m.insert("TDEN", "Encoding time");
        m.insert("TDLY", "Playlist delay");
        m.insert("TDOR", "Original release time");
        m.insert("TDRC", "Recording time");
        m.insert("TDRL", "Release time");
        m.insert("TDTG", "Tagging time");
        m.insert("TENC", "Encoded by");
        m.insert("TEXT", "Lyricist/Text writer");
        m.insert("TFLT", "File type");
        m.insert("TIME", "Time");
        m.insert("TIPL", "Involved people list");
        m.insert("TIT1", "Content group description");
        m.insert("TIT2", "Title/songname/content description");
        m.insert("TIT3", "Subtitle/Description refinement");
        m.insert("TKEY", "Initial key");
        m.insert("TLAN", "Language(s)");
        m.insert("TLEN", "Length");
        m.insert("TMCL", "Musician credits list");
        m.insert("TMED", "Media type");
        m.insert("TMOO", "Mood");
        m.insert("TOAL", "Original album/movie/show title");
        m.insert("TOFN", "Original filename");
        m.insert("TOLY", "Original lyricist(s)/text writer(s)");
        m.insert("TOPE", "Original artist(s)/performer(s)");
        m.insert("TORY", "Original release year");
        m.insert("TOWN", "File owner/licensee");
        m.insert("TPE1", "Lead performer(s)/Soloist(s)");
        m.insert("TPE2", "Band/orchestra/accompaniment");
        m.insert("TPE3", "Conductor/performer refinement");
        m.insert("TPE4", "Interpreted, remixed, or otherwise modified by");
        m.insert("TPOS", "Part of a set");
        m.insert("TPRO", "Produced notice");
        m.insert("TPUB", "Publisher");
        m.insert("TRCK", "Track number/Position in set");
        m.insert("TRDA", "Recording dates");
        m.insert("TRSN", "Internet radio station name");
        m.insert("TRSO", "Internet radio station owner");
        m.insert("TSIZ", "Size");
        m.insert("TSO2", "Album artist sort order");
        m.insert("TSOA", "Album sort order");
        m.insert("TSOC", "Composer sort order");
        m.insert("TSOP", "Performer sort order");
        m.insert("TSOT", "Title sort order");
        m.insert("TSRC", "ISRC (international standard recording code)");
        m.insert("TSSE", "Software/Hardware and settings used for encoding");
        m.insert("TYER", "Year");
        m.insert("TSST", "Set subtitle");
        m.insert("TXXX", "User defined text information frame");

        m.insert("UFID", "Unique file identifier");
        m.insert("USER", "Terms of use");
        m.insert("USLT", "Unsynchronised lyric/text transcription");

        m.insert("WCOM", "Commercial information");
        m.insert("WCOP", "Copyright/Legal information");
        m.insert("WOAF", "Official audio file webpage");
        m.insert("WOAR", "Official artist/performer webpage");
        m.insert("WOAS", "Official audio source webpage");
        m.insert("WORS", "Official Internet radio station homepage");
        m.insert("WPAY", "Payment");
        m.insert("WPUB", "Publishers official webpage");
        m.insert("WXXX", "User defined URL link frame");
        m
    };
}

/// Returns a string describing the frame type.
#[inline]
pub fn frame_description(id: &str) -> &'static str {
    match FRAME_DESCRIPTIONS.get(id).map(|t| *t){
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
        assert_eq!(&util::string_from_utf8(&utf8[..]).unwrap()[..], text);

        // should use little endian BOM
        assert_eq!(&util::string_to_utf16(text)[..], b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(&util::string_to_utf16be(text)[..], b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D");
        assert_eq!(&util::string_to_utf16le(text)[..], b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(&util::string_from_encoding(Encoding::UTF16BE, b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap()[..], text);
        assert_eq!(&util::string_from_utf16be(b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap()[..], text);

        assert_eq!(&util::string_from_utf16le(b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap()[..], text);

        // big endian BOM
        assert_eq!(&util::string_from_encoding(Encoding::UTF16, b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap()[..], text);
        assert_eq!(&util::string_from_utf16(b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap()[..], text);

        // little endian BOM 
        assert_eq!(&util::string_from_encoding(Encoding::UTF16, b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap()[..], text);
        assert_eq!(&util::string_from_utf16(b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap()[..], text);
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
}
