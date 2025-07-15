use crate::frame::{
    Chapter, Comment, Content, EncapsulatedObject, ExtendedLink, ExtendedText, InvolvedPeopleList,
    InvolvedPeopleListItem, Lyrics, MpegLocationLookupTable, MpegLocationLookupTableReference,
    Picture, PictureType, Popularimeter, Private, SynchronisedLyrics, SynchronisedLyricsType,
    TableOfContents, TimestampFormat, UniqueFileIdentifier, Unknown,
};
use crate::stream::encoding::Encoding;
use crate::stream::frame;
use crate::tag::Version;
use crate::{Error, ErrorKind};
use std::convert::{TryFrom, TryInto};
use std::io;
use std::iter;
use std::mem::size_of;

struct Encoder<W: io::Write> {
    w: W,
    version: Version,
    encoding: Encoding,
}

impl<W: io::Write> Encoder<W> {
    fn bytes(&mut self, bytes: impl AsRef<[u8]>) -> crate::Result<()> {
        let bytes = bytes.as_ref();
        self.w.write_all(bytes)?;
        Ok(())
    }

    fn byte(&mut self, b: u8) -> crate::Result<()> {
        self.bytes([b])
    }

    fn uint16(&mut self, int: u16) -> crate::Result<()> {
        self.bytes(int.to_be_bytes())
    }

    fn uint24(&mut self, int: u32) -> crate::Result<()> {
        self.bytes(&int.to_be_bytes()[1..])
    }

    fn uint32(&mut self, int: u32) -> crate::Result<()> {
        self.bytes(int.to_be_bytes())
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
        self.bytes(encoding.encode(string))
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
        match self.version {
            Version::Id3v22 | Version::Id3v23 => self.string(&content.replace('\0', "/")),
            Version::Id3v24 => self.string(content),
        }
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
        self.bytes(&content.data)?;
        Ok(())
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
            _ => Encoding::UTF16,
        };
        self.byte(match encoding {
            Encoding::Latin1 => 0,
            Encoding::UTF16 => 1,
            _ => unreachable!(),
        })?;
        self.bytes(
            content
                .lang
                .bytes()
                .chain(iter::repeat(b' '))
                .take(3)
                .collect::<Vec<u8>>(),
        )?;
        self.byte(match content.timestamp_format {
            TimestampFormat::Mpeg => 1,
            TimestampFormat::Ms => 2,
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
            Encoding::UTF16 => &[0, 0],
            _ => unreachable!(),
        };
        // Description
        self.string_with_other_encoding(encoding, &content.description)?;
        self.bytes(text_delim)?;
        for (timestamp, text) in &content.content {
            self.string_with_other_encoding(encoding, text)?;
            self.bytes(text_delim)?;
            self.uint32(*timestamp)?;
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

    fn popularimeter_content(&mut self, content: &Popularimeter) -> crate::Result<()> {
        self.string_with_other_encoding(Encoding::Latin1, &content.user)?;
        self.byte(0)?;
        self.byte(content.rating)?;
        let counter_bin = content.counter.to_be_bytes();
        let i = counter_bin
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(size_of::<u64>());
        self.bytes(&counter_bin[i..])
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
            Version::Id3v22 => self.picture_content_v2(content),
            Version::Id3v23 | Version::Id3v24 => self.picture_content_v3(content),
        }
    }

    fn chapter_content(&mut self, content: &Chapter) -> crate::Result<()> {
        self.string_with_other_encoding(Encoding::Latin1, &content.element_id)?;
        self.byte(0)?;
        self.uint32(content.start_time)?;
        self.uint32(content.end_time)?;
        self.uint32(content.start_offset)?;
        self.uint32(content.end_offset)?;
        for frame in &content.frames {
            frame::encode(&mut self.w, frame, self.version, false)?;
        }
        Ok(())
    }

    fn mpeg_location_lookup_table_content(
        &mut self,
        content: &MpegLocationLookupTable,
    ) -> crate::Result<()> {
        let ref_packed_size = content
            .bits_for_bytes
            .saturating_add(content.bits_for_millis);
        if ref_packed_size % 4 != 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "MLLT bits_for_bytes + bits_for_millis must be a multiple of 4",
            ));
        } else if ref_packed_size > 64 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "MLLT bits_for_bytes + bits_for_millis must be <= 64",
            ));
        }

        self.uint16(content.frames_between_reference)?;
        self.uint24(content.bytes_between_reference)?;
        self.uint24(content.millis_between_reference)?;
        self.byte(content.bits_for_bytes)?;
        self.byte(content.bits_for_millis)?;

        let mut carry = 0u64;
        let mut carry_bits = 0usize;
        for r in &content.references {
            for (ref_field, bits) in [
                (r.deviate_bytes, content.bits_for_bytes),
                (r.deviate_millis, content.bits_for_millis),
            ] {
                let deviate = u64::from(ref_field) & ((1 << bits) - 1);
                carry |= deviate << (64 - usize::from(bits) - carry_bits);
                carry_bits += usize::from(bits);
                let shift_out_bytes = carry_bits / 8;
                self.bytes(&carry.to_be_bytes()[..shift_out_bytes])?;
                carry <<= shift_out_bytes * 8;
                carry_bits -= shift_out_bytes * 8;
            }
        }
        debug_assert!(carry_bits < 8);
        if carry_bits > 0 {
            self.byte((carry >> 56) as u8)?;
        }
        Ok(())
    }

    fn private_content(&mut self, content: &Private) -> crate::Result<()> {
        self.bytes(content.owner_identifier.as_bytes())?;
        self.byte(0)?;
        self.bytes(content.private_data.as_slice())?;
        Ok(())
    }

    fn unique_file_identifier_content(
        &mut self,
        content: &UniqueFileIdentifier,
    ) -> crate::Result<()> {
        self.bytes(content.owner_identifier.as_bytes())?;
        self.byte(0)?;
        self.bytes(content.identifier.as_slice())?;
        Ok(())
    }

    fn involved_people_list(&mut self, content: &InvolvedPeopleList) -> crate::Result<()> {
        self.encoding()?;
        for item in &content.items {
            self.string(&item.involvement)?;
            self.delim()?;
            self.string(&item.involvee)?;
            self.delim()?;
        }
        Ok(())
    }

    fn table_of_contents_content(&mut self, content: &TableOfContents) -> crate::Result<()> {
        self.string_with_other_encoding(Encoding::Latin1, &content.element_id)?;
        self.byte(0)?;
        let top_level_flag = match content.top_level {
            true => 2,
            false => 0,
        };

        let ordered_flag = match content.ordered {
            true => 1,
            false => 0,
        };
        self.byte(top_level_flag | ordered_flag)?;
        self.byte(content.elements.len() as u8)?;

        for element in &content.elements {
            self.string_with_other_encoding(Encoding::Latin1, element)?;
            self.byte(0)?;
        }
        for frame in &content.frames {
            frame::encode(&mut self.w, frame, self.version, false)?;
        }
        Ok(())
    }
}

