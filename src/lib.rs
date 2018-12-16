#![cfg_attr(all(test, feature = "unstable"), feature(test))]

//! A library to read and write ID3v2 tags. ID3 versions v2.2, v2.3, and v2.4 are supported.
//!
//! # Modifying an existing tag
//!
//! ```no_run
//! use id3::{Tag, Version};
//!
//! let mut tag = Tag::read_from_path("music.mp3").unwrap();
//!
//! // print the artist the hard way
//! println!("{}", tag.get("TPE1").unwrap().content().text().unwrap());
//!
//! // or print it the easy way
//! println!("{}", tag.artist().unwrap());
//!
//! tag.write_to_path("music.mp3", Version::Id3v24).unwrap();
//! ```
//!
//! # Creating a new tag
//!
//! ```no_run
//! use id3::{Tag, Frame, Version};
//! use id3::frame::Content;
//!
//! let mut tag = Tag::with_version(Version::Id3v24);
//!
//! // set the album the hard way
//! let frame = Frame::with_content("TALB", Content::Text("album".to_string()));
//! tag.add_frame(frame);
//!
//! // or set it the easy way
//! tag.set_album("album");
//!
//! tag.write_to_path("music.mp3", Version::Id3v24).unwrap();
//! ```

#![warn(missing_docs)]

#[macro_use]
extern crate derive_builder;

pub use crate::error::{Error, ErrorKind, Result};
pub use crate::frame::{Content, Frame, Timestamp};
pub use crate::stream::tag::{Encoder, EncoderBuilder};
pub use crate::tag::{Tag, Version};

/// Contains types and methods for operating on ID3 frames.
pub mod frame;
/// Utilities for working with ID3v1 tags.
pub mod v1;

mod error;
mod storage;
mod stream;
mod tag;
mod util;
