rust-id3
========
[![Build Status](https://github.com/polyfloyd/rust-id3/workflows/CI/badge.svg)](https://github.com/polyfloyd/rust-id3/actions)
[![Crate](https://img.shields.io/crates/v/id3.svg)](https://crates.io/crates/id3)
[![Documentation](https://docs.rs/id3/badge.svg)](https://docs.rs/id3/)

A library for reading and writing ID3 metadata.


## Examples

### Reading tag frames
```rust
use id3::{Tag, Version};

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

### Modifying an existing tag.
```rust
use id3::{Tag, Version};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tag = Tag::read_from_path("music.mp3")?;
    tag.set_album("Fancy Album Title");

    tag.write_to_path("music.mp3", Version::Id3v24)?;
    Ok(())
}
```

### Creating a new tag, overwriting any old tag.
```rust
use id3::{Tag, Frame, Version};
use id3::frame::Content;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tag = Tag::new();
    tag.set_album("Fancy Album Title");

    // Set the album the hard way.
    tag.add_frame(Frame::with_content("TALB", Content::Text("album".to_string())));

    tag.write_to_path("music.mp3", Version::Id3v24)?;
    Ok(())
}
```


## Supported ID3 Versions

  * ID3v1 reading
  * ID3v2.2 reading/writing
  * ID3v2.3 reading/writing
  * ID3v2.4 reading/writing

## Unsupported Features

  * Grouping identity
  * Encryption
