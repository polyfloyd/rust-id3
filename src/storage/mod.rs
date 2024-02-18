//! Abstractions that expose a simple interface for reading and storing tags according to some
//! underlying file format.
//!
//! The need for this abstraction arises from the differences that audiofiles have when storing
//! metadata. For example, MP3 uses a header for ID3v2, a trailer for ID3v1 while WAV has a special
//! "RIFF-chunk" which stores an ID3 tag.

use std::fs;
use std::io;

pub mod plain;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Format {
    /// ID3 is typically written as a header that precedes any audio content. For MPEG files, it is
    /// always a header. However, other formats where it is possible for the tag to be located
    /// elsewhere may also have an ID3 header.
    Header,

    /// Aiff is a chunk-y format format like WAV. Here, the file is built up in chunks where every
    /// chunk is a size header and some binary data. Audio is a chunk, but an ID3 tag may also be
    /// written to a chunk.
    Aiff,

    /// Similar to Aiff.
    Wav,
}

impl Format {
    pub fn magic(probe: impl AsRef<[u8]>) -> Option<Self> {
        let probe = probe.as_ref();
        if probe.len() < 12 {
            return None;
        }
        match (&probe[..3], &probe[..4], &probe[8..12]) {
            (b"ID3", _, _) => Some(Format::Header),
            (_, b"FORM", _) => Some(Format::Aiff),
            (_, b"RIFF", b"WAVE") => Some(Format::Wav),
            _ => None,
        }
    }
}

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

/// This trait is the combination of the [`std::io`] stream traits with an additional method to resize the
/// file.
pub trait StorageFile: io::Read + io::Write + io::Seek + private::Sealed {
    /// Performs the resize. Assumes the same behaviour as [`std::fs::File::set_len`].
    fn set_len(&mut self, new_len: u64) -> io::Result<()>;
}

impl<'a, T: StorageFile> StorageFile for &'a mut T {
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
        self.get_mut().resize(new_len as usize, 0);
        Ok(())
    }
}

// https://rust-lang.github.io/api-guidelines/future-proofing.html#c-sealed
mod private {
    pub trait Sealed {}

    impl<'a, T: Sealed> Sealed for &'a mut T {}
    impl Sealed for std::fs::File {}
    impl Sealed for std::io::Cursor<Vec<u8>> {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::Path;

    fn probe(path: impl AsRef<Path>) -> [u8; 12] {
        let mut f = fs::File::open(path).unwrap();
        let mut b = [0u8; 12];
        f.read(&mut b[..]).unwrap();
        b
    }

    #[test]
    fn test_format_magic() {
        assert_eq!(
            Format::magic(probe("testdata/aiff/padding.aiff")),
            Some(Format::Aiff)
        );
        assert_eq!(
            Format::magic(probe("testdata/aiff/quiet.aiff")),
            Some(Format::Aiff)
        );
        assert_eq!(
            Format::magic(probe("testdata/wav/tagged-end.wav")),
            Some(Format::Wav)
        );
        assert_eq!(
            Format::magic(probe("testdata/wav/tagless.wav")),
            Some(Format::Wav)
        );
        assert_eq!(
            Format::magic(probe("testdata/id3v22.id3")),
            Some(Format::Header)
        );
        assert_eq!(Format::magic(probe("testdata/mpeg-header")), None);
    }
}
