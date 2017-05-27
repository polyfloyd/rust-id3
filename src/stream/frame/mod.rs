use std::io;
use flate2::read::ZlibDecoder;
use ::frame::Content;
use ::frame::flags::Flags;
use ::stream::unsynch;
use ::tag;
use ::frame::Frame;
use ::stream::encoding::Encoding;


macro_rules! id_or_padding {
    ($reader:ident, $n:expr) => {
        {
            let mut buf = [0u8; $n];
            try!($reader.read(&mut buf[..1]));
            if buf[0] == 0 { // padding
                return Ok(None);
            }
            try!($reader.read(&mut buf[1..]));
            try!(String::from_utf8(buf.to_vec()))
        }

    };
}

pub mod v2;
pub mod v3;
pub mod v4;
pub mod content;

pub fn decode<R>(reader: &mut R, version: tag::Version, unsynchronization: bool) -> ::Result<Option<(usize, Frame)>>
    where R: io::Read {
    match version {
        tag::Id3v22 => v2::decode(reader, unsynchronization),
        tag::Id3v23 => v3::decode(reader, unsynchronization),
        tag::Id3v24 => v4::decode(reader),
    }
}

pub fn decode_content<R>(reader: R, id: &str, flags: Flags) -> ::Result<Content>
    where R: io::Read {
    fn decode<RR>(mut reader: RR, id: &str) -> ::Result<Content>
        where RR: io::Read {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        let result = content::decode(id, &data[..])?;
        Ok(result.content)
    }

    fn decode_maybe_compressed<RR>(reader: RR, id: &str, compression: bool) -> ::Result<Content>
        where RR: io::Read {
        if compression {
            decode(ZlibDecoder::new(reader), id)
        } else {
            decode(reader, id)
        }
    }

    if flags.unsynchronization {
        decode_maybe_compressed(unsynch::Reader::new(reader), id, flags.compression)
    } else {
        decode_maybe_compressed(reader, id, flags.compression)
    }
}


/// Creates a vector representation of the content suitable for writing to an ID3 tag.
fn content_to_bytes(frame: &Frame, version: tag::Version, encoding: Encoding) -> Vec<u8> {
    let request = ::stream::frame::content::EncoderRequest { version: version, encoding: encoding, content: &frame.content };
    content::encode(request)
}


#[cfg(test)]
mod tests {
    use super::*;
    use frame::{Frame, Flags};
    use ::stream::encoding::Encoding;
    use ::stream::unsynch;

    fn u32_to_bytes(n: u32) -> Vec<u8> {
        vec!(((n & 0xFF000000) >> 24) as u8,
             ((n & 0xFF0000) >> 16) as u8,
             ((n & 0xFF00) >> 8) as u8,
             (n & 0xFF) as u8
            )
    }

    /// Parses the provided data and sets the `content` field. If the compression flag is set to
    /// true then decompression will be performed.
    ///
    /// Returns `Err` if the data is invalid for the frame type.
    fn parse_data(frame: &mut Frame, data: &[u8]) -> ::Result<()> {
        frame.content = ::stream::frame::decode_content(io::Cursor::new(data), frame.id(), frame.flags)?;
        Ok(())
    }

    #[test]
    fn test_frame_flags_to_bytes_v3() {
        let mut flags = Flags::new();
        assert_eq!(flags.to_bytes(0x3), vec!(0x0, 0x0));
        flags.tag_alter_preservation = true;
        flags.file_alter_preservation = true;
        flags.read_only = true;
        flags.compression = true;
        flags.encryption = true;
        flags.grouping_identity = true;
        assert_eq!(flags.to_bytes(0x3), vec!(0xE0, 0xE0));
    }

    #[test]
    fn test_frame_flags_to_bytes_v4() {
        let mut flags = Flags::new();
        assert_eq!(flags.to_bytes(0x4), vec!(0x0, 0x0));
        flags.tag_alter_preservation = true;
        flags.file_alter_preservation = true;
        flags.read_only = true;
        flags.grouping_identity = true;
        flags.compression = true;
        flags.encryption = true;
        flags.unsynchronization = true;
        flags.data_length_indicator = true;
        assert_eq!(flags.to_bytes(0x4), vec!(0x70, 0x4F));
    }

    #[test]
    fn test_to_bytes_v2() {
        let id = "TAL";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut frame = Frame::new(id);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(::util::string_to_utf16(text).into_iter());

        parse_data(&mut frame, &data[..]).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend((&u32_to_bytes(data.len() as u32)[1..]).iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        frame.write_to(&mut writer, tag::Id3v22, false).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v3() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut frame = Frame::new(id);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(::util::string_to_utf16(text).into_iter());

        parse_data(&mut frame, &data[..]).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(data.len() as u32).into_iter());
        bytes.extend([0x00, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        frame.write_to(&mut writer, tag::Id3v23, false).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v4() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF8;

        let mut frame = Frame::new(id);

        frame.flags.tag_alter_preservation = true;
        frame.flags.file_alter_preservation = true;

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(text.bytes());

        parse_data(&mut frame, &data[..]).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(unsynch::encode_u32(data.len() as u32)).into_iter());
        bytes.extend([0x60, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        frame.write_to(&mut writer, tag::Id3v24, false).unwrap();
        assert_eq!(writer, bytes);
    }
}
