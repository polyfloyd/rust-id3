extern crate audiotag;

use self::audiotag::{TagError, TagResult, InvalidInputError};

use picture::Picture;
use frame::{Contents, PictureContent, CommentContent, TextContent, ExtendedTextContent, LinkContent, ExtendedLinkContent, LyricsContent};
use encoding;
use util;

/// The result of a successfully parsed frame.
pub struct ParserResult {
    /// The text encoding used in the frame.
    pub encoding: encoding::Encoding,
    /// The parsed contents of the frame.
    pub contents: Contents 
}

impl ParserResult {
    /// Creates a new `ParserResult` with the provided encoding and contents.
    pub fn new(encoding: encoding::Encoding, contents: Contents) -> ParserResult {
        ParserResult { encoding: encoding, contents: contents }
    }
}

// Encoders {{{
/// Returns a vector representation of a text frame.
pub fn text_to_bytes(encoding: encoding::Encoding, text: &str) -> Vec<u8> {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + text.len());
            data.push(encoding as u8);
            data.extend(String::from_str(text).into_bytes().into_iter());
            data
        },
        encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + 2 + text.len() * 2);
            data.push(encoding as u8);
            data.extend(util::string_to_utf16(text).into_iter());
            data
        }
        encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + text.len() * 2);
            data.push(encoding as u8);
            data.extend(util::string_to_utf16be(text).into_iter());
            data
        }
    }
}

/// Returns a vector representation of a TXXX frame.
pub fn extended_text_to_bytes(encoding: encoding::Encoding, key: &str, value: &str) -> Vec<u8> {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + key.len() + 1 + value.len());
            data.push(encoding as u8);
            data.extend(String::from_str(key).into_bytes().into_iter());
            data.push(0x0);
            data.extend(String::from_str(value).into_bytes().into_iter());
            data
        },
        encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + (2 + key.len() * 2) + 2 + (2 + value.len() * 2));
            data.push(encoding as u8);
            data.extend(util::string_to_utf16(key).into_iter());
            data.push_all([0x0, 0x0]);
            data.extend(util::string_to_utf16(value).into_iter());
            data
        },
        encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + key.len() * 2 + 2 + value.len() * 2);
            data.push(encoding as u8);
            data.extend(util::string_to_utf16be(key).into_iter());
            data.push_all([0x0, 0x0]);
            data.extend(util::string_to_utf16be(value).into_iter());
            data
        }
    }
}

/// Returns a vector representation of a web link frame.
pub fn weblink_to_bytes(url: &str) -> Vec<u8> {
    String::from_str(url).into_bytes()
}

/// Returns a vector representation of a WXXX frame.
pub fn extended_weblink_to_bytes(encoding: encoding::Encoding, description: &str, link: &str) -> Vec<u8> {
    extended_text_to_bytes(encoding, description, link)
}

/// Returns a vector representation of a USLT frame.
pub fn lyrics_to_bytes(encoding: encoding::Encoding, description: &str, text: &str) -> Vec<u8> {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + 3 + 1 + text.len());
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(String::from_str(description).into_bytes().into_iter());
            data.push(0x0); 
            data.extend(String::from_str(text).into_bytes().into_iter());
            data
        },
        encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + 3 + 2 + (2 + text.len() * 2));
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(util::string_to_utf16(description).into_iter());
            data.push_all([0x0, 0x0]); 
            data.extend(util::string_to_utf16(text).into_iter());
            data
        },
        encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + 3 + 2 + (text.len() * 2));
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(util::string_to_utf16be(description).into_iter());
            data.push_all([0x0, 0x0]);
            data.extend(util::string_to_utf16be(text).into_iter());
            data
        }
    }
}

/// Returns a vector representation of a COMM frame.
pub fn comment_to_bytes(encoding: encoding::Encoding, description: &str, text: &str) -> Vec<u8> {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + 3 + description.len() + 1 + text.len());
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(String::from_str(description).into_bytes().into_iter());
            data.push(0x0);
            data.extend(String::from_str(text).into_bytes().into_iter());
            data
        },
        encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + 3 + (2 + description.len() * 2) + 2 + (2 + text.len() * 2));
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(util::string_to_utf16(description).into_iter());
            data.push_all([0x0, 0x0]);
            data.extend(util::string_to_utf16(text).into_iter());
            data
        },
        encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + 3 + (description.len() * 2) + 2 + (text.len() * 2));
            data.push(encoding as u8);
            data.push_all(b"eng".as_slice());
            data.extend(util::string_to_utf16be(description).into_iter());
            data.push_all([0x0, 0x0]);
            data.extend(util::string_to_utf16be(text).into_iter());
            data
        }
    }
}

