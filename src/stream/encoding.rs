use crate::{Error, ErrorKind};
use std::convert::TryInto;

/// Types of text encodings used in ID3 frames.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// ISO-8859-1 text encoding, also referred to as latin1 encoding.
    Latin1,
    /// UTF-16 text encoding with a byte order mark.
    UTF16,
    /// UTF-16BE text encoding without a byte order mark. This encoding is only used in id3v2.4.
    UTF16BE,
    /// UTF-8 text encoding. This encoding is only used in id3v2.4.
    UTF8,
}

impl Encoding {
    pub(crate) fn decode(&self, bytes: impl AsRef<[u8]>) -> crate::Result<String> {
        let bytes = bytes.as_ref();
        if bytes.is_empty() {
            // UTF16 decoding requires at least 2 bytes for it not to error.
            return Ok("".to_string());
        }
        match self {
            Encoding::Latin1 => Ok(string_from_latin1(bytes)),
            Encoding::UTF8 => Ok(String::from_utf8(bytes.to_vec())?),
            Encoding::UTF16 => string_from_utf16(bytes),
            Encoding::UTF16BE => string_from_utf16be(bytes),
        }
    }

    pub(crate) fn encode<'a>(&self, string: impl AsRef<str> + 'a) -> Vec<u8> {
        let string = string.as_ref();
        match self {
            Encoding::Latin1 => string_to_latin1(string),
            Encoding::UTF8 => string.as_bytes().to_vec(),
            Encoding::UTF16 => string_to_utf16(string),
            Encoding::UTF16BE => string_to_utf16be(string),
        }
    }
}

/// Returns a string created from the vector using Latin1 encoding.
/// Can never return None because all sequences of u8s are valid Latin1 strings.
fn string_from_latin1(data: &[u8]) -> String {
    data.iter().map(|b| *b as char).collect()
}

/// Returns a string created from the vector using UTF-16 (with byte order mark) encoding.
fn string_from_utf16(data: &[u8]) -> crate::Result<String> {
    if data.len() < 2 {
        return Err(Error::new(
            ErrorKind::StringDecoding(data.to_vec()),
            "data is not valid utf16",
        ));
    }
    if data[0] == 0xFF && data[1] == 0xFE {
        string_from_utf16le(&data[2..])
    } else {
        string_from_utf16be(&data[2..])
    }
}

fn string_from_utf16le(data: &[u8]) -> crate::Result<String> {
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

fn string_from_utf16be(data: &[u8]) -> crate::Result<String> {
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

fn string_to_latin1(text: &str) -> Vec<u8> {
    text.chars().map(|c| c as u8).collect()
}

/// Returns a UTF-16 (with native byte order) vector representation of the string.
fn string_to_utf16(text: &str) -> Vec<u8> {
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

fn string_to_utf16be(text: &str) -> Vec<u8> {
    let encoder = text.encode_utf16();
    let size_hint = encoder.size_hint();

    let mut out = Vec::with_capacity(size_hint.1.unwrap_or(size_hint.0) * 2);
    for encoded_char in encoder {
        out.extend_from_slice(&encoded_char.to_be_bytes());
    }
    out
}

fn string_to_utf16le(text: &str) -> Vec<u8> {
    let encoder = text.encode_utf16();
    let size_hint = encoder.size_hint();

    let mut out = Vec::with_capacity(size_hint.1.unwrap_or(size_hint.0) * 2);
    for encoded_char in encoder {
        out.extend_from_slice(&encoded_char.to_le_bytes());
    }
    out
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
        assert_eq!(&string_from_latin1(b"string\xFE")[..], text);
    }
}
