use crate::stream::encoding::Encoding;
use crate::{Error, ErrorKind};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::convert::TryInto;

/// Returns a string created from the vector using Latin1 encoding, removing any trailing null
/// bytes.
/// Can never return None because all sequences of u8s are valid Latin1 strings.
pub fn string_from_latin1(data: &[u8]) -> crate::Result<String> {
    let value: String = data
        .iter()
        .take_while(|c| **c != 0)
        .map(|b| *b as char)
        .collect();
    Ok(value)
}

/// Returns a string created from the vector using UTF-16 (with byte order mark) encoding.
/// Returns `None` if the vector is not a valid UTF-16 string.
pub fn string_from_utf16(data: &[u8]) -> crate::Result<String> {
    if data.len() < 2 {
        return Err(Error::new(
            ErrorKind::StringDecoding(data.to_vec()),
            "data is not valid utf16",
        ));
    }

    if data[0] == 0xFF && data[1] == 0xFE {
        // little endian
        string_from_utf16le(&data[2..])
    } else {
        // big endian
        string_from_utf16be(&data[2..])
    }
}

/// Returns a string created from the vector using UTF-16LE encoding.
/// Returns `None` if the vector is not a valid UTF-16LE string.
pub fn string_from_utf16le(data: &[u8]) -> crate::Result<String> {
    let mut data2 = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        let bytes = chunk.try_into().unwrap();
        data2.push(u16::from_le_bytes(bytes));
    }
    String::from_utf16(&data2).map_err(|_| {
        Error::new(
            ErrorKind::StringDecoding(data.to_vec()),
            "data is not valid utf16-le",
        )
    })
}

/// Returns a string created from the vector using UTF-16BE encoding.
/// Returns `None` if the vector is not a valid UTF-16BE string.
pub fn string_from_utf16be(data: &[u8]) -> crate::Result<String> {
    let mut data2 = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        let bytes = chunk.try_into().unwrap();
        data2.push(u16::from_be_bytes(bytes));
    }
    String::from_utf16(&data2).map_err(|_| {
        Error::new(
            ErrorKind::StringDecoding(data.to_vec()),
            "data is not valid utf16-le",
        )
    })
}

/// Returns a Latin1 vector representation of the string.
pub fn string_to_latin1(text: &str) -> Vec<u8> {
    text.chars().map(|c| c as u8).collect()
}

/// Returns a UTF-16 (with native byte order) vector representation of the string.
pub fn string_to_utf16(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + text.len() * 2);
    if cfg!(target_endian = "little") {
        out.extend(&[0xFF, 0xFE]); // add little endian BOM
        out.extend(string_to_utf16le(text));
    } else {
        out.extend(&[0xFE, 0xFF]); // add big endian BOM
        out.extend(string_to_utf16be(text));
    }
    out
}

/// Returns a UTF-16BE vector representation of the string.
pub fn string_to_utf16be(text: &str) -> Vec<u8> {
    let encoder = text.encode_utf16();
    let size_hint = encoder.size_hint();

    let mut out = Vec::with_capacity(size_hint.1.unwrap_or(size_hint.0) * 2);
    for encoded_char in encoder {
        out.extend_from_slice(&encoded_char.to_be_bytes());
    }
    out
}

/// Returns a UTF-16LE vector representation of the string.
pub fn string_to_utf16le(text: &str) -> Vec<u8> {
    let encoder = text.encode_utf16();
    let size_hint = encoder.size_hint();

    let mut out = Vec::with_capacity(size_hint.1.unwrap_or(size_hint.0) * 2);
    for encoded_char in encoder {
        out.extend_from_slice(&encoded_char.to_le_bytes());
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

            if i == data.len() {
                // delimiter was not found
                return None;
            }

            Some(i)
        }
        Encoding::UTF16 | Encoding::UTF16BE => {
            while i + 1 < data.len() && (data[i] != 0 || data[i + 1] != 0) {
                i += 2;
            }

            if i + 1 >= data.len() {
                // delimiter was not found
                return None;
            }

            Some(i)
        }
    }
}

/// Returns the delimiter length for the specified encoding.
pub fn delim_len(encoding: Encoding) -> usize {
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => 1,
        Encoding::UTF16 | Encoding::UTF16BE => 2,
    }
}

