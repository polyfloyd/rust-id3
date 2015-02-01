/// Types of pictures used in APIC frames.
#[derive(Debug, FromPrimitive, PartialEq, Clone, Copy)]
#[allow(missing_docs)]
pub enum PictureType {
    Other,
    Icon,
    OtherIcon,
    CoverFront,
    CoverBack,
    Leaflet,
    Media,
    LeadArtist,
    Artist,
    Conductor,
    Band,
    Composer,
    Lyricist,
    RecordingLocation,
    DuringRecording,
    DuringPerformance,
    ScreenCapture,
    BrightFish,
    Illustration,
    BandLogo,
    PublisherLogo
}

/// A structure representing an ID3 picture frame's contents.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    /// The picture's MIME type.
    pub mime_type: String,
    /// The type of picture.
    pub picture_type: PictureType,
    /// A description of the picture's contents.
    pub description: String,
    /// The image data.
    pub data: Vec<u8>
}

impl Picture {
    /// Creates a new `Picture` with empty values.
    pub fn new() -> Picture {
        Picture { mime_type: String::new(), picture_type: PictureType::Other, description: String::new(), data: Vec::new() } 
    }
}
