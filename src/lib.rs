//! A library to read and write ID3v2 tags. ID3 versions v2.2, v2.3, and v2.4 are supported.
//!
//! # Reading tag frames
//!
//! ```
//! use id3::{Tag, Version};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let tag = Tag::read_from_path("testdata/id3v24.id3")?;
//!
//!     // Get a bunch of frames...
//!     if let Some(artist) = tag.artist() {
//!         println!("artist: {}", artist);
//!     }
//!     if let Some(title) = tag.title() {
//!         println!("title: {}", title);
//!     }
//!     if let Some(album) = tag.album() {
//!         println!("album: {}", album);
//!     }
//!
//!     // Get frames before getting their content for more complex tags.
//!     if let Some(artist) = tag.get("TPE1").and_then(|frame| frame.content().text()) {
//!         println!("artist: {}", artist);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Modifying an existing tag.
//!
//! ```no_run
//! use id3::{Tag, Version};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut tag = Tag::read_from_path("music.mp3")?;
//!     tag.set_album("Fancy Album Title");
//!
//!     tag.write_to_path("music.mp3", Version::Id3v24)?;
//!     Ok(())
//! }
//! ```
//!
//! # Creating a new tag, overwriting any old tag.
//!
//! ```no_run
//! use id3::{Tag, Frame, Version};
//! use id3::frame::Content;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut tag = Tag::new();
//!     tag.set_album("Fancy Album Title");
//!
//!     // Set the album the hard way.
//!     tag.add_frame(Frame::with_content("TALB", Content::Text("album".to_string())));
//!
//!     tag.write_to_path("music.mp3", Version::Id3v24)?;
//!     Ok(())
//! }
//! ```
//!
//! # Resources
//!
//! * ID3v2.2 http://id3.org/id3v2-00
//! * ID3v2.3 http://id3.org/id3v2.3.0
//! * ID3v2.4 http://id3.org/id3v2.4.0-structure

#![warn(missing_docs)]

pub use crate::error::{Error, ErrorKind, Result};
pub use crate::frame::{Content, Frame, Timestamp};
pub use crate::stream::tag::Encoder;
pub use crate::tag::{Tag, Version};

/// Contains types and methods for operating on ID3 frames.
pub mod frame;
/// Utilities for working with ID3v1 tags.
pub mod v1;

mod chunk;
mod error;
mod storage;
mod stream;
mod tag;
mod util;
