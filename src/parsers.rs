use frame::{Encoding, Picture, PictureType, Content, ExtendedLink};

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
        &Content::Text(_) => text_to_bytes(request),
        &Content::ExtendedText(_) => extended_text_to_bytes(request),
        &Content::Link(_) => weblink_to_bytes(request),
        &Content::ExtendedLink(_) => extended_weblink_to_bytes(request),
        &Content::Lyrics(_) => lyrics_to_bytes(request),
        &Content::Comment(_) => comment_to_bytes(request),
        &Content::Picture(_) => picture_to_bytes(request),
        &Content::Unknown(ref data) => data.clone()
    }
}

/// Attempts to decode the request.
pub fn decode(request: DecoderRequest) -> ::Result<DecoderResult> {
    match request.id {
        "APIC" => parse_apic_v3(request.data),
        "PIC" => parse_apic_v2(request.data),
        "TXXX" | "TXX" => parse_txxx(request.data),
        "WXXX" | "WXX" => parse_wxxx(request.data),
        "COMM" | "COM" => parse_comm(request.data),
        "USLT" | "ULT" => parse_uslt(request.data),
        _ => {
            if request.id[..].len() > 0 {
                if request.id[..].chars().next().unwrap() == 'T' {
                    return parse_text(request.data);
                } else if request.id[..].chars().next().unwrap() == 'W' {
                    return parse_weblink(request.data);
                } 
            }

            Ok(DecoderResult::new(Encoding::UTF16, Content::Unknown(request.data.to_vec())))
        }
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
    let content = request.content.text();
    return encode!(encoding(request.encoding), string(content));
}

fn extended_text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_text();
    return encode!(encoding(request.encoding), string(content.key), delim(0), string(content.value));
}

fn weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    request.content.link().as_bytes().to_vec()
}

fn extended_weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.extended_link();
    return encode!(encoding(request.encoding), string(content.description), delim(0), 
                   bytes(content.link.as_bytes()));
}

fn lyrics_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.lyrics();
    return encode!(encoding(request.encoding), bytes(content.lang[..3].as_bytes()), 
                   string(content.description), delim(0), string(content.text));
}

fn comment_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.comment();
    return encode!(encoding(request.encoding), bytes(content.lang[..3].as_bytes()), 
                   string(content.description), delim(0), string(content.text));
}

fn picture_to_bytes_v3(request: EncoderRequest) -> Vec<u8> {
    let content = request.content.picture();
    return encode!(encoding(request.encoding), bytes(content.mime_type.as_bytes()), byte(0), 
            byte(content.picture_type), string(content.description), delim(0), bytes(content.data));
}

fn picture_to_bytes_v2(request: EncoderRequest) -> Vec<u8> {
    let picture = request.content.picture();

    let format = match &picture.mime_type[..] {
        "image/jpeg" => "JPG",
        "image/png" => "PNG",
        _ => panic!("unknown MIME type") // TODO handle this better
    };

    return encode!(encoding(request.encoding), bytes(format.as_bytes()), byte(picture.picture_type), 
            string(picture.description), delim(0), bytes(picture.data)); 
}

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
                    Ok(try!(String::from_utf8(bytes.to_vec())))
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


macro_rules! assert_data {
    ($bytes:ident) => {
        if $bytes.len() == 0 {
            return Err(::Error::new(::ErrorKind::Parsing, "frame does not contain any data"))
        }
    }
}


