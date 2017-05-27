use std::io::{Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use ::frame::Frame;
use ::tag;
use ::stream::encoding::Encoding;
use ::stream::frame;
use ::stream::unsynch;


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


pub fn decode(reader: &mut Read) -> ::Result<Option<(usize, Frame)>> {
    let id = id_or_padding!(reader, 4);
    let mut frame = Frame::new(id);
    debug!("reading {}", frame.id());

    let content_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?);

    let flags = Flags::from_bits(reader.read_u16::<BigEndian>()?)
        .ok_or(::Error::new(::ErrorKind::Parsing, "unknown frame header flags are set"))?;
    if flags.contains(ENCRYPTION) {
        debug!("[{}] encryption is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "encryption is not supported"));
    } else if flags.contains(GROUPING_IDENTITY) {
        debug!("[{}] grouping identity is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "grouping identity is not supported"));
    }

    let mut read_size = content_size;
    if flags.contains(DATA_LENGTH_INDICATOR) {
        let _decompressed_size = unsynch::decode_u32(reader.read_u32::<BigEndian>()?);
        read_size -= 4;
    }
    frame.content = super::decode_content(reader.take(read_size as u64), frame.id(), flags.contains(COMPRESSION), flags.contains(UNSYNCHRONISATION))?;

    Ok(Some((10 + content_size as usize, frame)))
}

pub fn write(writer: &mut Write, frame: &Frame, flags: Flags) -> ::Result<u32> {
    let mut content_bytes = frame::content_to_bytes(&frame, tag::Id3v24, Encoding::UTF8);
    let mut content_size = content_bytes.len() as u32;
    let decompressed_size = content_size;

    if flags.contains(COMPRESSION) {
        debug!("[{}] compressing frame content", frame.id());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::Default);
        try!(encoder.write_all(&content_bytes[..]));
        content_bytes = try!(encoder.finish());
        content_size = content_bytes.len() as u32;
    }

    if flags.contains(DATA_LENGTH_INDICATOR) {
        content_size += 4;
    }

    try!(writer.write_all(frame.id().as_bytes()));
    try!(writer.write_u32::<BigEndian>(unsynch::encode_u32(content_size)));;
    try!(writer.write_u16::<BigEndian>(flags.bits()));
    if flags.contains(DATA_LENGTH_INDICATOR) {
        debug!("[{}] adding data length indicator", frame.id());
        try!(writer.write_u32::<BigEndian>(unsynch::encode_u32(decompressed_size)));
    }
    if flags.contains(UNSYNCHRONISATION) {
        unsynch::encode_vec(&mut content_bytes);
    }
    try!(writer.write_all(&content_bytes[..]));

    Ok(10 + content_size)
}