pub fn encode(
    mut writer: impl io::Write,
    content: &Content,
    version: Version,
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
        Content::Popularimeter(c) => encoder.popularimeter_content(c)?,
        Content::Picture(c) => encoder.picture_content(c)?,
        Content::Chapter(c) => encoder.chapter_content(c)?,
        Content::MpegLocationLookupTable(c) => encoder.mpeg_location_lookup_table_content(c)?,
        Content::Private(c) => encoder.private_content(c)?,
        Content::TableOfContents(c) => encoder.table_of_contents_content(c)?,
        Content::UniqueFileIdentifier(c) => encoder.unique_file_identifier_content(c)?,
        Content::InvolvedPeopleList(c) => encoder.involved_people_list(c)?,
        Content::Unknown(c) => encoder.bytes(&c.data)?,
    };

    writer.write_all(&buf)?;
    Ok(buf.len())
}

pub fn decode(
    id: &str,
    version: Version,
    mut reader: impl io::Read,
) -> crate::Result<(Content, Option<Encoding>)> {
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    let decoder = Decoder {
        r: &mut data,
        version,
    };

    let mut encoding = None;
    let content = match id {
        "PIC" => {
            if cfg!(feature = "decode_picture") {
                decoder.picture_content_v2()
            } else {
                Ok(Content::Unknown(Unknown { data, version }))
            }
        }
        "APIC" => {
            if cfg!(feature = "decode_picture") {
                decoder.picture_content_v3()
            } else {
                Ok(Content::Unknown(Unknown { data, version }))
            }
        }
        "TXXX" | "TXX" => {
            let (content, enc) = decoder.extended_text_content()?;
            encoding = Some(enc);
            Ok(content)
        }
        "WXXX" | "WXX" => decoder.extended_link_content(),
        "COMM" | "COM" => decoder.comment_content(),
        "POPM" | "POP" => decoder.popularimeter_content(),
        "USLT" | "ULT" => decoder.lyrics_content(),
        "SYLT" | "SLT" => decoder.synchronised_lyrics_content(),
        "GEOB" | "GEO" => {
            let (content, enc) = decoder.encapsulated_object_content()?;
            encoding = Some(enc);
            Ok(content)
        }
        "IPLS" | "IPL" | "TMCL" | "TIPL" => decoder.involved_people_list(),
        // According to the ID3v2.2.0/ID3v2.3.0 specifications, these text frames may contain
        // multiple values that 'are seperated with the "/" character'. Hence, the "/" character is
        // replaced with a null byte for these frames, so that the values can be accessed
        // separately using the `text_values()` method.
        //
        // All other frames do not support multiple values in these ID3 versions and therefore their
        // contents are read verbatim. Note that when trying to write multiple values, the values
        // will be joined using "/" for  *all* tags, because the alternative would be to just throw
        // an error.
        "TCOM" | "TCM" | "TEXT" | "TXT" | "TOLY" | "TOL" | "TOPE" | "TOA" | "TPE1" | "TP1" => {
            decoder.text_content_multiple()
        }
        id if id.starts_with('T') => decoder.text_content(),
        id if id.starts_with('W') => decoder.link_content(),
        "GRP1" => decoder.text_content(),
        "CHAP" => decoder.chapter_content(),
        "MLLT" => decoder.mpeg_location_lookup_table_content(),
        "PRIV" => decoder.private_content(),
        "UFID" => decoder.unique_file_identifier_content(),
        "CTOC" => decoder.table_of_contents_content(),
        _ => Ok(Content::Unknown(Unknown { data, version })),
    }?;
    Ok((content, encoding))
}

