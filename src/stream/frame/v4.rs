use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use frame::Frame;
use std::io::{self, Read, Write};
use std::str;
use stream::encoding::Encoding;
use stream::frame;
use stream::unsynch;
use tag;

bitflags! {
    pub struct Flags: u16 {
        const TAG_ALTER_PRESERVATION  = 0x4000;
        const FILE_ALTER_PRESERVATION = 0x2000;
        const READ_ONLY               = 0x1000;
        const GROUPING_IDENTITY       = 0x0040;
        const COMPRESSION             = 0x0008;
        const ENCRYPTION              = 0x0004;
        const UNSYNCHRONISATION       = 0x0002;
        const DATA_LENGTH_INDICATOR   = 0x0001;
    }
}

pub fn decode<R>(reader: &mut R) -> ::Result<Option<(usize, Frame)>>
where
    R: io::Read,
{
    let mut frame_header = [0; 10];
    let nread = reader.read(&mut frame_header)?;
    if nread < frame_header.len() || frame_header[0] == 0x00 {
        return Ok(None);
    }
    let id = str::from_utf8(&frame_header[0..4])?;
    let content_size = BigEndian::read_u32(&frame_header[4..8]) as usize;
    let flags = Flags::from_bits(BigEndian::read_u16(&frame_header[8..10]))
        .ok_or_else(|| ::Error::new(::ErrorKind::Parsing, "unknown frame header flags are set"))?;
    if flags.contains(Flags::ENCRYPTION) {
        return Err(::Error::new(
            ::ErrorKind::UnsupportedFeature,
            "encryption is not supported",
        ));
    } else if flags.contains(Flags::GROUPING_IDENTITY) {
        return Err(::Error::new(
            ::ErrorKind::UnsupportedFeature,
            "grouping identity is not supported",
        ));
    }

    let read_size = if flags.contains(Flags::DATA_LENGTH_INDICATOR) {
        let _decompressed_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?);
        content_size - 4
    } else {
        content_size
    };

    let content = super::decode_content(
        reader.take(read_size as u64),
        id,
        flags.contains(Flags::COMPRESSION),
        flags.contains(Flags::UNSYNCHRONISATION),
    )?;
    let frame = Frame::with_content(id, content);
    Ok(Some((10 + content_size, frame)))
}

pub fn encode(writer: &mut Write, frame: &Frame, flags: Flags) -> ::Result<usize> {
    let (mut content_buf, comp_hint_delta, decompressed_size) =
        if flags.contains(Flags::COMPRESSION) {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            let content_size =
                frame::content::encode(&mut encoder, frame.content(), tag::Id3v24, Encoding::UTF8)?;
            let content_buf = encoder.finish()?;
            let cd = if flags.contains(Flags::DATA_LENGTH_INDICATOR) {
                4
            } else {
                0
            };
            (content_buf, cd, Some(content_size))
        } else {
            let mut content_buf = Vec::new();
            frame::content::encode(
                &mut content_buf,
                frame.content(),
                tag::Id3v24,
                Encoding::UTF8,
            )?;
            (content_buf, 0, None)
        };

    writer.write_all(frame.id().as_bytes())?;
    writer.write_u32::<BigEndian>(unsynch::encode_u32(
        (content_buf.len() + comp_hint_delta) as u32,
    ))?;
    writer.write_u16::<BigEndian>(flags.bits())?;
    if let Some(s) = decompressed_size {
        if flags.contains(Flags::DATA_LENGTH_INDICATOR) {
            writer.write_u32::<BigEndian>(unsynch::encode_u32(s as u32))?;
        }
    }
    if flags.contains(Flags::UNSYNCHRONISATION) {
        unsynch::encode_vec(&mut content_buf);
    }
    writer.write_all(&content_buf)?;
    Ok(10 + comp_hint_delta + content_buf.len())
}
