use crate::storage::{PlainStorage, Storage};
use crate::{Error, ErrorKind, Tag, Version};
use std::convert::TryFrom;
use std::fmt;
use std::fs;
use std::io::prelude::*;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;
use std::{convert::TryInto, io};

const RIFF_TAG: ChunkTag = ChunkTag(*b"RIFF");
const WAVE_TAG: ChunkTag = ChunkTag(*b"WAVE");
const ID3_TAG: ChunkTag = ChunkTag(*b"ID3 ");

const SIZE_LEN: u32 = 4; // Size of a 32 bits integer.
const CHUNK_HEADER_LEN: u32 = RIFF_TAG.len() + SIZE_LEN;

/// Attempts to load a ID3 tag from the given WAV stream.
pub fn load_wav_id3(mut reader: impl io::Read + io::Seek) -> crate::Result<Tag> {
    let riff_header = ChunkHeader::read_riff_header(&mut reader)?;

    // Prevent reading past the RIFF chunk, as there may be non-standard trailing data.
    let eof = riff_header
        .size
        .checked_sub(WAVE_TAG.len()) // We must disconsider the WAVE tag that was already read.
        .ok_or(Error::new(
            ErrorKind::InvalidInput,
            "Invalid RIFF chunk size",
        ))?;

    let tag_chunk = ChunkHeader::find_id3(&mut reader, eof.into())?;
    let chunk_reader = reader.take(tag_chunk.size.into());
    Tag::read_from(chunk_reader)
}

/// Writes a tag to the given file. If the file contains no previous tag data, a new ID3
/// chunk is created. Otherwise, the tag is overwritten in place.
pub fn write_wav_id3(path: impl AsRef<Path>, tag: &Tag, version: Version) -> crate::Result<()> {
    // Open the file:
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(false)
        .truncate(false)
        .open(path)?;

    // Locate relevant chunks:
    let (mut riff_chunk, id3_chunk_option) = locate_relevant_chunks(&file)?;

    let riff_chunk_pos = SeekFrom::Start(0);
    let id3_chunk_pos;
    let mut id3_chunk;

    // Prepare and write the chunk:
    // We must scope the writer to be able to seek back and update the chunk sizes later.
    {
        let mut storage;
        let mut writer;

        // If there is a ID3 chunk, use it. Otherwise, create one.
        id3_chunk = if let Some(chunk) = id3_chunk_option {
            let id3_tag_pos = file.stream_position()?;
            let id3_tag_end_pos = id3_tag_pos
                .checked_add(chunk.size.into())
                .ok_or(Error::new(
                    ErrorKind::InvalidInput,
                    "Invalid ID3 chunk size",
                ))?;

            id3_chunk_pos = SeekFrom::Start(
                id3_tag_pos
                    .checked_sub(CHUNK_HEADER_LEN.into())
                    .expect("failed to calculate id3 chunk position"),
            );

            storage = PlainStorage::new(&mut file, id3_tag_pos..id3_tag_end_pos);
            writer = storage.writer()?;

            // As we'll overwrite the existing tag, we must subtract it's size and sum the
            // new size later.
            riff_chunk.size = riff_chunk.size.checked_sub(chunk.size).ok_or(Error::new(
                ErrorKind::InvalidInput,
                "Invalid RIFF chunk size",
            ))?;

            chunk
        } else {
            let pos = file.stream_position()?;

            id3_chunk_pos = SeekFrom::Start(pos);

            storage = PlainStorage::new(&mut file, pos..pos);
            writer = storage.writer()?;

            // Create a new empty chunk at the end of the file:
            let chunk = ChunkHeader {
                tag: ID3_TAG,
                size: 0,
            };

            chunk.write(&mut writer)?;

            // Update the riff chunk size:
            riff_chunk.size = riff_chunk
                .size
                .checked_add(CHUNK_HEADER_LEN)
                .ok_or(Error::new(ErrorKind::InvalidInput, "RIFF max size reached"))?;

            chunk
        };

        // Write the tag:

        tag.write_to(&mut writer, version)?;

        id3_chunk.size = writer
            .stream_position()?
            .try_into()
            .expect("ID3 chunk max size reached");

        // Add padding if necessary.
        if id3_chunk.size % 2 == 1 {
            let padding = [0];
            writer.write_all(&padding)?;
            id3_chunk.size = id3_chunk
                .size
                .checked_add(padding.len() as u32)
                .ok_or(Error::new(
                    ErrorKind::InvalidInput,
                    "ID3 chunk max size reached",
                ))?;
        }

        // We must flush manually to prevent silecing write errors.
        writer.flush()?;
    }

    // Update chunk sizes in the file:

    file.seek(id3_chunk_pos)?;
    id3_chunk.write(&file)?;

    riff_chunk.size = riff_chunk
        .size
        .checked_add(id3_chunk.size)
        .ok_or(Error::new(ErrorKind::InvalidInput, "RIFF max size reached"))?;

    file.seek(riff_chunk_pos)?;
    riff_chunk.write(&file)?;

    Ok(())
}

