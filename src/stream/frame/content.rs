// This lint is not really fixable without a lot of effort due to all the macros being used here.
#![allow(clippy::vec_init_then_push)]

use crate::frame::{
    Content, EncapsulatedObject, ExtendedLink, Picture, PictureType, SynchronisedLyrics,
    SynchronisedLyricsType, TimestampFormat,
};
use crate::stream::encoding::Encoding;
use crate::tag;
use crate::util::{
    delim_len, string_from_latin1, string_from_utf16, string_from_utf16be, string_to_latin1,
    string_to_utf16, string_to_utf16be,
};
use crate::{Error, ErrorKind};
use std::io;
use std::iter;

#[derive(Copy, Clone)]
struct EncoderRequest<'a> {
    version: tag::Version,
    encoding: Encoding,
    content: &'a Content,
}

/// Creates a vector representation of the request.
pub fn encode(
    mut writer: impl io::Write,
    content: &Content,
    version: tag::Version,
    encoding: Encoding,
) -> crate::Result<usize> {
    let request = EncoderRequest {
        version,
        encoding,
        content,
    };
    let bytes = match content {
        Content::Text(_) => text_to_bytes(request),
        Content::ExtendedText(_) => extended_text_to_bytes(request),
        Content::Link(_) => weblink_to_bytes(request),
        Content::ExtendedLink(_) => extended_weblink_to_bytes(request),
        Content::EncapsulatedObject(_) => encapsulated_object_to_bytes(request),
        Content::Lyrics(_) => lyrics_to_bytes(request),
        Content::SynchronisedLyrics(_) => synchronised_lyrics_to_bytes(request),
        Content::Comment(_) => comment_to_bytes(request),
        Content::Picture(_) => picture_to_bytes(request)?,
        Content::Unknown(data) => data.clone(),
    };
    writer.write_all(&bytes)?;
    Ok(bytes.len())
}

/// Attempts to decode the request.
pub fn decode(
    id: &str,
    version: tag::Version,
    mut reader: impl io::Read,
) -> crate::Result<Content> {
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    match id {
        "APIC" => parse_apic_v3(data.as_slice()),
        "PIC" => parse_apic_v2(data.as_slice()),
        "TXXX" | "TXX" => parse_txxx(data.as_slice()),
        "WXXX" | "WXX" => parse_wxxx(data.as_slice()),
        "COMM" | "COM" => parse_comm(data.as_slice()),
        "USLT" | "ULT" => parse_uslt(data.as_slice()),
        "SYLT" | "SLT" => parse_sylt(data.as_slice()),
        "GEOB" | "GEO" => parse_geob(data.as_slice()),
        id if id.starts_with('T') => parse_text(data.as_slice(), version),
        id if id.starts_with('W') => parse_weblink(data.as_slice()),
        "GRP1" => parse_text(data.as_slice(), version),
        _ => Ok(Content::Unknown(data)),
    }
}

struct EncodingParams<'a> {
    delim_len: u8,
    string_func: Box<dyn Fn(&mut Vec<u8>, &str) + 'a>,
}

macro_rules! encode_part {
    ($buf:ident, encoding($encoding:expr)) => {
        $buf.push($encoding as u8)
    };
    ($buf:ident, $params:ident, string($string:expr)) => {
        ($params.string_func)(&mut $buf, &$string[..])
    };
    ($buf:ident, $params:ident, delim($ignored:expr)) => {
        for _ in 0..$params.delim_len {
            $buf.push(0);
        }
    };
    ($buf:ident, $params:ident, bytes($bytes:expr)) => {
        $buf.extend($bytes.iter().cloned());
    };
    ($buf:ident, $params:ident, byte($byte:expr)) => {
        $buf.push($byte as u8)
    };
}

