use crate::storage::{self, PlainStorage, Storage};
use crate::stream::frame;
use crate::stream::unsynch;
use crate::tag::{Tag, Version};
use crate::{Error, ErrorKind};
use bitflags::bitflags;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use std::cmp;
use std::fs;
use std::io::{self, Read, Write};
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

pub fn decode<R>(mut reader: R) -> crate::Result<Tag>
where
    R: io::Read,
{
    let mut tag_header = [0; 10];
    let nread = reader.read(&mut tag_header)?;
    if nread < tag_header.len() || &tag_header[0..3] != b"ID3" {
        return Err(Error::new(
            ErrorKind::NoTag,
            "reader does not contain an id3 tag",
        ));
    }
    let (ver_major, ver_minor) = (tag_header[4], tag_header[3]);
    let version = match (ver_major, ver_minor) {
        (_, 2) => Version::Id3v22,
        (_, 3) => Version::Id3v23,
        (_, 4) => Version::Id3v24,
        (_, _) => {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion(ver_major, ver_minor),
                "unsupported id3 tag version",
            ));
        }
    };
    let flags = Flags::from_bits(tag_header[5])
        .ok_or_else(|| Error::new(ErrorKind::Parsing, "unknown tag header flags are set"))?;
    let tag_size = unsynch::decode_u32(BigEndian::read_u32(&tag_header[6..10])) as usize;

    // compression only exists on 2.2 and conflicts with 2.3+'s extended header
    if version == Version::Id3v22 && flags.contains(Flags::COMPRESSION) {
        return Err(Error::new(
            ErrorKind::UnsupportedFeature,
            "id3v2.2 compression is not supported",
        ));
    }

    let mut offset = tag_header.len();

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
        offset += ext_size;
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

    let mut tag = Tag::new();
    while offset < tag_size + tag_header.len() {
        let (bytes_read, frame) = match frame::decode(
            &mut reader,
            version,
            flags.contains(Flags::UNSYNCHRONISATION),
        )? {
            Some(frame) => frame,
            None => break, // Padding.
        };
        tag.add_frame(frame);
        offset += bytes_read;
    }

    Ok(tag)
}

/// The Encoder may be used to encode tags.
#[derive(Debug, Builder)]
#[builder(pattern = "owned")]
pub struct Encoder {
    /// The tag version to encode to.
    #[builder(default = "Version::Id3v24")]
    version: Version,
    /// Enable the unsynchronisatin scheme. This avoids patterns that resemble MP3-frame headers
    /// from being encoded. If you are encoding to MP3 files, you probably want this enabled.
    #[builder(default = "true")]
    unsynchronisation: bool,
    /// Enable compression.
    #[builder(default = "false")]
    compression: bool,
    /// Informs the encoder that the file this tag belongs to has been changed.
    ///
    /// This subsequently discards any tags that have their File Alter Preservation bits set and
    /// that have a relation to the file contents:
    ///
    ///   AENC, ETCO, EQUA, MLLT, POSS, SYLT, SYTC, RVAD, TENC, TLEN, TSIZ
    #[builder(default = "false")]
    file_altered: bool,
}

impl Encoder {
    /// Encodes the specified tag using the settings set in the encoder.
    ///
    /// Note that the plain tag is written, regardless of the original contents. To safely encode a
    /// tag to an MP3 file, use `Encoder::encode_to_path`.
    pub fn encode<W>(&self, tag: &Tag, mut writer: W) -> crate::Result<()>
    where
        W: io::Write,
    {
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
        writer.write_all(b"ID3")?;
        writer.write_all(&[self.version.minor() as u8, 0])?;
        writer.write_u8(flags.bits())?;
        writer.write_u32::<BigEndian>(unsynch::encode_u32(frame_data.len() as u32))?;
        writer.write_all(&frame_data[..])?;
        Ok(())
    }

    /// Encodes a tag and replaces any existing tag in the file pointed to by the specified path.
    pub fn encode_to_path<P: AsRef<Path>>(&self, tag: &Tag, path: P) -> crate::Result<()> {
        let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        let location = storage::locate_id3v2(&mut file)?.unwrap_or(0..0); // Create a new tag if none could be located.

        let mut storage = PlainStorage::new(file, location);
        let mut w = storage.writer()?;
        self.encode(tag, &mut w)?;
        w.flush()?;
        Ok(())
    }
}

#[cfg(all(test, feature = "unstable"))]
mod benchmarks {
    extern crate test;
    use super::*;
    use std::fs;

    #[bench]
    fn read_id3v23(b: &mut test::Bencher) {
        let mut buf = Vec::new();
        fs::File::open("testdata/id3v23.id3")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        b.iter(|| {
            decode(&mut io::Cursor::new(buf.as_slice())).unwrap();
        });
    }

    #[bench]
    fn read_id3v24(b: &mut test::Bencher) {
        let mut buf = Vec::new();
        fs::File::open("testdata/id3v24.id3")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        b.iter(|| {
            decode(&mut io::Cursor::new(buf.as_slice())).unwrap();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Content, Frame, Picture, PictureType};
    use std::fs;
    use std::io;

    fn make_tag() -> Tag {
        let mut tag = Tag::new();
        tag.set_title("Title");
        tag.set_artist("Artist");
        tag.set_genre("Genre");
        tag.set_duration(1337);
        tag.add_picture(Picture {
            mime_type: "image/png".to_string(),
            picture_type: PictureType::CoverFront,
            description: "an image".to_string(),
            data: (0..255).cycle().take(8192).collect(),
        });
        tag
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
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .version(Version::Id3v22)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v22_unsynch() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .unsynchronisation(true)
            .version(Version::Id3v22)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v22_invalid_id() {
        let mut tag = make_tag();
        tag.add_frame(Frame::with_content("XXX", Content::Unknown(vec![1, 2, 3])));
        tag.add_frame(Frame::with_content("YYY", Content::Unknown(vec![4, 5, 6])));
        tag.add_frame(Frame::with_content("ZZZ", Content::Unknown(vec![7, 8, 9])));
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .version(Version::Id3v22)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v23() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .version(Version::Id3v23)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v23_compression() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .compression(true)
            .version(Version::Id3v23)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v23_unsynch() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .unsynchronisation(true)
            .version(Version::Id3v23)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .version(Version::Id3v24)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24_compression() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .compression(true)
            .version(Version::Id3v24)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }

    #[test]
    fn write_id3v24_unsynch() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .unsynchronisation(true)
            .version(Version::Id3v24)
            .build()
            .unwrap()
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
        EncoderBuilder::default()
            .version(Version::Id3v24)
            .file_altered(true)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer)
            .unwrap();

        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert!(tag_read.get("TLEN").is_none());
    }
}
