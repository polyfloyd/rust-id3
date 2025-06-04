use super::{Storage, StorageFile};
use std::cmp::{self, Ordering};
use std::io::{self, Write};
use std::ops;

const COPY_BUF_SIZE: usize = 65536;

/// `PlainStorage` keeps track of a writeable region in a file and prevents accidental overwrites
/// of unrelated data. Any data following after the region is moved left and right as needed.
///
/// Padding is included from the reader.
#[derive(Debug)]
pub struct PlainStorage<F: StorageFile> {
    /// The backing storage.
    file: F,
    /// The region that may be writen to including any padding.
    region: ops::Range<u64>,
}

impl<F: StorageFile> PlainStorage<F> {
    /// Creates a new storage.
    pub fn new(file: F, region: ops::Range<u64>) -> PlainStorage<F> {
        PlainStorage { file, region }
    }
}

impl<'a, F: StorageFile + 'a> Storage<'a> for PlainStorage<F> {
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

pub struct PlainReader<'a, F: StorageFile + 'a> {
    storage: &'a mut PlainStorage<F>,
}

impl<F: StorageFile> io::Read for PlainReader<'_, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let cur_pos = self.storage.file.stream_position()?;
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

impl<F: StorageFile> io::Seek for PlainReader<'_, F> {
    fn seek(&mut self, rel_pos: io::SeekFrom) -> io::Result<u64> {
        let abs_cur_pos = self.storage.file.stream_position()?;
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

pub struct PlainWriter<'a, F: StorageFile + 'a> {
    storage: &'a mut PlainStorage<F>,
    /// Data is writen to this buffer before it is committed to the underlying storage.
    buffer: io::Cursor<Vec<u8>>,
    /// A flag indicating that the buffer has been written to.
    buffer_changed: bool,
}

impl<F: StorageFile> io::Write for PlainWriter<'_, F> {
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

        match buf_len.cmp(&range_len(&self.storage.region)) {
            Ordering::Greater => {
                // The region is not able to store the contents of the buffer. Grow it by moving the
                // following data to the end.
                let old_file_end = self.storage.file.seek(io::SeekFrom::End(0))?;
                let new_file_end = old_file_end + (buf_len - range_len(&self.storage.region));
                let old_region_end = self.storage.region.end;
                let new_region_end = self.storage.region.start + buf_len;

                self.storage.file.set_len(new_file_end)?;
                let mut rwbuf = [0; COPY_BUF_SIZE];
                let rwbuf_len = rwbuf.len();
                for i in 1.. {
                    let raw_from = old_file_end as i64 - i as i64 * rwbuf.len() as i64;
                    let raw_to = new_file_end.saturating_sub(i * rwbuf.len() as u64);
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
            }
            Ordering::Less => {
                // Shrink the file by moving the following data closer to the start.
                let old_file_end = self.storage.file.seek(io::SeekFrom::End(0))?;
                let old_region_end = self.storage.region.end;
                let new_region_end = self.storage.region.start + buf_len;
                let new_file_end =
                    self.storage.region.start + buf_len + (old_file_end - old_region_end);

                let mut rwbuf = [0; COPY_BUF_SIZE];
                let rwbuf_len = rwbuf.len();
                for i in 0.. {
                    let from = old_region_end + i * rwbuf.len() as u64;
                    let to = new_region_end + i * rwbuf.len() as u64;
                    assert!(from > to);

                    if from >= old_file_end {
                        break;
                    }
                    let part = cmp::min(rwbuf_len as i64, (old_file_end - from) as i64);
                    let rwbuf_part = &mut rwbuf[..part as usize];
                    self.storage.file.seek(io::SeekFrom::Start(from))?;
                    self.storage.file.read_exact(rwbuf_part)?;
                    self.storage.file.seek(io::SeekFrom::Start(to))?;
                    self.storage.file.write_all(rwbuf_part)?;
                }

                self.storage.file.set_len(new_file_end)?;
                self.storage.region.end = new_region_end;
            }
            Ordering::Equal => {}
        }

        assert!(buf_len <= range_len(&self.storage.region));
        // Okay, it's safe to commit our buffer to disk now.
        self.storage
            .file
            .seek(io::SeekFrom::Start(self.storage.region.start))?;
        self.storage.file.write_all(&self.buffer.get_ref()[..])?;
        self.storage.file.flush()?;
        self.buffer_changed = false;
        Ok(())
    }
}

impl<F: StorageFile> io::Seek for PlainWriter<'_, F> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.buffer.seek(pos)
    }
}

impl<F: StorageFile> Drop for PlainWriter<'_, F> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
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
        let mut store = PlainStorage::new(io::Cursor::new(buf), 32..64);
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
        let mut store = PlainStorage::new(io::Cursor::new(buf), 64..64);
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
        let mut store = PlainStorage::new(io::Cursor::new(buf), 2_000..22_000);
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 40_000]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(2_000..42_000, store.region);
        assert_eq!(60_000, store.file.get_ref().len());
        assert!(buf_reference[..2_000] == store.file.get_ref()[..store.region.start as usize]);
        assert!(buf_reference[22_000..] == store.file.get_ref()[store.region.end as usize..]);
        assert_eq!(40_000, store.reader().unwrap().bytes().count());
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
        let mut store = PlainStorage::new(io::Cursor::new(buf), 32..96);
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
        let mut store = PlainStorage::new(io::Cursor::new(buf), 2_000..22_000);
        {
            let mut w = store.writer().unwrap();
            w.write_all(&[0xff; 9_000]).unwrap();
            w.flush().unwrap();
        }
        assert_eq!(2_000..11_000, store.region);
        assert_eq!(29_000, store.file.get_ref().len());
        assert!(buf_reference[22_000..] == store.file.get_ref()[store.region.end as usize..]);
        assert_eq!(9_000, store.reader().unwrap().bytes().count());
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
}
