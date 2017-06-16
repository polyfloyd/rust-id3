use std::cmp;
use std::io::{self, Read};
use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use ::stream::frame;
use ::stream::unsynch;
use ::tag::{Tag, Version};


static DEFAULT_FILE_DISCARD: &[&str] = &[
    "AENC",
    "ETCO",
    "EQUA",
    "MLLT",
    "POSS",
    "SYLT",
    "SYTC",
    "RVAD",
    "TENC",
    "TLEN",
    "TSIZ",
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


pub fn decode<R>(mut reader: R) -> ::Result<Tag>
    where R: io::Read {
    let mut tag_header = [0; 10];
    let nread = reader.read(&mut tag_header)?;
    if nread < tag_header.len() || &tag_header[0..3] != b"ID3" {
        return Err(::Error::new(::ErrorKind::NoTag, "reader does not contain an id3 tag"));
    }
    let (ver_major, ver_minor) = (tag_header[4], tag_header[3]);
    let version = match (ver_major, ver_minor) {
        (_, 2) => Version::Id3v22,
        (_, 3) => Version::Id3v23,
        (_, 4) => Version::Id3v24,
        (_, _) => {
            return Err(::Error::new(::ErrorKind::UnsupportedVersion(ver_major, ver_minor), "unsupported id3 tag version"));
        },
    };
    let flags = Flags::from_bits(tag_header[5])
        .ok_or(::Error::new(::ErrorKind::Parsing, "unknown tag header flags are set"))?;
    let tag_size = unsynch::decode_u32(BigEndian::read_u32(&tag_header[6..10])) as usize;

    if flags.contains(COMPRESSION) {
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "id3v2.2 compression is not supported"));
    }

    let mut offset = tag_header.len();

    // TODO: actually use the extended header data.
    if flags.contains(EXTENDED_HEADER) {
        let ext_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?) as usize;
        offset += 4 + ext_size;
        let mut ext_header = Vec::with_capacity(cmp::min(ext_size, 0xffff));
        reader.by_ref()
            .take(ext_size as u64)
            .read_to_end(&mut ext_header)?;
        if flags.contains(UNSYNCHRONISATION) {
            unsynch::decode_vec(&mut ext_header);
        }
    }

    let mut tag = Tag::new();
    while offset < tag_size + tag_header.len() {
        let (bytes_read, frame) = match frame::decode(&mut reader, version, flags.contains(UNSYNCHRONISATION))? {
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
#[builder(setter(into))]
pub struct Encoder {
    #[builder(default="Version::Id3v24")]
    version: Version,
    #[builder(default="true")]
    unsynchronisation: bool,
    #[builder(default="false")]
    compression: bool,
}

impl Encoder {
    /// Encodes the specified tag using the settings set in the endoder.
    pub fn encode<W>(&self, tag: &Tag, mut writer: W) -> ::Result<()>
        where W: io::Write {
        // remove frames which have the flags indicating they should be removed
        let saved_frames = tag.frames()
            .filter(|frame| {
                !(frame.tag_alter_preservation()
                  || (frame.file_alter_preservation()
                      || DEFAULT_FILE_DISCARD.contains(&&frame.id())))
            });

        let mut flags = Flags::empty();
        flags.set(UNSYNCHRONISATION, self.unsynchronisation);
        if self.version == Version::Id3v22 {
            flags.set(COMPRESSION, self.compression);
        }

        let mut frame_data = Vec::new();
        for frame in saved_frames {
            frame::encode(&mut frame_data, frame, self.version, self.unsynchronisation)?;
        }
        writer.write_all(b"ID3")?;
        writer.write_all(&[self.version.minor() as u8, 2])?;
        writer.write_u8(flags.bits())?;
        writer.write_u32::<BigEndian>(unsynch::encode_u32(frame_data.len() as u32))?;
        writer.write_all(&frame_data[..])?;
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
        fs::File::open("testdata/id3v23.id3").unwrap()
            .read_to_end(&mut buf).unwrap();
        b.iter(|| {
            decode(&mut io::Cursor::new(buf.as_slice())).unwrap();
        });
    }

    #[bench]
    fn read_id3v24(b: &mut test::Bencher) {
        let mut buf = Vec::new();
        fs::File::open("testdata/id3v24.id3").unwrap()
            .read_to_end(&mut buf).unwrap();
        b.iter(|| {
            decode(&mut io::Cursor::new(buf.as_slice())).unwrap();
        });
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io;
    use ::frame::PictureType;

    fn make_tag() -> Tag {
        let mut tag = Tag::new();
        tag.set_title("Title");
        tag.set_artist("Artist");
        tag.set_genre("Genre");
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
        assert_eq!(PictureType::CoverFront, tag.pictures().nth(0).unwrap().picture_type);
    }

    #[test]
    fn read_id3v24() {
        let mut file = fs::File::open("testdata/id3v24.id3").unwrap();
        let tag = decode(&mut file).unwrap();
        assert_eq!("Title", tag.title().unwrap());
        assert_eq!(1, tag.disc().unwrap());
        assert_eq!(1, tag.total_discs().unwrap());
        assert_eq!(PictureType::CoverFront, tag.pictures().nth(0).unwrap().picture_type);
    }

    #[test]
    fn write_id3v22() {
        let tag = make_tag();
        let mut buffer = Vec::new();
        EncoderBuilder::default()
            .version(Version::Id3v22)
            .build()
            .unwrap()
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
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
            .encode(&tag, &mut buffer).unwrap();
        let tag_read = decode(&mut io::Cursor::new(buffer)).unwrap();
        assert_eq!(tag, tag_read);
    }
}
