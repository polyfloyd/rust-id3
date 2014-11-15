extern crate std;
extern crate audiotag;
extern crate flate;

pub use self::contents::{Contents, TextContent, ExtendedTextContent, LinkContent, ExtendedLinkContent, CommentContent, LyricsContent, PictureContent, UnknownContent};
pub use self::flags::FrameFlags;

use self::stream::{FrameStream, FrameV2, FrameV3, FrameV4};
    
use self::audiotag::{TagError, TagResult, InvalidInputError};

use util;
use parsers;
use parsers::{DecoderRequest, EncoderRequest};

/// A module containing the `Encoding` enum. 
pub mod encoding;

mod contents;
mod flags;
mod stream;

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
    pub fn new<T: StrAllocating>(id: T) -> Frame {
        Frame { 
            uuid: util::uuid(), id: id.into_string(), version: 3, encoding: encoding::UTF16, 
            flags: FrameFlags::new(), contents: UnknownContent(Vec::new()), offset: 0 
        }
    }
    
    /// Creates a new frame with the specified identifier and version.
    ///
    /// # Example
    /// ```
    /// use id3::Frame;
    ///
    /// let frame = Frame::with_version("TALB".into_string(), 4);
    /// assert_eq!(frame.version(), 4);
    /// ```
    pub fn with_version<T: StrAllocating>(id: T, version: u8) -> Frame {
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
    /// let frame = Frame::with_version("USLT".into_string(), 4);
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
                Some(id) => id.into_string(),
                None => {
                    debug!("no ID3v2.3 to ID3v2.3 mapping for {}", self.id);
                    return false;
                }
            }
        } else if self.version == 2 && (version == 3 || version == 4) {
            // attempt to convert the id
            self.id = match util::convert_id_2_to_3(self.id.as_slice()) {
                Some(id) => id.into_string(),
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

    /// Generates a new uuid for this frame.
    ///
    /// # Example
    /// ```
    /// use id3::Frame;
    ///
    /// let mut frame = Frame::new("TYER".into_string());
    /// let prev_uuid = frame.uuid.clone();
    /// frame.generate_uuid();
    /// assert!(prev_uuid != frame.uuid);
    /// ```
    #[inline]
    pub fn generate_uuid(&mut self) {
        self.uuid = util::uuid();
    }

    /// Attempts to read a frame from the reader.
    ///
    /// Returns a tuple containing the number of bytes read and a frame. If pading is encountered
    /// then `None` is returned.
    ///
    /// Only reading from version 2, 3, and 4 is supported. Attempting to read any other version
    /// will return an `InvalidInputError`. 
    #[inline]
    pub fn read_from(reader: &mut Reader, version: u8) -> TagResult<Option<(u32, Frame)>> {
        match version {
            2 => FrameStream::read(reader, None::<FrameV2>),
            3 => FrameStream::read(reader, None::<FrameV3>),
            4 => FrameStream::read(reader, None::<FrameV4>),
            _ =>  Err(TagError::new(InvalidInputError, "unsupported id3 tag version"))
        }
    }

    /// Attempts to write the frame to the writer.
    #[inline]
    pub fn write_to(&self, writer: &mut Writer) -> TagResult<u32> {
        match self.version {
            2 => FrameStream::write(writer, self, None::<FrameV2>),
            3 => FrameStream::write(writer, self, None::<FrameV3>),
            4 => FrameStream::write(writer, self, None::<FrameV4>),
            _ =>  Err(TagError::new(InvalidInputError, "unsupported id3 tag version"))
        }
    }
  
    /// Creates a vector representation of the contents suitable for writing to an ID3 tag.
    #[inline]
    pub fn contents_to_bytes(&self) -> Vec<u8> {
        let request = EncoderRequest { version: self.version, encoding: self.encoding, contents: &self.contents };
        parsers::encode(request)
        
    }

    // Parsing {{{
    /// Parses the provided data and sets the `contents` field. If the compression flag is set to
    /// true then decompression will be performed.
    ///
    /// Returns `Err` if the data is invalid for the frame type.
    pub fn parse_data(&mut self, data: &[u8]) -> TagResult<()> {
        let decompressed_opt = if self.flags.compression {
            Some(flate::inflate_bytes_zlib(data).unwrap())
        } else {
            None
        };

        let result = try!(parsers::decode(DecoderRequest { 
            id: self.id.as_slice(), 
            data: match decompressed_opt {
                Some(ref decompressed) => decompressed.as_slice(),
                None => data
            }
        }));

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
    /// let mut title_frame = Frame::new("TIT2".into_string());
    /// title_frame.contents = TextContent("title".into_string());
    /// assert_eq!(title_frame.text().unwrap().as_slice(), "title");
    ///
    /// let mut txxx_frame = Frame::new("TXXX".into_string());
    /// txxx_frame.contents = ExtendedTextContent(("key".into_string(), "value".into_string())); 
    /// assert_eq!(txxx_frame.text().unwrap().as_slice(), "key: value");
    /// ```
    pub fn text(&self) -> Option<String> {
        match self.contents {
            TextContent(ref text) 
                | LinkContent(ref text) 
                | LyricsContent((_, ref text)) => Some(text.clone()),
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
 
// Tests {{{
#[cfg(test)]
mod tests {
    use std::io::MemWriter;
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

        let mut frame = Frame::with_version(id.into_string(), 2);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(util::string_to_utf16(text).into_iter());

        frame.parse_data(data.as_slice()).unwrap();

        let mut bytes = Vec::new();
        bytes.push_all(id.as_bytes());
        bytes.push_all(util::u32_to_bytes(data.len() as u32).slice_from(1));
        bytes.extend(data.into_iter());

        let mut writer = MemWriter::new();
        frame.write_to(&mut writer).unwrap();
        assert_eq!(writer.unwrap(), bytes);
    }

    #[test]
    fn test_to_bytes_v3() {
        let id = "TALB";
        let text = "album";
        let encoding = encoding::UTF16;

        let mut frame = Frame::with_version(id.into_string(), 4);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(util::string_to_utf16(text).into_iter());

        frame.parse_data(data.as_slice()).unwrap();

        let mut bytes = Vec::new();
        bytes.push_all(id.as_bytes());
        bytes.extend(util::u32_to_bytes(data.len() as u32).into_iter());
        bytes.push_all([0x00, 0x00]);
        bytes.extend(data.into_iter());

        let mut writer = MemWriter::new();
        frame.write_to(&mut writer).unwrap();
        assert_eq!(writer.unwrap(), bytes);
    }

    #[test]
    fn test_to_bytes_v4() {
        let id = "TALB";
        let text = "album";
        let encoding = encoding::UTF16;

        let mut frame = Frame::with_version(id.into_string(), 4);

        frame.flags.tag_alter_preservation = true;
        frame.flags.file_alter_preservation = true; 

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(util::string_to_utf16(text).into_iter());

        frame.parse_data(data.as_slice()).unwrap();

        let mut bytes = Vec::new();
        bytes.push_all(id.as_bytes());
        bytes.extend(util::u32_to_bytes(util::synchsafe(data.len() as u32)).into_iter());
        bytes.push_all([0x60, 0x00]);
        bytes.extend(data.into_iter());

        let mut writer = MemWriter::new();
        frame.write_to(&mut writer).unwrap();
        assert_eq!(writer.unwrap(), bytes);
    }
}
