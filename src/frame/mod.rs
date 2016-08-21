extern crate std;
extern crate flate2;

use std::io::{Read, Write};
use std::borrow::Cow;

pub use self::encoding::Encoding;
pub use self::content::Content;
pub use self::flags::Flags;
pub use self::picture::{Picture, PictureType};

use self::flate2::read::ZlibDecoder;

use self::stream::{v2, v3, v4};
    
use parsers::{self, DecoderRequest, EncoderRequest};

mod picture;
mod encoding;
mod content;
mod flags;
mod stream;

#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
/// The parsed contents of an extended text frame.
pub struct ExtendedText {
    pub key: String,
    pub value: String
}

#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
/// The parsed contents of an unsynchronized lyrics frame.
pub struct Lyrics {
    pub lang: String,
    pub description: String,
    pub text: String
}

#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
/// The parsed contents of a comment frame.
pub struct Comment {
    pub lang: String,
    pub description: String,
    pub text: String
}

#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
/// The parsed contents of an extended link frame.
pub struct ExtendedLink {
    pub description: String,
    pub link: String
}

/// A structure representing an ID3 frame.
pub struct Frame {
    /// A sequence of 16 bytes used to uniquely identify this frame. 
    pub uuid: Vec<u8>,
    /// The frame identifier.
    pub id: String,
    /// The preferred encoding to be used when converting this frame to bytes.
    pub encoding: Encoding,
    /// The frame flags.
    flags: Flags,
    /// The parsed content of the frame.
    pub content: Content,
    /// The offset of this frame in the file from which it was loaded.
    pub offset: u32,
}

impl PartialEq for Frame {
    fn eq(&self, other: &Frame) -> bool {
        &self.uuid[..] == &other.uuid[..]
    }

    fn ne(&self, other: &Frame) -> bool {
        &self.uuid[..] != &other.uuid[..]
    }
}

impl Frame {
    /// Creates a new ID3v2.3 frame with the specified identifier.
    pub fn new<T: Into<String>>(id: T) -> Frame {
        Frame { 
            uuid: ::util::uuid(), id: id.into(), encoding: Encoding::UTF16, 
            flags: Flags::new(), content: Content::Unknown(Vec::new()), offset: 0 
        }
    }
   
    /// Returns an encoding compatible with the version based on the requested encoding.
    fn compatible_encoding(requested_encoding: Encoding, version: u8) -> Encoding {
        if version < 4 {
            match requested_encoding {
                Encoding::Latin1 => Encoding::Latin1,
                _ => Encoding::UTF16, // if UTF16BE or UTF8 is requested, just return UTF16
            }
        } else {
            requested_encoding
        }
    }

    // Getters/Setters
    /// Returns whether the compression flag is set.
    pub fn compression(&self) -> bool {
        self.flags.compression
    }

    /// Sets the compression flag. 
    pub fn set_compression(&mut self, compression: bool) {
        self.flags.compression = compression;
        self.flags.data_length_indicator = compression;
    }

    /// Returns whether the tag_alter_preservation flag is set.
    pub fn tag_alter_preservation(&self) -> bool {
        self.flags.tag_alter_preservation
    }

    /// Sets the tag_alter_preservation flag.
    pub fn set_tag_alter_preservation(&mut self, tag_alter_preservation: bool) {
        self.flags.tag_alter_preservation = tag_alter_preservation;
    }

    /// Returns whether the file_alter_preservation flag is set.
    pub fn file_alter_preservation(&self) -> bool {
        self.flags.file_alter_preservation
    }

    /// Sets the file_alter_preservation flag.
    pub fn set_file_alter_preservation(&mut self, file_alter_preservation: bool) {
        self.flags.file_alter_preservation = file_alter_preservation;
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
        self.uuid = ::util::uuid();
    }

    /// Attempts to read a frame from the reader.
    ///
    /// Returns a tuple containing the number of bytes read and a frame. If pading is encountered
    /// then `None` is returned.
    ///
    /// Only reading from versions 2, 3, and 4 is supported. Attempting to read any other version
    /// will return an error with kind `UnsupportedVersion`. 
    pub fn read_from(reader: &mut Read, version: u8) -> ::Result<Option<(u32, Frame)>> {
        match version {
            2 => v2::read(reader as &mut Read),
            3 => v3::read(reader),
            4 => v4::read(reader as &mut Read),
            _ =>  Err(::Error::new(::ErrorKind::UnsupportedVersion(version), "unsupported id3 tag version"))
        }
    }

    /// Attempts to write the frame to the writer.
    ///
    /// Returns the number of bytes written.
    ///
    /// Only writing to versions 2, 3, and 4 is supported. Attempting to write using any other
    /// version will return an error with kind `UnsupportedVersion`.
    pub fn write_to(&self, writer: &mut Write, version: u8) -> ::Result<u32> {
        match version {
            2 => v2::write(writer, self),
            3 => v3::write(writer, self),
            4 => v4::write(writer, self),
            _ =>  Err(::Error::new(::ErrorKind::UnsupportedVersion(version), "unsupported id3 tag version"))
        }
    }
  