/// Returns a vector representation of an APIC frame.
pub fn picture_to_bytes(encoding: encoding::Encoding, picture: &Picture) -> Vec<u8> {
    match encoding {
        encoding::Latin1 | encoding::UTF8 => {
            let mut data = Vec::with_capacity(1 + picture.mime_type.len() + 1 + 1 + picture.description.len() + 1 + picture.data.len());
            data.push(encoding as u8);
            data.extend(picture.mime_type.clone().into_bytes().into_iter());
            data.push(0x0);
            data.push(picture.picture_type as u8);
            data.extend(picture.description.clone().into_bytes().into_iter());
            data.push(0x0);
            data.push_all(picture.data.as_slice());
            data
        },
        encoding::UTF16 => {
            let mut data = Vec::with_capacity(1 + picture.mime_type.len() + 1 + 1 + (2 + picture.description.len() * 2) + 2 + picture.data.len());
            data.push(encoding as u8);
            data.extend(picture.mime_type.clone().into_bytes().into_iter());
            data.push(0x0);
            data.push(picture.picture_type as u8);
            data.extend(util::string_to_utf16(picture.description.as_slice()).into_iter());
            data.push_all([0x0, 0x0]);
            data.push_all(picture.data.as_slice());
            data
        },
        encoding::UTF16BE => {
            let mut data = Vec::with_capacity(1 + picture.mime_type.len() + 1 + 1 + (picture.description.len() * 2) + 2 + picture.data.len());
            data.push(encoding as u8);
            data.extend(picture.mime_type.clone().into_bytes().into_iter());
            data.push(0x0);
            data.push(picture.picture_type as u8);
            data.extend(util::string_to_utf16be(picture.description.as_slice()).into_iter());
            data.push_all([0x0, 0x0]);
            data.push_all(picture.data.as_slice());
            data
        }
    }
}
// }}}

// Decoders {{{
/// Attempts to parse the data as a picture frame.
/// Returns a `PictureContent`.
pub fn parse_apic(data: &[u8]) -> TagResult<ParserResult> {
    let mut picture = Picture::new();

    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);

    let mut i = 1;
    let mut start = i;

    i = try_delim!(encoding::Latin1, data.as_slice(), i, "missing image mime type terminator"); 

    picture.mime_type = try_string!(encoding::Latin1, data.slice(start, i).to_vec());

    i += 1;

    match FromPrimitive::from_u8(data[i]) {
        Some(t) => picture.picture_type = t,
        None => return Err(TagError::new(InvalidInputError, "invalid picture type"))
    };

    i += 1;
    start = i;

    i = try_delim!(encoding, data.as_slice(), i, "missing image description terminator");

    picture.description = try_string!(encoding, data.slice(start, i).to_vec()); 

    i += util::delim_len(encoding);

    picture.data = data.slice_from(i).to_vec();

    Ok(ParserResult::new(encoding, PictureContent(picture)))
}

/// Attempts to parse the data as a comment frame.
/// Returns a `CommentContent`.
pub fn parse_comm(data: &[u8]) -> TagResult<ParserResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);

    let i = try_delim!(encoding, data.as_slice(), 4, "missing comment delimiter");

    let description = try_string!(encoding, data.slice(4, i).to_vec());
    let text = try_string!(encoding, data.slice_from(i + util::delim_len(encoding)).to_vec());

    Ok(ParserResult::new(encoding, CommentContent((description, text))))
}

/// Attempts to parse the data as a text frame.
/// Returns a `TextContent`.
pub fn parse_text(data: &[u8]) -> TagResult<ParserResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);
    let parsed = TextContent(try_string!(encoding, data.slice_from(1).to_vec()));

    Ok(ParserResult::new(encoding, parsed))
}

