/// Module containing the `PictureType` enum.
pub mod picture_type {
    /// Types of pictures used in APIC frames.
    #[deriving(Show, FromPrimitive, PartialEq, Clone)]
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
}

/// A structure representing an ID3 picture frame's contents.
#[deriving(Show, Clone, PartialEq)]
pub struct Picture {
    /// The picture's MIME type.
    pub mime_type: String,
    /// The type of picture.
    pub picture_type: picture_type::PictureType,
    /// A description of the picture's contents.
    pub description: String,
    /// The image data.
    pub data: Vec<u8>
}

impl Picture {
    /// Creates a new `Picture` with empty values.
    pub fn new() -> Picture {
        Picture { mime_type: String::new(), picture_type: picture_type::Other, description: String::new(), data: Vec::new() } 
    }
}