lazy_static! {
    static ref ID_2_TO_3: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("BUF", "RBUF");

        m.insert("CNT", "PCNT");
        m.insert("COM", "COMM");
        m.insert("CRA", "AENC");
        // "CRM" does not exist in ID3v2.3

        m.insert("ETC", "ETCO");
        m.insert("EQU", "EQUA");

        m.insert("GEO", "GEOB");

        m.insert("IPL", "IPLS");

        m.insert("LNK", "LINK");

        m.insert("MCI", "MCDI");
        m.insert("MLL", "MLLT");

        m.insert("PIC", "APIC");
        m.insert("POP", "POPM");

        m.insert("REV", "RVRB");
        m.insert("RVA", "RVA2");

        m.insert("SLT", "SYLT");
        m.insert("STC", "SYTC");

        m.insert("TAL", "TALB");
        m.insert("TBP", "TBPM");
        m.insert("TCM", "TCOM");
        m.insert("TCO", "TCON");
        m.insert("TCR", "TCOP");
        m.insert("TDA", "TDAT");
        m.insert("TDY", "TDLY");
        m.insert("TEN", "TENC");
        m.insert("TFT", "TFLT");
        m.insert("TIM", "TIME");
        m.insert("TKE", "TKEY");
        m.insert("TLA", "TLAN");
        m.insert("TLE", "TLEN");
        m.insert("TMT", "TMED");
        m.insert("TOA", "TOPE");
        m.insert("TOF", "TOFN");
        m.insert("TOL", "TOLY");
        m.insert("TOT", "TOAL");
        m.insert("TOR", "TORY");
        m.insert("TP1", "TPE1");
        m.insert("TP2", "TPE2");
        m.insert("TP3", "TPE3");
        m.insert("TP4", "TPE4");
        m.insert("TPA", "TPOS");
        m.insert("TPB", "TPUB");
        m.insert("TRC", "TSRC");
        m.insert("TRD", "TRDA");
        m.insert("TRK", "TRCK");
        m.insert("TSI", "TSIZ");
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

    static ref ID_3_TO_2: HashMap<&'static str, &'static str> = ID_2_TO_3.iter()
        .map(|(k, v)| (*v, *k))
        .collect();
}

/// Returns the coresponding ID3v2.3/ID3v2.4 ID given the ID3v2.2 ID.
pub fn convert_id_2_to_3(id: &str) -> Option<&'static str> {
    ID_2_TO_3.get(id).copied()
}

/// Returns the coresponding ID3v2.2 ID given the ID3v2.3/ID3v2.3 ID.
pub fn convert_id_3_to_2(id: &str) -> Option<&'static str> {
    ID_3_TO_2.get(id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strings() {
        let text: &str = "śốмễ śŧŗỉňĝ";

        let mut utf8 = text.as_bytes().to_vec();
        utf8.push(0);

        // should use little endian BOM
        assert_eq!(&string_to_utf16(text)[..], b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(&string_to_utf16be(text)[..], b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D");
        assert_eq!(&string_to_utf16le(text)[..], b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01");

        assert_eq!(&string_from_utf16be(b"\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap()[..], text);

        assert_eq!(&string_from_utf16le(b"\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap()[..], text);

        // big endian BOM
        assert_eq!(&string_from_utf16(b"\xFE\xFF\x01\x5B\x1E\xD1\x04\x3C\x1E\xC5\x00\x20\x01\x5B\x01\x67\x01\x57\x1E\xC9\x01\x48\x01\x1D").unwrap()[..], text);

        // little endian BOM
        assert_eq!(&string_from_utf16(b"\xFF\xFE\x5B\x01\xD1\x1E\x3C\x04\xC5\x1E\x20\x00\x5B\x01\x67\x01\x57\x01\xC9\x1E\x48\x01\x1D\x01").unwrap()[..], text);
    }

    #[test]
    fn test_latin1() {
        let text: &str = "stringþ";
        assert_eq!(&string_to_latin1(text)[..], b"string\xFE");
        assert_eq!(&string_from_latin1(b"string\xFE").unwrap()[..], text);
    }

    #[test]
    fn test_find_delim() {
        assert_eq!(
            find_delim(Encoding::UTF8, &[0x0, 0xFF, 0xFF, 0xFF, 0x0], 3).unwrap(),
            4
        );
        assert!(find_delim(Encoding::UTF8, &[0x0, 0xFF, 0xFF, 0xFF, 0xFF], 3).is_none());

        assert_eq!(
            find_delim(
                Encoding::UTF16,
                &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF],
                2
            )
            .unwrap(),
            4
        );
        assert!(find_delim(
            Encoding::UTF16,
            &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF],
            2
        )
        .is_none());

        assert_eq!(
            find_delim(
                Encoding::UTF16BE,
                &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF],
                2
            )
            .unwrap(),
            4
        );
        assert!(find_delim(
            Encoding::UTF16BE,
            &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF],
            2
        )
        .is_none());
    }
}
