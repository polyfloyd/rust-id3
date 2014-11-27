extern crate audiotag;

use self::audiotag::{TagError, TagResult, InvalidInputError, StringDecodingError, UnsupportedFeatureError};

use frame::{Encoding, Picture, PictureType};
use frame::Content::{
    mod, PictureContent, CommentContent, TextContent, ExtendedTextContent, LyricsContent,
    LinkContent, ExtendedLinkContent, UnknownContent
};
use util;

/// The result of a successfully parsed frame.
pub struct DecoderResult {
    /// The text encoding used in the frame.
    pub encoding: Encoding,
    /// The parsed content of the frame.
    pub content: Content 
}

impl DecoderResult {
    /// Creates a new `DecoderResult` with the provided encoding and content.
    #[inline]
    pub fn new(encoding: Encoding, content: Content) -> DecoderResult {
        DecoderResult { encoding: encoding, content: content }
    }
}

pub struct DecoderRequest<'a> {
    pub id: &'a str,
    pub data: &'a [u8]
}

pub struct EncoderRequest<'a> {
    pub version: u8,
    pub encoding: Encoding,
    pub content: &'a Content
}

/// Creates a vector representation of the request.
pub fn encode(request: EncoderRequest) -> Vec<u8> {
    match request.content {
        &TextContent(_) => text_to_bytes(request),
        &ExtendedTextContent(_) => extended_text_to_bytes(request),
        &LinkContent(_) => weblink_to_bytes(request),
        &ExtendedLinkContent(_) => extended_weblink_to_bytes(request),
        &LyricsContent(_) => lyrics_to_bytes(request),
        &CommentContent(_) => comment_to_bytes(request),
        &PictureContent(_) => picture_to_bytes(request),
        &UnknownContent(ref data) => data.clone()
    }
}

/// Attempts to decode the request.
pub fn decode(request: DecoderRequest) -> TagResult<DecoderResult> {
    match request.id {
        "APIC" => parse_apic_v3(request.data),
        "PIC" => parse_apic_v2(request.data),
        "TXXX" | "TXX" => parse_txxx(request.data),
        "WXXX" | "WXX" => parse_wxxx(request.data),
        "COMM" | "COM" => parse_comm(request.data),
        "USLT" | "ULT" => parse_uslt(request.data),
        _ => {
            if request.id.as_slice().len() > 0 {
                if request.id.as_slice().char_at(0) == 'T' {
                    return parse_text(request.data);
                } else if request.id.as_slice().char_at(0) == 'W' {
                    return parse_weblink(request.data);
                } 
            }

            Ok(DecoderResult::new(Encoding::UTF16, UnknownContent(request.data.to_vec())))
        }
    }
}

// Encoders {{{
struct EncodingParams<'a> {
    delim_len: u8,
    string_func: |&mut Vec<u8>, &str|:'a
}

macro_rules! encode_part {
    ($buf:ident, encoding($encoding:expr)) => { $buf.push($encoding as u8) };
    ($buf:ident, $params:ident, string($string:expr)) => { ($params.string_func)(&mut $buf, $string.as_slice()) };
    ($buf:ident, $params:ident, delim($ignored:expr)) => { for _ in range(0, $params.delim_len) { $buf.push(0); } };
    ($buf:ident, $params:ident, bytes($bytes:expr)) => { $buf.push_all($bytes.as_slice()); };
    ($buf:ident, $params:ident, byte($byte:expr)) => { $buf.push($byte as u8) };
}

