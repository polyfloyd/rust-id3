extern crate byteorder;

use self::byteorder::{ByteOrder, BigEndian};
use std::io::{Read, Write};
use frame::Frame;
use ::tag;

pub fn read(reader: &mut Read, unsynchronization: bool) -> ::Result<Option<(u32, Frame)>> {
    let id = id_or_padding!(reader, 3);
    let mut frame = Frame::new(id);
    debug!("reading {}", frame.id); 

    let mut sizebytes = [0u8; 3];
    try!(reader.read(&mut sizebytes));
    let read_size = ((sizebytes[0] as u32) << 16) | ((sizebytes[1] as u32) << 8) | sizebytes[2] as u32;

    let mut data = Vec::<u8>::with_capacity(read_size as usize);
    try!(reader.take(read_size as u64).read_to_end(&mut data));
    if unsynchronization {
        ::util::resynchronize(&mut data);
    }
    try!(frame.parse_data(&data));

    Ok(Some((6 + read_size, frame)))
}

pub fn write(writer: &mut Write, frame: &Frame, unsynchronization: bool) -> ::Result<u32> {
    let mut content_bytes = frame.content_to_bytes(tag::Id3v22);
    let content_size = content_bytes.len() as u32;

    try!(writer.write_all(frame.id[..3].as_bytes()));
    let mut content_size_buf = [0u8; 4];
    BigEndian::write_u32(&mut content_size_buf, content_size);
    try!(writer.write_all(&content_size_buf[1..4]));
    if unsynchronization {
        ::util::unsynchronize(&mut content_bytes);
    }
    try!(writer.write_all(&content_bytes[..]));

    Ok(6 + content_size)
}
