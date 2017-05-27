use std::io::{self, Read, Write};
use std::str;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use frame::Frame;
use ::tag;
use ::stream::encoding::Encoding;
use ::stream::frame;
use ::stream::unsynch;


bitflags! {
    pub struct Flags: u16 {
        const TAG_ALTER_PRESERVATION  = 0x8000;
        const FILE_ALTER_PRESERVATION = 0x4000;
        const READ_ONLY               = 0x2000;
        const COMPRESSION             = 0x0080;
        const ENCRYPTION              = 0x0040;
        const GROUPING_IDENTITY       = 0x0020;
    }
}


pub fn decode<R>(reader: &mut R, unsynchronisation: bool) -> ::Result<Option<(usize, Frame)>>
    where R: io::Read {
    let mut frame_header = [0; 10];
    let nread = reader.read(&mut frame_header)?;
    if nread < frame_header.len() || frame_header[0] == 0x00 {
        return Ok(None);
    }
    let id = str::from_utf8(&frame_header[0..4]).unwrap(); // FIXME

    let mut frame = Frame::new(id);

    let content_size = BigEndian::read_u32(&frame_header[4..8]) as usize;
    let flags = Flags::from_bits(BigEndian::read_u16(&frame_header[8..10]))
        .ok_or(::Error::new(::ErrorKind::Parsing, "unknown frame header flags are set"))?;
    if flags.contains(ENCRYPTION) {
        debug!("[{}] encryption is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "encryption is not supported"));
    } else if flags.contains(GROUPING_IDENTITY) {
        debug!("[{}] grouping identity is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "grouping identity is not supported"));
    }

    let read_size = if flags.contains(COMPRESSION) {
        let _decompressed_size = reader.read_u32::<BigEndian>()?;
        content_size - 4
    } else {
        content_size
    };
    frame.content = super::decode_content(reader.take(read_size as u64), id, flags.contains(COMPRESSION), unsynchronisation)?;

    Ok(Some((10 + content_size as usize, frame)))
}

pub fn write(writer: &mut Write, frame: &Frame, flags: Flags, unsynchronization: bool) -> ::Result<u32> {
    let mut content_bytes = frame::content_to_bytes(&frame, tag::Id3v23, Encoding::UTF16);
    let mut content_size = content_bytes.len() as u32;
    let decompressed_size = content_size;

    if flags.contains(COMPRESSION) {
        debug!("[{}] compressing frame content", frame.id());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::Default);
        try!(encoder.write_all(&content_bytes[..]));
        content_bytes = try!(encoder.finish());
        content_size = content_bytes.len() as u32 + 4;
    }

    try!(writer.write_all(frame.id().as_bytes()));
    try!(writer.write_u32::<BigEndian>(content_size));
    try!(writer.write_u16::<BigEndian>(flags.bits()));
    if flags.contains(COMPRESSION) {
        try!(writer.write_u32::<BigEndian>(decompressed_size));
    }
    if unsynchronization {
        unsynch::encode_vec(&mut content_bytes);
    }
    try!(writer.write_all(&content_bytes[..]));

    Ok(10 + content_size)
}

