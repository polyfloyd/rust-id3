use crate::frame::Content;
use crate::frame::Frame;
use crate::stream::unsynch;
use crate::tag;
use flate2::read::ZlibDecoder;
use std::io;

mod content;
pub mod v2;
mod v3;
mod v4;

pub fn decode(
    mut reader: impl io::Read,
    version: tag::Version,
    unsynchronization: bool,
) -> crate::Result<Option<(usize, Frame)>> {
    match version {
        tag::Id3v22 => unreachable!(), //We handled this already
        tag::Id3v23 => v3::decode(&mut reader, unsynchronization),
        tag::Id3v24 => v4::decode(&mut reader),
    }
}

fn decode_content(
    reader: impl io::Read,
    version: tag::Version,
    id: &str,
    compression: bool,
    unsynchronisation: bool,
) -> crate::Result<Content> {
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
    version: tag::Version,
    unsynchronization: bool,
) -> crate::Result<usize> {
    match version {
        tag::Id3v22 => v2::encode(writer, frame),
        tag::Id3v23 => {
            let mut flags = v3::Flags::empty();
            flags.set(
                v3::Flags::TAG_ALTER_PRESERVATION,
                frame.tag_alter_preservation(),
            );
            flags.set(
                v3::Flags::FILE_ALTER_PRESERVATION,
                frame.file_alter_preservation(),
            );
            v3::encode(writer, frame, flags, unsynchronization)
        }
        tag::Id3v24 => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Frame;
    use crate::stream::encoding::Encoding;
    use crate::stream::unsynch;
    use crate::util::string_to_utf16;

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
        data.extend(string_to_utf16(text).into_iter());

        let content = decode_content(&data[..], tag::Id3v22, id, false, false).unwrap();
        let frame = Frame::with_content(id, content);

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend((&u32_to_bytes(data.len() as u32)[1..]).iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        encode(&mut writer, &frame, tag::Id3v22, false).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v3() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(string_to_utf16(text).into_iter());

        let content = decode_content(&data[..], tag::Id3v23, id, false, false).unwrap();
        let frame = Frame::with_content(id, content);

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(data.len() as u32).into_iter());
        bytes.extend([0x00, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        encode(&mut writer, &frame, tag::Id3v23, false).unwrap();
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

        let content = decode_content(&data[..], tag::Id3v24, id, false, false).unwrap();
        let mut frame = Frame::with_content(id, content);
        frame.set_tag_alter_preservation(true);
        frame.set_file_alter_preservation(true);

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(unsynch::encode_u32(data.len() as u32)).into_iter());
        bytes.extend([0x60, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        encode(&mut writer, &frame, tag::Id3v24, false).unwrap();
        assert_eq!(writer, bytes);
    }
}