    /// Creates a vector representation of the content suitable for writing to an ID3 tag.
    pub fn content_to_bytes(&self, version: u8) -> Vec<u8> {
        let request = EncoderRequest { version: version, encoding: Frame::compatible_encoding(self.encoding, version), content: &self.content };
        parsers::encode(request)
    }

    // Parsing {{{
    /// Parses the provided data and sets the `content` field. If the compression flag is set to
    /// true then decompression will be performed.
    ///
    /// Returns `Err` if the data is invalid for the frame type.
    pub fn parse_data(&mut self, data: &[u8]) -> ::Result<()> {
        let decompressed_opt = if self.flags.compression {
            let mut decoder = ZlibDecoder::new(data);
            let mut decompressed = Vec::new();
            try!(decoder.read_to_end(&mut decompressed));
            Some(decompressed)
        } else {
            None
        };

        let result = try!(parsers::decode(DecoderRequest { 
            id: &self.id[..],
            data: match decompressed_opt {
                Some(ref decompressed) => &decompressed[..],
                None => data
            }
        }));

        self.encoding = result.encoding;
        self.content = result.content;

        Ok(())
    }
    // }}}

    /// Returns a string representing the parsed content.
    ///
    /// Returns `None` if the parsed content can not be represented as text.
    ///
    /// # Example
    /// ```
    /// use id3::frame::{self, Frame, Content};
    ///
    /// let mut title_frame = Frame::new("TIT2");
    /// title_frame.content = Content::Text("title".to_owned());
    /// assert_eq!(&title_frame.text().unwrap()[..], "title");
    ///
    /// let mut txxx_frame = Frame::new("TXXX");
    /// txxx_frame.content = Content::ExtendedText(frame::ExtendedText { 
    ///     key: "key".to_owned(), 
    ///     value: "value".to_owned()
    /// });
    /// assert_eq!(&txxx_frame.text().unwrap()[..], "key: value");
    /// ```
    pub fn text(&self) -> Option<Cow<str>> {
        match self.content {
            Content::Text(ref content) => Some(Cow::Borrowed(&content[..])),
            Content::Link(ref content) => Some(Cow::Borrowed(&content[..])), 
            Content::Lyrics(ref content) => Some(Cow::Borrowed(&content.text[..])),
            Content::ExtendedText(ref content) => Some(Cow::Owned(format!("{}: {}", content.key, content.value))), 
            Content::ExtendedLink(ref content) => Some(Cow::Owned(format!("{}: {}", content.description, content.link))), 
            Content::Comment(ref content) => Some(Cow::Owned(format!("{}: {}", content.description, content.text))),
            _ => None
        }
    }
}
 
// Tests {{{
#[cfg(test)]
mod tests {
    use frame::{Frame, Flags, Encoding};
    
    fn u32_to_bytes(n: u32) -> Vec<u8> {
        vec!(((n & 0xFF000000) >> 24) as u8, 
             ((n & 0xFF0000) >> 16) as u8, 
             ((n & 0xFF00) >> 8) as u8, 
             (n & 0xFF) as u8
            )
    }

    #[test]
    fn test_frame_flags_to_bytes_v3() {
        let mut flags = Flags::new();
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
        let mut flags = Flags::new();
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
        let encoding = Encoding::UTF16;

        let mut frame = Frame::new(id);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(::util::string_to_utf16(text).into_iter());

        frame.parse_data(&data[..]).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend((&u32_to_bytes(data.len() as u32)[1..]).iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        frame.write_to(&mut writer, 2).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v3() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut frame = Frame::new(id);

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(::util::string_to_utf16(text).into_iter());

        frame.parse_data(&data[..]).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(data.len() as u32).into_iter());
        bytes.extend([0x00, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        frame.write_to(&mut writer, 3).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn test_to_bytes_v4() {
        let id = "TALB";
        let text = "album";
        let encoding = Encoding::UTF16;

        let mut frame = Frame::new(id);

        frame.flags.tag_alter_preservation = true;
        frame.flags.file_alter_preservation = true; 

        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend(::util::string_to_utf16(text).into_iter());

        frame.parse_data(&data[..]).unwrap();

        let mut bytes = Vec::new();
        bytes.extend(id.bytes());
        bytes.extend(u32_to_bytes(::util::synchsafe(data.len() as u32)).into_iter());
        bytes.extend([0x60, 0x00].iter().cloned());
        bytes.extend(data.into_iter());

        let mut writer = Vec::new();
        frame.write_to(&mut writer, 4).unwrap();
        assert_eq!(writer, bytes);
    }
}
