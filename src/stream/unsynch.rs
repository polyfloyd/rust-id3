//! The only purpose of unsynchronisation is to make the ID3v2 tag as compatible as possible with
//! existing software and hardware. There is no use in 'unsynchronising' tags if the file is only
//! to be processed only by ID3v2 aware software and hardware. Unsynchronisation is only useful
//! with tags in MPEG 1/2 layer I, II and III, MPEG 2.5 and AAC files.
use std::cmp;
use std::io;
use std::mem;


/// Returns the synchsafe variant of a `u32` value.
pub fn encode_u32(n: u32) -> u32 {
    let mut x: u32 = n & 0x7F | (n & 0xFFFFFF80) << 1;
    x = x & 0x7FFF | (x & 0xFFFF8000) << 1;
    x = x & 0x7FFFFF | (x & 0xFF800000) << 1;
    x
}

/// Returns the unsynchsafe varaiant of a `u32` value.
pub fn decode_u32(n: u32) -> u32 {
    n & 0xFF
        | (n & 0xFF00) >> 1
        | (n & 0xFF0000) >> 2
        | (n & 0xFF000000) >> 3
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
        if self.next >= self.available {
            self.available = self.reader.read(&mut self.buf)?;
            self.next = 0;
            if self.available == 0 {
                return Ok(0);
            }
        }
        if self.discard_next_null_byte && self.buf[self.next] == 0x00 {
            self.next += 1;
        }
        self.discard_next_null_byte = false;

        let max = cmp::min(self.available - self.next, buf.len());
        let mut copy_range = self.next..self.next;
        while copy_range.end - copy_range.start < max {
            copy_range.end += 1;
            if self.buf[copy_range.end - 1] == 0xff {
                self.discard_next_null_byte = true;
                break;
            }
        }

        let copy_num = copy_range.end - copy_range.start;
        buf[0..copy_num].copy_from_slice(&self.buf[copy_range]);
        self.next += copy_num;
        assert!(self.next <= self.available);
        Ok(copy_num)
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
    fn test_synchsafe() {
        assert_eq!(681570, encode_u32(176994));
        assert_eq!(176994, decode_u32(681570));
    }

    #[test]
    fn test_synchronization() {
        let mut v = vec![66, 0, 255, 0, 255, 0, 0, 255, 66];
        encode_vec(&mut v);
        assert_eq!(v, [66, 0, 255, 0, 0, 255, 0, 0, 0, 255, 66]);
        decode_vec(&mut v);
        assert_eq!(v, [66, 0, 255, 0, 255, 0, 0, 255, 66]);
    }
}
