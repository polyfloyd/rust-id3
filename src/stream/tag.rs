use crate::chunk;
use crate::storage::{plain::PlainStorage, Format, Storage, StorageFile};
use crate::stream::{frame, unsynch};
use crate::tag::{Tag, Version};
use crate::taglike::TagLike;
use crate::{Error, ErrorKind};
use bitflags::bitflags;
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use std::cmp;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::ops::Range;
use std::path::Path;

static DEFAULT_FILE_DISCARD: &[&str] = &[
    "AENC", "ETCO", "EQUA", "MLLT", "POSS", "SYLT", "SYTC", "RVAD", "TENC", "TLEN", "TSIZ",
];

bitflags! {
    struct Flags: u8 {
        const UNSYNCHRONISATION = 0x80; // All versions
        const COMPRESSION       = 0x40; // =ID3v2.2
        const EXTENDED_HEADER   = 0x40; // >ID3v2.3, duplicate with TAG_COMPRESSION :(
        const EXPERIMENTAL      = 0x20; // >ID3v2.3
        const FOOTER            = 0x10; // >ID3v2.4
    }

    struct ExtFlags: u8 {
        const TAG_IS_UPDATE    = 0x40;
        const CRC_DATA_PRESENT = 0x20;
        const TAG_RESTRICTIONS = 0x10;
    }
}

/// Used for sharing code between sync/async parsers, which is mainly complicated by ext_headers.
struct HeaderBuilder {
    version: Version,
    flags: Flags,
    tag_size: u32,
}

impl HeaderBuilder {
    fn with_ext_header(self, size: u32) -> Header {
        Header {
            version: self.version,
            flags: self.flags,
            tag_size: self.tag_size,
            ext_header_size: size,
        }
    }
}

struct Header {
    version: Version,
    flags: Flags,
    tag_size: u32,

    // TODO: Extended header.
    ext_header_size: u32,
}

impl Header {
    fn size(&self) -> u64 {
        10 // Raw header.
    }

    fn frame_bytes(&self) -> u64 {
        u64::from(self.tag_size).saturating_sub(u64::from(self.ext_header_size))
    }

    fn tag_size(&self) -> u64 {
        self.size() + self.frame_bytes()
    }
}

impl Header {
    fn decode(mut reader: impl io::Read) -> crate::Result<Header> {
        let mut header = [0; 10];
        let nread = reader.read(&mut header)?;
        let base_header = Self::decode_base_header(&header[..nread])?;

        // TODO: actually use the extended header data.
        let ext_header_size = if base_header.flags.contains(Flags::EXTENDED_HEADER) {
            let mut ext_header = [0; 6];
            reader.read_exact(&mut ext_header)?;
            let ext_size = unsynch::decode_u32(BigEndian::read_u32(&ext_header[0..4]));
            // The extended header size includes itself and always has at least 2 bytes following.
            if ext_size < 6 {
                return Err(Error::new(
                    ErrorKind::Parsing,
                    "Extended header requires has a minimum size of 6",
                ));
            }

            let _ext_flags = ExtFlags::from_bits_truncate(ext_header[5]);

            let ext_remaining_size = ext_size - ext_header.len() as u32;
            let mut ext_header = Vec::with_capacity(cmp::min(ext_remaining_size as usize, 0xffff));
            reader
                .by_ref()
                .take(ext_remaining_size as u64)
                .read_to_end(&mut ext_header)?;

            ext_size
        } else {
            0
        };

        Ok(base_header.with_ext_header(ext_header_size))
    }