macro_rules! encode {
    (encoding($encoding:expr) $(, $part:ident( $value:expr ) )+) => {
        {
            let params = match $encoding {
                Encoding::Latin1 => EncodingParams {
                    delim_len: 1,
                    string_func: Box::new(|buf: &mut Vec<u8>, string: &str|
                        buf.extend(string_to_latin1(string).into_iter())
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
                        buf.extend(string_to_utf16(string).into_iter()))
                },
                Encoding::UTF16BE => EncodingParams {
                    delim_len: 2,
                    string_func: Box::new(|buf: &mut Vec<u8>, string: &str|
                        buf.extend(string_to_utf16be(string).into_iter()))
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
    #![allow(clippy::redundant_slicing)]
    let content = request.content.text().unwrap();
    encode!(encoding(request.encoding), string(content))
}

fn extended_text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_text().unwrap();
    encode!(
        encoding(request.encoding),
        string(content.description),
        delim(0),
        string(content.value)
    )
}

fn weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    request.content.link().unwrap().as_bytes().to_vec()
}

fn encapsulated_object_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.encapsulated_object().unwrap();
    encode!(
        encoding(request.encoding),
        bytes(content.mime_type.as_bytes()),
        byte(0),
        string(content.filename),
        delim(0),
        string(content.description),
        delim(0),
        bytes(content.data)
    )
}

fn extended_weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_link().unwrap();
    encode!(
        encoding(request.encoding),
        string(content.description),
        delim(0),
        bytes(content.link.as_bytes())
    )
}

fn lyrics_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.lyrics().unwrap();
    encode!(
        encoding(request.encoding),
        bytes(
            content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>()
        ),
        string(content.description),
        delim(0),
        string(content.text)
    )
}

fn synchronised_lyrics_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.synchronised_lyrics().unwrap();
    let encoding = match request.encoding {
        Encoding::Latin1 => Encoding::Latin1,
        _ => Encoding::UTF8,
    };
    let text_delim: &[u8] = match encoding {
        Encoding::Latin1 => &[0],
        Encoding::UTF8 => &[0],
        _ => unreachable!(),
    };

    let params = match encoding {
        Encoding::Latin1 => EncodingParams {
            delim_len: 1,
            string_func: Box::new(|buf: &mut Vec<u8>, string: &str| {
                buf.extend(string_to_latin1(string).into_iter())
            }),
        },
        Encoding::UTF8 => EncodingParams {
            // UTF-8
            delim_len: 1,
            string_func: Box::new(|buf: &mut Vec<u8>, string: &str| buf.extend(string.bytes())),
        },
        _ => unreachable!(),
    };

    let mut buf = Vec::new();
    encode_part!(
        buf,
        params,
        byte(match encoding {
            Encoding::Latin1 => 0,
            Encoding::UTF8 => 3,
            _ => unreachable!(),
        })
    );
    encode_part!(
        buf,
        params,
        bytes(
            content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>()
        )
    );
    encode_part!(
        buf,
        params,
        byte(match content.timestamp_format {
            TimestampFormat::MPEG => 1,
            TimestampFormat::MS => 2,
        })
    );
    encode_part!(
        buf,
        params,
        byte(match content.content_type {
            SynchronisedLyricsType::Other => 0,
            SynchronisedLyricsType::Lyrics => 1,
            SynchronisedLyricsType::Transcription => 2,
            SynchronisedLyricsType::PartName => 3,
            SynchronisedLyricsType::Event => 4,
            SynchronisedLyricsType::Chord => 5,
            SynchronisedLyricsType::Trivia => 6,
        })
    );
    // TODO: content descriptor would go here
    // instead we write an empty descriptor
    encode_part!(buf, params, bytes(text_delim));

    for (timestamp, text) in &content.content {
        encode_part!(buf, params, string(text));
        encode_part!(buf, params, bytes(text_delim));
        // NOTE: The ID3v2.3 spec is not clear on the encoding of the timestamp other
        // than "32 bit sized".
        encode_part!(buf, params, bytes(timestamp.to_be_bytes()));
    }
    buf.push(0); // delim.
    buf
}

fn comment_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.comment().unwrap();
    encode!(
        encoding(request.encoding),
        bytes(
            content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>()
        ),
        string(content.description),
        delim(0),
        string(content.text)
    )
}