/// Attempts to parse the data as a user defined text frame.
/// Returns an `ExtendedTextContent`.
pub fn parse_txxx(data: &[u8]) -> TagResult<ParserResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]); 

    let i = try_delim!(encoding, data.as_slice(), 1, "missing extended text delimiter"); 

    let key = try_string!(encoding, data.slice(1, i).to_vec());
    let val = try_string!(encoding, data.slice_from(i + util::delim_len(encoding)).to_vec());

    Ok(ParserResult::new(encoding, ExtendedTextContent((key, val))))
}

/// Attempts to parse the data as a web link frame.
/// Returns a `LinkContent`.
pub fn parse_weblink(data: &[u8]) -> TagResult<ParserResult> {
    Ok(ParserResult::new(encoding::Latin1, LinkContent(try_string!(encoding::Latin1, data.to_vec()))))
}

/// Attempts to parse the data as a user defined web link frame.
/// Returns an `ExtendedLinkContent`.
pub fn parse_wxxx(data: &[u8]) -> TagResult<ParserResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]); 

    let i = try_delim!(encoding, data.as_slice(), 1, "missing extended web frame delimiter"); 

    let key = try_string!(encoding, data.slice(1, i).to_vec());
    let val = try_string!(encoding, data.slice_from(i + util::delim_len(encoding)).to_vec());

    Ok(ParserResult::new(encoding, ExtendedLinkContent((key, val))))
}

/// Attempts to parse the data as an unsynchronized lyrics text frame.
/// Returns a `LyricsContent`.
pub fn parse_uslt(data: &[u8]) -> TagResult<ParserResult> {
    if data.len() == 0 {
        return Err(TagError::new(InvalidInputError, "frame does not contain any data"))
    }

    let encoding = try_encoding!(data[0]);

    // 4 to skip encoding byte and lang string
    let i = try_delim!(encoding, data.as_slice(), 4, "missing lyrics description terminator") + util::delim_len(encoding);

    Ok(ParserResult::new(encoding, LyricsContent(try_string!(encoding, data.slice_from(i).to_vec())))) 
}
// }}}

// Tests {{{
#[cfg(test)]
mod tests {
    use parsers;
    use encoding;
    use util;
    use picture::{Picture, picture_type};

    fn bytes_for_encoding(text: &str, encoding: encoding::Encoding) -> Vec<u8> {
        match encoding {
            encoding::Latin1 | encoding::UTF8 => String::from_str(text).into_bytes(),
            encoding::UTF16 => util::string_to_utf16(text),
            encoding::UTF16BE => util::string_to_utf16be(text)
        }
    }

    fn delim_for_encoding(encoding: encoding::Encoding) -> Vec<u8> {
        match encoding {
            encoding::Latin1 | encoding::UTF8 => Vec::from_elem(1, 0),
            encoding::UTF16 | encoding::UTF16BE => Vec::from_elem(2, 0)
        }
    }

