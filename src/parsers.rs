extern crate audiotag;

use self::audiotag::{TagError, TagResult, InvalidInputError, StringDecodingError, UnsupportedFeatureError};

use picture::Picture;
use frame::{Content, Encoding};
use frame::Content::{
    PictureContent, CommentContent, TextContent, ExtendedTextContent, LyricsContent,
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
fn text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let text = request.content.text().as_slice();
    match request.encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + text.len());
            data.push(request.encoding as u8);
            data.push_all(text.as_bytes());
            data
        },
        Encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + 2 + text.len() * 2);
            data.push(request.encoding as u8);
            data.extend(util::string_to_utf16(text).into_iter());
            data
        }
        Encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + text.len() * 2);
            data.push(request.encoding as u8);
            data.extend(util::string_to_utf16be(text).into_iter());
            data
        }
    }
}

fn extended_text_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let &(ref key, ref value) = request.content.extended_text(); 
    match request.encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + key.len() + 1 + value.len());
            data.push(request.encoding as u8);
            data.push_all(key.as_bytes());
            data.push(0x0);
            data.push_all(value.as_bytes());
            data
        },
        Encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + (2 + key.len() * 2) + 2 + (2 + value.len() * 2));
            data.push(request.encoding as u8);
            data.extend(util::string_to_utf16(key.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16(value.as_slice()).into_iter());
            data
        },
        Encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + key.len() * 2 + 2 + value.len() * 2);
            data.push(request.encoding as u8);
            data.extend(util::string_to_utf16be(key.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16be(value.as_slice()).into_iter());
            data
        }
    }
}

fn weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    request.content.link().as_bytes().to_vec()
}

fn extended_weblink_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let &(ref key, ref value) = request.content.extended_link(); 
    match request.encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + key.len() + 1 + value.len());
            data.push(request.encoding as u8);
            data.push_all(key.as_bytes());
            data.push(0x0);
            data.push_all(value.as_bytes());
            data
        },
        Encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + (2 + key.len() * 2) + 2 + (2 + value.len() * 2));
            data.push(request.encoding as u8);
            data.extend(util::string_to_utf16(key.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16(value.as_slice()).into_iter());
            data
        },
        Encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + key.len() * 2 + 2 + value.len() * 2);
            data.push(request.encoding as u8);
            data.extend(util::string_to_utf16be(key.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16be(value.as_slice()).into_iter());
            data
        }
    }
}

fn lyrics_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let &(ref description, ref text) = request.content.lyrics();
    match request.encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + 3 + 1 + text.len());
            data.push(request.encoding as u8);
            data.push_all(b"eng");
            data.push_all(description.as_bytes());
            data.push(0x0); 
            data.push_all(text.as_bytes());
            data
        },
        Encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + 3 + 2 + (2 + text.len() * 2));
            data.push(request.encoding as u8);
            data.push_all(b"eng");
            data.extend(util::string_to_utf16(description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]); 
            data.extend(util::string_to_utf16(text.as_slice()).into_iter());
            data
        },
        Encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + 3 + 2 + (text.len() * 2));
            data.push(request.encoding as u8);
            data.push_all(b"eng");
            data.extend(util::string_to_utf16be(description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16be(text.as_slice()).into_iter());
            data
        }
    }
}

fn comment_to_bytes(request: EncoderRequest) -> Vec<u8> {
    let &(ref description, ref text) = request.content.comment();
    match request.encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + 3 + description.len() + 1 + text.len());
            data.push(request.encoding as u8);
            data.push_all(b"eng");
            data.push_all(description.as_bytes());
            data.push(0x0);
            data.push_all(text.as_bytes());
            data
        },
        Encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + 3 + (2 + description.len() * 2) + 2 + (2 + text.len() * 2));
            data.push(request.encoding as u8);
            data.push_all(b"eng");
            data.extend(util::string_to_utf16(description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16(text.as_slice()).into_iter());
            data
        },
        Encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + 3 + (description.len() * 2) + 2 + (text.len() * 2));
            data.push(request.encoding as u8);
            data.push_all(b"eng".as_slice());
            data.extend(util::string_to_utf16be(description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.extend(util::string_to_utf16be(text.as_slice()).into_iter());
            data
        }
    }
}