struct Decoder<'a> {
    r: &'a [u8],
    version: Version,
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

    fn uint16(&mut self) -> crate::Result<u16> {
        let b = self.bytes(2)?;
        let a = b.try_into().unwrap();
        Ok(u16::from_be_bytes(a))
    }

    fn uint24(&mut self) -> crate::Result<u32> {
        let b3 = self.bytes(3)?;
        let mut b4 = [0; 4];
        b4[1..4].copy_from_slice(b3);
        Ok(u32::from_be_bytes(b4))
    }

    fn uint32(&mut self) -> crate::Result<u32> {
        let b = self.bytes(4)?;
        let a = b.try_into().unwrap();
        Ok(u32::from_be_bytes(a))
    }

    fn string_until_eof(&mut self, encoding: Encoding) -> crate::Result<String> {
        encoding.decode(self.r)
    }

    fn string_delimited(&mut self, encoding: Encoding) -> crate::Result<String> {
        let delim = find_delim(encoding, self.r, 0)
            .ok_or_else(|| Error::new(ErrorKind::Parsing, "delimiter not found"))?;
        let delim_len = delim_len(encoding);
        let b = self.bytes(delim)?;
        self.bytes(delim_len)?; // Skip.
        encoding.decode(b)
    }

    fn string_fixed(&mut self, bytes_len: usize) -> crate::Result<String> {
        let s = self.bytes(bytes_len)?;
        Encoding::Latin1.decode(s)
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
            Version::Id3v24 => match find_closing_delim(encoding, self.r) {
                Some(i) => (i, i + delim_len(encoding)),
                None => (self.r.len(), self.r.len()),
            },
            _ => match find_delim(encoding, self.r, 0) {
                Some(i) => (i, i + delim_len(encoding)),
                None => (self.r.len(), self.r.len()),
            },
        };
        let text = encoding.decode(self.bytes(end)?)?;
        Ok(Content::Text(text))
    }

    fn text_content_multiple(self) -> crate::Result<Content> {
        let version = self.version;
        self.text_content().map(|content| match content {
            Content::Text(text) => {
                let text = match version {
                    Version::Id3v22 | Version::Id3v23 => text.replace('/', "\0"),
                    Version::Id3v24 => text,
                };
                Content::Text(text)
            }
            content => content,
        })
    }

    fn involved_people_list(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let end = match self.version {
            Version::Id3v23 | Version::Id3v24 => find_closing_delim(encoding, self.r),
            _ => find_delim(encoding, self.r, 0),
        }
        .unwrap_or(self.r.len());

        let data = self.bytes(end)?;

        let mut pos = 0;
        let items = iter::repeat_with(|| {
            find_delim(encoding, data, pos)
                .map(|next_pos| {
                    let substr = encoding.decode(&data[pos..next_pos]);
                    pos = next_pos + delim_len(encoding);
                    substr
                })
                .or_else(|| {
                    if pos < data.len() {
                        let substr = encoding.decode(&data[pos..]);
                        pos = data.len();
                        Some(substr)
                    } else {
                        None
                    }
                })
        })
        .scan(None, |last_string, string| match (&last_string, string) {
            (None, Some(string)) => {
                *last_string = Some(string);
                Some(Ok(None))
            }
            (Some(_), Some(second)) => {
                let first = last_string.take().expect("option must be some");
                let result = first.and_then(|involvement| {
                    second.map(|involvee| {
                        Some(InvolvedPeopleListItem {
                            involvement,
                            involvee,
                        })
                    })
                });
                Some(result)
            }
            (Some(_), None) => {
                // This can only happen if there is an uneven number of elements. For
                // compatibility, we assume that the missing value is an empty string instead of
                // erroring out and failing to parse the entire tag.
                //
                // This is in line with what the Python mutagen library does. See this issue for
                // details:
                // - <https://github.com/polyfloyd/rust-id3/issues/147>
                let first = last_string.take().expect("option must be some");
                let result = first.map(|involvement| {
                    Some(InvolvedPeopleListItem {
                        involvement,
                        involvee: String::new(),
                    })
                });
                Some(result)
            }
            (None, None) => None,
        })
        .filter_map(|item| item.transpose())
        .collect::<crate::Result<Vec<InvolvedPeopleListItem>>>()?;

        Ok(Content::InvolvedPeopleList(InvolvedPeopleList { items }))
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

    fn popularimeter_content(mut self) -> crate::Result<Content> {
        let user = self.string_delimited(Encoding::Latin1)?;
        let rating = self.byte()?;
        let counter = {
            let r = match self.r.len() {
                0..=8 => self.r,
                9.. => &self.r[..8],
                // issue #126
                #[allow(unreachable_patterns)]
                _ => unimplemented!(),
            };
            let mut bin = [0; 8];
            bin[8 - r.len()..].copy_from_slice(r);
            u64::from_be_bytes(bin)
        };
        Ok(Content::Popularimeter(Popularimeter {
            user,
            rating,
            counter,
        }))
    }

    fn extended_text_content(mut self) -> crate::Result<(Content, Encoding)> {
        let encoding = self.encoding()?;
        let description = self.string_delimited(encoding)?;
        let value = self.string_until_eof(encoding)?;
        Ok((
            Content::ExtendedText(ExtendedText { description, value }),
            encoding,
        ))
    }

    fn extended_link_content(mut self) -> crate::Result<Content> {
        let encoding = self.encoding()?;
        let description = self.string_delimited(encoding)?;
        let link = self.string_until_eof(Encoding::Latin1)?;
        Ok(Content::ExtendedLink(ExtendedLink { description, link }))
    }

    fn encapsulated_object_content(mut self) -> crate::Result<(Content, Encoding)> {
        let encoding = self.encoding()?;
        let mime_type = self.string_delimited(Encoding::Latin1)?;
        let filename = self.string_delimited(encoding)?;
        let description = self.string_delimited(encoding)?;
        let data = self.r.to_vec();
        Ok((
            Content::EncapsulatedObject(EncapsulatedObject {
                mime_type,
                filename,
                description,
                data,
            }),
            encoding,
        ))
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
            1 => (Encoding::UTF16, &[0, 0][..]),
            _ => return Err(Error::new(ErrorKind::Parsing, "invalid SYLT encoding")),
        };

        let lang = self.string_fixed(3)?;
        let timestamp_format = match self.byte()? {
            1 => TimestampFormat::Mpeg,
            2 => TimestampFormat::Ms,
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

        let mut description = None;
        let mut content = Vec::new();
        while let Some(i) = self
            .r
            .chunks(text_delim.len())
            .position(|w| w == text_delim)
        {
            let i = i * text_delim.len();
            let text = encoding.decode(&self.r[..i])?;

            self.r = &self.r[i + text_delim.len()..];

            // Read description
            if description.is_none() {
                description = Some(text);
                continue;
            }

            let timestamp = self.uint32()?;
            content.push((timestamp, text));
        }

        Ok(Content::SynchronisedLyrics(SynchronisedLyrics {
            lang,
            timestamp_format,
            content_type,
            content,
            description: description.unwrap_or_default(),
        }))
    }

    fn chapter_content(mut self) -> crate::Result<Content> {
        let element_id = self.string_delimited(Encoding::Latin1)?;
        let start_time = self.uint32()?;
        let end_time = self.uint32()?;
        let start_offset = self.uint32()?;
        let end_offset = self.uint32()?;
        let mut frames = Vec::new();
        while let Some((_advance, frame)) = frame::decode(&mut self.r, self.version)? {
            frames.push(frame);
        }
        Ok(Content::Chapter(Chapter {
            element_id,
            start_time,
            end_time,
            start_offset,
            end_offset,
            frames,
        }))
    }

    fn mpeg_location_lookup_table_content(mut self) -> crate::Result<Content> {
        let frames_between_reference = self.uint16()?;
        let bytes_between_reference = self.uint24()?;
        let millis_between_reference = self.uint24()?;
        let bits_for_bytes = self.byte()?;
        let bits_for_millis = self.byte()?;

        if bits_for_bytes == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "MLLT bits_for_bytes must be > 0",
            ));
        } else if bits_for_millis == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "MLLT bits_for_millis must be > 0",
            ));
        }

        let bits_for_bytes_us = usize::from(bits_for_bytes);
        let bits_for_millis_us = usize::from(bits_for_millis);
        let mut references = Vec::new();
        let mut carry = 0u64;
        let mut carry_bits = 0usize;
        let mut bytes = self.r.iter().copied().peekable();
        while bytes.peek().is_some() {
            // Load enough bytes to shift the next reference from.
            for b in bytes
                .by_ref()
                .take((bits_for_bytes_us + bits_for_millis_us) / 8)
            {
                carry |= u64::from(b)
                    << (64_usize.checked_sub(carry_bits + 8_usize).ok_or_else(|| {
                        Error::new(ErrorKind::InvalidInput, "MLLT carry subtraction overflow")
                    })?);
                carry_bits += 8;
            }
            // Shift 2 deviation fields from the carry accumulator.
            let mut deviations = [0u32; 2];
            for (i, bits_us) in [bits_for_bytes_us, bits_for_millis_us]
                .into_iter()
                .enumerate()
            {
                if carry_bits < bits_us {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        format!("MLLT not enough bits left for reference: {carry_bits}<{bits_us}"),
                    ));
                }
                deviations[i] = u32::try_from(carry >> (64 - bits_us)).map_err(|_| {
                    Error::new(ErrorKind::InvalidInput, "MLLT deviation field overflow")
                })?;
                carry = carry
                    .checked_shl(
                        bits_us.try_into().map_err(|_| {
                            Error::new(ErrorKind::InvalidInput, "MLLT shift overflow")
                        })?,
                    )
                    .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "MLLT carry overflow"))?;
                carry_bits -= bits_us;
            }
            let [deviate_bytes, deviate_millis] = deviations;
            references.push(MpegLocationLookupTableReference {
                deviate_bytes,
                deviate_millis,
            });
        }

        Ok(Content::MpegLocationLookupTable(MpegLocationLookupTable {
            frames_between_reference,
            bytes_between_reference,
            millis_between_reference,
            bits_for_bytes,
            bits_for_millis,
            references,
        }))
    }

    fn private_content(mut self) -> crate::Result<Content> {
        let owner_identifier = self.string_delimited(Encoding::Latin1)?;
        let private_data = self.r.to_vec();

        Ok(Content::Private(Private {
            owner_identifier,
            private_data,
        }))
    }

    fn unique_file_identifier_content(mut self) -> crate::Result<Content> {
        let owner_identifier = self.string_delimited(Encoding::Latin1)?;
        let identifier = self.r.to_vec();

        Ok(Content::UniqueFileIdentifier(UniqueFileIdentifier {
            owner_identifier,
            identifier,
        }))
    }

    fn table_of_contents_content(mut self) -> crate::Result<Content> {
        let element_id = self.string_delimited(Encoding::Latin1)?;
        let flags = self.byte()?;
        let top_level = matches!(!!(flags & 2), 2);
        let ordered = matches!(!!(flags & 1), 1);
        let element_count = self.byte()?;
        let mut elements = Vec::new();
        for _ in 0..element_count {
            elements.push(self.string_delimited(Encoding::Latin1)?);
        }
        let mut frames = Vec::new();
        while let Some((_advance, frame)) = frame::decode(&mut self.r, self.version)? {
            frames.push(frame);
        }
        Ok(Content::TableOfContents(TableOfContents {
            element_id,
            top_level,
            ordered,
            elements,
            frames,
        }))
    }
}