macro_rules! encode {
    (encoding($encoding:expr) $(, $part:ident( $value:expr ) )+) => {
        {
            let params = match $encoding {
                Encoding::Latin1 | Encoding::UTF8 => EncodingParams { 
                    delim_len: 1,
                    string_func: |buf: &mut Vec<u8>, string: &str| 
                        buf.push_all(string.as_bytes())
                },
                Encoding::UTF16 => EncodingParams {
                    delim_len: 2,
                    string_func: |buf: &mut Vec<u8>, string: &str| 
                        buf.extend(util::string_to_utf16(string).into_iter())
                },
                Encoding::UTF16BE => EncodingParams {
                    delim_len: 2,
                    string_func: |buf: &mut Vec<u8>, string: &str| 
                        buf.extend(util::string_to_utf16be(string).into_iter())
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
    let content = request.content.text();
    return encode!(encoding(request.encoding), string(content.text));
}

fn extended_text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_text();
    return encode!(encoding(request.encoding), string(content.key), delim(0), string(content.value));
}

fn weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    request.content.link().link.as_bytes().to_vec()
}

fn extended_weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_link();
    return encode!(encoding(request.encoding), string(content.description), delim(0), 
                   string(content.link));
}

fn lyrics_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.lyrics();
    return encode!(encoding(request.encoding), bytes(content.lang.slice_to(3).as_bytes()), 
                   string(content.description), delim(0), string(content.text));
}

fn comment_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.comment();
    return encode!(encoding(request.encoding), bytes(content.lang.slice_to(3).as_bytes()), 
                   string(content.description), delim(0), string(content.text));
}

fn picture_to_bytes_v3(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.picture();
    return encode!(encoding(request.encoding), bytes(content.mime_type.as_bytes()), byte(0), 
            byte(content.picture_type), string(content.description), delim(0), bytes(content.data));
}

fn picture_to_bytes_v2(request: EncoderRequest) -> Vec<u8> {
    let picture = request.content.picture();

    let format = match picture.mime_type.as_slice() {
        "image/jpeg" => "JPG",
        "image/png" => "PNG",
        _ => panic!("unknown MIME type") // TODO handle this better
    };

    return encode!(encoding(request.encoding), bytes(format.as_bytes()), byte(picture.picture_type), 
            string(picture.description), delim(0), bytes(picture.data)); 
}

#[inline]
fn picture_to_bytes(request: EncoderRequest) -> Vec<u8> {
    if request.version == 2 {
        picture_to_bytes_v2(request)
    } else {
        picture_to_bytes_v3(request)
    }
}
// }}}

// Decoders {{{
struct DecodingParams<'a> {
    encoding: Encoding,
    string_func: |&[u8]|:'a -> Option<String>
}

impl<'a> DecodingParams<'a> {
    fn for_encoding(encoding: Encoding) -> DecodingParams<'a> {
        match encoding {
            Encoding::Latin1 | Encoding::UTF8 => DecodingParams {
                encoding: Encoding::UTF8,
                string_func: |bytes: &[u8]| -> Option<String>
                    String::from_utf8(bytes.to_vec()).ok()
            },
            Encoding::UTF16 => DecodingParams {
                encoding: Encoding::UTF16,
                string_func: |bytes: &[u8]| -> Option<String>
                    util::string_from_utf16(bytes)
            },
            Encoding::UTF16BE => DecodingParams {
                encoding: Encoding::UTF16BE,
                string_func: |bytes: &[u8]| -> Option<String>
                    util::string_from_utf16be(bytes)
            }
        }
    }
}

macro_rules! find_delim {
    ($bytes:ident, $encoding:expr, $i:ident, $terminated:expr) => {
        if !$terminated {
            ($bytes.len(), $bytes.len())
        } else {
            match util::find_delim($encoding, $bytes, $i) {
                Some(i) => (i, i + util::delim_len($encoding)),
                None => return Err(TagError::new(InvalidInputError, "delimiter not found"))
            }
        }
    };
}