fn picture_to_bytes_v3(request: EncoderRequest) -> Vec<u8> {
    let picture = request.content.picture();

    match request.encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + picture.mime_type.len() + 1 + 1 + picture.description.len() + 1 + picture.data.len());
            data.push(request.encoding as u8);
            data.push_all(picture.mime_type.as_bytes());
            data.push(0x0);
            data.push(picture.picture_type as u8);
            data.push_all(picture.description.as_bytes());
            data.push(0x0);
            data.push_all(picture.data.as_slice());
            data
        },
        Encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + picture.mime_type.len() + 1 + 1 + (2 + picture.description.len() * 2) + 2 + picture.data.len());
            data.push(request.encoding as u8);
            data.push_all(picture.mime_type.as_bytes());
            data.push(0x0);
            data.push(picture.picture_type as u8);
            data.extend(util::string_to_utf16(picture.description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.push_all(picture.data.as_slice());
            data
        },
        Encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + picture.mime_type.len() + 1 + 1 + (picture.description.len() * 2) + 2 + picture.data.len());
            data.push(request.encoding as u8);
            data.push_all(picture.mime_type.as_bytes());
            data.push(0x0);
            data.push(picture.picture_type as u8);
            data.extend(util::string_to_utf16be(picture.description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
            data.push_all(picture.data.as_slice());
            data
        }
    }
}

fn picture_to_bytes_v2(request: EncoderRequest) -> Vec<u8> {
    let picture = request.content.picture();

    let format = match picture.mime_type.as_slice() {
        "image/jpeg" => "JPG",
        "image/png" => "PNG",
        _ => panic!("unknown MIME type") // TODO handle this better
    };

    let mut data = match request.encoding {
        Encoding::Latin1 => Vec::with_capacity(1 + 3 + 1 + picture.description.len() + 1 + picture.data.len()),
        _ => Vec::with_capacity(1 + 3 + 1 + (2 + picture.description.len() * 2) + 2 + picture.data.len()),
    };

    data.push(request.encoding as u8);
    data.push_all(format.as_bytes());
    data.push(picture.picture_type as u8);

    match request.encoding {
        Encoding::Latin1 => {
            data.push_all(picture.description.as_bytes());
            data.push(0x0);
        },
        _ => { // ignore other encodings and just encode as UTF16
            data.extend(util::string_to_utf16(picture.description.as_slice()).into_iter());
            data.push_all(&[0x0, 0x0]);
        },
    }

    data.push_all(picture.data.as_slice());
    data
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
/// Attempts to parse the data as an ID3v2.2 picture frame.
/// Returns a `PictureContent`.
fn parse_apic_v2(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }
    
    let mut picture = Picture::new();

    let encoding = try_encoding!(data[0]);

    let format = match String::from_utf8(data.slice(1, 4).to_vec()) {
        Ok(format) => format,
        Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "image format is not valid utf8"))
    };

    picture.mime_type = match format.as_slice() {
        "PNG" => "image/png".into_string(),
        "JPG" => "image/jpeg".into_string(),
        other => {
            debug!("can't determine MIME type for `{}`", other);
            return Err(TagError::new(UnsupportedFeatureError, "can't determine MIME type for image format"))
        }
    }; 

    match FromPrimitive::from_u8(data[4]) {
        Some(t) => picture.picture_type = t,
        None => return Err(TagError::new(InvalidInputError, "invalid picture type"))
    };

    let start = 5;
    let mut i = try_delim!(encoding, data.as_slice(), start, "missing image description terminator");
    picture.description = try_string!(encoding, data.slice(start, i));

    i += util::delim_len(encoding);

    picture.data = data.slice_from(i).to_vec();

    Ok(DecoderResult::new(encoding, PictureContent(picture)))
}

/// Attempts to parse the data as an ID3v2.3/ID3v2.4 picture frame.
/// Returns a `PictureContent`.
fn parse_apic_v3(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }
    
    let mut picture = Picture::new();

    let encoding = try_encoding!(data[0]);

    let mut i = 1;
    let mut start = i;

    i = try_delim!(Encoding::Latin1, data.as_slice(), i, "missing image mime type terminator"); 

    picture.mime_type = try_string!(Encoding::Latin1, data.slice(start, i));

    i += 1;

    match FromPrimitive::from_u8(data[i]) {
        Some(t) => picture.picture_type = t,
        None => return Err(TagError::new(InvalidInputError, "invalid picture type"))
    };

    i += 1;
    start = i;

    i = try_delim!(encoding, data.as_slice(), i, "missing image description terminator");

    picture.description = try_string!(encoding, data.slice(start, i));

    i += util::delim_len(encoding);

    picture.data = data.slice_from(i).to_vec();

    Ok(DecoderResult::new(encoding, PictureContent(picture)))
}

/// Attempts to parse the data as a comment frame.
/// Returns a `CommentContent`.
fn parse_comm(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);

    let i = try_delim!(encoding, data.as_slice(), 4, "missing comment delimiter");

    let description = try_string!(encoding, data.slice(4, i));
    let text = try_string!(encoding, data.slice_from(i + util::delim_len(encoding)));

    Ok(DecoderResult::new(encoding, CommentContent((description, text))))
}

