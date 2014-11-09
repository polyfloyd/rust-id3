extern crate std;
extern crate audiotag;
extern crate flate;

use std::io::File;

use self::audiotag::{TagError, TagResult, InvalidInputError, UnsupportedFeatureError, StringDecodingError};

use util;
use parsers;
use parsers::ParserResult;
use picture::Picture;

/// A module containing the `Encoding` enum. 
pub mod encoding {
    /// Types of text encodings used in ID3 frames.
    #[deriving(Show, FromPrimitive, PartialEq)]
    pub enum Encoding {
        /// ISO-8859-1 text encoding, also referred to as latin1 encoding.
        Latin1,
        /// UTF-16 text encoding with a byte order mark.
        UTF16,
        /// UTF-16BE text encoding without a byte order mark. This encoding is only used in id3v2.4.
        UTF16BE,
        /// UTF-8 text encoding. This encoding is only used in id3v2.4.
        UTF8 
    }
}

/// The decoded contents of a frame.
pub enum Contents {
    /// A value containing the parsed contents of a text frame.
    TextContent(String),
    /// A value containing the parsed contents of a user defined text frame (TXXX).
    ExtendedTextContent((String, String)),
    /// A value containing the parsed contents of a web link frame.
    LinkContent(String),
    /// A value containing the parsed contents of a user defined web link frame (WXXX).
    ExtendedLinkContent((String, String)),
    /// A value containing the parsed contents of a comment frame (COMM).
    CommentContent((String, String)),
    /// A value containing the parsed contents of a lyrics frame (USLT).
    LyricsContent(String),
    /// A value containing the parsed contents of a picture frame (APIC).
    PictureContent(Picture),
    /// A value containing the bytes of a unknown frame.
    UnknownContent(Vec<u8>),
}

impl Contents {
    /// Returns the `TextContent`.
    /// Panics if the value is not `TextContent`.
    pub fn text(&self) -> &String {
        match *self {
            TextContent(ref text) => text,
            _ => panic!("called `Contents::text()` on a non `TextContent` value") 
        }
    }

    /// Returns the `ExtendedTextContent`.
    /// Panics if the value is not `ExtendedTextContent`.
    pub fn extended_text(&self) -> &(String, String) {
        match *self {
            ExtendedTextContent(ref pair) => pair,
            _ => panic!("called `Contents::extended_text()` on a non `ExtendedTextContent` value") 
        }
    }

    /// Returns the `LinkContent`.
    /// Panics if the value is not `LinkContent`.
    pub fn link(&self) -> &String {
        match *self {
            LinkContent(ref text) => text,
            _ => panic!("called `Contents::link()` on a non `LinkContent` value") 
        }
    }

    /// Returns the `ExtendedLinkContent`.
    /// Panics if the value is not `ExtendedLinkContent`.
    pub fn extended_link(&self) -> &(String, String) {
        match *self {
            ExtendedLinkContent(ref pair) => pair,
            _ => panic!("called `Contents::extended_link()` on a non `ExtendedLinkContent` value") 
        }
    }

    /// Returns the `CommentContent`.
    /// Panics if the value is not `CommentContent`.
    pub fn comment(&self) -> &(String, String) {
        match *self {
            CommentContent(ref pair) => pair,
            _ => panic!("called `Contents::comment()` on a non `CommentContent` value") 
        }
    }

    /// Returns the `LyricsContent`.
    /// Panics if the value is not `LyricsContent`.
    pub fn lyrics(&self) -> &String {
        match *self {
            LyricsContent(ref text) => text,
            _ => panic!("called `Contents::lyrics()` on a non `LyricsContent` value") 
        }
    }

    /// Returns the `PictureContent`.
    /// Panics if the value is not `PictureContent`.
    pub fn picture(&self) -> &Picture {
        match *self {
            PictureContent(ref picture) => picture,
            _ => panic!("called `Contents::picture()` on a non `PictureContent` value") 
        }
    }

    /// Returns the `UnknownContent`.
    /// Panics if the value is not `UnknownContent`.
    pub fn unknown(&self) -> &[u8] {
        match *self {
            UnknownContent(ref data) => data.as_slice(),
            _ => panic!("called `Contents::unknown()` on a non `UnknownContent` value") 
        }
    }
}