/// Returns the index of the first delimiter for the specified encoding.
fn find_delim(encoding: Encoding, data: &[u8], index: usize) -> Option<usize> {
    let mut i = index;
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            if i >= data.len() {
                return None;
            }

            for c in data[i..].iter() {
                if *c == 0 {
                    break;
                }
                i += 1;
            }

            if i == data.len() {
                // delimiter was not found
                return None;
            }

            Some(i)
        }
        Encoding::UTF16 | Encoding::UTF16BE => {
            while i + 1 < data.len() && (data[i] != 0 || data[i + 1] != 0) {
                i += 2;
            }

            if i + 1 >= data.len() {
                // delimiter was not found
                return None;
            }

            Some(i)
        }
    }
}

/// Returns the index of the last delimiter for the specified encoding.
pub fn find_closing_delim(encoding: Encoding, data: &[u8]) -> Option<usize> {
    let mut i = data.len();
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => {
            i = i.checked_sub(1)?;
            while i > 0 {
                if data[i] != 0 {
                    return if (i + 1) == data.len() {
                        None
                    } else {
                        Some(i + 1)
                    };
                }
                i -= 1;
            }
            None
        }
        Encoding::UTF16 | Encoding::UTF16BE => {
            i = i.checked_sub(2)?;
            i -= i % 2; // align to 2-byte boundary
            while i > 1 {
                if data[i] != 0 || data[i + 1] != 0 {
                    return if (i + 2) == data.len() {
                        None
                    } else {
                        Some(i + 2)
                    };
                }
                i -= 2;
            }
            None
        }
    }
}

