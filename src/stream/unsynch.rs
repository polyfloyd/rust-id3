//! The only purpose of unsynchronisation is to make the ID3v2 tag as compatible as possible with
//! existing software and hardware. There is no use in 'unsynchronising' tags if the file is only
//! to be processed only by ID3v2 aware software and hardware. Unsynchronisation is only useful
//! with tags in MPEG 1/2 layer I, II and III, MPEG 2.5 and AAC files.
use std::io;
use std::mem;


/// Returns the synchsafe variant of a `u32` value.
pub fn encode_u32(n: u32) -> u32 {
    let mut x: u32 = n & 0x7F | (n & 0xFFFF_FF80) << 1;
    x = x & 0x7FFF | (x & 0xFFFF_8000) << 1;
    x = x & 0x7F_FFFF | (x & 0xFF80_0000) << 1;
    x
}

/// Returns the unsynchsafe varaiant of a `u32` value.
pub fn decode_u32(n: u32) -> u32 {
    n & 0xFF
        | (n & 0xFF00) >> 1
        | (n & 0xFF_0000) >> 2
        | (n & 0xFF00_0000) >> 3
}

/// Decoder for an unsynchronized stream of bytes.
///
/// The decoder has an internal buffer.
pub struct Reader<R>
    where R: io::Read {
    reader: R,
    buf: [u8; 8192],
    next: usize,
    available: usize,
    discard_next_null_byte: bool,
}

impl<R> Reader<R>
    where R: io::Read {
    pub fn new(reader: R) -> Reader<R> {
        Reader {
            reader,
            buf: [0; 8192],
            next: 0,
            available: 0,
            discard_next_null_byte: false,
        }
    }
}

impl<R> io::Read for Reader<R>
    where R: io::Read {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut i = 0;

        while i < buf.len() {
            assert!(self.next <= self.available);
            if self.next == self.available {
                self.available = self.reader.read(&mut self.buf)?;
                self.next = 0;
                if self.available == 0 {
                    break;
                }
            }

            if self.discard_next_null_byte && self.buf[self.next] == 0x00 {
                self.discard_next_null_byte = false;
                self.next += 1;
                continue;
            }
            self.discard_next_null_byte = false;

            buf[i] = self.buf[self.next];
            i += 1;

            if self.buf[self.next] == 0xff {
                self.discard_next_null_byte = true;
            }
            self.next += 1;
        }

        Ok(i)
    }
}

/// Applies the unsynchronization scheme to a byte buffer.
pub fn encode_vec(buffer: &mut Vec<u8>) {
    let mut repeat_next_null_byte = false;
    let mut i = 0;
    while i < buffer.len() {
        if buffer[i] == 0x00 && repeat_next_null_byte {
            buffer.insert(i, 0);
            i += 1;
        }
        repeat_next_null_byte = buffer[i] == 0xFF;
        i += 1;
    }
}

/// Undoes the changes done to a byte buffer by the unsynchronization scheme.
pub fn decode_vec(buffer: &mut Vec<u8>) {
    let buf_len = buffer.len();
    let from_buf = mem::replace(buffer, Vec::with_capacity(buf_len));
    let mut reader = Reader::new(io::Cursor::new(from_buf));
    io::copy(&mut reader, buffer).unwrap();
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synchsafe() {
        assert_eq!(681_570, encode_u32(176_994));
        assert_eq!(176_994, decode_u32(681_570));
    }

    #[test]
    fn synchronization() {
        let mut v = vec![66, 0, 255, 0, 255, 0, 0, 255, 66];
        encode_vec(&mut v);
        assert_eq!(v, [66, 0, 255, 0, 0, 255, 0, 0, 0, 255, 66]);
        decode_vec(&mut v);
        assert_eq!(v, [66, 0, 255, 0, 255, 0, 0, 255, 66]);
    }

    #[test]
    fn synchronization_jpeg() {
        let orig = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x02, 0x00, 0x76];
        let mut recoded = orig.clone();
        encode_vec(&mut recoded);
        decode_vec(&mut recoded);
        assert_eq!(orig, recoded);
    }

    #[test]
    fn synchronization_large() {
        let mut orig = Vec::new();
        for i in 0..1_000_000 {
            orig.push(0xff);
            orig.push(i as u8);
        }

        let mut recoded = orig.clone();
        encode_vec(&mut recoded);
        decode_vec(&mut recoded);
        assert_eq!(orig, recoded);
    }
}
