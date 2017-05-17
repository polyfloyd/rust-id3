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
    UTF8
}
