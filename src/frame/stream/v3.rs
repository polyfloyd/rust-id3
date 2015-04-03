extern crate flate;
extern crate byteorder;

use self::byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};
use frame::stream::FrameStream;
use frame::Frame;

#[allow(dead_code)]
pub enum FrameV3 {}

impl FrameStream for FrameV3 {
    fn read(reader: &mut Read) -> ::Result<Option<(u32, Frame)>> {
        let id = id_or_padding!(reader, 4);
        let mut frame = Frame::new(id);
        debug!("reading {}", frame.id); 

        let content_size = try!(reader.read_u32::<BigEndian>());

        let frameflags = try!(reader.read_u16::<BigEndian>());
        frame.flags.tag_alter_preservation = frameflags & 0x8000 != 0;
        frame.flags.file_alter_preservation = frameflags & 0x4000 != 0;
        frame.flags.read_only = frameflags & 0x2000 != 0;
        frame.flags.compression = frameflags & 0x80 != 0;
        frame.flags.encryption = frameflags & 0x40 != 0;
        frame.flags.grouping_identity = frameflags & 0x20 != 0;

        if frame.flags.encryption {
            debug!("[{}] encryption is not supported", frame.id);
            return Err(::Error::new(::ErrorKind::UnsupportedFeature, "encryption is not supported"));
        } else if frame.flags.grouping_identity {
            debug!("[{}] grouping identity is not supported", frame.id);
            return Err(::Error::new(::ErrorKind::UnsupportedFeature, "grouping identity is not supported"));
        }

        let mut read_size = content_size;
        if frame.flags.compression {
            let _decompressed_size = try!(reader.read_u32::<BigEndian>());
            read_size -= 4;
        }
       
        let mut data = Vec::<u8>::with_capacity(read_size as usize);
        try!(reader.take(read_size as u64).read_to_end(&mut data));
        try!(frame.parse_data(&data));

        Ok(Some((10 + content_size, frame)))
    }

    fn write(writer: &mut Write, frame: &Frame) -> ::Result<u32> {
        let mut content_bytes = frame.content_to_bytes(3);
        let mut content_size = content_bytes.len() as u32;
        let decompressed_size = content_size;

        if frame.flags.compression {
            debug!("[{}] compressing frame content", frame.id);
            content_bytes = flate::deflate_bytes_zlib(&content_bytes[..])[..].to_vec();
            content_size = content_bytes.len() as u32 + 4;
        }

        try!(writer.write(frame.id[..4].as_bytes()));
        try!(writer.write_u32::<BigEndian>(content_size));
        try!(writer.write(&frame.flags.to_bytes(0x3)[..]));
        if frame.flags.compression {
            try!(writer.write_u32::<BigEndian>(decompressed_size));
        }
        try!(writer.write(&content_bytes[..]));

        Ok(10 + content_size)
    }
}

