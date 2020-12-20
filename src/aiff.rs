use safe_transmute::{transmute_one, transmute_one_to_bytes};
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::Cursor;
use crate::{Tag, Version};

pub fn load_aiff_id3(path: &str) -> Result<Option<Tag>, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    loop {
        //Read chunk ID
        let mut chunk_id_raw: [u8; 4] = [0; 4];
        //EOF
        if file.read(&mut chunk_id_raw)? == 0 {
            break;
        }
        let chunk_id = String::from_utf8_lossy(&chunk_id_raw);

        //Read chunk size
        let mut chunk_size_raw: [u8; 4] = [0; 4];
        file.read(&mut chunk_size_raw).ok();
        let chunk_size = transmute_one::<u32>(&chunk_size_raw).unwrap().to_be();

        //Skip FORM chunk type, get it's chunks
        if chunk_id == "FORM" {
            file.seek(SeekFrom::Current(4))?;
            continue;
        }

        //ID3 Chunk
        if &chunk_id[0..3] == "ID3" {
            let mut id3_data = vec![0; chunk_size as usize];
            file.read(&mut id3_data)?;
            return Ok(Some(Tag::read_from(Cursor::new(id3_data))?));
        }

        //Seek chunk
        file.seek(SeekFrom::Current(chunk_size as i64))?;
    }

    Ok(None)
}

//Wrapper to delete temp file
pub fn overwrite_aiff_id3(path: &str, tag: &Tag, version: Version) -> Result<(), Box<dyn std::error::Error>> {
    let res = _overwrite_aiff_id3(path, tag, version);
    if res.is_err() {
        let new_path = format!("{}.ID3TMP", path);
        fs::remove_file(new_path).ok();
        return res;
    }

    Ok(())
}

fn _overwrite_aiff_id3(path: &str, tag: &Tag, version: Version) -> Result<(), Box<dyn std::error::Error>> {
    let mut in_file = File::open(path)?;
    let new_path = format!("{}.ID3TMP", path);
    let mut out_file = File::create(&new_path)?;

    loop {
        //Read chunk ID
        let mut chunk_id_raw: [u8; 4] = [0; 4];
        //EOF
        if in_file.read(&mut chunk_id_raw)? < 4 {
            break;
        }
        out_file.write(&chunk_id_raw)?;
        let chunk_id = String::from_utf8_lossy(&chunk_id_raw);

        //Skip FORM chunk size & type
        if chunk_id == "FORM" {
            let mut buffer: [u8; 8] = [0; 8];
            in_file.read(&mut buffer)?;
            out_file.write(&buffer)?;
            continue;
        }

        //Read chunk size
        let mut chunk_size_raw: [u8; 4] = [0; 4];
        if in_file.read(&mut chunk_size_raw)? < 4 {
            break;
        }
        let chunk_size = transmute_one::<u32>(&chunk_size_raw).unwrap().to_be();

        //ID3 Chunk
        if &chunk_id[0..3] == "ID3" {
            //Get ID3 bytes
            let mut id3_buffer = vec![];
            tag.write_to(&mut id3_buffer, version)?;

            let mut buffer = vec![];
            //Size
            buffer.extend(transmute_one_to_bytes::<i32>(&(id3_buffer.len() as i32).to_be()));
            //ID3 Data
            buffer.extend(id3_buffer);
            //Write
            out_file.write(&buffer)?;

            //Seek main file
            in_file.seek(SeekFrom::Current(chunk_size as i64))?;
            continue;
        }

        //Pass thru
        let mut buffer = vec![0; chunk_size as usize];
        in_file.read(&mut buffer)?;
        out_file.write(&chunk_size_raw)?;
        out_file.write(&buffer)?;
    }
    
    fs::remove_file(path)?;
    fs::rename(&new_path, path)?;

    Ok(())
}