macro_rules! decode_part {
    ($bytes:ident, $params:ident, $i:ident, string($terminated:expr)) => {
        {
            let start = $i;
            let (end, with_delim) = find_delim!($bytes, $params.encoding, $i, $terminated);
            $i = with_delim; Some(&$i);

            match ($params.string_func)($bytes.slice(start, end)) {
                Some(string) => string,
                None => return Err(TagError::new(audiotag::StringDecodingError($bytes.slice(start, end).to_vec()), match $params.encoding {
                    Encoding::Latin1 | ::frame::Encoding::UTF8 => "string is not valid utf8",
                    Encoding::UTF16 => "string is not valid utf16",
                    Encoding::UTF16BE => "string is not valid utf16-be"
                }))
            }
        }
    };
    ($bytes:ident, $params:ident, $i:ident, fixed_string($len:expr)) => {
        {
            if $i + $len >= $bytes.len() {
                return Err(TagError::new(InvalidInputError, "insufficient data"));
            }

            let start = $i;
            $i += $len;

            try_string!($bytes.slice(start, $i).to_vec())
        }
    };
    ($bytes:ident, $params:ident, $i:ident, latin1($terminated:expr)) => {
        {
            let start = $i;
            let (end, with_delim) = find_delim!($bytes, Encoding::Latin1, $i, $terminated);
            $i = with_delim; Some(&$i);
            try_string!($bytes.slice(start, end).to_vec())
        }
    };
    ($bytes:ident, $params:ident, $i:ident, picture_type()) => {
        {
            if $i + 1 >= $bytes.len() {
                return Err(TagError::new(InvalidInputError, "insufficient data"));
            }

            let start = $i;
            $i += 1;

            let picture_type: PictureType = match FromPrimitive::from_u8($bytes[start]) {
                Some(t) => t,
                None => return Err(TagError::new(InvalidInputError, "invalid picture type"))
            };
            picture_type
        }
    };
    ($bytes:ident, $params:ident, $i:ident, bytes()) => {
        {
            let start = $i;
            $i = $bytes.len(); Some(&$i);
            $bytes.slice_from(start).to_vec()
        }
    };
}

macro_rules! decode {
    ($bytes:ident, $result_type:ident $(, $part:ident( $field:ident $(, $params:expr)* ) )+) => {
        {
            use frame::$result_type;

            if $bytes.len() == 0 {
                return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
            }

            let encoding = try_encoding!($bytes[0]);
            let params = DecodingParams::for_encoding(encoding);
            
            let mut i = 1;
            Ok(DecoderResult { 
                encoding: encoding, 
                content: concat_idents!($result_type, Content)( $result_type {
                    $($field: decode_part!($bytes, params, i, $part ( $($params)* ) ),)+
                })
            })
        }
    };
}

/// Attempts to parse the data as an ID3v2.2 picture frame.
/// Returns a `PictureContent`.
fn parse_apic_v2(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let mut picture = Picture::new();
   
    let encoding = try_encoding!(data[0]);
    let params = DecodingParams::for_encoding(encoding);
   
    let mut i = 1;
    let format = decode_part!(data, params, i, fixed_string(3));
    picture.mime_type = match format.as_slice() {
        "PNG" => "image/png".into_string(),
        "JPG" => "image/jpeg".into_string(),
        other => {
            debug!("can't determine MIME type for `{}`", other);
            return Err(TagError::new(UnsupportedFeatureError, "can't determine MIME type for image format"))
        }
    }; 

    picture.picture_type = decode_part!(data, params, i, picture_type());
    picture.description = decode_part!(data, params, i, string(true));
    picture.data = decode_part!(data, params, i, bytes());

    Ok(DecoderResult::new(encoding, PictureContent(picture)))
}

/// Attempts to parse the data as an ID3v2.3/ID3v2.4 picture frame.
/// Returns a `PictureContent`.
fn parse_apic_v3(data: &[u8]) -> TagResult<DecoderResult> {
    return decode!(data, Picture, latin1(mime_type, true), picture_type(picture_type), string(description, true), bytes(data));
}

/// Attempts to parse the data as a comment frame.
/// Returns a `CommentContent`.
fn parse_comm(data: &[u8]) -> TagResult<DecoderResult> {
    return decode!(data, Comment, fixed_string(lang, 3), string(description, true), string(text, false));
}

/// Attempts to parse the data as a text frame.
/// Returns a `TextContent`.
fn parse_text(data: &[u8]) -> TagResult<DecoderResult> {
    return decode!(data, Text, string(text, false));
}

/// Attempts to parse the data as a user defined text frame.
/// Returns an `ExtendedTextContent`.
fn parse_txxx(data: &[u8]) -> TagResult<DecoderResult> {
    return decode!(data, ExtendedText, string(key, true), string(value, false));
}

/// Attempts to parse the data as a web link frame.
/// Returns a `LinkContent`.
fn parse_weblink(data: &[u8]) -> TagResult<DecoderResult> {
    Ok(DecoderResult::new(Encoding::Latin1, LinkContent(::frame::Link { link: try_string!(Encoding::Latin1, data) })))
}