    #[cfg(feature = "tokio")]
    async fn async_decode(
        mut reader: impl tokio::io::AsyncRead + std::marker::Unpin,
    ) -> crate::Result<Header> {
        use tokio::io::AsyncReadExt;

        let mut header = [0; 10];
        let nread = reader.read(&mut header).await?;
        let base_header = Self::decode_base_header(&header[..nread])?;

        // TODO: actually use the extended header data.
        let ext_header_size = if base_header.flags.contains(Flags::EXTENDED_HEADER) {
            let mut ext_header = [0; 6];
            reader.read_exact(&mut ext_header).await?;
            let ext_size = unsynch::decode_u32(BigEndian::read_u32(&ext_header[0..4]));
            // The extended header size includes itself and always has at least 2 bytes following.
            if ext_size < 6 {
                return Err(Error::new(
                    ErrorKind::Parsing,
                    "Extended header requires has a minimum size of 6",
                ));
            }

            let _ext_flags = ExtFlags::from_bits_truncate(ext_header[5]);

            let ext_remaining_size = ext_size - ext_header.len() as u32;
            let mut ext_header = Vec::with_capacity(cmp::min(ext_remaining_size as usize, 0xffff));
            reader
                .take(ext_remaining_size as u64)
                .read_to_end(&mut ext_header)
                .await?;

            ext_size
        } else {
            0
        };

        Ok(base_header.with_ext_header(ext_header_size))
    }

    fn decode_base_header(header: &[u8]) -> crate::Result<HeaderBuilder> {
        if header.len() != 10 {
            return Err(Error::new(
                ErrorKind::NoTag,
                "reader is not large enough to contain a id3 tag",
            ));
        }

        if &header[0..3] != b"ID3" {
            return Err(Error::new(
                ErrorKind::NoTag,
                "reader does not contain an id3 tag",
            ));
        }

        let (ver_major, ver_minor) = (header[3], header[4]);
        let version = match (ver_major, ver_minor) {
            (2, _) => Version::Id3v22,
            (3, _) => Version::Id3v23,
            (4, _) => Version::Id3v24,
            (_, _) => {
                return Err(Error::new(
                    ErrorKind::UnsupportedFeature,
                    format!(
                        "Unsupported id3 tag version: v2.{}.{}",
                        ver_major, ver_minor
                    ),
                ));
            }
        };
        let flags = Flags::from_bits(header[5])
            .ok_or_else(|| Error::new(ErrorKind::Parsing, "unknown tag header flags are set"))?;
        let tag_size = unsynch::decode_u32(BigEndian::read_u32(&header[6..10]));

        // compression only exists on 2.2 and conflicts with 2.3+'s extended header
        if version == Version::Id3v22 && flags.contains(Flags::COMPRESSION) {
            return Err(Error::new(
                ErrorKind::UnsupportedFeature,
                "id3v2.2 compression is not supported",
            ));
        }

        Ok(HeaderBuilder {
            version,
            flags,
            tag_size,
        })
    }
}

pub fn decode(mut reader: impl io::Read) -> crate::Result<Tag> {
    let header = Header::decode(&mut reader)?;

    decode_remaining(reader, header)
}

#[cfg(feature = "tokio")]
pub async fn async_decode(
    mut reader: impl tokio::io::AsyncRead + std::marker::Unpin,
) -> crate::Result<Tag> {
    let header = Header::async_decode(&mut reader).await?;

    let reader = {
        use tokio::io::AsyncReadExt;

        let mut buf = Vec::new();

        reader
            .take(header.frame_bytes())
            .read_to_end(&mut buf)
            .await?;
        std::io::Cursor::new(buf)
    };

    decode_remaining(reader, header)
}

