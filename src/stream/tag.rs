use crate::storage::{PlainStorage, Storage};
use crate::stream::{frame, unsynch};
use crate::tag::{Tag, Version};
use crate::taglike::TagLike;
use crate::{Error, ErrorKind};
use bitflags::bitflags;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use std::cmp;
use std::fs;
use std::io::{self, Read, Write};
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
}

struct Header {
    version: Version,
    flags: Flags,
    tag_size: u32,
    // TODO: Extended header.
}

impl Header {
    fn size(&self) -> u64 {
        10 // Raw header.
    }

    fn frame_bytes(&self) -> u64 {
        u64::from(self.tag_size)
    }

    fn tag_size(&self) -> u64 {
        self.size() + self.frame_bytes()
    }
}

impl Header {
    fn decode(mut reader: impl io::Read) -> crate::Result<Header> {
        let mut header = [0; 10];
        let nread = reader.read(&mut header)?;
        if nread < header.len() || &header[0..3] != b"ID3" {
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
                    ErrorKind::UnsupportedVersion(ver_major, ver_minor),
                    "unsupported id3 tag version",
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

        // TODO: actually use the extended header data.
        if flags.contains(Flags::EXTENDED_HEADER) {
            let ext_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?) as usize;
            // the extended header size includes itself
            if ext_size < 6 {
                return Err(Error::new(
                    ErrorKind::Parsing,
                    "Extended header has a minimum size of 6",
                ));
            }
            let ext_remaining_size = ext_size - 4;
            let mut ext_header = Vec::with_capacity(cmp::min(ext_remaining_size, 0xffff));
            reader
                .by_ref()
                .take(ext_remaining_size as u64)
                .read_to_end(&mut ext_header)?;
            if flags.contains(Flags::UNSYNCHRONISATION) {
                unsynch::decode_vec(&mut ext_header);
            }
        }

        Ok(Header {
            version,
            flags,
            tag_size,
        })
    }
}