    #[test]
    fn test_apic() {
        assert!(parsers::parse_apic([]).is_err());

        for mime_type in vec!("", "image/jpeg").into_iter() {
            for description in vec!("", "description").into_iter() {
                let picture_type = picture_type::CoverFront;
                let picture_data = vec!(0xF9, 0x90, 0x3A, 0x02, 0xBD);

                let mut picture = Picture::new();
                picture.mime_type = String::from_str(mime_type);
                picture.picture_type = picture_type;
                picture.description = String::from_str(description);
                picture.data = picture_data.clone();

                for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{}`", mime_type, description, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(String::from_str(mime_type).into_bytes().into_iter());
                    data.push(0x0);
                    data.push(picture_type as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.push_all(picture_data.as_slice());
                    assert_eq!(*parsers::parse_apic(data.as_slice()).unwrap().contents.picture(), picture);
                    assert_eq!(parsers::picture_to_bytes(encoding, &picture), data);
                }
            }
        }
    }

    #[test]
    fn test_comm() {
        assert!(parsers::parse_comm([]).is_err());

        println!("valid");
        for description in vec!("", "description").into_iter() {
            for comment in vec!("", "comment").into_iter() {
                for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{}`", description, comment, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(b"eng");
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(comment, encoding).into_iter());
                    assert_eq!(*parsers::parse_comm(data.as_slice()).unwrap().contents.comment(), (String::from_str(description), String::from_str(comment)));
                    assert_eq!(parsers::comment_to_bytes(encoding, description, comment), data);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let comment = "comment";
        for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(comment, encoding).into_iter());
            assert!(parsers::parse_comm(data.as_slice()).is_err());
        }

    }

    #[test]
    fn test_text() {
        assert!(parsers::parse_text([]).is_err());

        for text in vec!("", "text").into_iter() {
            for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
                println!("`{}`, `{}`", text, encoding);
                let mut data = Vec::new();
                data.push(encoding as u8);
                data.extend(bytes_for_encoding(text, encoding).into_iter());
                assert_eq!(parsers::parse_text(data.as_slice()).unwrap().contents.text().as_slice(), text);
                assert_eq!(parsers::text_to_bytes(encoding, text), data);
            }
        }
    }

    #[test]
    fn test_txxx() {
        assert!(parsers::parse_txxx([]).is_err());

        println!("valid");
        for key in vec!("", "key").into_iter() {
            for value in vec!("", "value").into_iter() {
                for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
                    println!("{}", encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(bytes_for_encoding(key, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(value, encoding).into_iter());
                    assert_eq!(*parsers::parse_txxx(data.as_slice()).unwrap().contents.extended_text(), (String::from_str(key), String::from_str(value)));
                    assert_eq!(parsers::extended_text_to_bytes(encoding, key, value), data);
                }
            }
        }

        println!("invalid");
        let key = "key";
        let value = "value";
        for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(key, encoding).into_iter());
            data.extend(bytes_for_encoding(value, encoding).into_iter());
            assert!(parsers::parse_txxx(data.as_slice()).is_err());
        }
    }

    #[test]
    fn test_weblink() {
        for link in vec!("", "http://www.rust-lang.org/").into_iter() {
            println!("`{}`", link);
            let data = String::from_str(link).into_bytes();
            assert_eq!(parsers::parse_weblink(data.as_slice()).unwrap().contents.link().as_slice(), link);
            assert_eq!(parsers::weblink_to_bytes(link), data);
        }
    }

    #[test]
    fn test_wxxx() {
        assert!(parsers::parse_wxxx([]).is_err());

        println!("valid");
        for description in vec!("", "rust").into_iter() {
            for link in vec!("", "http://www.rust-lang.org/").into_iter() { 
                for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}`, `{}`", description, link, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(link, encoding).into_iter());
                    assert_eq!(*parsers::parse_wxxx(data.as_slice()).unwrap().contents.extended_link(), (String::from_str(description), String::from_str(link)));
                    assert_eq!(parsers::extended_weblink_to_bytes(encoding, description, link), data);
                }
            }
        }
        
        println!("invalid");
        let description = "rust";
        let link = "http://www.rust-lang.org/";
        for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(link, encoding).into_iter());
            assert!(parsers::parse_wxxx(data.as_slice()).is_err());
        }
    }

    #[test]
    fn test_uslt() {
        assert!(parsers::parse_uslt([]).is_err());

        println!("valid");
        for description in vec!("", "description").into_iter() {
            for lyrics in vec!("", "lyrics").into_iter() {
                for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
                    println!("`{}`, `{}, `{}`", description, lyrics, encoding);
                    let mut data = Vec::new();
                    data.push(encoding as u8);
                    data.push_all(b"eng");
                    data.extend(bytes_for_encoding(description, encoding).into_iter());
                    data.extend(delim_for_encoding(encoding).into_iter());
                    data.extend(bytes_for_encoding(lyrics, encoding).into_iter());
                    assert_eq!(parsers::parse_uslt(data.as_slice()).unwrap().contents.lyrics().as_slice(), lyrics);
                    assert_eq!(parsers::lyrics_to_bytes(encoding, description, lyrics), data);
                }
            }
        }

        println!("invalid");
        let description = "description";
        let lyrics = "lyrics";
        for encoding in vec!(encoding::UTF8, encoding::UTF16, encoding::UTF16BE).into_iter() {
            println!("`{}`", encoding);
            let mut data = Vec::new();
            data.push(encoding as u8);
            data.push_all(b"eng");
            data.extend(bytes_for_encoding(description, encoding).into_iter());
            data.extend(bytes_for_encoding(lyrics, encoding).into_iter());
            assert!(parsers::parse_uslt(data.as_slice()).is_err());
        }
    }
}
// }}}