fn decode_remaining(mut reader: impl io::Read, header: Header) -> crate::Result<Tag> {
    match header.version {
        Version::Id3v22 => {
            // Limit the reader only to the given tag_size, don't return any more bytes after that.
            let v2_reader = reader.take(header.frame_bytes());

            if header.flags.contains(Flags::UNSYNCHRONISATION) {
                // Unwrap all 'unsynchronized' bytes in the tag before parsing frames.
                decode_v2_frames(unsynch::Reader::new(v2_reader))
            } else {
                decode_v2_frames(v2_reader)
            }
        }
        Version::Id3v23 => {
            // Unsynchronization is applied to the whole tag, excluding the header.
            let mut reader: Box<dyn io::Read> = if header.flags.contains(Flags::UNSYNCHRONISATION) {
                Box::new(unsynch::Reader::new(reader))
            } else {
                Box::new(reader)
            };

            let mut offset = 0;
            let mut tag = Tag::with_version(header.version);
            while offset < header.frame_bytes() {
                let v = match frame::v3::decode(&mut reader) {
                    Ok(v) => v,
                    Err(err) => return Err(err.with_tag(tag)),
                };
                let (bytes_read, frame) = match v {
                    Some(v) => v,
                    None => break, // Padding.
                };
                tag.add_frame(frame);
                offset += bytes_read as u64;
            }
            Ok(tag)
        }
        Version::Id3v24 => {
            let mut offset = 0;
            let mut tag = Tag::with_version(header.version);

            while offset < header.frame_bytes() {
                let v = match frame::v4::decode(&mut reader) {
                    Ok(v) => v,
                    Err(err) => return Err(err.with_tag(tag)),
                };
                let (bytes_read, frame) = match v {
                    Some(v) => v,
                    None => break, // Padding.
                };
                tag.add_frame(frame);
                offset += bytes_read as u64;
            }
            Ok(tag)
        }
    }
}

pub fn decode_v2_frames(mut reader: impl io::Read) -> crate::Result<Tag> {
    let mut tag = Tag::with_version(Version::Id3v22);
    // Add all frames, until either an error is thrown or there are no more frames to parse
    // (because of EOF or a Padding).
    loop {
        let v = match frame::v2::decode(&mut reader) {
            Ok(v) => v,
            Err(err) => return Err(err.with_tag(tag)),
        };
        match v {
            Some((_bytes_read, frame)) => {
                tag.add_frame(frame);
            }
            None => break Ok(tag),
        }
    }
}

/// The `Encoder` may be used to encode tags with custom settings.
#[derive(Clone, Debug)]
pub struct Encoder {
    version: Version,
    unsynchronisation: bool,
    compression: bool,
    file_altered: bool,
    padding: Option<usize>,
}

impl Encoder {
    /// Constructs a new `Encoder` with the following configuration:
    ///
    /// * [`Version`] is ID3v2.4
    /// * Unsynchronization is disabled due to compatibility issues
    /// * No compression
    /// * File is not marked as altered
    pub fn new() -> Self {
        Self {
            version: Version::Id3v24,
            unsynchronisation: false,
            compression: false,
            file_altered: false,
            padding: None,
        }
    }

    /// Sets the padding that is written after the tag.
    ///
    /// Should be only used when writing to a MP3 file
    pub fn padding(mut self, padding: usize) -> Self {
        self.padding = Some(padding);
        self
    }

    /// Sets the ID3 version.
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Enables or disables the unsynchronisation scheme.
    ///
    /// This avoids patterns that resemble MP3-frame headers from being
    /// encoded. If you are encoding to MP3 files and wish to be compatible
    /// with very old tools, you probably want this enabled.
    pub fn unsynchronisation(mut self, unsynchronisation: bool) -> Self {
        self.unsynchronisation = unsynchronisation;
        self
    }

    /// Enables or disables compression.
    pub fn compression(mut self, compression: bool) -> Self {
        self.compression = compression;
        self
    }

    /// Informs the encoder whether the file this tag belongs to has been changed.
    ///
    /// This subsequently discards any tags that have their File Alter Preservation bits set and
    /// that have a relation to the file contents:
    ///
    ///   AENC, ETCO, EQUA, MLLT, POSS, SYLT, SYTC, RVAD, TENC, TLEN, TSIZ
    pub fn file_altered(mut self, file_altered: bool) -> Self {
        self.file_altered = file_altered;
        self
    }

