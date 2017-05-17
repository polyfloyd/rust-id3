//! A library to read and write ID3v2 tags. ID3 versions v2.2, v2.3, and v2.4 are supported.
//! 
//! # Modifying an existing tag
//!
//! ```no_run
//! use id3::Tag;
//!
//! let mut tag= Tag::read_from_path("music.mp3").unwrap();
//!
//! // print the artist the hard way
//! println!("{}", tag.get("TALB").unwrap().content.text().unwrap());
//! 
//! // or print it the easy way
//! println!("{}", tag.artist().unwrap());
//!
//! tag.save().unwrap();
//! ```
//!
//! # Creating a new tag
//!
//! ```no_run
//! use id3::{Tag, Frame, Version};
//! use id3::frame::{Content, Encoding};
//!
//! let mut tag = Tag::with_version(Version::Id3v24);
//! 
//! // set the album the hard way
//! let mut frame = Frame::new("TALB");
//! frame.encoding = Encoding::UTF8;
//! frame.content = Content::Text("album".to_owned());
//! tag.push(frame);
//!
//! // or set it the easy way
//! tag.set_album("album");
//!
//! tag.write_to_path("music.mp3").unwrap();
//! ```

#![crate_name = "id3"]
#![crate_type = "rlib"]
#![warn(missing_docs)]

extern crate byteorder;
extern crate encoding;
extern crate flate2;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate rand;
extern crate regex;

pub use tag::{Tag, Version};
pub use frame::Frame;
pub use frame::Timestamp;
pub use error::{Result, Error, ErrorKind};

/// Utilities used for reading/writing ID3 tags.
pub mod util;

/// Contains types and methods for operating on ID3 frames.
pub mod frame;

mod error;
mod id3v1;
mod tag;
mod parsers;
