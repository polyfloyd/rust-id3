extern crate std;
extern crate audiotag;
extern crate flate;

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
    #[inline]
    pub fn text(&self) -> &String {
        match *self {
            TextContent(ref text) => text,
            _ => panic!("called `Contents::text()` on a non `TextContent` value") 
        }
    }

    /// Returns the `ExtendedTextContent`.
    /// Panics if the value is not `ExtendedTextContent`.
    #[inline]
    pub fn extended_text(&self) -> &(String, String) {
        match *self {
            ExtendedTextContent(ref pair) => pair,
            _ => panic!("called `Contents::extended_text()` on a non `ExtendedTextContent` value") 
        }
    }

    /// Returns the `LinkContent`.
    /// Panics if the value is not `LinkContent`.
    #[inline]
    pub fn link(&self) -> &String {
        match *self {
            LinkContent(ref text) => text,
            _ => panic!("called `Contents::link()` on a non `LinkContent` value") 
        }
    }

    /// Returns the `ExtendedLinkContent`.
    /// Panics if the value is not `ExtendedLinkContent`.
    #[inline]
    pub fn extended_link(&self) -> &(String, String) {
        match *self {
            ExtendedLinkContent(ref pair) => pair,
            _ => panic!("called `Contents::extended_link()` on a non `ExtendedLinkContent` value") 
        }
    }

    /// Returns the `CommentContent`.
    /// Panics if the value is not `CommentContent`.
    #[inline]
    pub fn comment(&self) -> &(String, String) {
        match *self {
            CommentContent(ref pair) => pair,
            _ => panic!("called `Contents::comment()` on a non `CommentContent` value") 
        }
    }

    /// Returns the `LyricsContent`.
    /// Panics if the value is not `LyricsContent`.
    #[inline]
    pub fn lyrics(&self) -> &String {
        match *self {
            LyricsContent(ref text) => text,
            _ => panic!("called `Contents::lyrics()` on a non `LyricsContent` value") 
        }
    }

    /// Returns the `PictureContent`.
    /// Panics if the value is not `PictureContent`.
    #[inline]
    pub fn picture(&self) -> &Picture {
        match *self {
            PictureContent(ref picture) => picture,
            _ => panic!("called `Contents::picture()` on a non `PictureContent` value") 
        }
    }

    /// Returns the `UnknownContent`.
    /// Panics if the value is not `UnknownContent`.
    #[inline]
    pub fn unknown(&self) -> &[u8] {
        match *self {
            UnknownContent(ref data) => data.as_slice(),
            _ => panic!("called `Contents::unknown()` on a non `UnknownContent` value") 
        }
    }
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
    #[inline]
    pub fn new() -> FrameFlags {
        FrameFlags { 
            tag_alter_preservation: false, file_alter_preservation: false, read_only: false, compression: false, 
            encryption: false, grouping_identity: false, unsynchronization: false, data_length_indicator: false 
        }
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

/// A structure representing an ID3 frame.
pub struct Frame {
    /// A sequence of 16 bytes used to uniquely identify this frame. 
    pub uuid: Vec<u8>,
    /// The frame identifier.
    pub id: String,
    /// The major version of the tag which this frame belongs to.
    version: u8,
    /// The encoding to be used when converting this frame to bytes.
    encoding: encoding::Encoding,
    /// The frame flags.
    flags: FrameFlags,
    /// The parsed contents of the frame.
    pub contents: Contents,
    /// The offset of this frame in the file from which it was loaded.
    pub offset: u32,
}

// Frame {{{
impl PartialEq for Frame {
    #[inline]
    fn eq(&self, other: &Frame) -> bool {
        self.uuid.as_slice() == other.uuid.as_slice()
    }

    #[inline]
    fn ne(&self, other: &Frame) -> bool {
        self.uuid.as_slice() != other.uuid.as_slice()
    }
}

impl Frame {
    /// Creates a new ID3v2.3 frame with the specified identifier.
    #[inline]
    pub fn new(id: &str) -> Frame {
        Frame { 
            uuid: util::uuid(), id: String::from_str(id), version: 3, encoding: encoding::UTF16, 
            flags: FrameFlags::new(), contents: UnknownContent(Vec::new()), offset: 0 
        }
    }
    
    /// Creates a new frame with the specified identifier and version.
    ///
    /// # Example
    /// ```
    /// use id3::Frame;
    ///
    /// let frame = Frame::with_version("TALB", 4);
    /// assert_eq!(frame.version(), 4);
    /// ```
    pub fn with_version(id: &str, version: u8) -> Frame {
        let mut frame = Frame::new(id);
        frame.version = version;
        frame
    }

    /// Returns an encoding compatible with the current version based on the requested encoding.
    #[inline]
    fn compatible_encoding(&self, requested_encoding: encoding::Encoding) -> encoding::Encoding {
        if self.version < 4 {
            match requested_encoding {
                encoding::Latin1 => encoding::Latin1,
                _ => encoding::UTF16, // if UTF16BE or UTF8 is requested, just return UTF16
            }
        } else {
            requested_encoding
        }
    }

    // Getters/Setters
    #[inline]
    /// Returns the encoding.
    pub fn encoding(&self) -> encoding::Encoding {
        self.encoding
    }

    #[inline]
    /// Sets the encoding. If the encoding is not compatible with the frame version, another
    /// encoding will be chosen.
    pub fn set_encoding(&mut self, encoding: encoding::Encoding) {
        self.encoding = self.compatible_encoding(encoding);
    }

    #[inline]
    /// Returns whether the compression flag is set.
    pub fn compression(&self) -> bool {
        self.flags.compression
    }

    #[inline]
    /// Sets the compression flag. 
    pub fn set_compression(&mut self, compression: bool) {
        self.flags.compression = compression;
        if compression && self.version >= 4 {
            self.flags.data_length_indicator = true;
        }
    }

    #[inline]
    /// Returns whether the tag_alter_preservation flag is set.
    pub fn tag_alter_preservation(&self) -> bool {
        self.flags.tag_alter_preservation
    }

    #[inline]
    /// Sets the tag_alter_preservation flag.
    pub fn set_tag_alter_preservation(&mut self, tag_alter_preservation: bool) {
        self.flags.tag_alter_preservation = tag_alter_preservation;
    }

    #[inline]
    /// Returns whether the file_alter_preservation flag is set.
    pub fn file_alter_preservation(&self) -> bool {
        self.flags.file_alter_preservation
    }

    #[inline]
    /// Sets the file_alter_preservation flag.
    pub fn set_file_alter_preservation(&mut self, file_alter_preservation: bool) {
        self.flags.file_alter_preservation = file_alter_preservation;
    }

    /// Returns the version of the tag which this frame belongs to.
    ///
    /// # Example
    /// ```
    /// use id3::Frame;
    ///
    /// let frame = Frame::with_version("USLT", 4);
    /// assert_eq!(frame.version(), 4)
    /// ```
    #[inline]
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Sets the version of the tag. This converts the frame identifier from the previous version
    /// to the corresponding frame identifier in the new version.
    ///
    /// Returns true if the conversion was successful. Returns false if the frame identifier could
    /// not be converted.
    pub fn set_version(&mut self, version: u8) -> bool {
        if self.version == version || (self.version == 3 && version == 4) || (self.version == 4 && version == 3) {
            return true;
        }

        if (self.version == 3 || self.version == 4) && version == 2 {
            // attempt to convert the id
            self.id = match util::convert_id_3_to_2(self.id.as_slice()) {
                Some(id) => String::from_str(id),
                None => {
                    debug!("no ID3v2.3 to ID3v2.3 mapping for {}", self.id);
                    return false;
                }
            }
        } else if self.version == 2 && (version == 3 || version == 4) {
            // attempt to convert the id
            self.id = match util::convert_id_2_to_3(self.id.as_slice()) {
                Some(id) => String::from_str(id),
                None => {
                    debug!("no ID3v2.2 to ID3v2.3 mapping for {}", self.id);
                    return false;
                }
            };

            // if the new version is v2.4 and the frame is compressed, we must enable the
            // data_length_indicator flag
            if version == 4 && self.flags.compression {
                self.flags.data_length_indicator = true;
            }
        } else {
            // not sure when this would ever occur but lets just say the conversion failed
            return false;
        }

        let encoding = self.compatible_encoding(self.encoding);
        self.set_encoding(encoding);

        self.version = version;
        true
    }
    // }}}

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
    #[inline]
    pub fn generate_uuid(&mut self) {
        self.uuid = util::uuid();
    }

    // Reading {{{
    fn read_from_v2(reader: &mut Reader) -> TagResult<Option<(u32, Frame)>> {
        let mut bytes_read = 0u32;

        let mut frame = Frame::new("");
        frame.version = 2;

        let c = try!(reader.read_byte());
        bytes_read += 1;

        if c == 0 { // padding
            return Ok(None);
        }
       
        let mut frame_id = Vec::with_capacity(3);
        frame_id.push(c);
        frame_id.extend(try!(reader.read_exact(2)).into_iter());
        bytes_read += 2;

        frame.id = match String::from_utf8(frame_id) { 
            Ok(id) => id, 
            Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "frame identifier is not valid utf8")) 
        };
        
        debug!("reading {}", frame.id); 

        let sizebytes = try!(reader.read_exact(3));
        bytes_read += 3;
        let size = (sizebytes[0] as u32 << 16) | (sizebytes[1] as u32 << 8) | sizebytes[2] as u32;

        let data = try!(reader.read_exact(size as uint));
        bytes_read += size;
        match frame.parse_data(data.as_slice()) {
            Ok(_) => {},
            Err(err) => return Err(err)
        }

        Ok(Some((bytes_read, frame)))
    }

    fn read_from_v3(reader: &mut Reader) -> TagResult<Option<(u32, Frame)>> {
        let mut bytes_read = 0u32;

        let mut frame = Frame::new("");
        frame.version = 3;

        let c = try!(reader.read_byte());
        bytes_read += 1;
        if c == 0 { // padding
            return Ok(None);
        }

        let mut frame_id = Vec::new();
        frame_id.push(c);
        frame_id.extend(try!(reader.read_exact(3)).into_iter());
        bytes_read += 3;

        frame.id = match String::from_utf8(frame_id) { 
            Ok(id) => id, 
            Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "frame identifier is not valid utf8")) 
        };

        debug!("reading {}", frame.id); 

        let mut size = try!(reader.read_be_u32());
        bytes_read += 4;

        let frameflags = try!(reader.read_be_u16());
        bytes_read += 2;

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

            let decompressed_size = try!(reader.read_be_u32());
            bytes_read += 4;
            size -= 4;

            debug!("[{}] decompressed size: {}", frame.id, decompressed_size);
        }
        
        let data = try!(reader.read_exact(size as uint));
        bytes_read += size; 
        match frame.parse_data(data.as_slice()) {
            Ok(_) => { },
            Err(err) => return Err(err)
        }

        Ok(Some((bytes_read, frame)))
    }
    
    fn read_from_v4(reader: &mut Reader) -> TagResult<Option<(u32, Frame)>> {
        let mut bytes_read = 0u32;

        let mut frame = Frame::new("");
        frame.version = 4;

        let c = try!(reader.read_byte());
        bytes_read += 1;
        if c == 0 { // padding
            return Ok(None);
        }

        let mut frame_id = Vec::new();
        frame_id.push(c);
        frame_id.extend(try!(reader.read_exact(3)).into_iter());
        bytes_read += 3;

        frame.id = match String::from_utf8(frame_id) { 
            Ok(id) => id, 
            Err(bytes) => return Err(TagError::new(StringDecodingError(bytes), "frame identifier is not valid utf8")) 
        };

        debug!("reading {}", frame.id);

        let mut size = util::unsynchsafe(try!(reader.read_be_u32()));
        bytes_read += 4;

        let frameflags = try!(reader.read_be_u16());
        bytes_read += 2;

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
            let decompressed_size = util::unsynchsafe(try!(reader.read_be_u32()));
            bytes_read += 4;
            debug!("[{}] decompressed size: {}", frame.id, decompressed_size);
            size -= 4;
        }

        let data = try!(reader.read_exact(size as uint));
        bytes_read += size;
        match frame.parse_data(data.as_slice()) {
            Ok(_) => { },
            Err(err) => return Err(err)
        }

        Ok(Some((bytes_read, frame)))
    }

    /// Attempts to read from a file containing an ID3 tag of the specified version.
    ///
    /// Returns a `Frame` or `None` if padding is encountered.
    ///
    /// Only reading from version 2, 3, and 4 is supported. Attempting to read any other version
    /// will return an `InvalidInputError`. 
    pub fn read_from(reader: &mut Reader, version: u8) -> TagResult<Option<(u32, Frame)>> {
        match version {
            0x2 => Frame::read_from_v2(reader),
            0x3 => Frame::read_from_v3(reader),
            0x4 => Frame::read_from_v4(reader),
            _ =>  Err(TagError::new(InvalidInputError, "unsupported id3 tag version"))
        }
    }
    // }}}
  
    // To Bytes {{{
    /// Creates a vector representation of the frame suitable for writing to an ID3v2.2 tag.
    pub fn to_bytes_v2(&self) -> Vec<u8> {
        let contents_bytes = self.contents_to_bytes();
        let contents_size = contents_bytes.len();

        let mut bytes = Vec::with_capacity(3 + 3 + contents_size);
        bytes.extend(String::from_str(self.id.slice_to(3)).into_bytes().into_iter());
        bytes.push_all(util::u32_to_bytes(contents_size as u32).slice(1, 4));
        bytes.extend(contents_bytes.into_iter());
        bytes
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
        bytes.extend(String::from_str(self.id.slice_to(4)).into_bytes().into_iter());
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
    pub fn to_bytes_v4(&self) -> Vec<u8> {
        let mut contents_bytes = self.contents_to_bytes();
        let mut contents_size = contents_bytes.len();
        let contents_decompressed_size = contents_size;

        if self.flags.compression {
            debug!("[{}] compressing frame contents", self.id);
            contents_bytes = flate::deflate_bytes_zlib(contents_bytes.as_slice()).unwrap().as_slice().to_vec();
            contents_size = contents_bytes.len();
        }

        if self.flags.data_length_indicator {
            contents_size += 4;
        }

        let mut bytes = Vec::with_capacity(4 + 4 + 2 + contents_size);
        bytes.extend(String::from_str(self.id.slice_to(4)).into_bytes().into_iter());
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
    pub fn to_bytes(&self, version: u8) -> Vec<u8> {
        match version {
            2 => self.to_bytes_v2(),
            3 => self.to_bytes_v3(),
            4 => self.to_bytes_v4(),
            _ => panic!("no frame encoder for this version") // we shouldn't encouter this
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
            PictureContent(ref picture) => {
                if self.version == 2 {
                    parsers::picture_to_bytes_v2(self.encoding, picture)
                } else {
                    parsers::picture_to_bytes_v3(self.encoding, picture)
                }
            },
            UnknownContent(ref data) => data.clone()
        }
    }
    // }}}

    // Parsing {{{
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
            "APIC" => try!(parsers::parse_apic_v3(choose_data!(decompressed, data))),
            "PIC" => try!(parsers::parse_apic_v2(choose_data!(decompressed, data))),
            "TXXX" | "TXX" => try!(parsers::parse_txxx(choose_data!(decompressed, data))),
            "WXXX" | "WXX" => try!(parsers::parse_wxxx(choose_data!(decompressed, data))),
            "COMM" | "COM" => try!(parsers::parse_comm(choose_data!(decompressed, data))),
            "USLT" | "ULT" => try!(parsers::parse_uslt(choose_data!(decompressed, data))),
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
                    contents = Some(ParserResult::new(encoding::UTF16, UnknownContent(choose_data!(decompressed, data).to_vec())));
                }

                contents.unwrap()
            }
        };

        self.encoding = result.encoding;
        self.contents = result.contents;

        Ok(())
    }

    /// Reparses the frame's data.
    #[inline]
    pub fn reparse(&mut self) -> TagResult<()> {
        let data = self.contents_to_bytes();
        self.parse_data(data.as_slice())
    }
    // }}}

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
    #[inline]
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
    fn test_to_bytes_v2() {
        let id = "TAL";
        let text = "album";
        let encoding = encoding::UTF16;

        let mut frame = Frame::new(id);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(util::string_to_utf16(text).into_iter());

        frame.parse_data(data.as_slice()).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(String::from_str(id).into_bytes().into_iter());
        bytes.push_all(util::u32_to_bytes(data.len() as u32).slice_from(1));
        bytes.extend(data.into_iter());

        assert_eq!(frame.to_bytes_v2(), bytes);
        assert_eq!(frame.to_bytes(2), bytes);
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