    /// Encodes the specified [`Tag`] using the settings set in the [`Encoder`].
    ///
    /// Note that the plain tag is written, regardless of the original contents. To safely encode a
    /// tag to an MP3 file, use [`Encoder::encode_to_path`].
    pub fn encode(&self, tag: &Tag, mut writer: impl io::Write) -> crate::Result<()> {
        // remove frames which have the flags indicating they should be removed
        let saved_frames = tag
            .frames()
            // Assert that by encoding, we are changing the tag. If the Tag Alter Preservation bit
            // is set, discard the frame.
            .filter(|frame| !frame.tag_alter_preservation())
            // If the file this tag belongs to is updated, check for the File Alter Preservation
            // bit.
            .filter(|frame| !self.file_altered || !frame.file_alter_preservation())
            // Check whether this frame is part of the set of frames that should always be
            // discarded when the file is changed.
            .filter(|frame| !self.file_altered || !DEFAULT_FILE_DISCARD.contains(&frame.id()));

        let mut flags = Flags::empty();
        flags.set(Flags::UNSYNCHRONISATION, self.unsynchronisation);
        if self.version == Version::Id3v22 {
            flags.set(Flags::COMPRESSION, self.compression);
        }

        let mut frame_data = Vec::new();
        for frame in saved_frames {
            frame.validate()?;
            frame::encode(&mut frame_data, frame, self.version, self.unsynchronisation)?;
        }
        // In ID3v2.2/ID3v2.3, Unsynchronization is applied to the whole tag data at once, not for
        // each frame separately.
        if self.unsynchronisation {
            match self.version {
                Version::Id3v22 | Version::Id3v23 => unsynch::encode_vec(&mut frame_data),
                Version::Id3v24 => {}
            };
        }
        let tag_size = frame_data.len() + self.padding.unwrap_or(0);
        writer.write_all(b"ID3")?;
        writer.write_all(&[self.version.minor(), 0])?;
        writer.write_u8(flags.bits())?;
        writer.write_u32::<BigEndian>(unsynch::encode_u32(tag_size as u32))?;
        writer.write_all(&frame_data[..])?;

        if let Some(padding) = self.padding {
            writer.write_all(&vec![0; padding])?;
        }
        Ok(())
    }

    /// Encodes a [`Tag`] and replaces any existing tag in the file.
    pub fn write_to_file(&self, tag: &Tag, mut file: impl StorageFile) -> crate::Result<()> {
        let mut probe = [0; 12];
        let nread = file.read(&mut probe)?;
        file.seek(io::SeekFrom::Start(0))?;
        let storage_format = Format::magic(&probe[..nread]);

        match storage_format {
            Some(Format::Aiff) => {
                chunk::write_id3_chunk_file::<chunk::AiffFormat>(file, tag, self.version)?;
            }
            Some(Format::Wav) => {
                chunk::write_id3_chunk_file::<chunk::WavFormat>(file, tag, self.version)?;
            }
            Some(Format::Header) => {
                let location = locate_id3v2(&mut file)?;
                let mut storage = PlainStorage::new(file, location);
                let mut w = storage.writer()?;
                self.encode(tag, &mut w)?;
                w.flush()?;
            }
            None => {
                let mut storage = PlainStorage::new(file, 0..0);
                let mut w = storage.writer()?;
                self.encode(tag, &mut w)?;
                w.flush()?;
            }
        };

        Ok(())
    }

    /// Encodes a [`Tag`] and replaces any existing tag in the file.
    #[deprecated(note = "Use write_to_file")]
    pub fn encode_to_file(&self, tag: &Tag, file: &mut fs::File) -> crate::Result<()> {
        self.write_to_file(tag, file)
    }

    /// Encodes a [`Tag`] and replaces any existing tag in the file pointed to by the specified path.
    pub fn write_to_path(&self, tag: &Tag, path: impl AsRef<Path>) -> crate::Result<()> {
        let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        self.write_to_file(tag, &mut file)?;
        file.flush()?;
        Ok(())
    }

