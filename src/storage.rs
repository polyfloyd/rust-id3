//! Abstractions that expose a simple interface for reading and storing tags according to some
//! underlying file format.
//!
//! The need for this abstraction arises from the differences that audiofiles have when storing
//! metadata. For example, MP3 uses a header for ID3v2, a trailer for ID3v1 while WAV has a special
//! "RIFF-chunk" which stores an ID3 tag.

use crate::stream::unsynch;
use crate::{Error, ErrorKind};
use byteorder::{BigEndian, ByteOrder};
use std::cmp;
use std::fs;
use std::io::{self, Write};
use std::ops;

/// Refer to the module documentation.
pub trait Storage<'a> {
    type Reader: io::Read + io::Seek + 'a;
    type Writer: io::Write + io::Seek + 'a;

    /// Opens the storage for reading.
    fn reader(&'a mut self) -> io::Result<Self::Reader>;
    /// Opens the storage for writing.
    ///
    /// The written data is comitted to persistent storage when the
    /// writer is dropped, altough this will ignore any errors. The caller must manually commit by
    /// using `io::Write::flush` to check for errors.
    fn writer(&'a mut self) -> io::Result<Self::Writer>;
}

/// `PlainStorage` keeps track of a writeable region in a file and prevents accidental overwrites
/// of unrelated data.
///
/// If the writeable region becomes too small when writing, the data following the region is moved.
/// When the write has been completed and data was moved, a zero padding is inserted to optimize
/// future writes.
///
/// Padding is included from the reader.
#[derive(Debug)]
pub struct PlainStorage<F>
where
    F: StorageFile,
{
    /// The backing storage.
    file: F,
    /// The region that may be writen to including any padding.
    region: ops::Range<u64>,
    /// Controls how many bytes of padding are need to be reserved when data is moved.
    preferred_padding: u32,
    /// When newly written data is smaller than the writeable region, it is possible to shrink the
    /// region by configuring the maximum amount of padding to retain before moving data to free up
    /// space.
    ///
    /// Resizes will leave `preferred_padding` amount of bytes of padding.
    /// Setting this to `None` will disable shrinkage.
    max_padding: Option<u32>,
}

pub trait StorageFile: io::Read + io::Write + io::Seek {
    fn set_len(&mut self, new_len: u64) -> io::Result<()>;
}

impl<'a, T> StorageFile for &'a mut T
where
    T: StorageFile,
{
    fn set_len(&mut self, new_len: u64) -> io::Result<()> {
        (*self).set_len(new_len)
    }
}

impl StorageFile for fs::File {
    fn set_len(&mut self, new_len: u64) -> io::Result<()> {
        fs::File::set_len(self, new_len)
    }
}

impl StorageFile for io::Cursor<Vec<u8>> {
    fn set_len(&mut self, new_len: u64) -> io::Result<()> {
        self.get_mut().resize(new_len as usize, 0xff);
        Ok(())
    }
}

impl<F> PlainStorage<F>
where
    F: StorageFile,
{
    /// Creates a new storage with a default padding of 2048 bytes and no shrinkage.
    pub fn new(file: F, region: ops::Range<u64>) -> PlainStorage<F> {
        PlainStorage::with_padding(file, region, 2048, None)
    }

    /// Creates a new storage with the specified amount of padding.
    ///
    /// # Panics
    /// If `max_padding` is set and smaller than `preferred_padding`.
    pub fn with_padding(
        file: F,
        region: ops::Range<u64>,
        preferred_padding: u32,
        max_padding: Option<u32>,
    ) -> PlainStorage<F> {
        if let Some(max) = max_padding {
            assert!(preferred_padding <= max);
        }
        PlainStorage {
            file,
            region,
            preferred_padding,
            max_padding,
        }
    }
}

impl<'a, F> Storage<'a> for PlainStorage<F>
where
    F: StorageFile + 'a,
{
    type Reader = PlainReader<'a, F>;
    type Writer = PlainWriter<'a, F>;

    fn reader(&'a mut self) -> io::Result<Self::Reader> {
        self.file.seek(io::SeekFrom::Start(self.region.start))?;
        Ok(PlainReader::<'a, F> { storage: self })
    }

    fn writer(&'a mut self) -> io::Result<Self::Writer> {
        self.file.seek(io::SeekFrom::Start(self.region.start))?;
        Ok(PlainWriter::<'a, F> {
            storage: self,
            buffer: io::Cursor::new(Vec::new()),
            buffer_changed: true,
        })
    }
}

pub struct PlainReader<'a, F>
where
    F: StorageFile + 'a,
{
    storage: &'a mut PlainStorage<F>,
}

impl<'a, F> io::Read for PlainReader<'a, F>
where
    F: StorageFile,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let cur_pos = self.storage.file.seek(io::SeekFrom::Current(0))?;
        assert!(self.storage.region.start <= cur_pos);
        if self.storage.region.end <= cur_pos {
            return Ok(0);
        }
        let buf_upper_bound = cmp::min(
            buf.len(),
            cmp::max(self.storage.region.end - cur_pos, 0) as usize,
        );
        self.storage.file.read(&mut buf[0..buf_upper_bound])
    }
}

