rust-id3
========
[![Build Status](https://github.com/polyfloyd/rust-id3/workflows/CI/badge.svg)](https://github.com/polyfloyd/rust-id3/actions)
[![Crate](https://img.shields.io/crates/v/id3.svg)](https://crates.io/crates/id3)
[![Documentation](https://docs.rs/id3/badge.svg)](https://docs.rs/id3/)

A library for reading and writing ID3 metadata.


## Implemented Features
* ID3v1 reading
* ID3v2.2, ID3v2.3, ID3v2.4 reading/writing
* MP3, WAV and AIFF files
* Latin1, UTF16 and UTF8 encodings
* Text frames
* Extended Text frames
* Link frames
* Extended Link frames
* Comment frames
* Lyrics frames
* Synchronised Lyrics frames
* Picture frames
* Encapsulated Object frames
* Chapter frames
* Unsynchronisation
* Compression
* Tag and File Alter Preservation bits


## Examples

### Reading tag frames
```rust
use id3::{Tag, TagLike};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tag = Tag::read_from_path("testdata/id3v24.id3")?;

    // Get a bunch of frames...
    if let Some(artist) = tag.artist() {
        println!("artist: {}", artist);
    }
    if let Some(title) = tag.title() {
        println!("title: {}", title);
    }
    if let Some(album) = tag.album() {
        println!("album: {}", album);
    }

    // Get frames before getting their content for more complex tags.
    if let Some(artist) = tag.get("TPE1").and_then(|frame| frame.content().text()) {
        println!("artist: {}", artist);
    }
    Ok(())
}
```

### Modifying any existing tag.
```rust
use id3::{Error, ErrorKind, Tag, TagLike, Version};
use std::fs::copy;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    copy("testdata/quiet.mp3", "/tmp/music.mp3")?;

    let mut tag = match Tag::read_from_path("/tmp/music.mp3") {
        Ok(tag) => tag,
        Err(Error{kind: ErrorKind::NoTag, ..}) => Tag::new(),
        Err(err) => return Err(Box::new(err)),
    };

    tag.set_album("Fancy Album Title");

    tag.write_to_path("/tmp/music.mp3", Version::Id3v24)?;
    Ok(())
}
```

### Creating a new tag, overwriting any old tag.
```rust
use id3::{Tag, TagLike, Frame, Version};
use id3::frame::Content;
use std::fs::copy;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    copy("testdata/quiet.mp3", "/tmp/music.mp3")?;

    let mut tag = Tag::new();
    tag.set_album("Fancy Album Title");

    // Set the album the hard way.
    tag.add_frame(Frame::with_content("TALB", Content::Text("album".to_string())));

    tag.write_to_path("/tmp/music.mp3", Version::Id3v24)?;
    Ok(())
}
```
