//! A library to read and write ID3 tags

#![crate_name = "id3"]
#![crate_type = "rlib"]
#![warn(missing_docs)]
#![feature(macro_rules)]

#![feature(phase)]
#[phase(plugin, link)] extern crate log;

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
