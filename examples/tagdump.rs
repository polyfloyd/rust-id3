//! This is not actually an example that uses the `id3` crate, but helps dumping entire ID3 tags
//! from MP3 files to stdout. These files can then be used as testdata for this crate.

use std::env::args;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};

const CHUNK_SIZE: usize = 2048;

fn main() -> Result<(), Box<dyn Error>> {
    let Some(path) = args().nth(1) else {
        return Err("No path specified!".into());
    };

    let mut file = File::open(&path)?;

    let mut header = [0u8; 10];
    file.read_exact(&mut header)?;
    assert!(&header[..3] == b"ID3");
    let tag_size: u32 = u32::from(header[9])
        | u32::from(header[8]) << 7
        | u32::from(header[7]) << 14
        | u32::from(header[6]) << 21;
    eprintln!("Tag size: {tag_size}");
    let has_footer = (header[5] & 0x10) != 0;

    let mut bytes_left: usize = tag_size.try_into()?;
    if has_footer {
        // Footer is present, add 10 bytes to the size.
        eprintln!("Footer: yes");
        bytes_left += 10;
    } else {
        eprintln!("Footer: no");
    }

    eprintln!("Writing {bytes_left} bytes to stdout...");
    let mut stdout = io::stdout().lock();
    stdout.write_all(&header)?;

    let mut buffer: [u8; CHUNK_SIZE] = [0; CHUNK_SIZE];
    while bytes_left > 0 {
        let bytes_to_read = bytes_left.min(CHUNK_SIZE);
        file.read_exact(&mut buffer[..bytes_to_read])?;
        stdout.write_all(&buffer[..bytes_to_read])?;
        bytes_left -= bytes_to_read;
    }
    stdout.flush()?;
    eprintln!("Done.");

    Ok(())
}
