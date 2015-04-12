extern crate num;
use num::FromPrimitive;

/// Types of pictures used in APIC frames.
#[derive(Debug, PartialEq, Clone, Copy)]
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

impl FromPrimitive for PictureType {
    fn from_i64(n: i64) -> Option<PictureType> {
        FromPrimitive::from_u64(n as u64)
    }

    fn from_u64(n: u64) -> Option<PictureType> {
        match n {
            0 => Some(PictureType::Other),
            1 => Some(PictureType::Icon),
            2 => Some(PictureType::OtherIcon),
            3 => Some(PictureType::CoverFront),
            4 => Some(PictureType::CoverBack),
            5 => Some(PictureType::Leaflet),
            6 => Some(PictureType::Media),
            7 => Some(PictureType::LeadArtist),
            8 => Some(PictureType::Artist),
            9 => Some(PictureType::Conductor),
            10 => Some(PictureType::Band),
            11 => Some(PictureType::Composer),
            12 => Some(PictureType::Lyricist),
            13 => Some(PictureType::RecordingLocation),
            14 => Some(PictureType::DuringRecording),
            15 => Some(PictureType::DuringPerformance),
            16 => Some(PictureType::ScreenCapture),
            17 => Some(PictureType::BrightFish),
            18 => Some(PictureType::Illustration),
            19 => Some(PictureType::BandLogo),
            20 => Some(PictureType::PublisherLogo),
            _ => None,
        }
    }
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
