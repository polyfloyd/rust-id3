//! A library to read and write ID3v2 tags. ID3 versions v2.2, v2.3, and v2.4 are supported.
//! 
//! # Modifying an existing tag
//!
//! ```no_run
//! use id3::AudioTag;
//!
//! let mut tag = AudioTag::read_from_path(&Path::new("music.mp3")).unwrap();
//!
//! // print the artist the hard way
//! println!("{}", tag.get_frame_by_id("TALB").unwrap().contents.text());
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
//! use id3::{ID3Tag, AudioTag, Frame, TextContent};
//! use id3::encoding::UTF8;
//!
//! let mut tag = ID3Tag::with_version(4);
//! 
//! // set the album the hard way
//! let mut frame = Frame::with_version("TALB", 4);
//! frame.set_encoding(UTF8);
//! frame.contents = TextContent(String::from_str("album"));
//! tag.add_frame(frame);
//!
//! // or set it the easy way
//! tag.set_album("album");
//!
//! tag.write_to_path(&Path::new("music.mp3")).unwrap();
//! ```

#![crate_name = "id3"]
#![crate_type = "rlib"]
#![warn(missing_docs)]
#![feature(macro_rules)]

#![feature(phase)]
#[phase(plugin, link)] extern crate log;

#[phase(plugin)]
extern crate phf_mac;
extern crate phf;

extern crate audiotag; 

pub use self::audiotag::{
    AudioTag,
    TagResult, 
    
    TagError,
        InternalIoError,
        StringDecodingError,
        InvalidInputError,
        UnsupportedFeatureError
};

pub use tag::ID3Tag;
pub use frame::{Frame, FrameFlags, encoding, Contents, PictureContent, CommentContent, TextContent, ExtendedTextContent, LinkContent, ExtendedLinkContent, LyricsContent};
pub use picture::{Picture, picture_type};

mod macros;

/// Utilities used for reading/writing ID3 tags.
pub mod util;

mod tag;
mod frame;
mod parsers;
mod picture;