/// Returns the delimiter length for the specified encoding.
fn delim_len(encoding: Encoding) -> usize {
    match encoding {
        Encoding::Latin1 | Encoding::UTF8 => 1,
        Encoding::UTF16 | Encoding::UTF16BE => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Content;
    use crate::frame::{self, Picture, PictureType};
    use std::collections::HashMap;
    use std::io::Cursor;

    fn bytes_for_encoding(text: &str, encoding: Encoding) -> Vec<u8> {
        encoding.encode(text)
    }

    fn delim_for_encoding(encoding: Encoding) -> Vec<u8> {
        vec![0; delim_len(encoding)]
    }

    #[test]
    fn test_apic_v2() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        assert!(decode("PIC", Version::Id3v22, &[][..]).is_err());

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
                        *decode("PIC", Version::Id3v22, &data[..])
                            .unwrap()
                            .0
                            .picture()
                            .unwrap(),
                        picture
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Picture(picture.clone()),
                        Version::Id3v22,
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
        if !cfg!(feature = "decode_picture") {
            return;
        }

        assert!(decode("APIC", Version::Id3v23, &[][..]).is_err());

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
                        *decode("APIC", Version::Id3v23, &data[..])
                            .unwrap()
                            .0
                            .picture()
                            .unwrap(),
                        picture
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Picture(picture.clone()),
                        Version::Id3v23,
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
        assert!(decode("COMM", Version::Id3v23, &[][..]).is_err());

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
                        *decode("COMM", Version::Id3v23, &data[..])
                            .unwrap()
                            .0
                            .comment()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Comment(content),
                        Version::Id3v23,
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
            assert!(decode("COMM", Version::Id3v23, &data[..]).is_err());
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
                *decode("COMM", Version::Id3v23, &data[..])
                    .unwrap()
                    .0
                    .comment()
                    .unwrap(),
                content
            );
        }
    }

    #[test]
    fn test_popm() {
        // Counter with 3 bytes
        let bin = b"\x00\xff\xaa\xaa\xaa";
        assert_eq!(
            decode("POPM", Version::Id3v23, &bin[..]).unwrap().0,
            Content::Popularimeter(Popularimeter {
                user: "".to_string(),
                rating: 255,
                counter: 0xaaaaaa,
            })
        );

        // Counter with 12 bytes
        let bin = b"\x00\xff\xaa\xaa\xaa\xaa\xaa\xaa\xaa\xaa\xbb\xbb\xbb\xbb";
        assert_eq!(
            decode("POPM", Version::Id3v23, &bin[..]).unwrap().0,
            Content::Popularimeter(Popularimeter {
                user: "".to_string(),
                rating: 255,
                counter: 0xaaaaaaaaaaaaaaaa,
            })
        );
    }

    #[test]
    fn test_text() {
        assert!(decode("TALB", Version::Id3v23, &[][..]).is_err());

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
                    decode("TALB", Version::Id3v23, &data[..])
                        .unwrap()
                        .0
                        .text()
                        .unwrap(),
                    *text
                );
                let mut data_out = Vec::new();
                encode(
                    &mut data_out,
                    &Content::Text(text.to_string()),
                    Version::Id3v23,
                    *encoding,
                )
                .unwrap();
                assert_eq!(data, data_out);
            }
        }
    }

    #[test]
    fn test_null_terminated_text_v4() {
        assert!(decode("TRCK", Version::Id3v24, &[][..]).is_err());
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
                decode("TALB", Version::Id3v24, &data[..])
                    .unwrap()
                    .0
                    .text()
                    .unwrap(),
                "text\u{0}text"
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Text(text.to_string()),
                Version::Id3v24,
                *encoding,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_non_null_terminated_text_v4() {
        assert!(decode("TRCK", Version::Id3v24, &[][..]).is_err());
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
                decode("TALB", Version::Id3v24, &data[..])
                    .unwrap()
                    .0
                    .text()
                    .unwrap(),
                "text\u{0}text"
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Text(text.to_string()),
                Version::Id3v24,
                *encoding,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_txxx() {
        assert!(decode("TXXX", Version::Id3v23, &[][..]).is_err());

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
                        *decode("TXXX", Version::Id3v23, &data[..])
                            .unwrap()
                            .0
                            .extended_text()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::ExtendedText(content),
                        Version::Id3v23,
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
            assert!(decode("TXXX", Version::Id3v23, &data[..]).is_err());
        }
    }

    #[test]
    fn test_weblink() {
        for link in &["", "http://www.rust-lang.org/"] {
            println!("`{:?}`", link);
            let data = link.as_bytes().to_vec();

            assert_eq!(
                decode("WOAF", Version::Id3v23, &data[..])
                    .unwrap()
                    .0
                    .link()
                    .unwrap(),
                *link
            );
            let mut data_out = Vec::new();
            encode(
                &mut data_out,
                &Content::Link(link.to_string()),
                Version::Id3v23,
                Encoding::Latin1,
            )
            .unwrap();
            assert_eq!(data, data_out);
        }
    }

    #[test]
    fn test_wxxx() {
        assert!(decode("WXXX", Version::Id3v23, &[][..]).is_err());

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
                        *decode("WXXX", Version::Id3v23, &data[..])
                            .unwrap()
                            .0
                            .extended_link()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::ExtendedLink(content),
                        Version::Id3v23,
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
            assert!(decode("WXXX", Version::Id3v23, &data[..]).is_err());
        }
    }

    #[test]
    fn test_uslt() {
        assert!(decode("USLT", Version::Id3v23, &[][..]).is_err());

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
                        *decode("USLT", Version::Id3v23, &data[..])
                            .unwrap()
                            .0
                            .lyrics()
                            .unwrap(),
                        content
                    );
                    let mut data_out = Vec::new();
                    encode(
                        &mut data_out,
                        &Content::Lyrics(content),
                        Version::Id3v23,
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
            assert!(decode("USLT", Version::Id3v23, &data[..]).is_err());
        }
    }

    #[test]
    fn test_ipls() {
        check_involved_people_list("IPLS", Version::Id3v23);
    }

    #[test]
    fn test_tmcl() {
        check_involved_people_list("TMCL", Version::Id3v24);
    }

    #[test]
    fn test_tipl() {
        check_involved_people_list("TIPL", Version::Id3v24);
    }

    fn check_involved_people_list(frame_id: &str, version: Version) {
        assert!(decode(frame_id, version, &[][..]).is_err());

        println!("valid");
        for people_list in &[
            vec![],
            vec![("involvement", "involvee")],
            vec![
                ("double bass", "Israel Crosby"),
                ("drums (drum set)", "Vernell Fournier"),
                ("piano", "Ahmad Jamal"),
                ("producer", "Dave Usher"),
            ],
        ] {
            for encoding in &[
                Encoding::Latin1,
                Encoding::UTF8,
                Encoding::UTF16,
                Encoding::UTF16BE,
            ] {
                println!("`{:?}`, `{:?}`", people_list, encoding);
                let mut data = Vec::new();
                data.push(*encoding as u8);
                for (involvement, involvee) in people_list {
                    data.extend(bytes_for_encoding(&involvement, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                    data.extend(bytes_for_encoding(&involvee, *encoding).into_iter());
                    data.extend(delim_for_encoding(*encoding).into_iter());
                }

                let content = frame::InvolvedPeopleList {
                    items: people_list
                        .iter()
                        .map(|(involvement, involvee)| InvolvedPeopleListItem {
                            involvement: involvement.to_string(),
                            involvee: involvee.to_string(),
                        })
                        .collect(),
                };
                assert_eq!(
                    *decode(frame_id, version, &data[..])
                        .unwrap()
                        .0
                        .involved_people_list()
                        .unwrap(),
                    content
                );
                let mut data_out = Vec::new();
                encode(
                    &mut data_out,
                    &&Content::InvolvedPeopleList(content),
                    Version::Id3v23,
                    *encoding,
                )
                .unwrap();
                assert_eq!(data, data_out);
            }
        }

        println!("invalid");
        for encoding in &[
            Encoding::Latin1,
            Encoding::UTF8,
            Encoding::UTF16,
            Encoding::UTF16BE,
        ] {
            println!("`{:?}`", encoding);
            let mut data = Vec::new();
            data.push(*encoding as u8);
            data.extend(bytes_for_encoding("involvement", *encoding).into_iter());
            data.extend(delim_for_encoding(*encoding).into_iter());
            data.extend(bytes_for_encoding("involvee", *encoding).into_iter());
            data.extend(delim_for_encoding(*encoding).into_iter());
            data.extend(bytes_for_encoding("other involvement", *encoding).into_iter());
            data.extend(delim_for_encoding(*encoding).into_iter());
            // involveee missing here

            let content = frame::InvolvedPeopleList {
                items: vec![
                    InvolvedPeopleListItem {
                        involvement: "involvement".to_string(),
                        involvee: "involvee".to_string(),
                    },
                    InvolvedPeopleListItem {
                        involvement: "other involvement".to_string(),
                        // Assume empty string if value is missing
                        involvee: "".to_string(),
                    },
                ],
            };
            assert_eq!(
                *decode(frame_id, version, &data[..])
                    .unwrap()
                    .0
                    .involved_people_list()
                    .unwrap(),
                content
            );
        }
    }

    #[test]
    fn test_mllt_4_4() {
        let mllt = Content::MpegLocationLookupTable(MpegLocationLookupTable {
            frames_between_reference: 1,
            bytes_between_reference: 418,
            millis_between_reference: 15,
            bits_for_bytes: 4,
            bits_for_millis: 4,
            references: vec![
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x1,
                    deviate_millis: 0x2,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x3,
                    deviate_millis: 0x4,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x5,
                    deviate_millis: 0x6,
                },
            ],
        });
        let mut data_out = Vec::new();
        encode(&mut data_out, &mllt, Version::Id3v23, Encoding::UTF8).unwrap();
        let expect_data = b"\x00\x01\x00\x01\xa2\x00\x00\x0f\x04\x04\x12\x34\x56";
        assert_eq!(format!("{:x?}", data_out), format!("{:x?}", expect_data));
        let mllt_decoded = decode("MLLT", Version::Id3v23, &*data_out).unwrap().0;
        assert_eq!(mllt, mllt_decoded);
    }

    #[test]
    fn test_mllt_8_8() {
        let mllt = Content::MpegLocationLookupTable(MpegLocationLookupTable {
            frames_between_reference: 1,
            bytes_between_reference: 418,
            millis_between_reference: 15,
            bits_for_bytes: 8,
            bits_for_millis: 8,
            references: vec![
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x11,
                    deviate_millis: 0x22,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x33,
                    deviate_millis: 0x44,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x55,
                    deviate_millis: 0x66,
                },
            ],
        });
        let mut data_out = Vec::new();
        encode(&mut data_out, &mllt, Version::Id3v23, Encoding::UTF8).unwrap();
        let expect_data = b"\x00\x01\x00\x01\xa2\x00\x00\x0f\x08\x08\x11\x22\x33\x44\x55\x66";
        assert_eq!(format!("{:x?}", data_out), format!("{:x?}", expect_data));
        let mllt_decoded = decode("MLLT", Version::Id3v23, &*data_out).unwrap().0;
        assert_eq!(mllt, mllt_decoded);
    }

    #[test]
    fn test_mllt_12_12() {
        let mllt = Content::MpegLocationLookupTable(MpegLocationLookupTable {
            frames_between_reference: 1,
            bytes_between_reference: 418,
            millis_between_reference: 15,
            bits_for_bytes: 12,
            bits_for_millis: 12,
            references: vec![
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x111,
                    deviate_millis: 0x222,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x333,
                    deviate_millis: 0x444,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x555,
                    deviate_millis: 0x666,
                },
            ],
        });
        let mut data_out = Vec::new();
        encode(&mut data_out, &mllt, Version::Id3v23, Encoding::UTF8).unwrap();
        let expect_data =
            b"\x00\x01\x00\x01\xa2\x00\x00\x0f\x0c\x0c\x11\x12\x22\x33\x34\x44\x55\x56\x66";
        assert_eq!(format!("{:x?}", data_out), format!("{:x?}", expect_data));
        let mllt_decoded = decode("MLLT", Version::Id3v23, &*data_out).unwrap().0;
        assert_eq!(mllt, mllt_decoded);
    }

    #[test]
    fn test_find_delim() {
        assert_eq!(
            find_delim(Encoding::UTF8, &[0x0, 0xFF, 0xFF, 0xFF, 0x0], 3).unwrap(),
            4
        );
        assert!(find_delim(Encoding::UTF8, &[0x0, 0xFF, 0xFF, 0xFF, 0xFF], 3).is_none());

        assert_eq!(
            find_delim(
                Encoding::UTF16,
                &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF],
                2
            )
            .unwrap(),
            4
        );
        assert!(find_delim(
            Encoding::UTF16,
            &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF],
            2
        )
        .is_none());

        assert_eq!(
            find_delim(
                Encoding::UTF16BE,
                &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0x0, 0xFF, 0xFF],
                2
            )
            .unwrap(),
            4
        );
        assert!(find_delim(
            Encoding::UTF16BE,
            &[0x0, 0xFF, 0x0, 0xFF, 0x0, 0xFF, 0xFF, 0xFF],
            2
        )
        .is_none());
    }

    #[test]
    fn test_encode_mllt_overflow() {
        let mllt = Content::MpegLocationLookupTable(MpegLocationLookupTable {
            frames_between_reference: 1,
            bytes_between_reference: 418,
            millis_between_reference: 15,
            bits_for_bytes: 252,
            bits_for_millis: 252, // This would cause an overflow if not for saturating_add
            references: vec![
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x1,
                    deviate_millis: 0x2,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x3,
                    deviate_millis: 0x4,
                },
                MpegLocationLookupTableReference {
                    deviate_bytes: 0x5,
                    deviate_millis: 0x6,
                },
            ],
        });

        let mut data_out = Vec::new();
        let result = encode(&mut data_out, &mllt, Version::Id3v23, Encoding::UTF8);

        // Error since 255 is not a multiple of 4, but no panic.
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e.kind, ErrorKind::InvalidInput));
            assert_eq!(
                e.description,
                "MLLT bits_for_bytes + bits_for_millis must be a multiple of 4"
            );
        }
    }

    #[test]
    fn test_decode_mllt_deviation_overflow() {
        // Create a payload with large deviation values that would overflow u32
        let payload = [
            0xFF, 0xFF, // frames_between_reference (u16::MAX)
            0xFF, 0xFF, 0xFF, // bytes_between_reference (u24::MAX)
            0xFF, 0xFF, 0xFF, // millis_between_reference (u24::MAX)
            0x38, // bits_for_bytes (56)
            0x1C, // bits_for_millis (28)
            0xFF, 0xFF, 0xFF, 0xFF, // deviate_bytes (u32::MAX)
            0xFF, 0xFF, 0xFF, 0xFF, // deviate_millis (u32::MAX)
        ];

        // Combine header and payload into a single data stream
        let mut data = Vec::new();
        data.extend_from_slice(&payload);

        let mut reader = Cursor::new(data);

        // Attempt to decode the frame
        let result = decode("MLLT", Version::Id3v23, &mut reader);

        // Ensure that the result is an error due to overflow
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e.kind, ErrorKind::InvalidInput));
            assert_eq!(e.description, "MLLT deviation field overflow");
        }
    }

    #[test]
    fn test_mllt_shift_overflow() {
        // Create a payload with large deviation values that would cause an overflow
        let payload = [
            0xFF, 0x02, // frames_between_reference (65282)
            0x00, 0x00, 0x00, // bytes_between_reference (0)
            0xFF, 0xFF, 0x62, // millis_between_reference (16777058)
            0x40, // bits_for_bytes (64)
            0x01, // bits_for_millis (01)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x80, 0x44, 0x37, 0x00,
            0x62, // carry (4), carry_bits(64)
        ];

        // Combine header and payload into a single data stream
        let mut data = Vec::new();
        data.extend_from_slice(&payload);

        let mut reader = Cursor::new(data);

        // Attempt to decode the frame
        let result = decode("MLLT", Version::Id3v23, &mut reader);

        // Ensure that the result is an error due to overflow
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e.kind, ErrorKind::InvalidInput));
            assert_eq!(e.description, "MLLT carry overflow");
        }
    }

    #[test]
    fn test_mllt_subtract_overflow() {
        // Create a payload with large deviation values that would cause an overflow
        let payload = [
            0xBC, 0xBC, // frames_between_reference (48316)
            0xBC, 0xBC, 0xBC, // bytes_between_reference (12369084)
            0xBC, 0xBC, 0xBC, // millis_between_reference (12369084)
            0xBC, // bits_for_bytes (118)
            0xBC, // bits_for_millis (118)
            0x62, 0x62, 0x63, 0x00, 0x3B, 0x00, 0x65, 0x62, 0x62, 0x7A, 0x62, 0x62, 0xFF, 0x42,
            0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
            0x42, 0x42, 0x42, 0x42, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xBC, 0xBC, 0xBC, 0xBC, 0xBC, 0xBC, 0xBC, 0x62, 0x62, 0x63, 0x00, 0x3B, 0x00, 0x65,
            0x62, 0x62, 0x7A, 0x62, 0x62, 0xFF, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
            0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x09,
            0x00, 0xFF, 0xC1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x3F, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0x62, 0xFF, 0x39, 0x00, 0xD3, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xEA,
        ];

        // Combine header and payload into a single data stream
        let mut data = Vec::new();
        data.extend_from_slice(&payload);

        let mut reader = Cursor::new(data);

        // Attempt to decode the frame
        let result = decode("MLLT", Version::Id3v23, &mut reader);

        // Ensure that the result is an error due to overflow
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e.kind, ErrorKind::InvalidInput));
            assert_eq!(e.description, "MLLT carry subtraction overflow");
        }
    }
}
