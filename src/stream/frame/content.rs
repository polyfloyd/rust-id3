use std::io;
use std::iter;
use ::frame::{Picture, PictureType, Content, ExtendedLink};
use ::stream::encoding::Encoding;
use ::tag;

/// The result of a successfully parsed frame.
pub struct DecoderResult {
    /// The text encoding used in the frame.
    pub encoding: Encoding,
    /// The parsed content of the frame.
    pub content: Content
}

impl DecoderResult {
    /// Creates a new `DecoderResult` with the provided encoding and content.
    pub fn new(encoding: Encoding, content: Content) -> DecoderResult {
        DecoderResult { encoding, content }
    }
}

#[derive(Copy, Clone)]
struct EncoderRequest<'a> {
    version: tag::Version,
    encoding: Encoding,
    content: &'a Content
}

/// Creates a vector representation of the request.
pub fn encode<W>(mut writer: W, content: &Content, version: tag::Version, encoding: Encoding) -> ::Result<usize>
    where W: io::Write {
    let request = EncoderRequest {
        version,
        encoding,
        content,
    };
    let bytes = match *content {
        Content::Text(_) => text_to_bytes(request),
        Content::ExtendedText(_) => extended_text_to_bytes(request),
        Content::Link(_) => weblink_to_bytes(request),
        Content::ExtendedLink(_) => extended_weblink_to_bytes(request),
        Content::Lyrics(_) => lyrics_to_bytes(request),
        Content::Comment(_) => comment_to_bytes(request),
        Content::Picture(_) => picture_to_bytes(request),
        Content::Unknown(ref data) => data.clone()
    };
    writer.write_all(&bytes)?;
    Ok(bytes.len())
}

/// Attempts to decode the request.
pub fn decode<R>(id: &str, mut reader: R) -> ::Result<DecoderResult>
    where R: io::Read {
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    match id {
        "APIC" => parse_apic_v3(data.as_slice()),
        "PIC" => parse_apic_v2(data.as_slice()),
        "TXXX" | "TXX" => parse_txxx(data.as_slice()),
        "WXXX" | "WXX" => parse_wxxx(data.as_slice()),
        "COMM" | "COM" => parse_comm(data.as_slice()),
        "USLT" | "ULT" => parse_uslt(data.as_slice()),
        id if id.starts_with('T') => parse_text(data.as_slice()),
        id if id.starts_with('W') => parse_weblink(data.as_slice()),
        _ => Ok(DecoderResult::new(Encoding::UTF16, Content::Unknown(data))),
    }
}

// Encoders {{{
struct EncodingParams<'a> {
    delim_len: u8,
    string_func: Box<Fn(&mut Vec<u8>, &str) + 'a>
}

macro_rules! encode_part {
    ($buf:ident, encoding($encoding:expr)) => { $buf.push($encoding as u8) };
    ($buf:ident, $params:ident, string($string:expr)) => { ($params.string_func)(&mut $buf, &$string[..]) };
    ($buf:ident, $params:ident, delim($ignored:expr)) => { for _ in 0..$params.delim_len { $buf.push(0); } };
    ($buf:ident, $params:ident, bytes($bytes:expr)) => { $buf.extend($bytes.iter().cloned()); };
    ($buf:ident, $params:ident, byte($byte:expr)) => { $buf.push($byte as u8) };
}

macro_rules! encode {
    (encoding($encoding:expr) $(, $part:ident( $value:expr ) )+) => {
        {
            let params = match $encoding {
                Encoding::Latin1 => EncodingParams {
                    delim_len: 1,
                    string_func: Box::new(|buf: &mut Vec<u8>, string: &str|
                        buf.extend(::util::string_to_latin1(string).into_iter())
                    )
                },
                Encoding::UTF8 => EncodingParams {
                    delim_len: 1,
                    string_func: Box::new(|buf: &mut Vec<u8>, string: &str|
                        buf.extend(string.bytes()))
                },
                Encoding::UTF16 => EncodingParams {
                    delim_len: 2,
                    string_func: Box::new(|buf: &mut Vec<u8>, string: &str|
                        buf.extend(::util::string_to_utf16(string).into_iter()))
                },
                Encoding::UTF16BE => EncodingParams {
                    delim_len: 2,
                    string_func: Box::new(|buf: &mut Vec<u8>, string: &str|
                        buf.extend(::util::string_to_utf16be(string).into_iter()))
                }
            };
            let mut buf = Vec::new();
            encode_part!(buf, encoding($encoding));
            $(encode_part!(buf, params, $part ( $value ));)+
            buf
        }
    };
}

