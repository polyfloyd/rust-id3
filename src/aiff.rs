use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;
use crate::{Error, ErrorKind};
use crate::{Tag, Version};

pub fn load_aiff_id3(path: impl AsRef<Path>) -> crate::Result<Tag> {
    let mut file = File::open(path)?;
    loop {
        // Read chunk ID
        let mut chunk_id: [u8; 4] = [0; 4];
        // EOF
        if file.read(&mut chunk_id)? == 0 {
            break;
        }

        // Read chunk size
        let mut chunk_size_raw: [u8; 4] = [0; 4];
        file.read(&mut chunk_size_raw)?;
        let chunk_size = u32::from_be_bytes(chunk_size_raw);

        // Skip FORM chunk type, get its chunks
        if &chunk_id == b"FORM" {
            file.seek(SeekFrom::Current(4))?;
            continue;
        }

        if &chunk_id[0..3] == b"ID3" {
            return Ok(Tag::read_from(file.take(chunk_size as u64))?);
        }

        file.seek(SeekFrom::Current(chunk_size as i64))?;
    }

    Err(Error::new(ErrorKind::NoTag, "No tag chunk found!"))
}

// Wrapper to delete temp file
pub fn overwrite_aiff_id3(path: impl AsRef<Path>, tag: &Tag, version: Version) -> crate::Result<()> {
    let res = overwrite_aiff_id3_raw(&path, tag, version);
    if res.is_err() {
        let new_path = format!("{}.ID3TMP", path.as_ref().to_str().unwrap());
        // Ignore error as the file might be missing / not important.
        fs::remove_file(new_path).ok();
        return res;
    }

    Ok(())
}

fn overwrite_aiff_id3_raw(path: impl AsRef<Path>, tag: &Tag, version: Version) -> crate::Result<()> {
    let mut in_file = File::open(&path)?;
    let new_path = format!("{}.ID3TMP", &path.as_ref().to_str().unwrap());
    let mut out_file = File::create(&new_path)?;

    loop {
        // Read chunk ID
        let mut chunk_id: [u8; 4] = [0; 4];
        // EOF
        if in_file.read(&mut chunk_id)? < 4 {
            break;
        }
        out_file.write(&chunk_id)?;

        // Skip FORM chunk size & type
        if &chunk_id == b"FORM" {
            let mut buffer: [u8; 8] = [0; 8];
            in_file.read(&mut buffer)?;
            out_file.write(&buffer)?;
            continue;
        }

        // Read chunk size
        let mut chunk_size_raw: [u8; 4] = [0; 4];
        if in_file.read(&mut chunk_size_raw)? < 4 {
            break;
        }
        let chunk_size = u32::from_be_bytes(chunk_size_raw);

        // ID3 Chunk
        if &chunk_id[0..3] == b"ID3" {
            // Get ID3 bytes
            let mut id3_buffer = vec![];
            tag.write_to(&mut id3_buffer, version)?;

            let mut buffer = vec![];
            // Size
            buffer.extend(&(id3_buffer.len() as i32).to_be_bytes());
            // ID3 Data
            buffer.extend(id3_buffer);
            // Write
            out_file.write(&buffer)?;

            // Seek main file
            in_file.seek(SeekFrom::Current(chunk_size as i64))?;
            continue;
        }

        // Pass thru
        let mut buffer = vec![0; chunk_size as usize];
        in_file.read(&mut buffer)?;
        out_file.write(&chunk_size_raw)?;
        out_file.write(&buffer)?;
    }
    
    fs::remove_file(&path)?;
    fs::rename(&new_path, &path)?;

    Ok(())
}