/// Attempts to parse the data as a text frame.
/// Returns a `TextContent`.
fn parse_text(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);
    let parsed = TextContent(try_string!(encoding, data.slice_from(1)));

    Ok(DecoderResult::new(encoding, parsed))
}

/// Attempts to parse the data as a user defined text frame.
/// Returns an `ExtendedTextContent`.
fn parse_txxx(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]); 

    let i = try_delim!(encoding, data.as_slice(), 1, "missing extended text delimiter"); 

    let key = try_string!(encoding, data.slice(1, i));
    let val = try_string!(encoding, data.slice_from(i + util::delim_len(encoding)));

    Ok(DecoderResult::new(encoding, ExtendedTextContent((key, val))))
}

/// Attempts to parse the data as a web link frame.
/// Returns a `LinkContent`.
fn parse_weblink(data: &[u8]) -> TagResult<DecoderResult> {
    Ok(DecoderResult::new(Encoding::Latin1, LinkContent(try_string!(Encoding::Latin1, data))))
}

/// Attempts to parse the data as a user defined web link frame.
/// Returns an `ExtendedLinkContent`.
fn parse_wxxx(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]); 

    let i = try_delim!(encoding, data.as_slice(), 1, "missing extended web frame delimiter"); 

    let key = try_string!(encoding, data.slice(1, i));
    let val = try_string!(encoding, data.slice_from(i + util::delim_len(encoding)));

    Ok(DecoderResult::new(encoding, ExtendedLinkContent((key, val))))
}

/// Attempts to parse the data as an unsynchronized lyrics text frame.
/// Returns a `LyricsContent`.
fn parse_uslt(data: &[u8]) -> TagResult<DecoderResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);

    // 4 to skip encoding byte and lang string
    let mut i = try_delim!(encoding, data.as_slice(), 4, "missing lyrics description terminator");

    let description = try_string!(encoding, data.slice(4, i));
   
    i += util::delim_len(encoding);

    let text = try_string!(encoding, data.slice_from(i));

    Ok(DecoderResult::new(encoding, LyricsContent((description, text))))
}
// }}}

// Tests {{{
#[cfg(test)]
mod tests {
    use parsers;
    use parsers::{DecoderRequest, EncoderRequest};
    use util;
    use frame::Encoding;
    use frame::Content::{PictureContent, CommentContent, TextContent, ExtendedTextContent, LinkContent, ExtendedLinkContent, LyricsContent};
    use picture::{Picture, PictureType};
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

                    let pair = (description.into_string(), comment.into_string());
                    assert_eq!(*parsers::decode(DecoderRequest { id: "COMM", data: data.as_slice() } ).unwrap().content.comment(), pair);
                
                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &CommentContent(pair), version: 3 }), data);
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
                assert_eq!(parsers::decode(DecoderRequest { id: "TALB", data: data.as_slice() } ).unwrap().content.text().as_slice(), text);

                assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &TextContent(text.into_string()), version: 3 } ), data);
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

                    let pair = (key.into_string(), value.into_string());
                    assert_eq!(*parsers::decode(DecoderRequest { id: "TXXX", data: data.as_slice() } ).unwrap().content.extended_text(), pair);

                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &ExtendedTextContent(pair), version: 3 } ), data);
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
            assert_eq!(parsers::decode(DecoderRequest { id: "WOAF", data: data.as_slice() } ).unwrap().content.link().as_slice(), link);

            assert_eq!(parsers::encode(EncoderRequest { encoding: Encoding::Latin1, content: &LinkContent(link.into_string()), version: 3 } ), data);
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

                    let pair = (description.into_string(), link.into_string());
                    assert_eq!(*parsers::decode(DecoderRequest { id: "WXXX", data: data.as_slice() } ).unwrap().content.extended_link(), pair);

                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &ExtendedLinkContent(pair), version: 3 } ), data);
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
            for lyrics in vec!("", "lyrics").into_iter() {
                for encoding in vec!(Encoding::UTF8, Encoding::UTF16, Encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}, `{}`", description, lyrics, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(b"eng");
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(lyrics, encoding).into_iter());

                    let pair = (description.into_string(), lyrics.into_string());
                    assert_eq!(*parsers::decode(DecoderRequest { id: "USLT", data: data.as_slice() } ).unwrap().content.lyrics(), pair);

                    assert_eq!(parsers::encode(EncoderRequest { encoding: encoding, content: &LyricsContent(pair), version: 3 } ), data);
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