fn text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.text().unwrap();
    return encode!(encoding(request.encoding), string(content));
}

fn extended_text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_text().unwrap();
    return encode!(encoding(request.encoding), string(content.description), delim(0), string(content.value));
}

fn weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    request.content.link().unwrap().as_bytes().to_vec()
}

fn extended_weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_link().unwrap();
    return encode!(encoding(request.encoding), string(content.description), delim(0),
                   bytes(content.link.as_bytes()));
}

fn lyrics_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.lyrics().unwrap();
    return encode!(encoding(request.encoding),
                   bytes(content.lang.bytes().chain(iter::repeat(b' ')).take(3).collect::<Vec<u8>>()),
                   string(content.description), delim(0), string(content.text));
}

fn comment_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.comment().unwrap();
    return encode!(encoding(request.encoding),
                   bytes(content.lang.bytes().chain(iter::repeat(b' ')).take(3).collect::<Vec<u8>>()),
                   string(content.description), delim(0), string(content.text));
}

fn picture_to_bytes_v3(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.picture().unwrap();
    return encode!(encoding(request.encoding), bytes(content.mime_type.as_bytes()), byte(0),
            byte(content.picture_type), string(content.description), delim(0), bytes(content.data));
}

fn picture_to_bytes_v2(request: EncoderRequest) -> Vec<u8> {
    let picture = request.content.picture().unwrap();

    let format = match &picture.mime_type[..] {
        "image/jpeg" => "JPG",
        "image/png" => "PNG",
        _ => panic!("unknown MIME type") // TODO handle this better. Return None?
    };

    return encode!(encoding(request.encoding), bytes(format.as_bytes()), byte(picture.picture_type),
            string(picture.description), delim(0), bytes(picture.data));
}

fn picture_to_bytes(request: EncoderRequest) -> Vec<u8> {
    match request.version {
        tag::Id3v22 => picture_to_bytes_v2(request),
        tag::Id3v23|tag::Id3v24 => picture_to_bytes_v3(request),
    }
}
// }}}

// Decoders {{{
struct DecodingParams<'a> {
    encoding: Encoding,
    string_func: Box<Fn(&[u8]) -> ::Result<String> + 'a>
}

impl<'a> DecodingParams<'a> {
    fn for_encoding(encoding: Encoding) -> DecodingParams<'a> {
        match encoding {
                Encoding::Latin1 => DecodingParams {
                encoding: Encoding::Latin1,
                string_func: Box::new(|bytes: &[u8]| -> ::Result<String> {
                    ::util::string_from_latin1(bytes)
                })
            },
            Encoding::UTF8 => DecodingParams {
                encoding: Encoding::UTF8,
                string_func: Box::new(|bytes: &[u8]| -> ::Result<String> {
                    Ok(String::from_utf8(bytes.to_vec())?)
                })
            },
            Encoding::UTF16 => DecodingParams {
                encoding: Encoding::UTF16,
                string_func: Box::new(|bytes: &[u8]| -> ::Result<String> {
                    ::util::string_from_utf16(bytes)
                })
            },
            Encoding::UTF16BE => DecodingParams {
                encoding: Encoding::UTF16BE,
                string_func: Box::new(|bytes: &[u8]| -> ::Result<String> {
                    ::util::string_from_utf16be(bytes)
                })
            }
        }
    }
}

fn encoding_from_byte(n: u8) -> ::Result<Encoding> {
    match n {
        0 => Ok(Encoding::Latin1),
        1 => Ok(Encoding::UTF16),
        2 => Ok(Encoding::UTF16BE),
        3 => Ok(Encoding::UTF8),
        _ => Err(::Error::new(::ErrorKind::Parsing, "unknown encoding"))
    }
}

