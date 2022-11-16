use crate::frame::Content;
use crate::frame::Frame;
use crate::stream::encoding::Encoding;
use crate::stream::unsynch;
use crate::tag::Version;
use flate2::read::ZlibDecoder;
use std::io;
use std::str;

pub mod content;
pub mod v2;
pub mod v3;
pub mod v4;

pub fn decode(reader: impl io::Read, version: Version) -> crate::Result<Option<(usize, Frame)>> {
    match version {
        Version::Id3v22 => unimplemented!(),
        Version::Id3v23 => v3::decode(reader),
        Version::Id3v24 => v4::decode(reader),
    }
}

fn decode_content(
    reader: impl io::Read,
    version: Version,
    id: &str,
    compression: bool,
    unsynchronisation: bool,
) -> crate::Result<(Content, Option<Encoding>)> {
    if unsynchronisation {
        let reader_unsynch = unsynch::Reader::new(reader);
        if compression {
            content::decode(id, version, ZlibDecoder::new(reader_unsynch))
        } else {
            content::decode(id, version, reader_unsynch)
        }
    } else if compression {
        content::decode(id, version, ZlibDecoder::new(reader))
    } else {
        content::decode(id, version, reader)
    }
}

pub fn encode(
    writer: impl io::Write,
    frame: &Frame,
    version: Version,
    unsynchronization: bool,
) -> crate::Result<usize> {
    match version {
        Version::Id3v22 => v2::encode(writer, frame),
        Version::Id3v23 => {
            let mut flags = v3::Flags::empty();
            flags.set(
                v3::Flags::TAG_ALTER_PRESERVATION,
                frame.tag_alter_preservation(),
            );
            flags.set(
                v3::Flags::FILE_ALTER_PRESERVATION,
                frame.file_alter_preservation(),
            );
            v3::encode(writer, frame, flags)
        }
        Version::Id3v24 => {
            let mut flags = v4::Flags::empty();
            flags.set(v4::Flags::UNSYNCHRONISATION, unsynchronization);
            flags.set(
                v4::Flags::TAG_ALTER_PRESERVATION,
                frame.tag_alter_preservation(),
            );
            flags.set(
                v4::Flags::FILE_ALTER_PRESERVATION,
                frame.file_alter_preservation(),
            );
            v4::encode(writer, frame, flags)
        }
    }
}

/// Helper for str::from_utf8 that preserves any problematic pattern if applicable.
pub fn str_from_utf8(b: &[u8]) -> crate::Result<&str> {
    str::from_utf8(b).map_err(|err| {
        let bad = b[err.valid_up_to()..].to_vec();
        crate::Error {
            kind: crate::ErrorKind::StringDecoding(bad.to_vec()),
            description: "data is not valid utf-8".to_string(),
            partial_tag: None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Frame;
    use crate::stream::encoding::Encoding;
    use crate::stream::unsynch;

    fn u32_to_bytes(n: u32) -> Vec<u8> {
        vec![
            ((n & 0xFF00_0000) >> 24) as u8,
            ((n & 0xFF_0000) >> 16) as u8,
            ((n & 0xFF00) >> 8) as u8,
            (n & 0xFF) as u8,
        ]
    }

    #[test]
    fn test_to_bytes_v2() {
        let id = "TAL";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(Encoding::UTF16.encode(text).into_iter());

        let content = decode_content(&data[..], Version::Id3v22, id, false, false)
            .unwrap()
            .0;
        let frame = Frame::with_content(id, content);

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend((u32_to_bytes(data.len() as u32)[1..]).iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        encode(&mut writer, &frame, Version::Id3v22, false).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v3() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(Encoding::UTF16.encode(text).into_iter());

        let content = decode_content(&data[..], Version::Id3v23, id, false, false)
            .unwrap()
            .0;
        let frame = Frame::with_content(id, content);

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(data.len() as u32).into_iter());
        bytes.extend([0x00, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        encode(&mut writer, &frame, Version::Id3v23, false).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v4() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF8;

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(text.bytes());

        let content = decode_content(&data[..], Version::Id3v24, id, false, false)
            .unwrap()
            .0;
        let mut frame = Frame::with_content(id, content);
        frame.set_tag_alter_preservation(true);
        frame.set_file_alter_preservation(true);

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(unsynch::encode_u32(data.len() as u32)).into_iter());
        bytes.extend([0x60, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        encode(&mut writer, &frame, Version::Id3v24, false).unwrap();
        assert_eq!(writer, bytes);
    }
}