fn picture_to_bytes_v3(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.picture().unwrap();
    encode!(
        encoding(request.encoding),
        bytes(content.mime_type.as_bytes()),
        byte(0),
        byte(u8::from(content.picture_type)),
        string(content.description),
        delim(0),
        bytes(content.data)
    )
}

fn picture_to_bytes_v2(request: EncoderRequest) -> crate::Result<Vec<u8>> {
    let picture = request.content.picture().unwrap();
    let format = match &picture.mime_type[..] {
        "image/jpeg" | "image/jpg" => "JPG",
        "image/png" => "PNG",
        _ => return Err(Error::new(ErrorKind::Parsing, "unsupported MIME type")),
    };
    Ok(encode!(
        encoding(request.encoding),
        bytes(format.as_bytes()),
        byte(u8::from(picture.picture_type)),
        string(picture.description),
        delim(0),
        bytes(picture.data)
    ))
}

fn picture_to_bytes(request: EncoderRequest) -> crate::Result<Vec<u8>> {
    match request.version {
        tag::Id3v22 => picture_to_bytes_v2(request),
        tag::Id3v23 | tag::Id3v24 => Ok(picture_to_bytes_v3(request)),
    }
}

struct DecodingParams<'a> {
    encoding: Encoding,
    string_func: Box<dyn Fn(&[u8]) -> crate::Result<String> + 'a>,
}

impl<'a> DecodingParams<'a> {
    fn for_encoding(encoding: Encoding) -> DecodingParams<'a> {
        match encoding {
            Encoding::Latin1 => DecodingParams {
                encoding: Encoding::Latin1,
                string_func: Box::new(|bytes: &[u8]| -> crate::Result<String> {
                    string_from_latin1(bytes)
                }),
            },
            Encoding::UTF8 => DecodingParams {
                encoding: Encoding::UTF8,
                string_func: Box::new(|bytes: &[u8]| -> crate::Result<String> {
                    Ok(String::from_utf8(bytes.to_vec())?)
                }),
            },
            Encoding::UTF16 => DecodingParams {
                encoding: Encoding::UTF16,
                string_func: Box::new(|bytes: &[u8]| -> crate::Result<String> {
                    string_from_utf16(bytes)
                }),
            },
            Encoding::UTF16BE => DecodingParams {
                encoding: Encoding::UTF16BE,
                string_func: Box::new(|bytes: &[u8]| -> crate::Result<String> {
                    string_from_utf16be(bytes)
                }),
            },
        }
    }
}

fn encoding_from_byte(n: u8) -> crate::Result<Encoding> {
    match n {
        0 => Ok(Encoding::Latin1),
        1 => Ok(Encoding::UTF16),
        2 => Ok(Encoding::UTF16BE),
        3 => Ok(Encoding::UTF8),
        _ => Err(Error::new(ErrorKind::Parsing, "unknown encoding")),
    }
}

macro_rules! assert_data {
    ($bytes:ident) => {
        if $bytes.len() == 0 {
            return Err(Error::new(
                ErrorKind::Parsing,
                "frame does not contain any data",
            ));
        }
    };
}

fn find_delim(
    bytes: &[u8],
    encoding: Encoding,
    i: usize,
    terminated: bool,
) -> crate::Result<(usize, usize)> {
    if !terminated {
        return Ok((bytes.len(), bytes.len()));
    }
    let delim = crate::util::find_delim(encoding, bytes, i)
        .ok_or_else(|| Error::new(ErrorKind::Parsing, "delimiter not found"))?;
    Ok((delim, delim + delim_len(encoding)))
}