/// Attempts to parse the data as a user defined web link frame.
/// Returns an `ExtendedLinkContent`.
fn parse_wxxx(data: &[u8]) -> TagResult<DecoderResult> {
    return decode!(data, ExtendedLink, string(description, true), string(link, false));
}

/// Attempts to parse the data as an unsynchronized lyrics text frame.
/// Returns a `LyricsContent`.
fn parse_uslt(data: &[u8]) -> TagResult<DecoderResult> {
    return decode!(data, Lyrics, fixed_string(lang, 3), string(description, true), string(text, false));
}
// }}}

// Tests {{{
#[cfg(test)]
mod tests {
    use parsers;
    use parsers::{DecoderRequest, EncoderRequest};
    use util;
    use frame::{mod, Picture, PictureType, Encoding};
    use frame::Content::{PictureContent, CommentContent, TextContent, ExtendedTextContent, LinkContent, ExtendedLinkContent, LyricsContent};
    use std::collections::HashMap;

    fn bytes_for_encoding(text: &str, encoding: Encoding) -> Vec<u8> {
        match encoding {
            Encoding::Latin1 | Encoding::UTF8 => text.as_bytes().to_vec(),
            Encoding::UTF16 => util::string_to_utf16(text),
            Encoding::UTF16BE => util::string_to_utf16be(text)
        }
    }

    fn delim_for_encoding(encoding: Encoding) -> Vec<u8> {
        match encoding {
            Encoding::Latin1 | Encoding::UTF8 => Vec::from_elem(1, 0),
            Encoding::UTF16 | Encoding::UTF16BE => Vec::from_elem(2, 0)
        }
    }