pub fn decode(mut reader: impl io::Read) -> crate::Result<Tag> {
    let header = Header::decode(&mut reader)?;

    if header.version == Version::Id3v22 {
        // Limit the reader only to the given tag_size, don't return any more bytes after that.
        let v2_reader = reader.take(header.frame_bytes());

        if header.flags.contains(Flags::UNSYNCHRONISATION) {
            // Unwrap all 'unsynchronized' bytes in the tag before parsing frames.
            decode_v2_frames(unsynch::Reader::new(v2_reader))
        } else {
            decode_v2_frames(v2_reader)
        }
    } else {
        let mut offset = 0;
        let mut tag = Tag::with_version(header.version);
        while offset < header.frame_bytes() {
            let rs = frame::decode(
                &mut reader,
                header.version,
                header.flags.contains(Flags::UNSYNCHRONISATION),
            );
            let v = match rs {
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

/// The `Encoder` may be used to encode tags.
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
    /// * version is ID3v2.4
    /// * unsynchronization is disabled due to compatibility issues
    /// * no compression
    /// * file is not marked as altered
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

    /// Encodes the specified tag using the settings set in the encoder.
    ///
    /// Note that the plain tag is written, regardless of the original contents. To safely encode a
    /// tag to an MP3 file, use `Encoder::encode_to_path`.
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
            frame::encode(&mut frame_data, frame, self.version, self.unsynchronisation)?;
        }
        // In ID3v2, Unsynchronization is applied to the whole tag data at once, not for each frame
        // separately.
        if self.version == Version::Id3v22 && self.unsynchronisation {
            unsynch::encode_vec(&mut frame_data)
        }
        let tag_size = frame_data.len() + self.padding.unwrap_or(0);
        writer.write_all(b"ID3")?;
        writer.write_all(&[self.version.minor() as u8, 0])?;
        writer.write_u8(flags.bits())?;
        writer.write_u32::<BigEndian>(unsynch::encode_u32(tag_size as u32))?;
        writer.write_all(&frame_data[..])?;

        if let Some(padding) = self.padding {
            writer.write_all(&vec![0; padding])?;
        }
        Ok(())
    }

    /// Encodes a tag and replaces any existing tag in the file pointed to by the specified path.
    pub fn encode_to_path(&self, tag: &Tag, path: impl AsRef<Path>) -> crate::Result<()> {
        let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        #[allow(clippy::reversed_empty_ranges)]
        let location = locate_id3v2(&mut file)?.unwrap_or(0..0); // Create a new tag if none could be located.

        let mut storage = PlainStorage::new(file, location);
        let mut w = storage.writer()?;
        self.encode(tag, &mut w)?;
        w.flush()?;
        Ok(())
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn locate_id3v2(mut reader: impl io::Read + io::Seek) -> crate::Result<Option<Range<u64>>> {
    let header = match Header::decode(&mut reader) {
        Ok(v) => v,
        Err(err) => match err.kind {
            ErrorKind::NoTag => return Ok(None),
            _ => return Err(err),
        },
    };

    let tag_size = header.tag_size();
    reader.seek(io::SeekFrom::Start(tag_size))?;
    let num_padding = reader
        .bytes()
        .take_while(|rs| rs.as_ref().map(|b| *b == 0x00).unwrap_or(false))
        .count();
    Ok(Some(0..tag_size + num_padding as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{
        Chapter, Content, Frame, Picture, PictureType, SynchronisedLyrics, SynchronisedLyricsType,
        TimestampFormat,
    };
    use std::fs;
    use std::io::{self, Read};

    fn make_tag(version: Version) -> Tag {
        let mut tag = Tag::new();
        tag.set_title("Title");
        tag.set_artist("Artist");
        tag.set_genre("Genre");
        tag.set_duration(1337);
        tag.add_encapsulated_object(
            "Some Object",
            "application/octet-stream",
            "",
            &b"\xC0\xFF\xEE\x00"[..],
        );
        let mut image_data = Vec::new();
        fs::File::open("testdata/image.jpg")
            .unwrap()
            .read_to_end(&mut image_data)
            .unwrap();
        tag.add_picture(Picture {
            mime_type: "image/jpeg".to_string(),
            picture_type: PictureType::CoverFront,
            description: "an image".to_string(),
            data: image_data,
        });
        tag.add_synchronised_lyrics(SynchronisedLyrics {
            lang: "eng".to_string(),
            timestamp_format: TimestampFormat::Ms,
            content_type: SynchronisedLyricsType::Lyrics,
            content: vec![
                (1000, "he".to_string()),
                (1100, "llo".to_string()),
                (1200, "world".to_string()),
            ],
        });
        if let Version::Id3v23 | Version::Id3v24 = version {
            tag.add_chapter(Chapter {
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
        assert_eq!(
            PictureType::Other,
            tag.pictures().nth(0).unwrap().picture_type
        );
        assert_eq!("", tag.pictures().nth(0).unwrap().description);
        assert_eq!("image/jpeg", tag.pictures().nth(0).unwrap().mime_type);
    }

    #[test]
    fn read_id3v23() {
        let mut file = fs::File::open("testdata/id3v23.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!("Genre", tag.genre().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        assert_eq!(
            PictureType::CoverFront,
            tag.pictures().nth(0).unwrap().picture_type
        );
    }

    #[test]
    fn read_id3v23_geob() {
        let mut file = fs::File::open("testdata/id3v23_geob.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!(tag.encapsulated_objects().count(), 7);

        let geob = tag.encapsulated_objects().nth(0).unwrap();
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
    fn read_id3v24() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        assert_eq!(
            PictureType::CoverFront,
            tag.pictures().nth(0).unwrap().picture_type
        );
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

    #[test]
    fn write_id3v22() {
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
        let mut tag = make_tag(Version::Id3v22);
        tag.add_frame(Frame::with_content("XXX", Content::Unknown(vec![1, 2, 3])));
        tag.add_frame(Frame::with_content("YYY", Content::Unknown(vec![4, 5, 6])));
        tag.add_frame(Frame::with_content("ZZZ", Content::Unknown(vec![7, 8, 9])));
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
        assert_eq!(Some(0..0x0000c3ea), location);
    }

    #[test]
    fn test_locate_id3v23() {
        let file = fs::File::open("testdata/id3v23.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(Some(0..0x00006c0a), location);
    }

    #[test]
    fn test_locate_id3v24() {
        let file = fs::File::open("testdata/id3v24.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(Some(0..0x00006c0a), location);
    }

    #[test]
    fn test_locate_id3v24_ext() {
        let file = fs::File::open("testdata/id3v24_ext.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(Some(0..0x0000018d), location);
    }

    #[test]
    fn test_locate_no_tag() {
        let file = fs::File::open("testdata/mpeg-header").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert_eq!(None, location);
    }

    #[test]
    fn read_github_issue_60() {
        let mut file = fs::File::open("testdata/github-issue-60.id3").unwrap();
        let err = decode(&mut file).err().unwrap();
        err.partial_tag.unwrap();
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
}