macro_rules! decode_part {
    ($bytes:expr, $params:ident, string($terminated:expr)) => {{
        let (end, with_delim) = find_delim($bytes, $params.encoding, 0, $terminated)?;
        if end == 0 {
            ("".to_string(), &$bytes[with_delim..])
        } else {
            (
                ($params.string_func)(&$bytes[..end])?,
                &$bytes[with_delim..],
            )
        }
    }};
    ($bytes:expr, $params:ident, text_v3()) => {{
        let (end, with_delim) = match crate::util::find_delim($params.encoding, $bytes, 0) {
            Some(i) => (i, i + delim_len($params.encoding)),
            None => ($bytes.len(), $bytes.len()),
        };
        if end == 0 {
            ("".to_string(), &$bytes[with_delim..])
        } else {
            (
                ($params.string_func)(&$bytes[..end])?,
                &$bytes[with_delim..],
            )
        }
    }};
    ($bytes:expr, $params:ident, text_v4()) => {{
        let (end, with_delim) = match crate::util::find_closing_delim($params.encoding, $bytes) {
            Some(i) => (i, i + delim_len($params.encoding)),
            None => ($bytes.len(), $bytes.len()),
        };
        if end == 0 {
            ("".to_string(), &$bytes[with_delim..])
        } else {
            (
                ($params.string_func)(&$bytes[..end])?,
                &$bytes[with_delim..],
            )
        }
    }};
    ($bytes:expr, $params:ident, fixed_string($len:expr)) => {{
        if $len >= $bytes.len() {
            return Err(Error::new(
                ErrorKind::Parsing,
                "insufficient data to decode fixed string",
            ));
        }
        (string_from_latin1(&$bytes[..$len])?, &$bytes[$len..])
    }};
    ($bytes:expr, $params:ident, latin1($terminated:expr)) => {{
        let (end, with_delim) = find_delim($bytes, Encoding::Latin1, 0, $terminated)?;
        (
            String::from_utf8($bytes[..end].to_vec())?,
            &$bytes[with_delim..],
        )
    }};
    ($bytes:expr, $params:ident, picture_type()) => {{
        if 1 >= $bytes.len() {
            return Err(Error::new(
                ErrorKind::Parsing,
                "insufficient data to decode picture type",
            ));
        }
        let ty = match $bytes[0] {
            0 => PictureType::Other,
            1 => PictureType::Icon,
            2 => PictureType::OtherIcon,
            3 => PictureType::CoverFront,
            4 => PictureType::CoverBack,
            5 => PictureType::Leaflet,
            6 => PictureType::Media,
            7 => PictureType::LeadArtist,
            8 => PictureType::Artist,
            9 => PictureType::Conductor,
            10 => PictureType::Band,
            11 => PictureType::Composer,
            12 => PictureType::Lyricist,
            13 => PictureType::RecordingLocation,
            14 => PictureType::DuringRecording,
            15 => PictureType::DuringPerformance,
            16 => PictureType::ScreenCapture,
            17 => PictureType::BrightFish,
            18 => PictureType::Illustration,
            19 => PictureType::BandLogo,
            20 => PictureType::PublisherLogo,
            b => PictureType::Undefined(b),
        };
        (ty, &$bytes[1..])
    }};
    ($bytes:expr, $params:ident, bytes()) => {{
        ($bytes.to_vec(), &[0u8; 0])
    }};
}

macro_rules! decode {
    ($bytes:ident, $result_type:ident, $($field:ident : $part:ident ( $($params:tt)* ) ),+) => {
        {
            use crate::frame::$result_type;

            assert_data!($bytes);

            let encoding = encoding_from_byte($bytes[0])?;
            let params = DecodingParams::for_encoding(encoding);

            let next = &$bytes[1..];
            $(
                let ($field, next) = decode_part!(next, params, $part ( $($params)* ));
            )+
            let _ = next;

            Ok(Content::$result_type( $result_type {
                $($field,)+
            }))
        }
    };
}

/// Attempts to parse the data as an ID3v2.2 picture frame.
/// Returns a `Content::Picture`.
fn parse_apic_v2(data: &[u8]) -> crate::Result<Content> {
    assert_data!(data);

    let encoding = encoding_from_byte(data[0])?;
    let params = DecodingParams::for_encoding(encoding);

    let (format, next) = decode_part!(&data[1..], params, fixed_string(3));
    let mime_type = match &format[..] {
        "PNG" => "image/png".to_string(),
        "JPG" => "image/jpeg".to_string(),
        _ => {
            return Err(Error::new(
                ErrorKind::UnsupportedFeature,
                "can't determine MIME type for image format",
            ));
        }
    };
    let (picture_type, next) = decode_part!(next, params, picture_type());
    let (description, next) = decode_part!(next, params, string(true));
    let (picture_data, _) = decode_part!(next, params, bytes());

    let picture = Picture {
        mime_type,
        picture_type,
        description,
        data: picture_data,
    };
    Ok(Content::Picture(picture))
}