/// A structure representing an ID3 frame.
pub struct Frame {
    /// A sequence of 16 bytes used to uniquely identify this frame. 
    pub uuid: Vec<u8>,
    /// The frame identifier.
    pub id: String,
    /// The encoding to be used when converting this frame to bytes.
    pub encoding: encoding::Encoding,
    /// The offset of the frame in the file.
    pub offset: u64,
    /// The frame flags.
    pub flags: FrameFlags,
    /// The parsed contents of the frame.
    pub contents: Contents
}

/// Flags used in ID3 frames.
pub struct FrameFlags {
    /// Indicates whether or not this frame should be discarded if the tag is altered.
    /// A value of `true` indicates the frame should be discarded.
    pub tag_alter_preservation: bool,
    /// Indicates whether or not this frame should be discarded if the file is altered.
    /// A value of `true` indicates the frame should be discarded.
    pub file_alter_preservation: bool,
    /// Indicates whether or not this frame is intended to be read only.
    pub read_only: bool,
    /// Indicates whether or not the frame is compressed using zlib.
    /// If set 4 bytes for "decompressed size" are appended to the header.
    pub compression: bool,
    /// Indicates whether or not the frame is encrypted.
    /// If set a byte indicating which encryption method was used will be appended to the header.
    pub encryption: bool,
    /// Indicates whether or not the frame belongs in a group with other frames.
    /// If set a group identifier byte is appended to the header.
    pub grouping_identity: bool,
    ///This flag indicates whether or not unsynchronisation was applied
    ///to this frame.
    pub unsynchronization: bool,
    ///This flag indicates that a data length indicator has been added to
    ///the frame.
    pub data_length_indicator: bool
}

// FrameFlags {{{
impl FrameFlags {
    /// Returns a new `FrameFlags` with all flags set to false.
    pub fn new() -> FrameFlags {
        FrameFlags { tag_alter_preservation: false, file_alter_preservation: false, read_only: false, compression: false, 
            encryption: false, grouping_identity: false, unsynchronization: false, data_length_indicator: false }
    }

    /// Returns a vector representation suitable for writing to a file containing an ID3v2.3
    /// tag.
    pub fn to_bytes_v3(&self) -> Vec<u8> {
        let mut bytes = [0x0, ..2];

        if self.tag_alter_preservation {
            bytes[0] |= 0x80;
        }
        if self.file_alter_preservation {
            bytes[0] |= 0x40;
        }
        if self.read_only {
            bytes[0] |= 0x20;
        }
        if self.compression {
            bytes[1] |= 0x80;
        }
        if self.encryption {
            bytes[1] |= 0x40;
        }
        if self.grouping_identity {
            bytes[1] |= 0x20;
        }

        bytes.to_vec()
    }

    /// Returns a vector representation suitable for writing to a file containing an ID3v2.4
    /// tag.
    pub fn to_bytes_v4(&self) -> Vec<u8> {
        let mut bytes = [0x0, ..2];

        if self.tag_alter_preservation {
            bytes[0] |= 0x40;
        }
        if self.file_alter_preservation {
            bytes[0] |= 0x20;
        }
        if self.read_only {
            bytes[0] |= 0x10;
        }
        if self.grouping_identity {
            bytes[1] |= 0x40;
        }
        if self.compression {
            bytes[1] |= 0x08;
        }
        if self.encryption {
            bytes[1] |= 0x04;
        } 
        if self.unsynchronization {
            bytes[1] |= 0x02;
        }
        if self.data_length_indicator {
            bytes[1] |= 0x01;
        }

        bytes.to_vec()
    }

    /// Returns a vector representation suitable for writing to a file containing an ID3 tag
    /// of the specified version.
    pub fn to_bytes(&self, version: u8) -> Vec<u8> {
        match version {
            0x3 => self.to_bytes_v3(),
            0x4 => self.to_bytes_v4(),
            _ => [0x0, ..2].to_vec()
        }
    }
}
// }}}

// Frame {{{
impl Frame {
    /// Creates a new `Frame` with the specified identifier.
    pub fn new(id: &str) -> Frame {
        Frame { uuid: util::uuid(), id: String::from_str(id), encoding: encoding::UTF16, offset: 0, flags: FrameFlags::new(), contents: UnknownContent(Vec::new()) }
    }

    /// Generates a new uuid for this frame.
    ///
    /// # Example
    /// ```
    /// use id3::Frame;
    ///
    /// let mut frame = Frame::new("TYER");
    /// let prev_uuid = frame.uuid.clone();
    /// frame.generate_uuid();
    /// assert!(prev_uuid != frame.uuid);
    /// ```
    pub fn generate_uuid(&mut self) {
        self.uuid = util::uuid();
    }