impl<'a, F> io::Seek for PlainReader<'a, F>
where
    F: StorageFile,
{
    fn seek(&mut self, rel_pos: io::SeekFrom) -> io::Result<u64> {
        let abs_cur_pos = self.storage.file.seek(io::SeekFrom::Current(0))?;
        let abs_pos = match rel_pos {
            io::SeekFrom::Start(i) => (self.storage.region.start + i) as i64,
            io::SeekFrom::End(i) => self.storage.region.end as i64 + i,
            io::SeekFrom::Current(i) => abs_cur_pos as i64 + i,
        };
        if abs_pos < self.storage.region.start as i64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "attempted to seek to before the start of the region",
            ));
        }
        let new_abs_pos = self
            .storage
            .file
            .seek(io::SeekFrom::Start(abs_pos as u64))?;
        Ok(new_abs_pos - self.storage.region.start)
    }
}

pub struct PlainWriter<'a, F>
where
    F: StorageFile + 'a,
{
    storage: &'a mut PlainStorage<F>,
    /// Data is writen to this buffer before it is committed to the underlying storage.
    buffer: io::Cursor<Vec<u8>>,
    /// A flag indicating that the buffer has been written to.
    buffer_changed: bool,
}

impl<'a, F> io::Write for PlainWriter<'a, F>
where
    F: StorageFile,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let nwritten = self.buffer.write(buf)?;
        self.buffer_changed = true;
        Ok(nwritten)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Check whether the buffer and file are out of sync.
        if !self.buffer_changed {
            return Ok(());
        }

        let buf_len = self.buffer.get_ref().len() as u64;
        fn range_len(r: &ops::Range<u64>) -> u64 {
            r.end - r.start
        }
        let pref_pad = u64::from(self.storage.preferred_padding);

        if buf_len > range_len(&self.storage.region) {
            // The region is not able to store the contents of the buffer. Grow it by moving the
            // following data to the end.
            let old_file_end = self.storage.file.seek(io::SeekFrom::End(0))?;
            let new_file_end =
                old_file_end + (buf_len - range_len(&self.storage.region)) + pref_pad;
            let old_region_end = self.storage.region.end;
            let new_region_end = self.storage.region.start + buf_len + pref_pad;

            self.storage.file.set_len(new_file_end)?;
            let mut rwbuf = [0; 8192];
            let rwbuf_len = rwbuf.len();
            for i in 1.. {
                let raw_from = old_file_end as i64 - i as i64 * rwbuf.len() as i64;
                let raw_to = new_file_end
                    .checked_sub(i * rwbuf.len() as u64)
                    .unwrap_or(0);
                let from = cmp::max(old_region_end as i64, raw_from) as u64;
                let to = cmp::max(new_region_end, raw_to);
                assert!(from < to);

                let diff = cmp::max(old_region_end as i64 - raw_from, 0) as usize;
                let rwbuf_part = &mut rwbuf[cmp::min(diff, rwbuf_len)..];
                self.storage.file.seek(io::SeekFrom::Start(from))?;
                self.storage.file.read_exact(rwbuf_part)?;
                self.storage.file.seek(io::SeekFrom::Start(to))?;
                self.storage.file.write_all(rwbuf_part)?;
                if rwbuf_part.len() < rwbuf_len {
                    break;
                }
            }

            self.storage.region.end = new_region_end;
        } else if let Some(max) = self.storage.max_padding {
            if (range_len(&self.storage.region) - buf_len) + pref_pad > u64::from(max) {
                // There is more padding than allowed by max_padding, shrink the file by moving the
                // following data closer to the start.
                let old_file_end = self.storage.file.seek(io::SeekFrom::End(0))?;
                let old_region_end = self.storage.region.end;
                let new_region_end = self.storage.region.start + buf_len + pref_pad;
                let new_file_end = old_file_end - (old_region_end - new_region_end);

                let mut rwbuf = [0; 1000];
                let rwbuf_len = rwbuf.len();
                for i in 0.. {
                    let from = old_region_end + i * rwbuf.len() as u64;
                    let to = new_region_end + i * rwbuf.len() as u64;
                    assert!(from > to);

                    let part = (to + rwbuf_len as u64)
                        .checked_sub(new_file_end)
                        .unwrap_or(0);
                    let rwbuf_part = &mut rwbuf[part as usize..];
                    self.storage.file.seek(io::SeekFrom::Start(from))?;
                    self.storage.file.read_exact(rwbuf_part)?;
                    self.storage.file.seek(io::SeekFrom::Start(to))?;
                    self.storage.file.write_all(rwbuf_part)?;
                    if rwbuf_part.len() < rwbuf_len {
                        break;
                    }
                }

                self.storage.file.set_len(new_file_end)?;
                self.storage.region.end = new_region_end;
            }
        }

        assert!(buf_len <= range_len(&self.storage.region));
        // Okay, it's safe to commit our buffer to disk now.
        self.storage
            .file
            .seek(io::SeekFrom::Start(self.storage.region.start))?;
        self.storage.file.write_all(&self.buffer.get_ref()[..])?;
        // Write padding to erase any old data.
        for _ in 0..range_len(&self.storage.region) - buf_len {
            self.storage.file.write_all(&[0x00])?;
        }
        self.storage.file.flush()?;
        self.buffer_changed = false;
        Ok(())
    }
}