macro_rules! find_delim {
    ($bytes:ident, $encoding:expr, $i:ident, $terminated:expr) => {
        if !$terminated {
            ($bytes.len(), $bytes.len())
        } else {
            match ::util::find_delim($encoding, $bytes, $i) {
                Some(i) => (i, i + ::util::delim_len($encoding)),
                None => return Err(::Error::new(::ErrorKind::Parsing, "delimiter not found"))
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

            if start == end {
                "".to_string()
            } else {
                try!(($params.string_func)(&$bytes[start..end]))
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
            $i = with_delim; Some(&$i);

            if start == end {
                "".to_string()
            } else {
                try!(($params.string_func)(&$bytes[start..end]))
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
            try!(::util::string_from_latin1(&$bytes[start..$i]))
        }
    };
    ($bytes:ident, $params:ident, $i:ident, latin1($terminated:expr)) => {
        {
            let start = $i;
            let (end, with_delim) = find_delim!($bytes, Encoding::Latin1, $i, $terminated);
            $i = with_delim; Some(&$i);
            try!(String::from_utf8($bytes[start..end].to_vec()))
        }
    };
    ($bytes:ident, $params:ident, $i:ident, picture_type()) => {
        {
            if $i + 1 >= $bytes.len() {
                return Err(::Error::new(::ErrorKind::Parsing, "insufficient data"));
            }

            let start = $i;
            $i += 1;

            let picture_type = match PictureType::from_u8($bytes[start]) {
                Some(t) => t,
                None => return Err(::Error::new(::ErrorKind::Parsing, "invalid picture type"))
            };
            picture_type
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

            let encoding = match Encoding::from_u8($bytes[0]) {
                Some(encoding) => encoding,
                None => return Err(::Error::new(::ErrorKind::Parsing, "invalid encoding byte"))
            };

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

    let mut picture = Picture::new();
   
    let encoding = match Encoding::from_u8(data[0]) {
        Some(encoding) => encoding,
        None => return Err(::Error::new(::ErrorKind::Parsing, "invalid encoding byte"))
    };

    let params = DecodingParams::for_encoding(encoding);
   
    let mut i = 1;
    let format = decode_part!(data, params, i, fixed_string(3));
    picture.mime_type = match &format[..] {
        "PNG" => "image/png".to_owned(),
        "JPG" => "image/jpeg".to_owned(),
        other => {
            debug!("can't determine MIME type for `{}`", other);
            return Err(::Error::new(::ErrorKind::UnsupportedFeature, 
                                     "can't determine MIME type for image format"))
        }
    }; 

    picture.picture_type = decode_part!(data, params, i, picture_type());
    picture.description = decode_part!(data, params, i, string(true));
    picture.data = decode_part!(data, params, i, bytes());

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
    let encoding = match Encoding::from_u8(data[0]) {
        Some(encoding) => encoding,
        None => return Err(::Error::new(::ErrorKind::Parsing, "invalid encoding byte"))
    };

    let params = DecodingParams::for_encoding(encoding);
    let mut i = 1;
    Ok(DecoderResult::new(encoding, Content::Text(decode_part!(data, params, i, text()))))
}

/// Attempts to parse the data as a user defined text frame.
/// Returns an `Content::ExtendedText`.
fn parse_txxx(data: &[u8]) -> ::Result<DecoderResult> {
    return decode!(data, ExtendedText, key: string(true), value: string(false));
}

/// Attempts to parse the data as a web link frame.
/// Returns a `Content::Link`.
fn parse_weblink(data: &[u8]) -> ::Result<DecoderResult> {
    Ok(DecoderResult::new(Encoding::Latin1, Content::Link(try!(String::from_utf8(data.to_vec())))))
}

/// Attempts to parse the data as a user defined web link frame.
/// Returns an `Content::ExtendedLink`.
fn parse_wxxx(data: &[u8]) -> ::Result<DecoderResult> {
    assert_data!(data);

    let encoding = match Encoding::from_u8(data[0]) {
        Some(encoding) => encoding,
        None => return Err(::Error::new(::ErrorKind::Parsing, "invalid encoding byte"))
    };

    let params = DecodingParams::for_encoding(encoding);
    let mut i = 1;
    let description = decode_part!(data, params, i, string(true));

    let uparams = DecodingParams::for_encoding(Encoding::Latin1);
    let url = decode_part!(data, uparams, i, string(false));

    let elink = ExtendedLink {description: description, link: url};
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
    use parsers;
    use parsers::{DecoderRequest, EncoderRequest};
    use frame::{self, Picture, PictureType, Encoding};
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
        assert!(parsers::decode(DecoderRequest { id: "PIC", data: &[] } ).is_err());

        let mut format_map = HashMap::new();
        format_map.insert("image/jpeg", "JPG");
        format_map.insert("image/png", "PNG");

        for (mime_type, format) in format_map.into_iter() {
            for description in vec!("", "description").into_iter() {
                let picture_type = PictureType::CoverFront;
                let picture_data = vec!(0xF9, 0x90, 0x3A, 0x02, 0xBD);

                let mut picture = Picture::new();
                picture.mime_type = mime_type.to_owned();
                picture.picture_type = picture_type;
                picture.description = description.to_owned();
                picture.data = picture_data.clone();

                for encoding in vec!(Encoding::Latin1, Encoding::UTF16).into_iter() {
                    println!("`{}`, `{}`, `{:?}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(format.bytes());
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(picture_data.iter().cloned());

                    assert_eq!(*parsers::decode(DecoderRequest { 
                        id: "PIC", 
                        data: &data[..]
                    }).unwrap().content.picture(), picture);
                    assert_eq!(parsers::encode(EncoderRequest { 
                        encoding: encoding, 
                        content: &Content::Picture(picture.clone()), 
                        version: 2 
                    }), data);
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
                picture.mime_type = mime_type.to_owned();
                picture.picture_type = picture_type;
                picture.description = description.to_owned();
                picture.data = picture_data.clone();

                for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{:?}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(mime_type.bytes());
                    data.push(0x0);
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(picture_data.iter().cloned());
                    
                    assert_eq!(*parsers::decode(DecoderRequest { 
                        id: "APIC", 
                        data: &data[..]
                    }).unwrap().content.picture(), picture);
                    assert_eq!(parsers::encode(EncoderRequest { 
                        encoding: encoding, 
                        content: &Content::Picture(picture.clone()), 
                        version: 3 }), data);
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
                for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{:?}`", description, comment, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(b"eng".iter().cloned());
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(comment, encoding).into_iter());

                    let content = frame::Comment { 
                        lang: "eng".to_owned(), 
                        description: description.to_owned(), 
                        text: comment.to_owned() 
                    };
                    assert_eq!(*parsers::decode(DecoderRequest { 
                        id: "COMM", 
                        data: &data[..]
                    }).unwrap().content.comment(), content);
                    assert_eq!(parsers::encode(EncoderRequest { 
                        encoding: encoding, 
                        content: &Content::Comment(content), version: 3 
                    }), data);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let comment = "comment";
        for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(comment, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { 
                id: "COMM", 
                data: &data[..]
            }).is_err());
        }
        println!("Empty description");
        let comment = "comment";
        for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(delim_for_encoding(encoding));
            data.extend(bytes_for_encoding(comment, encoding).into_iter());
            let content = frame::Comment {
                lang: "eng".to_owned(),
                description: "".to_owned(),
                text: comment.to_owned()
            };
            println!("data == {:?}", data);
            println!("content == {:?}", content);
            assert_eq!(*parsers::decode(DecoderRequest {
                id: "COMM",
                data: &data[..]
            }).unwrap().content.comment(), content);
        }
    }

    #[test]
    fn test_text() {
        assert!(parsers::decode(DecoderRequest { id: "TALB", data: &[] } ).is_err());

        for text in vec!("", "text").into_iter() {
            for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                println!("`{}`, `{:?}`", text, encoding);
                let mut data = Vec::new();
                data.push(encoding as u8);
                data.extend(bytes_for_encoding(text, encoding).into_iter());

                assert_eq!(&parsers::decode(DecoderRequest { 
                    id: "TALB", 
                    data: &data[..]
                }).unwrap().content.text()[..], text);
                assert_eq!(parsers::encode(EncoderRequest { 
                    encoding: encoding, 
                    content: &Content::Text(text.to_owned()), 
                    version: 3 
                } ), data);
            }
        }
    }

    #[test]
    fn test_null_terminated_text() {
        assert!(parsers::decode(DecoderRequest { id: "TRCK", data: &[] } ).is_err());
        let text = "text\u{0}\u{0}";
        for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{}`, `{:?}`", text, encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(text, encoding).into_iter());

            assert_eq!(&parsers::decode(DecoderRequest {
                id: "TALB",
                data: &data[..]
            }).unwrap().content.text()[..], "text");
            assert_eq!(parsers::encode(EncoderRequest {
                encoding: encoding,
                content: &Content::Text(text.to_owned()),
                version: 3
            } ), data);
        }
    }

    #[test]
    fn test_txxx() {
        assert!(parsers::decode(DecoderRequest { id: "TXXX", data: &[] } ).is_err());

        println!("valid");
        for key in vec!("", "key").into_iter() {
            for value in vec!("", "value").into_iter() {
                for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("{:?}", encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(bytes_for_encoding(key, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(value, encoding).into_iter());

                    let content = frame::ExtendedText { 
                        key: key.to_owned(), 
                        value: value.to_owned() 
                    };
                    assert_eq!(*parsers::decode(DecoderRequest { 
                        id: "TXXX", 
                        data: &data[..]
                    }).unwrap().content.extended_text(), content);
                    assert_eq!(parsers::encode(EncoderRequest { 
                        encoding: encoding, 
                        content: &Content::ExtendedText(content), 
                        version: 3
                    }), data);
                }
            }
        }

        println!("invalid");
        let key = "key";
        let value = "value";
        for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(key, encoding).into_iter());
            data.extend(bytes_for_encoding(value, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { 
                id: "TXXX", 
                data: &data[..]
            }).is_err());
        }
    }

    #[test]
    fn test_weblink() {
        for link in vec!("", "http://www.rust-lang.org/").into_iter() {
            println!("`{:?}`", link);
            let data = link.as_bytes().to_vec();

            assert_eq!(&parsers::decode(DecoderRequest { 
                id: "WOAF", 
                data: &data[..]
            }).unwrap().content.link()[..], link);
            assert_eq!(parsers::encode(EncoderRequest { 
                encoding: Encoding::Latin1, 
                content: &Content::Link(link.to_owned()), 
                version: 3 
            }), data);
        }
    }

    #[test]
    fn test_wxxx() {
        assert!(parsers::decode(DecoderRequest { id: "WXXX", data: &[] } ).is_err());

        println!("valid");
        for description in vec!("", "rust").into_iter() {
            for link in vec!("", "http://www.rust-lang.org/").into_iter() { 
                for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{:?}`", description, link, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(link, Encoding::Latin1).into_iter());

                    let content = frame::ExtendedLink { 
                        description: description.to_owned(), 
                        link: link.to_owned() 
                    };
                    assert_eq!(*parsers::decode(DecoderRequest { 
                        id: "WXXX", 
                        data: &data[..]
                    }).unwrap().content.extended_link(), content);
                    assert_eq!(parsers::encode(EncoderRequest { 
                        encoding: encoding, 
                        content: &Content::ExtendedLink(content), 
                        version: 3
                    }), data);
                }
            }
        }

        println!("invalid");
        let description = "rust";
        let link = "http://www.rust-lang.org/";
        for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(link, Encoding::Latin1).into_iter());
            assert!(parsers::decode(DecoderRequest { 
                id: "WXXX", 
                data: &data[..]
            }).is_err());
        }
    }

    #[test]
    fn test_uslt() {
        assert!(parsers::decode(DecoderRequest { id: "USLT", data: &[] } ).is_err());

        println!("valid");
        for description in vec!("", "description").into_iter() {
            for text in vec!("", "lyrics").into_iter() {
                for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}, `{:?}`", description, text, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(b"eng".iter().cloned());
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(text, encoding).into_iter());

                    let content = frame::Lyrics { 
                        lang: "eng".to_owned(), 
                        description: description.to_owned(), 
                        text: text.to_owned() 
                    };
                    assert_eq!(*parsers::decode(DecoderRequest { 
                        id: "USLT", 
                        data: &data[..]
                    }).unwrap().content.lyrics(), content);
                    assert_eq!(parsers::encode(EncoderRequest { 
                        encoding: encoding, 
                        content: &Content::Lyrics(content), 
                        version: 3 
                    }), data);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let lyrics = "lyrics";
        for encoding in vec!(Encoding::Latin1, Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(b"eng".iter().cloned());
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(lyrics, encoding).into_iter());
            assert!(parsers::decode(DecoderRequest { 
                id: "USLT", 
                data: &data[..]
            }).is_err());
        }
    }
}
// }}}
