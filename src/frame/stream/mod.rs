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
