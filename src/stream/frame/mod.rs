use std::io;
use flate2::read::ZlibDecoder;
use ::parsers;
use ::frame::Content;
use ::frame::flags::Flags;
use ::stream::unsynch;
use ::tag;
use ::frame::Frame;


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
        let result = parsers::decode(id, &data[..])?;
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
