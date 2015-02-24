//! A library to read and write ID3v2 tags. ID3 versions v2.2, v2.3, and v2.4 are supported.
//! 
//! # Modifying an existing tag
//!
//! ```no_run
//! use id3::{ID3Tag, AudioTag};
//!
//! let mut tag: ID3Tag = AudioTag::read_from_path(&Path::new("music.mp3")).unwrap();
//!
//! // print the artist the hard way
//! println!("{}", tag.get_frame_by_id("TALB").unwrap().content.text());
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
//! // you need to use AudioTag in order to use the trait features
//! use id3::{ID3Tag, AudioTag, Frame};
//! use id3::Content::TextContent;
//! use id3::Encoding::UTF8;
//!
//! let mut tag = ID3Tag::with_version(4);
//! 
//! // set the album the hard way
//! let mut frame = Frame::with_version("TALB".to_string(), 4);
//! frame.set_encoding(UTF8);
//! frame.content = TextContent("album".to_string());
//! tag.add_frame(frame);
//!
//! // or set it the easy way
//! tag.set_album("album".to_string());
//!
//! tag.write_to_path(&Path::new("music.mp3")).unwrap();
//! ```

#![crate_name = "id3"]
#![crate_type = "rlib"]
#![warn(missing_docs)]
#![feature(plugin)]
#![plugin(phf_macros)]
#![feature(core, old_io, collections, rustc_private, concat_idents, old_path)]

#[macro_use] 
extern crate log;

extern crate phf;
extern crate audiotag;

pub use self::audiotag::{AudioTag, TagResult, TagError, ErrorKind};
pub use tag::ID3Tag;
pub use frame::{Frame, FrameFlags, Encoding, Content};

#[macro_use]
mod macros;

/// Utilities used for reading/writing ID3 tags.
pub mod util;

/// Contains types and methods for operating on ID3 frames.
pub mod frame;

mod id3v1;
mod tag;
mod parsers;