macro_rules! assert_data {
    ($bytes:ident) => {
        if $bytes.len() == 0 {
            return Err(::Error::new(::ErrorKind::Parsing, "frame does not contain any data"))
        }
    }
}

fn find_delim(bytes: &[u8], encoding: Encoding, i: usize, terminated: bool) -> Result<(usize, usize), ::Error> {
    if !terminated {
        return Ok((bytes.len(), bytes.len()));
    }
    let delim = ::util::find_delim(encoding, bytes, i)
        .ok_or_else(|| ::Error::new(::ErrorKind::Parsing, "delimiter not found"))?;
    Ok((delim, delim + ::util::delim_len(encoding)))
}

macro_rules! decode_part {
    ($bytes:ident, $params:ident, $i:ident, string($terminated:expr)) => {
        {
            let start = $i;
            let (end, with_delim) = find_delim($bytes, $params.encoding, $i, $terminated)?;
            $i = with_delim;

            if start == end {
                "".to_string()
            } else {
                ($params.string_func)(&$bytes[start..end])?
            }
        }
    };
    ($bytes: ident, $params:ident, $i:ident, text()) => {
        {
            let start = $i;
            let (end, with_delim) = match ::util::find_delim($params.encoding, $bytes, $i) {
                Some(i) => (i, i + ::util::delim_len($params.encoding)),
                None => ($bytes.len(), $bytes.len()),
            };
            $i = with_delim;

            if start == end {
                "".to_string()
            } else {
                ($params.string_func)(&$bytes[start..end])?
            }
        }
    };
    ($bytes:ident, $params:ident, $i:ident, fixed_string($len:expr)) => {
        {
            if $i + $len >= $bytes.len() {
                return Err(::Error::new(::ErrorKind::Parsing, "insufficient data"));
            }

            let start = $i;
            $i += $len;
            ::util::string_from_latin1(&$bytes[start..$i])?
        }
    };
    ($bytes:ident, $params:ident, $i:ident, latin1($terminated:expr)) => {
        {
            let start = $i;
            let (end, with_delim) = find_delim($bytes, Encoding::Latin1, $i, $terminated)?;
            $i = with_delim;
            String::from_utf8($bytes[start..end].to_vec())?
        }
    };
    ($bytes:ident, $params:ident, $i:ident, picture_type()) => {
        {
            if $i + 1 >= $bytes.len() {
                return Err(::Error::new(::ErrorKind::Parsing, "insufficient data"));
            }

            let start = $i;
            $i += 1;

            let picture_type = match $bytes[start] {
                0 => Some(PictureType::Other),
                1 => Some(PictureType::Icon),
                2 => Some(PictureType::OtherIcon),
                3 => Some(PictureType::CoverFront),
                4 => Some(PictureType::CoverBack),
                5 => Some(PictureType::Leaflet),
                6 => Some(PictureType::Media),
                7 => Some(PictureType::LeadArtist),
                8 => Some(PictureType::Artist),
                9 => Some(PictureType::Conductor),
                10 => Some(PictureType::Band),
                11 => Some(PictureType::Composer),
                12 => Some(PictureType::Lyricist),
                13 => Some(PictureType::RecordingLocation),
                14 => Some(PictureType::DuringRecording),
                15 => Some(PictureType::DuringPerformance),
                16 => Some(PictureType::ScreenCapture),
                17 => Some(PictureType::BrightFish),
                18 => Some(PictureType::Illustration),
                19 => Some(PictureType::BandLogo),
                20 => Some(PictureType::PublisherLogo),
                _ => None,
            };
            match picture_type {
                Some(t) => t,
                None => return Err(::Error::new(::ErrorKind::Parsing, "invalid picture type"))
            }
        }
    };
    ($bytes:ident, $params:ident, $i:ident, bytes()) => {
        {
            let start = $i;
            $i = $bytes.len(); Some(&$i);
            $bytes[start..].to_vec()
        }
    };
}

macro_rules! decode {
    ($bytes:ident, $result_type:ident, $($field:ident : $part:ident ( $($params:tt)* ) ),+) => {
        {
            use frame::$result_type;

            assert_data!($bytes);

            let encoding = encoding_from_byte($bytes[0])?;

            let params = DecodingParams::for_encoding(encoding);

            let mut i = 1;
            Ok(DecoderResult {
                encoding: encoding,
                content: Content::$result_type( $result_type {
                    $($field: decode_part!($bytes, params, i, $part ( $($params)* ) ),)+
                })
            })
        }
    };
}

