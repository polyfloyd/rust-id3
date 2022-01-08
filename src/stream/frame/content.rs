use crate::frame::{
    Comment, Content, EncapsulatedObject, ExtendedLink, ExtendedText, Lyrics, Picture, PictureType,
    SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat,
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

struct Encoder<W: io::Write> {
    w: W,
    version: tag::Version,
    encoding: Encoding,
}

impl<W: io::Write> Encoder<W> {
    fn bytes(&mut self, bytes: impl AsRef<[u8]>) -> crate::Result<()> {
        let bytes = bytes.as_ref();
        self.w.write_all(bytes)?;
        Ok(())
    }

    fn byte(&mut self, b: u8) -> crate::Result<()> {
        self.bytes(&[b])
    }

    fn delim(&mut self) -> crate::Result<()> {
        self.bytes(match self.encoding {
            Encoding::Latin1 | Encoding::UTF8 => &[0][..],
            Encoding::UTF16 | Encoding::UTF16BE => &[0, 0][..],
        })
    }

    fn string(&mut self, string: &str) -> crate::Result<()> {
        self.string_with_other_encoding(self.encoding, string)
    }

    fn string_with_other_encoding(
        &mut self,
        encoding: Encoding,
        string: &str,
    ) -> crate::Result<()> {
        match encoding {
            Encoding::Latin1 => self.bytes(string_to_latin1(string)),
            Encoding::UTF8 => self.bytes(string.as_bytes()),
            Encoding::UTF16 => self.bytes(string_to_utf16(string)),
            Encoding::UTF16BE => self.bytes(string_to_utf16be(string)),
        }
    }

    fn encoding(&mut self) -> crate::Result<()> {
        self.byte(match self.encoding {
            Encoding::Latin1 => 0,
            Encoding::UTF16 => 1,
            Encoding::UTF16BE => 2,
            Encoding::UTF8 => 3,
        })
    }

    fn text_content(&mut self, content: &str) -> crate::Result<()> {
        self.encoding()?;
        self.string(content)
    }

    fn extended_text_content(&mut self, content: &ExtendedText) -> crate::Result<()> {
        self.encoding()?;
        self.string(&content.description)?;
        self.delim()?;
        self.string(&content.value)
    }

    fn link_content(&mut self, content: &str) -> crate::Result<()> {
        self.bytes(content.as_bytes())
    }

    fn extended_link_content(&mut self, content: &ExtendedLink) -> crate::Result<()> {
        self.encoding()?;
        self.string(&content.description)?;
        self.delim()?;
        self.bytes(content.link.as_bytes())
    }

    fn encapsulated_object_content(&mut self, content: &EncapsulatedObject) -> crate::Result<()> {
        self.encoding()?;
        self.bytes(content.mime_type.as_bytes())?;
        self.byte(0)?;
        self.string(&content.filename)?;
        self.delim()?;
        self.string(&content.description)?;
        self.delim()?;
        self.bytes(&content.data)
    }

    fn lyrics_content(&mut self, content: &Lyrics) -> crate::Result<()> {
        self.encoding()?;
        self.bytes(
            content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>(),
        )?;
        self.string(&content.description)?;
        self.delim()?;
        self.string(&content.text)
    }

    fn synchronised_lyrics_content(&mut self, content: &SynchronisedLyrics) -> crate::Result<()> {
        // SYLT frames are really weird because they encode the text encoding and delimiters in a
        // different way.
        let encoding = match self.encoding {
            Encoding::Latin1 => Encoding::Latin1,
            _ => Encoding::UTF8,
        };
        self.byte(match encoding {
            Encoding::Latin1 => 0,
            Encoding::UTF8 => 1,
            _ => unreachable!(),
        })?;
        self.bytes(
            &content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>(),
        )?;
        self.byte(match content.timestamp_format {
            TimestampFormat::MPEG => 1,
            TimestampFormat::MS => 2,
        })?;
        self.byte(match content.content_type {
            SynchronisedLyricsType::Other => 0,
            SynchronisedLyricsType::Lyrics => 1,
            SynchronisedLyricsType::Transcription => 2,
            SynchronisedLyricsType::PartName => 3,
            SynchronisedLyricsType::Event => 4,
            SynchronisedLyricsType::Chord => 5,
            SynchronisedLyricsType::Trivia => 6,
        })?;
        let text_delim: &[u8] = match encoding {
            Encoding::Latin1 => &[0],
            Encoding::UTF8 => &[0, 0],
            _ => unreachable!(),
        };
        for (timestamp, text) in &content.content {
            self.string_with_other_encoding(encoding, text)?;
            self.bytes(text_delim)?;
            self.bytes(timestamp.to_be_bytes())?;
        }
        self.byte(0)
    }

    fn comment_content(&mut self, content: &Comment) -> crate::Result<()> {
        self.encoding()?;
        self.bytes(
            content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>(),
        )?;
        self.string(&content.description)?;
        self.delim()?;
        self.string(&content.text)
    }

    fn picture_content_v2(&mut self, content: &Picture) -> crate::Result<()> {
        self.encoding()?;
        let format = match &content.mime_type[..] {
            "image/jpeg" | "image/jpg" => "JPG",
            "image/png" => "PNG",
            _ => return Err(Error::new(ErrorKind::Parsing, "unsupported MIME type")),
        };
        self.bytes(format.as_bytes())?;
        self.byte(u8::from(content.picture_type))?;
        self.string(&content.description)?;
        self.delim()?;
        self.bytes(&content.data)
    }

    fn picture_content_v3(&mut self, content: &Picture) -> crate::Result<()> {
        self.encoding()?;
        self.bytes(content.mime_type.as_bytes())?;
        self.byte(0)?;
        self.byte(u8::from(content.picture_type))?;
        self.string(&content.description)?;
        self.delim()?;
        self.bytes(&content.data)
    }

    fn picture_content(&mut self, content: &Picture) -> crate::Result<()> {
        match self.version {
            tag::Id3v22 => self.picture_content_v2(content),
            tag::Id3v23 | tag::Id3v24 => self.picture_content_v3(content),
        }
    }
}

pub fn encode(
    mut writer: impl io::Write,
    content: &Content,
    version: tag::Version,
    encoding: Encoding,
) -> crate::Result<usize> {
    let mut buf = Vec::new();

    let mut encoder = Encoder {
        w: &mut buf,
        version,
        encoding,
    };
    match content {
        Content::Text(c) => encoder.text_content(c)?,
        Content::ExtendedText(c) => encoder.extended_text_content(c)?,
        Content::Link(c) => encoder.link_content(c)?,
        Content::ExtendedLink(c) => encoder.extended_link_content(c)?,
        Content::EncapsulatedObject(c) => encoder.encapsulated_object_content(c)?,
        Content::Lyrics(c) => encoder.lyrics_content(c)?,
        Content::SynchronisedLyrics(c) => encoder.synchronised_lyrics_content(c)?,
        Content::Comment(c) => encoder.comment_content(c)?,
        Content::Picture(c) => encoder.picture_content(c)?,
        Content::Unknown(c) => encoder.bytes(c)?,
    };

    writer.write_all(&buf)?;
    Ok(buf.len())
}

pub fn decode(
    id: &str,
    version: tag::Version,
    mut reader: impl io::Read,
) -> crate::Result<Content> {
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    let decoder = Decoder {
        r: &mut data,
        version,
    };

    match id {
        "PIC" => decoder.picture_content_v2(),
        "APIC" => decoder.picture_content_v3(),
        "TXXX" | "TXX" => decoder.extended_text_content(),
        "WXXX" | "WXX" => decoder.extended_link_content(),
        "COMM" | "COM" => decoder.comment_content(),
        "USLT" | "ULT" => decoder.lyrics_content(),
        "SYLT" | "SLT" => decoder.synchronised_lyrics_content(),
        "GEOB" | "GEO" => decoder.encapsulated_object_content(),
        id if id.starts_with('T') => decoder.text_content(),
        id if id.starts_with('W') => decoder.link_content(),
        "GRP1" => decoder.text_content(),
        _ => Ok(Content::Unknown(data)),
    }
}

struct Decoder<'a> {
    r: &'a [u8],
    version: tag::Version,
}

