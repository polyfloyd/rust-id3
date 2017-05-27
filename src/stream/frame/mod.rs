use std::io;
use flate2::read::ZlibDecoder;
use ::frame::Content;
use ::frame::flags::Flags;
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

pub fn decode_content<R>(reader: R, id: &str, flags: Flags) -> ::Result<Content>
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

    if flags.unsynchronization {
        decode_maybe_compressed(unsynch::Reader::new(reader), id, flags.compression)
    } else {
        decode_maybe_compressed(reader, id, flags.compression)
    }
}


/// Creates a vector representation of the content suitable for writing to an ID3 tag.
fn content_to_bytes(frame: &Frame, version: tag::Version, encoding: Encoding) -> Vec<u8> {
    let request = ::stream::frame::content::EncoderRequest { version: version, encoding: encoding, content: &frame.content };
    content::encode(request)
}