    #[test]
    fn test_apic_v2() {
        assert!(parsers::decode(DecoderRequest { id: "PIC", data: &[] } ).is_err());

        let mut format_map = HashMap::new();
        format_map.insert("image/jpeg", "JPG");
        format_map.insert("image/png", "PNG");

        for (mime_type, format) in format_map.into_iter() {
            for description in vec!("", "description").into_iter() {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec!(0xF9, 0x90, 0x3A, 0x02, 0xBD);

                let mut picture = Picture::new();
                picture.mime_type = mime_type.into_string();
                picture.picture_type = picture_type;
                picture.description = description.into_string();
                picture.data = picture_data.clone();

                for encoding in vec!(Encoding::Latin1, Encoding::UTF16).into_iter() {
                    println!("`{}`, `{}`, `{}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(format.as_bytes());
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.push_all(picture_data.as_slice());

                    assert_eq!(*parsers::decode(DecoderRequest { id: "PIC", data: data.as_slice() } ).unwrap().content.picture(), picture);
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &PictureContent(picture.clone()), version: 2 } ), data);
                }
            }
        }
    }

    #[test]
    fn test_apic_v3() {
        assert!(parsers::decode(DecoderRequest { id: "APIC", data: &[] } ).is_err());

        for mime_type in vec!("", "image/jpeg").into_iter() {
            for description in vec!("", "description").into_iter() {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec!(0xF9, 0x90, 0x3A, 0x02, 0xBD);

                let mut picture = Picture::new();
                picture.mime_type = mime_type.into_string();
                picture.picture_type = picture_type;
                picture.description = description.into_string();
                picture.data = picture_data.clone();

                for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(mime_type.as_bytes());
                    data.push(0x0);
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.push_all(picture_data.as_slice());
                    
                    assert_eq!(*parsers::decode(DecoderRequest { id: "APIC", data: data.as_slice() } ).unwrap().content.picture(), picture);
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &PictureContent(picture.clone()), version: 3 } ), data);
                }
            }
        }
    }

    #[test]
    fn test_comm() {
        assert!(parsers::decode(DecoderRequest { id: "COMM", data: &[] } ).is_err());

        println!("valid");
        for description in vec!("", "description").into_iter() {
            for comment in vec!("", "comment").into_iter() {
                for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{}`", description, comment, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(b"eng");
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(comment, encoding).into_iter());

                    let content = frame::Comment { lang: "eng".into_string(), description: description.into_string(), text: comment.into_string() };
                    assert_eq!(*parsers::decode(DecoderRequest { id: "COMM", data: data.as_slice() } ).unwrap().content.comment(), content);
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &CommentContent(content), version: 3 }), data);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let comment = "comment";
        for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(comment, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { id: "COMM", data: data.as_slice() } ).is_err());
        }

    }

    #[test]
    fn test_text() {
        assert!(parsers::decode(DecoderRequest { id: "TALB", data: &[] } ).is_err());

        for text in vec!("", "text").into_iter() {
            for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                println!("`{}`, `{}`", text, encoding);
                let mut data = Vec::new();
                data.push(encoding as u8);
                data.extend(bytes_for_encoding(text, encoding).into_iter());

                let content = frame::Text { text: text.into_string() };
                assert_eq!(*parsers::decode(DecoderRequest { id: "TALB", data: data.as_slice() } ).unwrap().content.text(), content);
                assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &TextContent(content), version: 3 } ), data);
            }
        }
    }

    #[test]
    fn test_txxx() {
        assert!(parsers::decode(DecoderRequest { id: "TXXX", data: &[] } ).is_err());

        println!("valid");
        for key in vec!("", "key").into_iter() {
            for value in vec!("", "value").into_iter() {
                for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("{}", encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(bytes_for_encoding(key, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(value, encoding).into_iter());

                    let content = frame::ExtendedText { key: key.into_string(), value: value.into_string() };
                    assert_eq!(*parsers::decode(DecoderRequest { id: "TXXX", data: data.as_slice() } ).unwrap().content.extended_text(), content);
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &ExtendedTextContent(content), version: 3 } ), data);
                }
            }
        }

        println!("invalid");
        let key = "key";
        let value = "value";
        for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(key, encoding).into_iter());
            data.extend(bytes_for_encoding(value, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { id: "TXXX", data: data.as_slice() } ).is_err());
        }
    }

    #[test]
    fn test_weblink() {
        for link in vec!("", "http://www.rust-lang.org/").into_iter() {
            println!("`{}`", link);
            let data = link.as_bytes().to_vec();

            let content = frame::Link { link: link.into_string() };
            assert_eq!(*parsers::decode(DecoderRequest { id: "WOAF", data: data.as_slice() } ).unwrap().content.link(), content);
            assert_eq!(parsers::encode(EncoderRequest { encoding: Encoding::Latin1, content: &LinkContent(content), version: 3 } ), data);
        }
    }

    #[test]
    fn test_wxxx() {
        assert!(parsers::decode(DecoderRequest { id: "WXXX", data: &[] } ).is_err());

        println!("valid");
        for description in vec!("", "rust").into_iter() {
            for link in vec!("", "http://www.rust-lang.org/").into_iter() { 
                for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{}`", description, link, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(link, encoding).into_iter());

                    let content = frame::ExtendedLink { description: description.into_string(), link: link.into_string() };
                    assert_eq!(*parsers::decode(DecoderRequest { id: "WXXX", data: data.as_slice() } ).unwrap().content.extended_link(), content);
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &ExtendedLinkContent(content), version: 3 } ), data);
                }
            }
        }

        println!("invalid");
        let description = "rust";
        let link = "http://www.rust-lang.org/";
        for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(link, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { id: "WXXX", data: data.as_slice() } ).is_err());
        }
    }

    #[test]
    fn test_uslt() {
        assert!(parsers::decode(DecoderRequest { id: "USLT", data: &[] } ).is_err());

        println!("valid");
        for description in vec!("", "description").into_iter() {
            for text in vec!("", "lyrics").into_iter() {
                for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}, `{}`", description, text, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(b"eng");
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(text, encoding).into_iter());

                    let content = frame::Lyrics { lang: "eng".into_string(), description: description.into_string(), text: text.into_string() };
                    assert_eq!(*parsers::decode(DecoderRequest { id: "USLT", data: data.as_slice() } ).unwrap().content.lyrics(), content);
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &LyricsContent(content), version: 3 } ), data);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let lyrics = "lyrics";
        for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(lyrics, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { id: "USLT", data: data.as_slice() } ).is_err());
        }
    }
}
// }}}
