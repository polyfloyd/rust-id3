use byteorder::{ByteOrder, BigEndian};
use std::io::{Read, Write};
use frame::{Encoding,Frame};
use ::tag::{self, Version};
use ::unsynch;

pub fn read(reader: &mut Read, unsynchronization: bool) -> ::Result<Option<(usize, Frame)>> {
    let id = id_or_padding!(reader, 3);
    let mut frame = Frame::new(id);
    frame.flags.unsynchronization = unsynchronization;
    debug!("reading {}", frame.id());

    let mut sizebytes = [0u8; 3];
    reader.read(&mut sizebytes)?;
    let read_size = ((sizebytes[0] as u32) << 16) | ((sizebytes[1] as u32) << 8) | sizebytes[2] as u32;
    frame.content = super::decode_frame_content(reader.take(read_size as u64), frame.id(), frame.flags)?;

    Ok(Some((6 + read_size as usize, frame)))
}

pub fn write(writer: &mut Write, frame: &Frame, unsynchronization: bool) -> ::Result<u32> {
    let mut content_bytes = frame.content_to_bytes(tag::Id3v22, Encoding::UTF16);
    let content_size = content_bytes.len() as u32;

    let id = frame.id_for_version(Version::Id3v22)
        .ok_or(::Error::new(::ErrorKind::InvalidInput, "Unable to downgrade frame ID to ID3v2.2"))?;
    try!(writer.write_all(id.as_bytes()));
    let mut content_size_buf = [0u8; 4];
    BigEndian::write_u32(&mut content_size_buf, content_size);
    try!(writer.write_all(&content_size_buf[1..4]));
    if unsynchronization {
        unsynch::encode_vec(&mut content_bytes);
    }
    try!(writer.write_all(&content_bytes[..]));

    Ok(6 + content_size)
}
