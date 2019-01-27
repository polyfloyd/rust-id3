use crate::frame::Frame;
use crate::stream::encoding::Encoding;
use crate::stream::frame;
use crate::stream::unsynch;
use crate::tag::{self, Version};
use crate::{Error, ErrorKind};
use byteorder::{BigEndian, ByteOrder};
use std::io;
use std::str;

pub fn decode(
    mut reader: impl io::Read,
    unsynchronisation: bool,
) -> crate::Result<Option<(usize, Frame)>> {
    let mut frame_header = [0; 6];
    let nread = reader.read(&mut frame_header)?;
    if nread < frame_header.len() || frame_header[0] == 0x00 {
        return Ok(None);
    }
    let id = str::from_utf8(&frame_header[0..3])?;
    let read_size = {
        let sizebytes = &frame_header[3..6];
        let raw = (u32::from(sizebytes[0]) << 16)
            | (u32::from(sizebytes[1]) << 8)
            | u32::from(sizebytes[2]);
        unsynch::decode_u32(raw)
    };
    let content = super::decode_content(
        reader.take(u64::from(read_size)),
        id,
        false,
        unsynchronisation,
    )?;
    let frame = Frame::with_content(id, content);
    Ok(Some((6 + read_size as usize, frame)))
}

pub fn encode(
    mut writer: impl io::Write,
    frame: &Frame,
    unsynchronisation: bool,
) -> crate::Result<usize> {
    let mut content_buf = Vec::new();
    frame::content::encode(
        &mut content_buf,
        frame.content(),
        tag::Id3v22,
        Encoding::UTF16,
    )?;
    assert_ne!(0, content_buf.len());
    let id = frame.id_for_version(Version::Id3v22).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "Unable to downgrade frame ID to ID3v2.2",
        )
    })?;
    assert_eq!(3, id.len());
    writer.write_all(id.as_bytes())?;
    let mut size_buf = [0; 4];
    if unsynchronisation {
        unsynch::encode_vec(&mut content_buf);
    }
    BigEndian::write_u32(&mut size_buf, unsynch::encode_u32(content_buf.len() as u32));
    writer.write_all(&size_buf[1..4])?;
    writer.write_all(&content_buf)?;
    Ok(7 + content_buf.len())
}