/// Locates the RIFF and ID3 chunks, returning their headers. The ID3 chunk may not be
/// present. Returns a pair of (RIFF header, ID3 header).
fn locate_relevant_chunks(
    mut input: impl Read + Seek,
) -> crate::Result<(ChunkHeader, Option<ChunkHeader>)> {
    // We must scope this reader to prevent conflict with the following writer.
    let mut reader = BufReader::new(&mut input);

    let riff_chunk = ChunkHeader::read_riff_header(&mut reader)?;

    // Prevent reading past the RIFF chunk, as there may be non-standard trailing data.
    let eof = riff_chunk
        .size
        .checked_sub(WAVE_TAG.len()) // We must disconsider the WAVE tag that was already read.
        .ok_or(Error::new(
            ErrorKind::InvalidInput,
            "Invalid RIFF chunk size",
        ))?;

    let id3_chunk = match ChunkHeader::find_id3(&mut reader, eof.into()) {
        Ok(chunk) => Some(chunk),
        Err(Error {
            kind: ErrorKind::NoTag,
            ..
        }) => None,
        Err(error) => return Err(error),
    };

    // BufReader may read past the id3 chunk. To seek back to the chunk, we must first
    // drop the BufReader, and then seek.
    let pos = reader.stream_position()?;
    drop(reader);
    input.seek(SeekFrom::Start(pos))?;

    Ok((riff_chunk, id3_chunk))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkTag(pub [u8; 4]);

impl ChunkTag {
    /// Checks if two chunk tags match, case insensitive.
    pub fn is(&self, tag: &ChunkTag) -> bool {
        self.0.eq_ignore_ascii_case(&tag.0)
    }

    pub const fn len(&self) -> u32 {
        self.0.len() as u32
    }
}

impl TryFrom<&[u8]> for ChunkTag {
    type Error = std::array::TryFromSliceError;

    fn try_from(tag: &[u8]) -> Result<Self, Self::Error> {
        let tag = tag.try_into()?;
        Ok(Self(tag))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkHeader {
    tag: ChunkTag,
    size: u32,
}

impl ChunkHeader {
    /// Reads a RIFF header from the input stream. Such header is composed of:
    ///
    /// | Field    | Size | Value                         |
    /// |----------+------+-------------------------------|
    /// | RIFF tag |    4 | "RIFF"                        |
    /// | size     |    4 | 32 bits little endian integer |
    /// | WAVE tag |    4 | "WAVE"                        |
    pub fn read_riff_header<R>(mut reader: R) -> crate::Result<ChunkHeader>
    where
        R: io::Read,
    {
        const RIFF: ChunkTag = ChunkTag(*b"RIFF");
        const WAVE: ChunkTag = ChunkTag(*b"WAVE");

        let invalid_header_error = Error::new(ErrorKind::InvalidInput, "invalid WAV/RIFF header");

        const BUFFER_SIZE: usize = (CHUNK_HEADER_LEN + WAVE_TAG.len()) as usize;

        let mut buffer = [0; BUFFER_SIZE];

        // Use a single read call to improve performance on unbuffered readers.
        reader.read_exact(&mut buffer)?;

        let chunk_header: ChunkHeader = buffer[0..8]
            .try_into()
            .expect("slice with incorrect length");

        if !chunk_header.tag.is(&RIFF) {
            return Err(invalid_header_error);
        }

        let wave_tag: ChunkTag = buffer[8..12]
            .try_into()
            .expect("slice with incorrect length");

        if !wave_tag.is(&WAVE) {
            return Err(invalid_header_error);
        }

        Ok(chunk_header)
    }

    /// Reads a chunk header from the input stream. A header is composed of:
    ///
    /// | Field | Size | Value                         |
    /// |-------+------+-------------------------------|
    /// | tag   |    4 | chunk type                    |
    /// | size  |    4 | 32 bits little endian integer |
    pub fn read<R>(mut reader: R) -> io::Result<Self>
    where
        R: io::Read,
    {
        const BUFFER_SIZE: usize = CHUNK_HEADER_LEN as usize;

        let mut header = [0; BUFFER_SIZE];

        // Use a single read call to improve performance on unbuffered readers.
        reader.read_exact(&mut header)?;

        Ok(header.into())
    }

    /// Finds an ID3 chunk in a flat sequence of chunks. This should be called after reading
    /// the root RIFF chunk.
    ///
    /// # Arguments
    ///
    /// * `reader` - The input stream. The reader must be positioned right after the RIFF
    ///              chunk header.
    /// * `end` - The stream position where the chunk sequence ends. This is used to
    ///           prevent searching past the end.
    pub fn find_id3<R>(reader: R, end: u64) -> crate::Result<Self>
    where
        R: io::Read + io::Seek,
    {
        const ID3: ChunkTag = ChunkTag(*b"ID3 ");
        Self::find(&ID3, reader, end)?.ok_or(Error::new(ErrorKind::NoTag, "No tag chunk found!"))
    }

    /// Finds a chunk in a flat sequence of chunks. This won't search chunks recursively.
    ///
    /// # Arguments
    ///
    /// * `tag` - The chunk tag to search for.
    /// * `reader` - The input stream. The reader must be positioned at the start of a
    ///              sequence of chunks.
    /// * `end` - The stream position where the chunk sequence ends. This is used to
    ///           prevent searching past the end.
    fn find<R>(tag: &ChunkTag, mut reader: R, end: u64) -> crate::Result<Option<Self>>
    where
        R: io::Read + io::Seek,
    {
        let mut pos = 0;

        while pos < end {
            let chunk = Self::read(&mut reader)?;

            if chunk.tag.is(tag) {
                return Ok(Some(chunk));
            }

            // Skip the chunk's contents, and padding if any.
            let skip = chunk.size + (chunk.size % 2);

            pos = reader.seek(SeekFrom::Current(skip as i64))?;
        }

        Ok(None)
    }

    /// Writes a chunk header to the given stream. A header is composed of:
    ///
    /// | Field | Size | Value                         |
    /// |-------+------+-------------------------------|
    /// | tag   |    4 | chunk type                    |
    /// | size  |    4 | 32 bits little endian integer |
    pub fn write<W>(&self, mut writer: W) -> io::Result<()>
    where
        W: io::Write,
    {
        const BUFFER_SIZE: usize = CHUNK_HEADER_LEN as usize;

        let mut buffer = [0; BUFFER_SIZE];

        buffer[0..4].copy_from_slice(&self.tag.0);

        buffer[4..8].copy_from_slice(&self.size.to_le_bytes());

        // Use a single write call to improve performance on unbuffered writers.
        writer.write_all(&buffer)
    }
}

impl fmt::Debug for ChunkHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let tag = String::from_utf8_lossy(&self.tag.0);

        f.debug_struct("ChunkHeader")
            .field("tag", &tag)
            .field("size", &self.size)
            .finish()
    }
}

impl From<[u8; 8]> for ChunkHeader {
    fn from(buffer: [u8; 8]) -> Self {
        let tag: ChunkTag = buffer[0..4]
            .try_into()
            .expect("slice with incorrect length");

        let size = u32::from_le_bytes(
            buffer[4..8]
                .try_into()
                .expect("slice with incorrect length"),
        );

        Self { tag, size }
    }
}

impl TryFrom<&[u8]> for ChunkHeader {
    type Error = std::array::TryFromSliceError;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        let header: [u8; 8] = buffer.try_into()?;
        Ok(header.into())
    }
}
