/// Flags used in ID3 frames.
#[derive(Copy, Clone)]
pub struct Flags {
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

impl Flags {
    /// Returns a new `Flags` with all flags set to false.
    pub fn new() -> Flags {
        Flags { 
            tag_alter_preservation: false, file_alter_preservation: false, read_only: false, compression: false, 
            encryption: false, grouping_identity: false, unsynchronization: false, data_length_indicator: false 
        }
    }

    /// Returns a vector representation suitable for writing to a file containing an ID3v2.3
    /// tag.
    fn to_bytes_v3(&self) -> Vec<u8> {
        let mut bytes = [0; 2];

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
    fn to_bytes_v4(&self) -> Vec<u8> {
        let mut bytes = [0; 2];

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
            3 => self.to_bytes_v3(),
            4 => self.to_bytes_v4(),
            _ => [0; 2].to_vec()
        }
    }
}