/// Attempts to parse the data as an ID3v2.2 picture frame.
/// Returns a `Content::Picture`.
fn parse_apic_v2(data: &[u8]) -> ::Result<DecoderResult> {
    assert_data!(data);

    let encoding = encoding_from_byte(data[0])?;

    let params = DecodingParams::for_encoding(encoding);

    let mut i = 1;
    let format = decode_part!(data, params, i, fixed_string(3));
    let mime_type = match &format[..] {
        "PNG" => "image/png".to_string(),
        "JPG" => "image/jpeg".to_string(),
        _ => {
            return Err(::Error::new(::ErrorKind::UnsupportedFeature,
                                     "can't determine MIME type for image format"))
        }
    };

    let picture_type = decode_part!(data, params, i, picture_type());
    let description = decode_part!(data, params, i, string(true));
    let picture_data = decode_part!(data, params, i, bytes());

    let picture = Picture {
        mime_type,
        picture_type,
        description,
        data: picture_data,
    };
    Ok(DecoderResult::new(encoding, Content::Picture(picture)))
}


/// Attempts to parse the data as an ID3v2.3/ID3v2.4 picture frame.
/// Returns a `Content::Picture`.
fn parse_apic_v3(data: &[u8]) -> ::Result<DecoderResult> {
    return decode!(data, Picture, mime_type: latin1(true), picture_type : picture_type(),
                   description: string(true), data: bytes());
}

/// Attempts to parse the data as a comment frame.
/// Returns a `Content::Comment`.
fn parse_comm(data: &[u8]) -> ::Result<DecoderResult> {
    return decode!(data, Comment, lang: fixed_string(3), description: string(true),
                   text: string(false));
}

/// Attempts to parse the data as a text frame.
/// Returns a `Content::Text`.
fn parse_text(data: &[u8]) -> ::Result<DecoderResult> {
    assert_data!(data);
    let encoding = encoding_from_byte(data[0])?;

    let params = DecodingParams::for_encoding(encoding);
    let mut i = 1;
    Ok(DecoderResult::new(encoding, Content::Text(decode_part!(data, params, i, text()))))
}

/// Attempts to parse the data as a user defined text frame.
/// Returns an `Content::ExtendedText`.
fn parse_txxx(data: &[u8]) -> ::Result<DecoderResult> {
    return decode!(data, ExtendedText, description: string(true), value: string(false));
}

/// Attempts to parse the data as a web link frame.
/// Returns a `Content::Link`.
fn parse_weblink(data: &[u8]) -> ::Result<DecoderResult> {
    Ok(DecoderResult::new(Encoding::Latin1, Content::Link(String::from_utf8(data.to_vec())?)))
}

/// Attempts to parse the data as a user defined web link frame.
/// Returns an `Content::ExtendedLink`.
fn parse_wxxx(data: &[u8]) -> ::Result<DecoderResult> {
    assert_data!(data);

    let encoding = encoding_from_byte(data[0])?;

    let params = DecodingParams::for_encoding(encoding);
    let mut i = 1;
    let description = decode_part!(data, params, i, string(true));

    let uparams = DecodingParams::for_encoding(Encoding::Latin1);
    let link = decode_part!(data, uparams, i, string(false));

    let elink = ExtendedLink {description, link};
    Ok(DecoderResult::new(encoding, Content::ExtendedLink(elink)))
}

/// Attempts to parse the data as an unsynchronized lyrics text frame.
/// Returns a `Content::Lyrics`.
fn parse_uslt(data: &[u8]) -> ::Result<DecoderResult> {
    return decode!(data, Lyrics, lang: fixed_string(3), description: string(true),
                   text: string(false));
}
// }}}

// Tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use frame::{self, Picture, PictureType};
    use frame::Content;
    use std::collections::HashMap;

    fn bytes_for_encoding(text: &str, encoding: Encoding) -> Vec<u8> {
        match encoding {
            //string.chars().map(|c| c as u8)
            Encoding::Latin1 => text.chars().map(|c| c as u8).collect(),
            Encoding::UTF8 => text.as_bytes().to_vec(),
            Encoding::UTF16 => ::util::string_to_utf16(text),
            Encoding::UTF16BE => ::util::string_to_utf16be(text)
        }
    }

    fn delim_for_encoding(encoding: Encoding) -> Vec<u8> {
        match encoding {
            Encoding::Latin1 | Encoding::UTF8 => vec!(0),
            Encoding::UTF16 | Encoding::UTF16BE => vec!(0, 0)
        }
    }

    #[test]
    fn test_apic_v2() {
        assert!(decode("PIC",&[][..]).is_err());

        let mut format_map = HashMap::new();
        format_map.insert("image/jpeg", "JPG");
        format_map.insert("image/png", "PNG");

        for (mime_type, format) in format_map {
            for description in &["", "description"] {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec!(0xF9, 0x90, 0x3A, 0x02, 0xBD);
                let picture = Picture {
                    mime_type: mime_type.to_string(),
                    picture_type,
                    description: description.to_string(),
                    data: picture_data.clone(),
                };

                for encoding in &[Encoding::Latin1, Encoding::UTF16] {
                    println!("`{}`, `{}`, `{:?}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(format.bytes());
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(picture_data.iter().cloned());

                    assert_eq!(*decode("PIC", &data[..]).unwrap().content.picture().unwrap(), picture);
                    let mut data_out = Vec::new();
                    encode(&mut data_out, &Content::Picture(picture.clone()), tag::Id3v22, *encoding).unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }
    }

    #[test]
    fn test_apic_v3() {
        assert!(decode("APIC", &[][..]).is_err());

        for mime_type in &["", "image/jpeg"] {
            for description in &["", "description"] {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec!(0xF9, 0x90, 0x3A, 0x02, 0xBD);
                let picture = Picture {
                    mime_type: mime_type.to_string(),
                    picture_type: picture_type,
                    description: description.to_string(),
                    data: picture_data.clone(),
                };

                for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
                    println!("`{}`, `{}`, `{:?}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(mime_type.bytes());
                    data.push(0x0);
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(picture_data.iter().cloned());

                    assert_eq!(*decode("APIC", &data[..]).unwrap().content.picture().unwrap(), picture);
                    let mut data_out = Vec::new();
                    encode(&mut data_out, &Content::Picture(picture.clone()), tag::Id3v23, *encoding).unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }
    }

    #[test]
    fn test_comm() {
        assert!(decode("COMM", &[][..]).is_err());

        println!("valid");
        for description in &["", "description"] {
            for comment in &["", "comment"] {
                for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
                    println!("`{}`, `{}`, `{:?}`", description, comment, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(b"eng".iter().cloned());
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(comment, *encoding).into_iter());

                    let content = frame::Comment {
                        lang: "eng".to_string(),
                        description: description.to_string(),
                        text: comment.to_string()
                    };
                    assert_eq!(*decode("COMM", &data[..]).unwrap().content.comment().unwrap(), content);
                    let mut data_out = Vec::new();
                    encode(&mut data_out, &Content::Comment(content), tag::Id3v23, *encoding).unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let comment = "comment";
        for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(bytes_for_encoding(description, *encoding).into_iter());
            data.extend(bytes_for_encoding(comment, *encoding).into_iter());
            assert!(decode("COMM", &data[..]).is_err());
        }
        println!("Empty description");
        let comment = "comment";
        for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(delim_for_encoding(*encoding));
            data.extend(bytes_for_encoding(comment, *encoding).into_iter());
            let content = frame::Comment {
                lang: "eng".to_string(),
                description: "".to_string(),
                text: comment.to_string()
            };
            println!("data == {:?}", data);
            println!("content == {:?}", content);
            assert_eq!(*decode("COMM", &data[..]).unwrap().content.comment().unwrap(), content);
        }
    }

    #[test]
    fn test_text() {
        assert!(decode("TALB", &[][..]).is_err());

        for text in &["", "text"] {
            for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
                println!("`{}`, `{:?}`", text, *encoding);
                let mut data = Vec::new();
                data.push(*encoding as u8);
                data.extend(bytes_for_encoding(text, *encoding).into_iter());

                assert_eq!(decode("TALB", &data[..]).unwrap().content.text().unwrap(), *text);
                let mut data_out = Vec::new();
                encode(&mut data_out, &Content::Text(text.to_string()), tag::Id3v23, *encoding).unwrap();
                assert_eq!(data, data_out);
            }
        }
    }

    #[test]
    fn test_null_terminated_text() {
        assert!(decode("TRCK", &[][..]).is_err());
        let text = "text\u{0}\u{0}";
        for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
            println!("`{}`, `{:?}`", text, encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(text, *encoding).into_iter());

            assert_eq!(decode("TALB", &data[..]).unwrap().content.text().unwrap(), "text");
            let mut data_out = Vec::new();
            encode(&mut data_out, &Content::Text(text.to_string()), tag::Id3v23, *encoding).unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_txxx() {
        assert!(decode("TXXX", &[][..]).is_err());

        println!("valid");
        for key in &["", "key"] {
            for value in &["", "value"] {
                for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
                    println!("{:?}", encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(bytes_for_encoding(key, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(value, *encoding).into_iter());

                    let content = frame::ExtendedText {
                        description: key.to_string(),
                        value: value.to_string()
                    };
                    assert_eq!(*decode("TXXX", &data[..]).unwrap().content.extended_text().unwrap(), content);
                    let mut data_out = Vec::new();
                    encode(&mut data_out, &Content::ExtendedText(content), tag::Id3v23, *encoding).unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let key = "key";
        let value = "value";
        for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(key, *encoding).into_iter());
            data.extend(bytes_for_encoding(value, *encoding).into_iter());
            assert!(decode("TXXX", &data[..]).is_err());
        }
    }

    #[test]
    fn test_weblink() {
        for link in &["", "http://www.rust-lang.org/"] {
            println!("`{:?}`", link);
            let data = link.as_bytes().to_vec();

            assert_eq!(decode("WOAF", &data[..]).unwrap().content.link().unwrap(), *link);
            let mut data_out = Vec::new();
            encode(&mut data_out, &Content::Link(link.to_string()), tag::Id3v23, Encoding::Latin1).unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_wxxx() {
        assert!(decode("WXXX", &[][..]).is_err());

        println!("valid");
        for description in &["", "rust"] {
            for link in &["", "http://www.rust-lang.org/"] {
                for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
                    println!("`{}`, `{}`, `{:?}`", description, link, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(link, Encoding::Latin1).into_iter());

                    let content = frame::ExtendedLink {
                        description: description.to_string(),
                        link: link.to_string()
                    };
                    assert_eq!(*decode("WXXX", &data[..]).unwrap().content.extended_link().unwrap(), content);
                    let mut data_out = Vec::new();
                    encode(&mut data_out, &Content::ExtendedLink(content), tag::Id3v23, *encoding).unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let description = "rust";
        let link = "http://www.rust-lang.org/";
        for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(description, *encoding).into_iter());
            data.extend(bytes_for_encoding(link, Encoding::Latin1).into_iter());
            assert!(decode("WXXX", &data[..]).is_err());
        }
    }

    #[test]
    fn test_uslt() {
        assert!(decode("USLT", &[][..]).is_err());

        println!("valid");
        for description in &["", "description"] {
            for text in &["", "lyrics"] {
                for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
                    println!("`{}`, `{}, `{:?}`", description, text, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(b"eng".iter().cloned());
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(text, *encoding).into_iter());

                    let content = frame::Lyrics {
                        lang: "eng".to_string(),
                        description: description.to_string(),
                        text: text.to_string(),
                    };
                    assert_eq!(*decode("USLT", &data[..]).unwrap().content.lyrics().unwrap(), content);
                    let mut data_out = Vec::new();
                    encode(&mut data_out, &Content::Lyrics(content), tag::Id3v23, *encoding).unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let lyrics = "lyrics";
        for encoding in &[Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(bytes_for_encoding(description, *encoding).into_iter());
            data.extend(bytes_for_encoding(lyrics, *encoding).into_iter());
            assert!(decode("USLT", &data[..]).is_err());
        }
    }
}
// }}}