/// Attempts to parse the data as an ID3v2.3/ID3v2.4 picture frame.
/// Returns a `Content::Picture`.
fn parse_apic_v3(data: &[u8]) -> crate::Result<Content> {
    return decode!(data, Picture, mime_type: latin1(true), picture_type : picture_type(),
                   description: string(true), data: bytes());
}

/// Attempts to parse the data as a comment frame.
/// Returns a `Content::Comment`.
fn parse_comm(data: &[u8]) -> crate::Result<Content> {
    return decode!(data, Comment, lang: fixed_string(3), description: string(true),
                   text: string(false));
}

/// Attempts to parse the data as a text frame.
/// Returns a `Content::Text`.
fn parse_text(data: &[u8], version: tag::Version) -> crate::Result<Content> {
    assert_data!(data);
    let encoding = encoding_from_byte(data[0])?;

    let params = DecodingParams::for_encoding(encoding);
    match version {
        tag::Version::Id3v24 => Ok(Content::Text(decode_part!(&data[1..], params, text_v4()).0)),
        _ => Ok(Content::Text(decode_part!(&data[1..], params, text_v3()).0)),
    }
}

/// Attempts to parse the data as a user defined text frame.
/// Returns an `Content::ExtendedText`.
fn parse_txxx(data: &[u8]) -> crate::Result<Content> {
    return decode!(data, ExtendedText, description: string(true), value: string(false));
}

/// Attempts to parse the data as a web link frame.
/// Returns a `Content::Link`.
fn parse_weblink(data: &[u8]) -> crate::Result<Content> {
    Ok(Content::Link(String::from_utf8(data.to_vec())?))
}

/// Attempts to parse the data as a general encapsulated object.
/// Returns an `Content::EncapsulatedObject`.
fn parse_geob(data: &[u8]) -> crate::Result<Content> {
    assert_data!(data);

    let encoding = encoding_from_byte(data[0])?;

    let uparams = DecodingParams::for_encoding(Encoding::Latin1);
    let (mime_type, next) = decode_part!(&data[1..], uparams, string(true));

    let params = DecodingParams::for_encoding(encoding);
    let (filename, next) = decode_part!(next, params, string(true));

    let (description, next) = decode_part!(next, params, string(true));

    let data = next.to_vec();

    let obj = EncapsulatedObject {
        mime_type,
        filename,
        description,
        data,
    };
    Ok(Content::EncapsulatedObject(obj))
}

/// Attempts to parse the data as a user defined web link frame.
/// Returns an `Content::ExtendedLink`.
fn parse_wxxx(data: &[u8]) -> crate::Result<Content> {
    assert_data!(data);

    let encoding = encoding_from_byte(data[0])?;

    let params = DecodingParams::for_encoding(encoding);
    let (description, next) = decode_part!(&data[1..], params, string(true));

    let uparams = DecodingParams::for_encoding(Encoding::Latin1);
    let (link, _) = decode_part!(next, uparams, string(false));

    let elink = ExtendedLink { description, link };
    Ok(Content::ExtendedLink(elink))
}

/// Attempts to parse the data as an unsynchronized lyrics text frame.
/// Returns a `Content::Lyrics`.
fn parse_uslt(data: &[u8]) -> crate::Result<Content> {
    return decode!(data, Lyrics, lang: fixed_string(3), description: string(true),
                   text: string(false));
}

