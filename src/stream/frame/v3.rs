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


pub fn decode<R>(reader: &mut R, unsynchronization: bool) -> ::Result<Option<(usize, Frame)>>
    where R: io::Read {
    let mut frame_header = [0; 10];
    let nread = reader.read(&mut frame_header)?;
    if nread < frame_header.len() || frame_header[0] == 0x00 {
        return Ok(None);
    }
    let id = str::from_utf8(&frame_header[0..4]).unwrap(); // FIXME

    let mut frame = Frame::new(id);

    let content_size = BigEndian::read_u32(&frame_header[4..8]) as usize;
    let frameflags = BigEndian::read_u16(&frame_header[8..10]);
    frame.flags.tag_alter_preservation = frameflags & 0x8000 != 0;
    frame.flags.file_alter_preservation = frameflags & 0x4000 != 0;
    frame.flags.read_only = frameflags & 0x2000 != 0;
    frame.flags.compression = frameflags & 0x80 != 0;
    frame.flags.encryption = frameflags & 0x40 != 0;
    frame.flags.grouping_identity = frameflags & 0x20 != 0;
    frame.flags.unsynchronization = unsynchronization;

    if frame.flags.encryption {
        debug!("[{}] encryption is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "encryption is not supported"));
    } else if frame.flags.grouping_identity {
        debug!("[{}] grouping identity is not supported", frame.id());
        return Err(::Error::new(::ErrorKind::UnsupportedFeature, "grouping identity is not supported"));
    }

    let read_size = if frame.flags.compression {
        let _decompressed_size = reader.read_u32::<BigEndian>()?;
        content_size - 4
    } else {
        content_size
    };
    frame.content = super::decode_content(reader.take(read_size as u64), id, frame.flags)?;

    Ok(Some((10 + content_size as usize, frame)))
}

pub fn write(writer: &mut Write, frame: &Frame, unsynchronization: bool) -> ::Result<u32> {
    let mut content_bytes = frame::content_to_bytes(&frame, tag::Id3v23, Encoding::UTF16);
    let mut content_size = content_bytes.len() as u32;
    let decompressed_size = content_size;

    if frame.flags.compression {
        debug!("[{}] compressing frame content", frame.id());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::Default);
        try!(encoder.write_all(&content_bytes[..]));
        content_bytes = try!(encoder.finish());
        content_size = content_bytes.len() as u32 + 4;
    }

    try!(writer.write_all(frame.id().as_bytes()));
    try!(writer.write_u32::<BigEndian>(content_size));
    try!(writer.write_all(&frame.flags.to_bytes(0x3)[..]));
    if frame.flags.compression {
        try!(writer.write_u32::<BigEndian>(decompressed_size));
    }
    if unsynchronization {
        unsynch::encode_vec(&mut content_bytes);
    }
    try!(writer.write_all(&content_bytes[..]));

    Ok(10 + content_size)
}