    /// Attempts to reads from a file containing an ID3v2.2 tag.
    ///
    /// Returns a `Frame` or `None` if padding is encountered.
    pub fn read_v2(file: &mut File) -> TagResult<Option<Frame>> {
        let mut frame = Frame::new("");
        frame.uuid = util::uuid();

        frame.offset = try!(file.tell());

        let c = try!(file.read_byte());
        try!(file.seek(-1, std::io::SeekCur)); 
        if c == 0 { // padding
            return Ok(None);
        }
        let frameid = try!(file.read_exact(3));
        let frameid_str = match String::from_utf8(frameid) { Ok(id) => id, Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "frame identifier is not valid utf8")) };

        frame.id = match util::convert_id_2_to_3(frameid_str.as_slice()) {
            Some(id) => String::from_str(id),
            None => return Err(TagError::new(UnsupportedFeatureError, "frame type is not supported for id3v2.2 to id3v2.3/4 conversion"))
        };

        let sizebytes = try!(file.read_exact(3));
        let size = (sizebytes[0] as uint << 16) | (sizebytes[1] as uint << 8) | sizebytes[2] as uint;

        let data = try!(file.read_exact(size as uint));

        if frame.id.as_slice() == "APIC" {
            let encoding = data[0];
            let format = match String::from_utf8(data.slice(1, 4).to_vec()) {
                Ok(format) => format,
                Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "image format is not valid utf8"))
            };
            let mime_type = match format.as_slice() {
                "PNG" => "image/png",
                "JPG" => "image/jpeg",
                _ => return Err(TagError::new(UnsupportedFeatureError, "image format is not supported for id3v2.2 to id3v2.3/4 conversion"))
            };

            let pictype = data[4];
            let remaining = data.slice_from(5).to_vec();

            let mut new_data = Vec::with_capacity(1 + mime_type.len() + 1 +  remaining.len());
            new_data.push(encoding);
            new_data.extend(String::from_str(mime_type).into_bytes().into_iter());
            new_data.push(0x0);
            new_data.push(pictype);
            new_data.extend(remaining.into_iter());

            match frame.parse_data(new_data.as_slice()) {
                Ok(_) => { },
                Err(_) => return Err(TagError::new(InvalidInputError, "converted image frame was invalid"))
            }
        } else {
            match frame.parse_data(data.as_slice()) {
                Ok(_) => { },
                Err(err) => return Err(err)
            }
        }

        Ok(Some(frame))
    }

    /// Attempts to read from a file containing an ID3v2.3 tag.
    ///
    /// Returns a `Frame` or `None` if padding is encountered.
    pub fn read_v3(file: &mut File) -> TagResult<Option<Frame>> {
        let mut frame = Frame::new("");
        frame.uuid = util::uuid();

        frame.offset = try!(file.tell());

        let c = try!(file.read_byte());
        try!(file.seek(-1, std::io::SeekCur)); 
        if c == 0 { // padding
            return Ok(None);
        }
        let frameid = try!(file.read_exact(4));
        frame.id = match String::from_utf8(frameid) { Ok(id) => id, Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "frame identifier is not valid utf8")) };

        debug!("reading {}", frame.id); 

        let mut size = try!(file.read_be_u32());

        let frameflags = try!(file.read_be_u16());
        frame.flags.tag_alter_preservation = frameflags & 0x8000 != 0;
        frame.flags.file_alter_preservation = frameflags & 0x4000 != 0;
        frame.flags.read_only = frameflags & 0x2000 != 0;
        frame.flags.compression = frameflags & 0x80 != 0;
        frame.flags.encryption = frameflags & 0x40 != 0;
        frame.flags.grouping_identity = frameflags & 0x20 != 0;

        if frame.flags.encryption {
            debug!("[{}] encryption is not supported", frame.id);
            return Err(TagError::new(UnsupportedFeatureError, "encryption is not supported"));
        } else if frame.flags.grouping_identity {
            debug!("[{}] grouping identity is not supported", frame.id);
            return Err(TagError::new(UnsupportedFeatureError, "grouping identity is not supported"));
        }

        if frame.flags.compression {
            debug!("[{}] frame is zlib compressed", frame.id);
            let decompressed_size = try!(file.read_be_u32());
            debug!("[{}] decompressed size: {}", frame.id, decompressed_size);
            size -= 4;
        }
        
        let data = try!(file.read_exact(size as uint));
        match frame.parse_data(data.as_slice()) {
            Ok(_) => { },
            Err(err) => return Err(err)
        }

        Ok(Some(frame))
    }
    
    /// Attempts to read from a file containing an ID3v2.4 tag.
    ///
    /// Returns a `Frame` or `None` if padding is encountered.
    pub fn read_v4(file: &mut File) -> TagResult<Option<Frame>> {
        let mut frame = Frame::new("");
        frame.uuid = util::uuid();

        frame.offset = try!(file.tell());

        let c = try!(file.read_byte());
        try!(file.seek(-1, std::io::SeekCur)); 
        if c == 0 { // padding
            return Ok(None);
        }
        let frameid = try!(file.read_exact(4));
        frame.id = match String::from_utf8(frameid) { Ok(id) => id, Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "frame identifier is not valid utf8")) };

        let mut size = util::unsynchsafe(try!(file.read_be_u32()));

        let frameflags = try!(file.read_be_u16());
        frame.flags.tag_alter_preservation = frameflags & 0x4000 != 0;
        frame.flags.file_alter_preservation = frameflags & 0x2000 != 0;
        frame.flags.read_only = frameflags & 0x1000 != 0;
        frame.flags.grouping_identity = frameflags & 0x40 != 0;
        frame.flags.compression = frameflags & 0x08 != 0;
        frame.flags.encryption = frameflags & 0x04 != 0;
        frame.flags.unsynchronization = frameflags & 0x02 != 0;
        frame.flags.data_length_indicator = frameflags & 0x01 != 0;

        if frame.flags.encryption {
            debug!("[{}] encryption is not supported", frame.id);
            return Err(TagError::new(UnsupportedFeatureError, "encryption is not supported"));
        } else if frame.flags.grouping_identity {
            debug!("[{}] grouping identity is not supported", frame.id);
            return Err(TagError::new(UnsupportedFeatureError, "grouping identity is not supported"));
        } else if frame.flags.unsynchronization {
            debug!("[{}] unsynchronization is not supported", frame.id);
            return Err(TagError::new(UnsupportedFeatureError, "unsynchronization is not supported"));
        }

        if frame.flags.data_length_indicator {
            debug!("[{}] frame has data length indicator", frame.id);
            let decompressed_size = util::unsynchsafe(try!(file.read_be_u32()));
            debug!("[{}] decompressed size: {}", frame.id, decompressed_size);
            size -= 4;
        }

        let data = try!(file.read_exact(size as uint));
        match frame.parse_data(data.as_slice()) {
            Ok(_) => { },
            Err(err) => return Err(err)
        }

        Ok(Some(frame))
    }

    /// Attempts to read from a file containing an ID3 tag of the specified version.
    ///
    /// Returns a `Frame` or `None` if padding is encountered.
    ///
    /// Only reading from version 2, 3, and 4 is supported. Attempting to read any other version
    /// will return an `InvalidInputError`. 
    pub fn read(version: u8, file: &mut File) -> TagResult<Option<Frame>> {
        match version {
            0x2 => Frame::read_v2(file),
            0x3 => Frame::read_v3(file),
            0x4 => Frame::read_v4(file),
            _ =>  Err(TagError::new(InvalidInputError, "unsupported id3 tag version"))
        }

    }
    
    /// Creates a vector representation of the frame suitable for writing to an ID3v2.3 tag.
    ///
    /// If the compression flag is set to true then contents will be compressed.
    pub fn to_bytes_v3(&self) -> Vec<u8> {
        let mut contents_bytes = self.contents_to_bytes();
        let mut contents_size = contents_bytes.len();
        let contents_decompressed_size = contents_size;

        if self.flags.compression {
            debug!("[{}] compressing frame contents", self.id);
            contents_bytes = flate::deflate_bytes_zlib(contents_bytes.as_slice()).unwrap().as_slice().to_vec();
            contents_size = contents_bytes.len() + 4;
        }

        let mut bytes = Vec::with_capacity(4 + 4 + 2 + contents_size);
        bytes.extend(self.id.clone().into_bytes().into_iter());
        bytes.extend(util::u32_to_bytes(contents_size as u32).into_iter());
        bytes.extend(self.flags.to_bytes(0x3).into_iter());
        if self.flags.compression {
            bytes.extend(util::u32_to_bytes(contents_decompressed_size as u32).into_iter());
        }
        bytes.extend(contents_bytes.into_iter());
        bytes
    }

    /// Creates a vector representation of the frame suitable for writing to an ID3v2.4 tag.
    ///
    /// If the compression flag is set to true then contents will be compressed.
    pub fn to_bytes_v4(&mut self) -> Vec<u8> {
        let mut contents_bytes = self.contents_to_bytes();
        let mut contents_size = contents_bytes.len();
        let contents_decompressed_size = contents_size;

        if self.flags.compression {
            self.flags.data_length_indicator = true;
            debug!("[{}] compressing frame contents", self.id);
            contents_bytes = flate::deflate_bytes_zlib(contents_bytes.as_slice()).unwrap().as_slice().to_vec();
            contents_size = contents_bytes.len();
        }

        if self.flags.data_length_indicator {
            contents_size += 4;
        }

        let mut bytes = Vec::with_capacity(4 + 4 + 2 + contents_size);
        bytes.extend(self.id.clone().into_bytes().into_iter());
        bytes.extend(util::u32_to_bytes(util::synchsafe(contents_size as u32)).into_iter());
        bytes.extend(self.flags.to_bytes(0x4).into_iter());
        if self.flags.data_length_indicator {
            debug!("[{}] adding data length indicator", self.id);
            bytes.extend(util::u32_to_bytes(util::synchsafe(contents_decompressed_size as u32)).into_iter());
        }
        bytes.extend(contents_bytes.into_iter());
        bytes
    }

    /// Creates a vector representation of the frame suitable for writing to an ID3 tag of the
    /// specified version. 
    /// 
    /// If the compression flag is set to true then contents will be compressed.
    pub fn to_bytes(&mut self, version: u8) -> Vec<u8> {
        match version {
            0x3 => self.to_bytes_v3(),
            _ => self.to_bytes_v4()
        }
    }

    /// Creates a vector representation of the contents suitable for writing to an ID3 tag.
    pub fn contents_to_bytes(&self) -> Vec<u8> {
        match self.contents {
            TextContent(ref text) => parsers::text_to_bytes(self.encoding, text.as_slice()),
            ExtendedTextContent((ref key, ref value)) => parsers::extended_text_to_bytes(self.encoding, key.as_slice(), value.as_slice()),
            LinkContent(ref url) => parsers::weblink_to_bytes(url.as_slice()),
            ExtendedLinkContent((ref key, ref value)) => parsers::extended_weblink_to_bytes(self.encoding, key.as_slice(), value.as_slice()),
            LyricsContent(ref text) => parsers::lyrics_to_bytes(self.encoding, "", text.as_slice()),
            CommentContent((ref description, ref text)) => parsers::comment_to_bytes(self.encoding, description.as_slice(), text.as_slice()),
            PictureContent(ref picture) => parsers::picture_to_bytes(self.encoding, picture),
            UnknownContent(ref data) => data.clone()
        }
    }

    /// Parses the provided data and sets the `contents` field. If the compression flag is set to
    /// true then decompression will be performed.
    ///
    /// Returns `Err` if the data is invalid for the frame type.
    pub fn parse_data(&mut self, data: &[u8]) -> TagResult<()> {
        let decompressed = if self.flags.compression {
            Some(flate::inflate_bytes_zlib(data).unwrap())
        } else {
            None
        };

        macro_rules! choose_data {
            ($decompressed:ident, $data:ident) => {
                match $decompressed {
                    Some(ref bytes) => bytes.as_slice(),
                    None => $data
                }
            };
        }

        let result = match self.id.as_slice() {
            "APIC" => try!(parsers::parse_apic(choose_data!(decompressed, data))),
            "TXXX" => try!(parsers::parse_txxx(choose_data!(decompressed, data))),
            "WXXX" => try!(parsers::parse_wxxx(choose_data!(decompressed, data))),
            "COMM" => try!(parsers::parse_comm(choose_data!(decompressed, data))),
            "USLT" => try!(parsers::parse_uslt(choose_data!(decompressed, data))),
            _ => {
                let mut contents = None;
                if self.id.as_slice().len() > 0 {
                    if self.id.as_slice().char_at(0) == 'T' {
                        contents = Some(try!(parsers::parse_text(choose_data!(decompressed, data))));
                    } else if self.id.as_slice().char_at(0) == 'W' {
                        contents = Some(try!(parsers::parse_weblink(choose_data!(decompressed, data))));
                    } 
                }
               
                if contents.is_none() {
                    contents = Some(ParserResult::new(self.encoding, UnknownContent(choose_data!(decompressed, data).to_vec())));
                }

                contents.unwrap()
            }
        };

        self.encoding = result.encoding;
        self.contents = result.contents;

        Ok(())
    }

    /// Reparses the frame's data.
    pub fn reparse(&mut self) -> TagResult<()> {
        let data = self.contents_to_bytes();
        self.parse_data(data.as_slice())
    }

    /// Returns a string representing the parsed content.
    ///
    /// Returns `None` if the parsed content can not be represented as text.
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, ExtendedTextContent, TextContent};
    ///
    /// let mut title_frame = Frame::new("TIT2");
    /// title_frame.contents = TextContent(String::from_str("title"));
    /// assert_eq!(title_frame.text().unwrap().as_slice(), "title");
    ///
    /// let mut txxx_frame = Frame::new("TXXX");
    /// txxx_frame.contents = ExtendedTextContent((String::from_str("key"), 
    /// String::from_str("value")));
    /// assert_eq!(txxx_frame.text().unwrap().as_slice(), "key: value");
    /// ```
    pub fn text(&self) -> Option<String> {
        match self.contents {
            TextContent(ref text) 
                | LinkContent(ref text) 
                | LyricsContent(ref text) => Some(text.clone()),
            ExtendedTextContent((ref key, ref value)) 
                | ExtendedLinkContent((ref key, ref value)) 
                | CommentContent((ref key, ref value)) => Some(format!("{}: {}", key, value)),
            _ => None
        }
    }

    /// Returns a string describing the frame type.
    pub fn description(&self) -> &str {
        util::frame_description(self.id.as_slice())
    }
}
// }}}
 
