extern crate num;
use num::FromPrimitive;

/// Types of text encodings used in ID3 frames.
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Encoding {
    /// ISO-8859-1 text encoding, also referred to as latin1 encoding.
    Latin1,
    /// UTF-16 text encoding with a byte order mark.
    UTF16,
    /// UTF-16BE text encoding without a byte order mark. This encoding is only used in id3v2.4.
    UTF16BE,
    /// UTF-8 text encoding. This encoding is only used in id3v2.4.
    UTF8 
}

impl FromPrimitive for Encoding {
    fn from_i64(n: i64) -> Option<Encoding> {
        FromPrimitive::from_u64(n as u64)
    }

    fn from_u64(n: u64) -> Option<Encoding> {
        match n {
            0 => Some(Encoding::Latin1),
            1 => Some(Encoding::UTF16),
            2 => Some(Encoding::UTF16BE),
            3 => Some(Encoding::UTF8),
            _ => None,
        }
    }
}
