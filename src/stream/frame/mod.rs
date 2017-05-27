use std::io;
use flate2::read::ZlibDecoder;
use ::frame::Content;
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

pub fn decode_content<R>(reader: R, id: &str, compression: bool, unsynchronisation: bool) -> ::Result<Content>
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

    if unsynchronisation {
        decode_maybe_compressed(unsynch::Reader::new(reader), id, compression)
    } else {
        decode_maybe_compressed(reader, id, compression)
    }
}


pub fn encode<W>(writer: &mut W, frame: &Frame, version: tag::Version, unsynchronization: bool) -> ::Result<u32>
    where W: io::Write {
    match version {
        tag::Id3v22 => v2::write(writer, frame, unsynchronization),
        tag::Id3v23 => {
            let mut flags = v3::Flags::empty();
            flags.set(v3::TAG_ALTER_PRESERVATION, frame.tag_alter_preservation());
            flags.set(v3::FILE_ALTER_PRESERVATION, frame.file_alter_preservation());
            v3::write(writer, frame, v3::Flags::empty(), unsynchronization)
        },
        tag::Id3v24 => {
            let mut flags = v4::Flags::empty();
            flags.set(v4::UNSYNCHRONISATION, unsynchronization);
            flags.set(v4::TAG_ALTER_PRESERVATION, frame.tag_alter_preservation());
            flags.set(v4::FILE_ALTER_PRESERVATION, frame.file_alter_preservation());
            v4::write(writer, frame, flags)
        },
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
    use frame::Frame;
    use ::stream::encoding::Encoding;
    use ::stream::unsynch;

    fn u32_to_bytes(n: u32) -> Vec<u8> {
        vec!(((n & 0xFF000000) >> 24) as u8,
             ((n & 0xFF0000) >> 16) as u8,
             ((n & 0xFF00) >> 8) as u8,
             (n & 0xFF) as u8
            )
    }

    #[test]
    fn test_to_bytes_v2() {
        let id = "TAL";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(::util::string_to_utf16(text).into_iter());

        let content = decode_content(&data[..], id, false, false).unwrap();
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
        data.extend(::util::string_to_utf16(text).into_iter());

        let content = decode_content(&data[..], id, false, false).unwrap();
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

        let content = decode_content(&data[..], id, false, false).unwrap();
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