fn parse_sylt(data: &[u8]) -> crate::Result<Content> {
    let (encoding, text_delim) = match data[0] {
        0 => (Encoding::Latin1, &[0][..]),
        1 => (Encoding::UTF8, &[0, 0][..]),
        _ => return Err(Error::new(ErrorKind::Parsing, "invalid SYLT encoding")),
    };
    let decode_str: &dyn Fn(&[u8]) -> crate::Result<String> = match encoding {
        Encoding::Latin1 => &string_from_latin1,
        Encoding::UTF8 => &|d: &[u8]| Ok(String::from_utf8(d.to_vec())?),
        _ => unreachable!(),
    };
    let next = &data[1..];

    let (lang, next) = decode_part!(&next, params, fixed_string(3));
    let timestamp_format = match next[0] {
        0 => TimestampFormat::MPEG,
        1 => TimestampFormat::MS,
        _ => {
            return Err(Error::new(
                ErrorKind::Parsing,
                "invalid SYLT timestamp format",
            ))
        }
    };
    let next = &next[1..];

    let content_type = match next[0] {
        0 => SynchronisedLyricsType::Other,
        1 => SynchronisedLyricsType::Lyrics,
        2 => SynchronisedLyricsType::Transcription,
        3 => SynchronisedLyricsType::PartName,
        4 => SynchronisedLyricsType::Event,
        5 => SynchronisedLyricsType::Chord,
        6 => SynchronisedLyricsType::Trivia,
        _ => return Err(Error::new(ErrorKind::Parsing, "invalid SYLT content type")),
    };
    let mut next = &next[1..];

    let mut content = Vec::new();
    loop {
        let ii = next.windows(text_delim.len()).position(|w| w == text_delim);
        let i = match ii {
            Some(i) => i,
            None => break,
        };

        let text = decode_str(&next[..i])?;
        let timestamp = u32::from_be_bytes({
            let t = &next[i + text_delim.len()..i + text_delim.len() + 4];
            let mut a = [0; 4];
            a.copy_from_slice(t);
            a
        });
        content.push((timestamp, text));

        next = &next[i + text_delim.len() + 4..];
    }

    Ok(Content::SynchronisedLyrics(SynchronisedLyrics {
        lang,
        timestamp_format,
        content_type,
        content,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Content;
    use crate::frame::{self, Picture, PictureType};
    use std::collections::HashMap;

    fn bytes_for_encoding(text: &str, encoding: Encoding) -> Vec<u8> {
        match encoding {
            //string.chars().map(|c| c as u8)
            Encoding::Latin1 => text.chars().map(|c| c as u8).collect(),
            Encoding::UTF8 => text.as_bytes().to_vec(),
            Encoding::UTF16 => string_to_utf16(text),
            Encoding::UTF16BE => string_to_utf16be(text),
        }
    }

    fn delim_for_encoding(encoding: Encoding) -> Vec<u8> {
        match encoding {
            Encoding::Latin1 | Encoding::UTF8 => vec![0],
            Encoding::UTF16 | Encoding::UTF16BE => vec![0, 0],
        }
    }

    #[test]
    fn test_apic_v2() {
        assert!(decode("PIC", tag::Id3v22, &[][..]).is_err());

        let mut format_map = HashMap::new();
        format_map.insert("image/jpeg", "JPG");
        format_map.insert("image/png", "PNG");

        for (mime_type, format) in format_map {
            for description in &["", "description"] {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec![0xF9, 0x90, 0x3A, 0x02, 0xBD];
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
                    data.push(picture_type.into());
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(picture_data.iter().cloned());

                    assert_eq!(
                        *decode("PIC", tag::Id3v22, &data[..])
                            .unwrap()
                            .picture()
                            .unwrap(),
                        picture
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Picture(picture.clone()),
                        tag::Id3v22,
                        *encoding,
                    )
                    .unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }
    }

    #[test]
    fn test_apic_v3() {
        assert!(decode("APIC", tag::Id3v23, &[][..]).is_err());

        for mime_type in &["", "image/jpeg"] {
            for description in &["", "description"] {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec![0xF9, 0x90, 0x3A, 0x02, 0xBD];
                let picture = Picture {
                    mime_type: mime_type.to_string(),
                    picture_type,
                    description: description.to_string(),
                    data: picture_data.clone(),
                };

                for encoding in &[
                    Encoding::Latin1,
                    Encoding::UTF8,
                    Encoding::UTF16,
                    Encoding::UTF16BE,
                ] {
                    println!("`{}`, `{}`, `{:?}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(mime_type.bytes());
                    data.push(0x0);
                    data.push(picture_type.into());
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(picture_data.iter().cloned());

                    assert_eq!(
                        *decode("APIC", tag::Id3v23, &data[..])
                            .unwrap()
                            .picture()
                            .unwrap(),
                        picture
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Picture(picture.clone()),
                        tag::Id3v23,
                        *encoding,
                    )
                    .unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }
    }

    #[test]
    fn test_comm() {
        assert!(decode("COMM", tag::Id3v23, &[][..]).is_err());

        println!("valid");
        for description in &["", "description"] {
            for comment in &["", "comment"] {
                for encoding in &[
                    Encoding::Latin1,
                    Encoding::UTF8,
                    Encoding::UTF16,
                    Encoding::UTF16BE,
                ] {
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
                        text: comment.to_string(),
                    };
                    assert_eq!(
                        *decode("COMM", tag::Id3v23, &data[..])
                            .unwrap()
                            .comment()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Comment(content),
                        tag::Id3v23,
                        *encoding,
                    )
                    .unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let comment = "comment";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(bytes_for_encoding(description, *encoding).into_iter());
            data.extend(bytes_for_encoding(comment, *encoding).into_iter());
            assert!(decode("COMM", tag::Id3v23, &data[..]).is_err());
        }
        println!("Empty description");
        let comment = "comment";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(delim_for_encoding(*encoding));
            data.extend(bytes_for_encoding(comment, *encoding).into_iter());
            let content = frame::Comment {
                lang: "eng".to_string(),
                description: "".to_string(),
                text: comment.to_string(),
            };
            println!("data == {:?}", data);
            println!("content == {:?}", content);
            assert_eq!(
                *decode("COMM", tag::Id3v23, &data[..])
                    .unwrap()
                    .comment()
                    .unwrap(),
                content
            );
        }
    }

    #[test]
    fn test_text() {
        assert!(decode("TALB", tag::Id3v23, &[][..]).is_err());

        for text in &["", "text"] {
            for encoding in &[
                Encoding::Latin1,
                Encoding::UTF8,
                Encoding::UTF16,
                Encoding::UTF16BE,
            ] {
                println!("`{}`, `{:?}`", text, *encoding);
                let mut data = Vec::new();
                data.push(*encoding as u8);
                data.extend(bytes_for_encoding(text, *encoding).into_iter());

                assert_eq!(
                    decode("TALB", tag::Id3v23, &data[..])
                        .unwrap()
                        .text()
                        .unwrap(),
                    *text
                );
                let mut data_out = Vec::new();
                encode(
                    &mut data_out,
                    &Content::Text(text.to_string()),
                    tag::Id3v23,
                    *encoding,
                )
                .unwrap();
                assert_eq!(data, data_out);
            }
        }
    }

    #[test]
    fn test_null_terminated_text_v3() {
        assert!(decode("TRCK", tag::Id3v23, &[][..]).is_err());
        let text = "text\u{0}test\u{0}";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{}`, `{:?}`", text, encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(text, *encoding).into_iter());

            assert_eq!(
                decode("TALB", tag::Id3v23, &data[..])
                    .unwrap()
                    .text()
                    .unwrap(),
                "text"
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Text(text.to_string()),
                tag::Id3v23,
                *encoding,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_null_terminated_text_v4() {
        assert!(decode("TRCK", tag::Id3v24, &[][..]).is_err());
        let text = "text\u{0}text\u{0}";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{}`, `{:?}`", text, encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(text, *encoding).into_iter());

            assert_eq!(
                decode("TALB", tag::Id3v24, &data[..])
                    .unwrap()
                    .text()
                    .unwrap(),
                "text\u{0}text"
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Text(text.to_string()),
                tag::Id3v24,
                *encoding,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_non_null_terminated_text_v4() {
        assert!(decode("TRCK", tag::Id3v24, &[][..]).is_err());
        let text = "text\u{0}text";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{}`, `{:?}`", text, encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(text, *encoding).into_iter());

            assert_eq!(
                decode("TALB", tag::Id3v24, &data[..])
                    .unwrap()
                    .text()
                    .unwrap(),
                "text\u{0}text"
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Text(text.to_string()),
                tag::Id3v24,
                *encoding,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_txxx() {
        assert!(decode("TXXX", tag::Id3v23, &[][..]).is_err());

        println!("valid");
        for key in &["", "key"] {
            for value in &["", "value"] {
                for encoding in &[
                    Encoding::Latin1,
                    Encoding::UTF8,
                    Encoding::UTF16,
                    Encoding::UTF16BE,
                ] {
                    println!("{:?}", encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(bytes_for_encoding(key, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(value, *encoding).into_iter());

                    let content = frame::ExtendedText {
                        description: key.to_string(),
                        value: value.to_string(),
                    };
                    assert_eq!(
                        *decode("TXXX", tag::Id3v23, &data[..])
                            .unwrap()
                            .extended_text()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::ExtendedText(content),
                        tag::Id3v23,
                        *encoding,
                    )
                    .unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let key = "key";
        let value = "value";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(key, *encoding).into_iter());
            data.extend(bytes_for_encoding(value, *encoding).into_iter());
            assert!(decode("TXXX", tag::Id3v23, &data[..]).is_err());
        }
    }

    #[test]
    fn test_weblink() {
        for link in &["", "http://www.rust-lang.org/"] {
            println!("`{:?}`", link);
            let data = link.as_bytes().to_vec();

            assert_eq!(
                decode("WOAF", tag::Id3v23, &data[..])
                    .unwrap()
                    .link()
                    .unwrap(),
                *link
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Link(link.to_string()),
                tag::Id3v23,
                Encoding::Latin1,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_wxxx() {
        assert!(decode("WXXX", tag::Id3v23, &[][..]).is_err());

        println!("valid");
        for description in &["", "rust"] {
            for link in &["", "http://www.rust-lang.org/"] {
                for encoding in &[
                    Encoding::Latin1,
                    Encoding::UTF8,
                    Encoding::UTF16,
                    Encoding::UTF16BE,
                ] {
                    println!("`{}`, `{}`, `{:?}`", description, link, encoding);
                    let mut data = Vec::new();
                    data.push(*encoding as u8);
                    data.extend(bytes_for_encoding(description, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(link, Encoding::Latin1).into_iter());

                    let content = frame::ExtendedLink {
                        description: description.to_string(),
                        link: link.to_string(),
                    };
                    assert_eq!(
                        *decode("WXXX", tag::Id3v23, &data[..])
                            .unwrap()
                            .extended_link()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::ExtendedLink(content),
                        tag::Id3v23,
                        *encoding,
                    )
                    .unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let description = "rust";
        let link = "http://www.rust-lang.org/";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding(description, *encoding).into_iter());
            data.extend(bytes_for_encoding(link, Encoding::Latin1).into_iter());
            assert!(decode("WXXX", tag::Id3v23, &data[..]).is_err());
        }
    }

    #[test]
    fn test_uslt() {
        assert!(decode("USLT", tag::Id3v23, &[][..]).is_err());

        println!("valid");
        for description in &["", "description"] {
            for text in &["", "lyrics"] {
                for encoding in &[
                    Encoding::Latin1,
                    Encoding::UTF8,
                    Encoding::UTF16,
                    Encoding::UTF16BE,
                ] {
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
                    assert_eq!(
                        *decode("USLT", tag::Id3v23, &data[..])
                            .unwrap()
                            .lyrics()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Lyrics(content),
                        tag::Id3v23,
                        *encoding,
                    )
                    .unwrap();
                    assert_eq!(data, data_out);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let lyrics = "lyrics";
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(bytes_for_encoding(description, *encoding).into_iter());
            data.extend(bytes_for_encoding(lyrics, *encoding).into_iter());
            assert!(decode("USLT", tag::Id3v23, &data[..]).is_err());
        }
    }
}
