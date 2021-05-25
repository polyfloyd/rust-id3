use crate::storage::{PlainStorage, Storage};
use crate::{Error, ErrorKind, Tag, Version};
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use std::convert::TryFrom;
use std::fmt;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufReader, Seek, SeekFrom};
use std::{convert::TryInto, io};

const TAG_LEN: u32 = 4; // Size of a tag.
const SIZE_LEN: u32 = 4; // Size of a 32 bits integer.
const CHUNK_HEADER_LEN: u32 = TAG_LEN + SIZE_LEN;

const ID3_TAG: ChunkTag = ChunkTag(*b"ID3 ");

/// Attempts to load a ID3 tag from the given chunk stream.
pub fn load_id3_chunk<F, R>(mut reader: R) -> crate::Result<Tag>
where
    F: ChunkFormat,
    R: io::Read + io::Seek,
{
    let root_chunk = ChunkHeader::read_root_chunk_header::<F, _>(&mut reader)?;

    // Prevent reading past the root chunk, as there may be non-standard trailing data.
    let eof = root_chunk
        .size
        .checked_sub(TAG_LEN) // We must disconsider the format tag that was already read.
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Invalid root chunk size"))?;

    let tag_chunk = ChunkHeader::find_id3::<F, _>(&mut reader, eof.into())?;
    let chunk_reader = reader.take(tag_chunk.size.into());
    Tag::read_from(chunk_reader)
}

/// Writes a tag to the given file. If the file contains no previous tag data, a new ID3
/// chunk is created. Otherwise, the tag is overwritten in place.
pub fn write_id3_chunk<F: ChunkFormat>(
    mut file: File,
    tag: &Tag,
    version: Version,
) -> crate::Result<()>
where
    F: ChunkFormat,
{
    // Locate relevant chunks:
    let (mut root_chunk, id3_chunk_option) = locate_relevant_chunks::<F, _>(&file)?;

    let root_chunk_pos = SeekFrom::Start(0);
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
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Invalid ID3 chunk size"))?;

            id3_chunk_pos = SeekFrom::Start(
                id3_tag_pos
                    .checked_sub(CHUNK_HEADER_LEN.into())
                    .expect("failed to calculate id3 chunk position"),
            );

            storage = PlainStorage::new(&mut file, id3_tag_pos..id3_tag_end_pos);
            writer = storage.writer()?;

            // As we'll overwrite the existing tag, we must subtract it's size and sum the
            // new size later.
            root_chunk.size = root_chunk
                .size
                .checked_sub(chunk.size)
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Invalid root chunk size"))?;

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

            chunk.write_to::<F, _>(&mut writer)?;

            // Update the riff chunk size:
            root_chunk.size = root_chunk
                .size
                .checked_add(CHUNK_HEADER_LEN)
                .ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "root chunk max size reached")
                })?;

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
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "ID3 chunk max size reached"))?;
        }

        // We must flush manually to prevent silencing write errors.
        writer.flush()?;
    }

    // Update chunk sizes in the file:

    file.seek(id3_chunk_pos)?;
    id3_chunk.write_to::<F, _>(&file)?;

    root_chunk.size = root_chunk
        .size
        .checked_add(id3_chunk.size)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "root chunk max size reached"))?;

    file.seek(root_chunk_pos)?;
    root_chunk.write_to::<F, _>(&file)?;

    Ok(())
}