impl<'a> Decoder<'a> {
    fn bytes(&mut self, len: usize) -> crate::Result<&'a [u8]> {
        if len > self.r.len() {
            return Err(Error::new(
                ErrorKind::Parsing,
                "Insufficient data to decode bytes",
            ));
        }
        let (head, tail) = self.r.split_at(len);
        self.r = tail;
        Ok(head)
    }

    fn byte(&mut self) -> crate::Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn uint32(&mut self) -> crate::Result<u32> {
        let b = self.bytes(4)?;
        let mut a = [0; 4];
        a.copy_from_slice(b);
        Ok(u32::from_be_bytes(a))
    }

    fn decode_string(encoding: Encoding, bytes: &[u8]) -> crate::Result<String> {
        if bytes.is_empty() {
            // UTF16 decoding requires at least 2 bytes for it not to error.
            return Ok("".to_string());
        }
        match encoding {
            Encoding::Latin1 => string_from_latin1(bytes),
            Encoding::UTF8 => Ok(String::from_utf8(bytes.to_vec())?),
            Encoding::UTF16 => string_from_utf16(bytes),
            Encoding::UTF16BE => string_from_utf16be(bytes),
        }
    }

    fn string_until_eof(&mut self, encoding: Encoding) -> crate::Result<String> {
        Self::decode_string(encoding, self.r)
    }

    fn string_delimited(&mut self, encoding: Encoding) -> crate::Result<String> {
        let delim = crate::util::find_delim(encoding, self.r, 0)
            .ok_or_else(|| Error::new(ErrorKind::Parsing, "delimiter not found"))?;
        let delim_len = delim_len(encoding);
        let b = self.bytes(delim)?;
        self.bytes(delim_len)?; // Skip.
        Self::decode_string(encoding, b)
    }

    fn string_fixed(&mut self, bytes_len: usize) -> crate::Result<String> {
        let s = self.bytes(bytes_len)?;
        Self::decode_string(Encoding::Latin1, s)
    }

    fn encoding(&mut self) -> crate::Result<Encoding> {
        match self.byte()? {
            0 => Ok(Encoding::Latin1),
            1 => Ok(Encoding::UTF16),
            2 => Ok(Encoding::UTF16BE),
            3 => Ok(Encoding::UTF8),
            _ => Err(Error::new(ErrorKind::Parsing, "unknown encoding")),
        }
    }

    fn text_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let (end, _) = match self.version {
            tag::Version::Id3v24 => match crate::util::find_closing_delim(encoding, self.r) {
                Some(i) => (i, i + delim_len(encoding)),
                None => (self.r.len(), self.r.len()),
            },
            _ => match crate::util::find_delim(encoding, self.r, 0) {
                Some(i) => (i, i + delim_len(encoding)),
                None => (self.r.len(), self.r.len()),
            },
        };
        let text = Self::decode_string(encoding, self.bytes(end)?)?;
        Ok(Content::Text(text))
    }

    fn link_content(self) -> crate::Result<Content> {
        Ok(Content::Link(String::from_utf8(self.r.to_vec())?))
    }

    fn picture_type(&mut self) -> crate::Result<PictureType> {
        Ok(match self.byte()? {
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
        })
    }

    fn picture_content_v2(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let mime_type = match self.string_fixed(3)?.as_str() {
            "PNG" => "image/png".to_string(),
            "JPG" => "image/jpeg".to_string(),
            _ => {
                return Err(Error::new(
                    ErrorKind::UnsupportedFeature,
                    "can't determine MIME type for image format",
                ))
            }
        };
        let picture_type = self.picture_type()?;
        let description = self.string_delimited(encoding)?;
        let data = self.r.to_vec();
        Ok(Content::Picture(Picture {
            mime_type,
            picture_type,
            description,
            data,
        }))
    }

    fn picture_content_v3(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let mime_type = self.string_delimited(Encoding::Latin1)?;
        let picture_type = self.picture_type()?;
        let description = self.string_delimited(encoding)?;
        let data = self.r.to_vec();
        Ok(Content::Picture(Picture {
            mime_type,
            picture_type,
            description,
            data,
        }))
    }

    fn comment_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let lang = self.string_fixed(3)?;
        let description = self.string_delimited(encoding)?;
        let text = self.string_until_eof(encoding)?;
        Ok(Content::Comment(Comment {
            lang,
            description,
            text,
        }))
    }

    fn extended_text_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let description = self.string_delimited(encoding)?;
        let value = self.string_until_eof(encoding)?;
        Ok(Content::ExtendedText(ExtendedText { description, value }))
    }

    fn extended_link_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let description = self.string_delimited(encoding)?;
        let link = self.string_until_eof(Encoding::Latin1)?;
        Ok(Content::ExtendedLink(ExtendedLink { description, link }))
    }

    fn encapsulated_object_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let mime_type = self.string_delimited(Encoding::Latin1)?;
        let filename = self.string_delimited(encoding)?;
        let description = self.string_delimited(encoding)?;
        let data = self.r.to_vec();
        Ok(Content::EncapsulatedObject(EncapsulatedObject {
            mime_type,
            filename,
            description,
            data,
        }))
    }

    fn lyrics_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let lang = self.string_fixed(3)?;
        let description = self.string_delimited(encoding)?;
        let text = self.string_until_eof(encoding)?;
        Ok(Content::Lyrics(Lyrics {
            lang,
            description,
            text,
        }))
    }

    fn synchronised_lyrics_content(mut self) -> crate::Result<Content> {
        let (encoding, text_delim) = match self.byte()? {
            0 => (Encoding::Latin1, &[0][..]),
            1 => (Encoding::UTF8, &[0, 0][..]),
            _ => return Err(Error::new(ErrorKind::Parsing, "invalid SYLT encoding")),
        };

        let lang = self.string_fixed(3)?;
        let timestamp_format = match self.byte()? {
            1 => TimestampFormat::MPEG,
            2 => TimestampFormat::MS,
            _ => {
                return Err(Error::new(
                    ErrorKind::Parsing,
                    "invalid SYLT timestamp format",
                ))
            }
        };
        let content_type = match self.byte()? {
            0 => SynchronisedLyricsType::Other,
            1 => SynchronisedLyricsType::Lyrics,
            2 => SynchronisedLyricsType::Transcription,
            3 => SynchronisedLyricsType::PartName,
            4 => SynchronisedLyricsType::Event,
            5 => SynchronisedLyricsType::Chord,
            6 => SynchronisedLyricsType::Trivia,
            _ => return Err(Error::new(ErrorKind::Parsing, "invalid SYLT content type")),
        };

        let mut content = Vec::new();
        while let Some(i) = self
            .r
            .windows(text_delim.len())
            .position(|w| w == text_delim)
        {
            let text = Self::decode_string(encoding, &self.r[..i])?;
            self.r = &self.r[i + text_delim.len()..];
            let timestamp = self.uint32()?;
            content.push((timestamp, text));
        }

        Ok(Content::SynchronisedLyrics(SynchronisedLyrics {
            lang,
            timestamp_format,
            content_type,
            content,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Content;
    use crate::frame::{self, Picture, PictureType};
    use std::collections::HashMap;

    fn bytes_for_encoding(text: &str, encoding: Encoding) -> Vec<u8> {
        match encoding {
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