// Tests {{{
#[cfg(test)]
mod tests {
    use frame::{Frame, FrameFlags};
    use encoding;
    use util;

    #[test]
    fn test_frame_flags_to_bytes_v3() {
        let mut flags = FrameFlags::new();
        assert_eq!(flags.to_bytes(0x3), vec!(0x0, 0x0));
        flags.tag_alter_preservation = true;
        flags.file_alter_preservation = true;
        flags.read_only = true;
        flags.compression = true;
        flags.encryption = true;
        flags.grouping_identity = true;
        assert_eq!(flags.to_bytes(0x3), vec!(0xE0, 0xE0));
    }

    #[test]
    fn test_frame_flags_to_bytes_v4() {
        let mut flags = FrameFlags::new();
        assert_eq!(flags.to_bytes(0x4), vec!(0x0, 0x0));
        flags.tag_alter_preservation = true;
        flags.file_alter_preservation = true;
        flags.read_only = true;
        flags.grouping_identity = true;
        flags.compression = true;
        flags.encryption = true;
        flags.unsynchronization = true;
        flags.data_length_indicator = true;
        assert_eq!(flags.to_bytes(0x4), vec!(0x70, 0x4F));
    }

    #[test]
    fn test_to_bytes_v3() {
        let id = "TALB";
        let text = "album";
        let encoding = encoding::UTF16;

        let mut frame = Frame::new(id);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(util::string_to_utf16(text).into_iter());

        frame.parse_data(data.as_slice()).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(String::from_str(id).into_bytes().into_iter());
        bytes.extend(util::u32_to_bytes(data.len() as u32).into_iter());
        bytes.push_all([0x00, 0x00]);
        bytes.extend(data.into_iter());

        assert_eq!(frame.to_bytes_v3(), bytes);
        assert_eq!(frame.to_bytes(3), bytes);
    }

    #[test]
    fn test_to_bytes_v4() {
        let id = "TALB";
        let text = "album";
        let encoding = encoding::UTF16;

        let mut frame = Frame::new(id);

        frame.flags.tag_alter_preservation = true;
        frame.flags.file_alter_preservation = true; 

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(util::string_to_utf16(text).into_iter());

        frame.parse_data(data.as_slice()).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(String::from_str(id).into_bytes().into_iter());
        bytes.extend(util::u32_to_bytes(util::synchsafe(data.len() as u32)).into_iter());
        bytes.push_all([0x60, 0x00]);
        bytes.extend(data.into_iter());

        assert_eq!(frame.to_bytes_v4(), bytes);
        assert_eq!(frame.to_bytes(4), bytes);
    }
}