impl<'a, F> io::Seek for PlainWriter<'a, F>
where
    F: StorageFile,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.buffer.seek(pos)
    }
}

impl<'a, F> Drop for PlainWriter<'a, F>
where
    F: StorageFile,
{
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

pub fn locate_id3v2<R>(mut reader: R) -> crate::Result<Option<ops::Range<u64>>>
where
    R: io::Read + io::Seek,
{
    let mut header = [0u8; 10];
    let nread = reader.read(&mut header)?;
    if nread < header.len() || &header[..3] != b"ID3" {
        return Ok(None);
    }
    match header[3] {
        2 | 3 | 4 => (),
        _ => {
            return Err(Error::new(
                ErrorKind::UnsupportedVersion(header[4], header[3]),
                "unsupported id3 tag version",
            ));
        }
    };

    let size = unsynch::decode_u32(BigEndian::read_u32(&header[6..10]));
    reader.seek(io::SeekFrom::Start(u64::from(size)))?;
    let num_padding = reader
        .bytes()
        .take_while(|rs| rs.as_ref().map(|b| *b == 0x00).unwrap_or(false))
        .count();
    Ok(Some(0..u64::from(size) + num_padding as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};
    use std::iter;

    #[test]
    fn plain_reader_range() {
        let buf: Vec<u8> = iter::repeat(0xff)
            .take(128)
            .chain(iter::repeat(0x00).take(128))
            .chain(iter::repeat(0xff).take(128))
            .collect();
        let mut store = PlainStorage::new(io::Cursor::new(buf), 128..256);
        assert_eq!(128, store.reader().unwrap().bytes().count());
        assert!(store.reader().unwrap().bytes().all(|b| b.unwrap() == 0x00));
    }

    #[test]
    fn plain_reader_seek() {
        let buf: Vec<u8> = (0..128).collect();
        let mut store = PlainStorage::new(io::Cursor::new(buf), 32..64);
        let mut r = store.reader().unwrap();
        let mut rbuf = [0; 4];
        assert_eq!(28, r.seek(io::SeekFrom::Start(28)).unwrap());
        assert_eq!(4, r.read(&mut rbuf).unwrap());
        assert_eq!(rbuf, [60, 61, 62, 63]);
        assert_eq!(0, r.read(&mut rbuf[..1]).unwrap());
        assert_eq!(28, r.seek(io::SeekFrom::End(-4)).unwrap());
        assert_eq!(4, r.read(&mut rbuf).unwrap());
        assert_eq!(0, r.read(&mut rbuf[..1]).unwrap());
        assert_eq!(48, r.seek(io::SeekFrom::Start(48)).unwrap());
        assert_eq!(0, r.read(&mut rbuf[..1]).unwrap());
        assert_eq!(28, r.seek(io::SeekFrom::Current(-20)).unwrap());
        assert_eq!(4, r.read(&mut rbuf).unwrap());
        assert_eq!(0, r.read(&mut rbuf[..1]).unwrap());
    }

    #[test]
    fn plain_write_to_padding() {
        let buf: Vec<u8> = (0..128).collect();
        let buf_reference = buf.clone();
        let mut store = PlainStorage::with_padding(io::Cursor::new(buf), 32..64, 32, Some(32));
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 32]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(32..64, store.region);
        assert_eq!(128, store.file.get_ref().len());
        assert_eq!(
            &buf_reference[0..32],
            &store.file.get_ref()[..store.region.start as usize]
        );
        assert_eq!(
            &buf_reference[64..128],
            &store.file.get_ref()[store.region.end as usize..]
        );
        assert_eq!(32, store.reader().unwrap().bytes().count());
        assert!(store
            .reader()
            .unwrap()
            .bytes()
            .take(32)
            .all(|b| b.unwrap() == 0xff));
        assert!(store
            .reader()
            .unwrap()
            .bytes()
            .skip(32)
            .all(|b| b.unwrap() == 0x00));
    }

    #[test]
    fn plain_writer_grow() {
        let buf: Vec<u8> = (0..128).collect();
        let buf_reference = buf.clone();
        let mut store = PlainStorage::with_padding(io::Cursor::new(buf), 64..64, 0, None);
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 64]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(64..128, store.region);
        assert_eq!(192, store.file.get_ref().len());
        assert_eq!(
            &buf_reference[0..64],
            &store.file.get_ref()[..store.region.start as usize]
        );
        assert_eq!(
            &buf_reference[64..128],
            &store.file.get_ref()[store.region.end as usize..]
        );
        assert_eq!(64, store.reader().unwrap().bytes().count());
        assert!(store.reader().unwrap().bytes().all(|b| b.unwrap() == 0xff));
    }

    #[test]
    fn plain_writer_grow_large() {
        let buf: Vec<u8> = (0..40_000).map(|i| (i & 0xff) as u8).collect();
        let buf_reference = buf.clone();
        let mut store = PlainStorage::with_padding(io::Cursor::new(buf), 2_000..22_000, 1000, None);
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 40_000]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(2_000..43_000, store.region);
        assert_eq!(61_000, store.file.get_ref().len());
        assert!(buf_reference[..2_000] == store.file.get_ref()[..store.region.start as usize]);
        assert!(buf_reference[22_000..] == store.file.get_ref()[store.region.end as usize..]);
        assert_eq!(41_000, store.reader().unwrap().bytes().count());
        assert!(store
            .reader()
            .unwrap()
            .bytes()
            .take(40_000)
            .all(|b| b.unwrap() == 0xff));
        assert!(store
            .reader()
            .unwrap()
            .bytes()
            .skip(40_000)
            .all(|b| b.unwrap() == 0x00));
    }

    #[test]
    fn plain_writer_shrink() {
        let buf: Vec<u8> = (0..128).collect();
        let mut store = PlainStorage::with_padding(io::Cursor::new(buf), 32..96, 0, Some(0));
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 32]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(32..64, store.region);
        assert_eq!(96, store.file.get_ref().len());
        assert_eq!(32, store.reader().unwrap().bytes().count());
        assert!(store.reader().unwrap().bytes().all(|b| b.unwrap() == 0xff));
    }

    #[test]
    fn plain_writer_shrink_large() {
        let buf: Vec<u8> = (0..40_000).map(|i| (i & 0xff) as u8).collect();
        let buf_reference = buf.clone();
        let mut store =
            PlainStorage::with_padding(io::Cursor::new(buf), 2_000..22_000, 1000, Some(1000));
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 9_000]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(2_000..12_000, store.region);
        assert_eq!(30_000, store.file.get_ref().len());
        assert!(buf_reference[22_000..] == store.file.get_ref()[store.region.end as usize..]);
        assert_eq!(10_000, store.reader().unwrap().bytes().count());
        assert!(store
            .reader()
            .unwrap()
            .bytes()
            .take(9_000)
            .all(|b| b.unwrap() == 0xff));
        assert!(store
            .reader()
            .unwrap()
            .bytes()
            .skip(9_000)
            .all(|b| b.unwrap() == 0x00));
    }

    #[test]
    fn test_locate_id3v2() {
        let file = fs::File::open("testdata/id3v24.id3").unwrap();
        let location = locate_id3v2(file).unwrap();
        assert!(location.is_some());
    }
}