/// Locates the root and ID3 chunks, returning their headers. The ID3 chunk may not be
/// present. Returns a pair of (root chunk header, ID3 header).
fn locate_relevant_chunks<F, R>(mut input: R) -> crate::Result<(ChunkHeader, Option<ChunkHeader>)>
where
    F: ChunkFormat,
    R: Read + Seek,
{
    let mut reader = BufReader::new(&mut input);

    let root_chunk = ChunkHeader::read_root_chunk_header::<F, _>(&mut reader)?;

    // Prevent reading past the root chunk, as there may be non-standard trailing data.
    let eof = root_chunk
        .size
        .checked_sub(TAG_LEN) // We must disconsider the WAVE tag that was already read.
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Invalid root chunk size"))?;

    let id3_chunk = match ChunkHeader::find_id3::<F, _>(&mut reader, eof.into()) {
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

    Ok((root_chunk, id3_chunk))
}

#[derive(Debug, Clone, Copy, Eq)]
pub struct ChunkTag(pub [u8; TAG_LEN as usize]);

/// Equality for chunk tags is case insensitive.
impl PartialEq for ChunkTag {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl TryFrom<&[u8]> for ChunkTag {
    type Error = std::array::TryFromSliceError;

    fn try_from(tag: &[u8]) -> Result<Self, Self::Error> {
        let tag = tag.try_into()?;
        Ok(Self(tag))
    }
}

pub trait ChunkFormat {
    type Endianness: ByteOrder;
    const ROOT_TAG: ChunkTag;
    const ROOT_FORMAT: Option<ChunkTag>;
}

#[derive(Debug)]
pub struct AiffFormat;

impl ChunkFormat for AiffFormat {
    type Endianness = BigEndian;

    const ROOT_TAG: ChunkTag = ChunkTag(*b"FORM");
    // AIFF may have many formats, beign AIFF and AIFC the most common. Technically, it
    // can be anything, so we won't check those.
    const ROOT_FORMAT: Option<ChunkTag> = None;
}

#[derive(Debug)]
pub struct WavFormat;

impl ChunkFormat for WavFormat {
    type Endianness = LittleEndian;

    const ROOT_TAG: ChunkTag = ChunkTag(*b"RIFF");
    const ROOT_FORMAT: Option<ChunkTag> = Some(ChunkTag(*b"WAVE"));
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ChunkHeader {
    tag: ChunkTag,
    size: u32,
}

impl ChunkHeader {
    /// Reads a root chunk from the input stream. Such header is composed of:
    ///
    /// | Field   | Size | Type            |
    /// |---------+------+-----------------|
    /// | tag     |    4 | ChunkTag        |
    /// | size    |    4 | 32 bits integer |
    /// | format  |    4 | ChunkTag        |
    pub fn read_root_chunk_header<F, R>(mut reader: R) -> crate::Result<Self>
    where
        F: ChunkFormat,
        R: io::Read,
    {
        let invalid_header_error = Error::new(ErrorKind::InvalidInput, "invalid chunk header");

        const BUFFER_SIZE: usize = (CHUNK_HEADER_LEN + TAG_LEN) as usize;

        let mut buffer = [0; BUFFER_SIZE];

        // Use a single read call to improve performance on unbuffered readers.
        reader.read_exact(&mut buffer)?;

        let tag = buffer[0..4]
            .try_into()
            .expect("slice with incorrect length");

        let size = F::Endianness::read_u32(&buffer[4..8]);

        if tag != F::ROOT_TAG {
            return Err(invalid_header_error);
        }

        let chunk_format: ChunkTag = buffer[8..12]
            .try_into()
            .expect("slice with incorrect length");

        if let Some(format_tag) = F::ROOT_FORMAT {
            if chunk_format != format_tag {
                return Err(invalid_header_error);
            }
        }

        Ok(Self { tag, size })
    }

    /// Reads a chunk header from the input stream. A header is composed of:
    ///
    /// | Field | Size | Value           |
    /// |-------+------+-----------------|
    /// | tag   |    4 | chunk type      |
    /// | size  |    4 | 32 bits integer |
    pub fn read<F, R>(mut reader: R) -> io::Result<Self>
    where
        F: ChunkFormat,
        R: io::Read,
    {
        const BUFFER_SIZE: usize = CHUNK_HEADER_LEN as usize;

        let mut header = [0; BUFFER_SIZE];

        // Use a single read call to improve performance on unbuffered readers.
        reader.read_exact(&mut header)?;

        let tag = header[0..4]
            .try_into()
            .expect("slice with incorrect length");

        let size = F::Endianness::read_u32(&header[4..8]);

        Ok(Self { tag, size })
    }

    /// Finds an ID3 chunk in a flat sequence of chunks. This should be called after reading
    /// the root chunk.
    ///
    /// # Arguments
    ///
    /// * `reader` - The input stream. The reader must be positioned right after the root
    ///              chunk header.
    /// * `end` - The stream position where the chunk sequence ends. This is used to
    ///           prevent searching past the end.
    pub fn find_id3<F, R>(reader: R, end: u64) -> crate::Result<Self>
    where
        F: ChunkFormat,
        R: io::Read + io::Seek,
    {
        Self::find::<F, _>(&ID3_TAG, reader, end)?
            .ok_or_else(|| Error::new(ErrorKind::NoTag, "No tag chunk found!"))
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
    fn find<F, R>(tag: &ChunkTag, mut reader: R, end: u64) -> crate::Result<Option<Self>>
    where
        F: ChunkFormat,
        R: io::Read + io::Seek,
    {
        let mut pos = 0;

        while pos < end {
            let chunk = Self::read::<F, _>(&mut reader)?;

            if &chunk.tag == tag {
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
    pub fn write_to<F, W>(&self, mut writer: W) -> io::Result<()>
    where
        F: ChunkFormat,
        W: io::Write,
    {
        const BUFFER_SIZE: usize = CHUNK_HEADER_LEN as usize;

        let mut buffer = [0; BUFFER_SIZE];

        buffer[0..4].copy_from_slice(&self.tag.0);

        F::Endianness::write_u32(&mut buffer[4..8], self.size);

        // Use a single write call to improve performance on unbuffered writers.
        writer.write_all(&buffer)
    }
}

impl fmt::Debug for ChunkHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let tag = String::from_utf8_lossy(&self.tag.0);

        f.debug_struct(std::any::type_name::<Self>())
            .field("tag", &tag)
            .field("size", &self.size)
            .finish()
    }
}