    /// Encodes a [`Tag`] and replaces any existing tag in the file pointed to by the specified path.
    #[deprecated(note = "Use write_to_path")]
    pub fn encode_to_path(&self, tag: &Tag, path: impl AsRef<Path>) -> crate::Result<()> {
        self.write_to_path(tag, path)
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn locate_id3v2(reader: impl io::Read + io::Seek) -> crate::Result<Range<u64>> {
    let mut reader = io::BufReader::new(reader);

    let header = Header::decode(&mut reader)?;

    let tag_size = header.tag_size();
    reader.seek(io::SeekFrom::Start(tag_size))?;
    let num_padding = reader
        .bytes()
        .take_while(|rs| rs.as_ref().map(|b| *b == 0x00).unwrap_or(false))
        .count();
    Ok(0..tag_size + num_padding as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{
        Chapter, Content, EncapsulatedObject, Frame, MpegLocationLookupTable,
        MpegLocationLookupTableReference, Picture, PictureType, Popularimeter, Private,
        SynchronisedLyrics, SynchronisedLyricsType, TableOfContents, TimestampFormat,
        UniqueFileIdentifier, Unknown,
    };
    use std::fs::{self};
    use std::io::{self, Read};

    fn make_tag(version: Version) -> Tag {
        let mut tag = Tag::new();
        tag.set_title("Title");
        tag.set_artist("Artist");
        tag.set_genre("Genre");
        tag.add_frame(Frame::with_content(
            "TPE1",
            Content::new_text_values(["artist 1", "artist 2", "artist 3"]),
        ));
        tag.set_duration(1337);
        tag.add_frame(EncapsulatedObject {
            mime_type: "Some Object".to_string(),
            filename: "application/octet-stream".to_string(),
            description: "".to_string(),
            data: b"\xC0\xFF\xEE\x00".to_vec(),
        });
        let mut image_data = Vec::new();
        fs::File::open("testdata/image.jpg")
            .unwrap()
            .read_to_end(&mut image_data)
            .unwrap();
        tag.add_frame(Picture {
            mime_type: "image/jpeg".to_string(),
            picture_type: PictureType::CoverFront,
            description: "an image".to_string(),
            data: image_data,
        });
        tag.add_frame(Popularimeter {
            user: "user@example.com".to_string(),
            rating: 255,
            counter: 1337,
        });
        tag.add_frame(SynchronisedLyrics {
            lang: "eng".to_string(),
            timestamp_format: TimestampFormat::Ms,
            content_type: SynchronisedLyricsType::Lyrics,
            content: vec![
                (1000, "he".to_string()),
                (1100, "llo".to_string()),
                (1200, "world".to_string()),
            ],
            description: String::from("description"),
        });
        if let Version::Id3v23 | Version::Id3v24 = version {
            tag.add_frame(Chapter {
                element_id: "01".to_string(),
                start_time: 1000,
                end_time: 2000,
                start_offset: 0xff,
                end_offset: 0xff,
                frames: vec![
                    Frame::with_content("TIT2", Content::Text("Foo".to_string())),
                    Frame::with_content("TALB", Content::Text("Bar".to_string())),
                    Frame::with_content("TCON", Content::Text("Baz".to_string())),
                ],
            });
            tag.add_frame(TableOfContents {
                element_id: "table01".to_string(),
                top_level: true,
                ordered: true,
                elements: vec!["01".to_string()],
                frames: Vec::new(),
            });
            tag.add_frame(MpegLocationLookupTable {
                frames_between_reference: 1,
                bytes_between_reference: 418,
                millis_between_reference: 12,
                bits_for_bytes: 4,
                bits_for_millis: 4,
                references: vec![
                    MpegLocationLookupTableReference {
                        deviate_bytes: 0xa,
                        deviate_millis: 0xf,
                    },
                    MpegLocationLookupTableReference {
                        deviate_bytes: 0xa,
                        deviate_millis: 0x0,
                    },
                ],
            });
            tag.add_frame(Private {
                owner_identifier: "PrivateFrameIdentifier1".to_string(),
                private_data: "SomePrivateBytes".into(),
            });
            tag.add_frame(UniqueFileIdentifier {
                owner_identifier: String::from("http://www.id3.org/dummy/ufid.html"),
                identifier: "7FZo5fMqyG5Ys1dm8F1FHa".into(),
            });
            tag.add_frame(UniqueFileIdentifier {
                owner_identifier: String::from("example.com"),
                identifier: "3107f6e3-99c0-44c1-9785-655fc9c32d8b".into(),
            });
        }
        tag
    }

    #[test]
    fn read_id3v22() {
        let mut file = fs::File::open("testdata/id3v22.id3").unwrap();
        let tag: Tag = decode(&mut file).unwrap();
        assert_eq!("Henry Frottey INTRO", tag.title().unwrap());
        assert_eq!("Hörbuch & Gesprochene Inhalte", tag.genre().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(27, tag.total_discs().unwrap());
        assert_eq!(2015, tag.year().unwrap());
        if cfg!(feature = "decode_picture") {
            assert_eq!(
                PictureType::Other,
                tag.pictures().next().unwrap().picture_type
            );
            assert_eq!("", tag.pictures().next().unwrap().description);
            assert_eq!("image/jpeg", tag.pictures().next().unwrap().mime_type);
        }
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn read_id3v22_tokio() {
        let mut file = tokio::fs::File::open("testdata/id3v22.id3").await.unwrap();
        let tag: Tag = async_decode(&mut file).await.unwrap();
        assert_eq!("Henry Frottey INTRO", tag.title().unwrap());
        assert_eq!("Hörbuch & Gesprochene Inhalte", tag.genre().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(27, tag.total_discs().unwrap());
        assert_eq!(2015, tag.year().unwrap());
        if cfg!(feature = "decode_picture") {
            assert_eq!(
                PictureType::Other,
                tag.pictures().next().unwrap().picture_type
            );
            assert_eq!("", tag.pictures().next().unwrap().description);
            assert_eq!("image/jpeg", tag.pictures().next().unwrap().mime_type);
        }
    }

    #[test]
    fn read_id3v23() {
        let mut file = fs::File::open("testdata/id3v23.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!("Genre", tag.genre().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        if cfg!(feature = "decode_picture") {
            assert_eq!(
                PictureType::CoverFront,
                tag.pictures().next().unwrap().picture_type
            );
        }
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn read_id3v23_tokio() {
        let mut file = tokio::fs::File::open("testdata/id3v23.id3").await.unwrap();
        let tag = async_decode(&mut file).await.unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!("Genre", tag.genre().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        if cfg!(feature = "decode_picture") {
            assert_eq!(
                PictureType::CoverFront,
                tag.pictures().next().unwrap().picture_type
            );
        }
    }

    #[test]
    fn read_id3v23_geob() {
        let mut file = fs::File::open("testdata/id3v23_geob.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!(tag.encapsulated_objects().count(), 7);

        let geob = tag.encapsulated_objects().next().unwrap();
        assert_eq!(geob.description, "Serato Overview");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 3842);

        let geob = tag.encapsulated_objects().nth(1).unwrap();
        assert_eq!(geob.description, "Serato Analysis");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 2);

        let geob = tag.encapsulated_objects().nth(2).unwrap();
        assert_eq!(geob.description, "Serato Autotags");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 21);

        let geob = tag.encapsulated_objects().nth(3).unwrap();
        assert_eq!(geob.description, "Serato Markers_");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 318);

        let geob = tag.encapsulated_objects().nth(4).unwrap();
        assert_eq!(geob.description, "Serato Markers2");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 470);

        let geob = tag.encapsulated_objects().nth(5).unwrap();
        assert_eq!(geob.description, "Serato BeatGrid");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 39);

        let geob = tag.encapsulated_objects().nth(6).unwrap();
        assert_eq!(geob.description, "Serato Offsets_");
        assert_eq!(geob.mime_type, "application/octet-stream");
        assert_eq!(geob.filename, "");
        assert_eq!(geob.data.len(), 29829);
    }

    #[test]
    fn read_id3v23_chap() {
        let mut file = fs::File::open("testdata/id3v23_chap.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!(tag.chapters().count(), 7);

        let chapter_titles = tag
            .chapters()
            .map(|chap| chap.frames.first().unwrap().content().text().unwrap())
            .collect::<Vec<&str>>();
        assert_eq!(
            chapter_titles,
            &[
                "MPU 554",
                "Read-it-Later Services?",
                "Safari Reading List",
                "Third-Party Services",
                "What We’re Using",
                "David’s Research Workflow",
                "Apple’s September"
            ]
        );
    }

    #[test]
    fn read_id3v23_ctoc() {
        let mut file = fs::File::open("testdata/id3v23_chap.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!(tag.tables_of_contents().count(), 1);

        for x in tag.tables_of_contents() {
            println!("{:?}", x);
        }

        let ctoc = tag.tables_of_contents().last().unwrap();

        assert_eq!(ctoc.element_id, "toc");
        assert!(ctoc.top_level);
        assert!(ctoc.ordered);
        assert_eq!(
            ctoc.elements,
            &["chp0", "chp1", "chp2", "chp3", "chp4", "chp5", "chp6"]
        );
        assert!(ctoc.frames.is_empty());
    }

    #[test]
    fn read_id3v24() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        if cfg!(feature = "decode_picture") {
            assert_eq!(
                PictureType::CoverFront,
                tag.pictures().next().unwrap().picture_type
            );
        }
    }

    #[test]
    fn read_id3v24_extended() {
        let mut file = fs::File::open("testdata/id3v24_ext.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!("Genre", tag.genre().unwrap());
        assert_eq!("Artist", tag.artist().unwrap());
        assert_eq!("Album", tag.album().unwrap());
        assert_eq!(2, tag.track().unwrap());
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn read_id3v24_extended_tokio() {
        let mut file = tokio::fs::File::open("testdata/id3v24_ext.id3")
            .await
            .unwrap();
        let tag = async_decode(&mut file).await.unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!("Genre", tag.genre().unwrap());
        assert_eq!("Artist", tag.artist().unwrap());
        assert_eq!("Album", tag.album().unwrap());
        assert_eq!(2, tag.track().unwrap());
    }

    #[test]
    fn write_id3v22() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v22);
        let mut buffer = Vec::new();
        Encoder::new()
            .version(Version::Id3v22)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v22_unsynch() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v22);
        let mut buffer = Vec::new();
        Encoder::new()
            .unsynchronisation(true)
            .version(Version::Id3v22)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v22_invalid_id() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let mut tag = make_tag(Version::Id3v22);
        tag.add_frame(Frame::with_content(
            "XXX",
            Content::Unknown(Unknown {
                version: Version::Id3v22,
                data: vec![1, 2, 3],
            }),
        ));
        tag.add_frame(Frame::with_content(
            "YYY",
            Content::Unknown(Unknown {
                version: Version::Id3v22,
                data: vec![4, 5, 6],
            }),
        ));
        tag.add_frame(Frame::with_content(
            "ZZZ",
            Content::Unknown(Unknown {
                version: Version::Id3v22,
                data: vec![7, 8, 9],
            }),
        ));
        let mut buffer = Vec::new();
        Encoder::new()
            .version(Version::Id3v22)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v23() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v23);
        let mut buffer = Vec::new();
        Encoder::new()
            .version(Version::Id3v23)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v23_compression() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v23);
        let mut buffer = Vec::new();
        Encoder::new()
            .compression(true)
            .version(Version::Id3v23)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v23_unsynch() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v23);
        let mut buffer = Vec::new();
        Encoder::new()
            .unsynchronisation(true)
            .version(Version::Id3v23)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v24);
        let mut buffer = Vec::new();
        Encoder::new()
            .version(Version::Id3v24)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24_compression() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v24);
        let mut buffer = Vec::new();
        Encoder::new()
            .compression(true)
            .version(Version::Id3v24)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24_unsynch() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let tag = make_tag(Version::Id3v24);
        let mut buffer = Vec::new();
        Encoder::new()
            .unsynchronisation(true)
            .version(Version::Id3v24)
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24_alter_file() {
        if !cfg!(feature = "decode_picture") {
            return;
        }

        let mut tag = Tag::new();
        tag.set_duration(1337);

        let mut buffer = Vec::new();
        Encoder::new()
            .version(Version::Id3v24)
            .file_altered(true)
            .encode(&tag, &mut buffer)
            .unwrap();

        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert!(tag_read.get("TLEN").is_none());
    }

    #[test]
    fn test_locate_id3v22() {
        let file = fs::File::open("testdata/id3v22.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(0..0x0000c3ea, location);
    }

    #[test]
    fn test_locate_id3v23() {
        let file = fs::File::open("testdata/id3v23.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(0..0x00006c0a, location);
    }

    #[test]
    fn test_locate_id3v24() {
        let file = fs::File::open("testdata/id3v24.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(0..0x00006c0a, location);
    }

    #[test]
    fn test_locate_id3v24_ext() {
        let file = fs::File::open("testdata/id3v24_ext.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(0..0x0000018d, location);
    }

    #[test]
    fn test_locate_no_tag() {
        let file = fs::File::open("testdata/mpeg-header").unwrap();
        let location = locate_id3v2(file).unwrap_err();
        assert!(matches!(
            location,
            Error {
                kind: ErrorKind::NoTag,
                ..
            }
        ));
    }

    #[test]
    fn read_github_issue_60() {
        let mut file = fs::File::open("testdata/github-issue-60.id3").unwrap();
        let _tag = decode(&mut file).unwrap();
    }

    #[test]
    fn read_github_issue_73() {
        let mut file = fs::File::open("testdata/github-issue-73.id3").unwrap();
        let mut tag = decode(&mut file).unwrap();
        assert_eq!(tag.track(), Some(9));

        tag.set_total_tracks(16);
        assert_eq!(tag.track(), Some(9));
        assert_eq!(tag.total_tracks(), Some(16));
    }

    #[test]
    fn write_id3v24_ufids() {
        let mut tag = make_tag(Version::Id3v24);
        tag.add_frame(UniqueFileIdentifier {
            owner_identifier: String::from("http://www.id3.org/dummy/ufid.html"),
            identifier: "7FZo5fMqyG5Ys1dm8F1FHa".into(),
        });
        assert_eq!(tag.unique_file_identifiers().count(), 2);

        tag.add_frame(UniqueFileIdentifier {
            owner_identifier: String::from("http://www.id3.org/dummy/ufid.html"),
            identifier: "09FxXfNTQsCgzkPmCeFwlr".into(),
        });
        assert_eq!(tag.unique_file_identifiers().count(), 2);

        tag.add_frame(UniqueFileIdentifier {
            owner_identifier: String::from("open.blotchify.com"),
            identifier: "09FxXfNTQsCgzkPmCeFwlr".into(),
        });

        assert_eq!(tag.unique_file_identifiers().count(), 3);

        let mut buffer = Vec::new();
        Encoder::new()
            .compression(true)
            .version(Version::Id3v24)
            .encode(&tag, &mut buffer)
            .unwrap();
        let mut tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();

        if !cfg!(feature = "decode_picture") {
            tag_read.remove_all_pictures();
            tag.remove_all_pictures();
        }

        assert_eq!(tag, tag_read);
    }

    #[test]
    fn test_frame_bytes_underflow() {
        let header = Header {
            version: Version::Id3v24,
            flags: Flags::empty(),
            tag_size: 10,
            ext_header_size: 20,
        };

        // Without saturating_sub, this would underflow and cause a panic.
        assert_eq!(header.frame_bytes(), 0);
    }
}
