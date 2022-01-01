use crate::{Error, ErrorKind};
use std::cmp;
use std::fs;
use std::io::{self, Read, Seek};
use std::ops;
use std::path::Path;

/// Location of the ID3v1 tag chunk relative to the end of the file.
static TAG_CHUNK: ops::Range<i64> = -128..0;
/// Location of the ID3v1 extended tag chunk relative to the end of the file.
static XTAG_CHUNK: ops::Range<i64> = -355..-128;

static GENRE_LIST: &[&str] = &[
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
    "Fusion",
    "Trance",
    "Classical",
    "Instrumental",
    "Acid",
    "House",
    "Game",
    "Sound Clip",
    "Gospel",
    "Noise",
    "Alternative Rock",
    "Bass",
    "Soul",
    "Punk",
    "Space",
    "Meditative",
    "Instrumental Pop",
    "Instrumental Rock",
    "Ethnic",
    "Gothic",
    "Darkwave",
    "Techno-Industrial",
    "Electronic",
    "Pop-Folk",
    "Eurodance",
    "Dream",
    "Southern Rock",
    "Comedy",
    "Cult",
    "Gangsta",
    "Top 40",
    "Christian Rap",
    "Pop/Funk",
    "Jungle",
    "Native US",
    "Cabaret",
    "New Wave",
    "Psychadelic",
    "Rave",
    "Showtunes",
    "Trailer",
    "Lo-Fi",
    "Tribal",
    "Acid Punk",
    "Acid Jazz",
    "Polka",
    "Retro",
    "Musical",
    "Rock & Roll",
    "Hard Rock",
    "Folk",
    "Folk-Rock",
    "National Folk",
    "Swing",
    "Fast Fusion",
    "Bebob",
    "Latin",
    "Revival",
    "Celtic",
    "Bluegrass",
    "Avantgarde",
    "Gothic Rock",
    "Progressive Rock",
    "Psychedelic Rock",
    "Symphonic Rock",
    "Slow Rock",
    "Big Band",
    "Chorus",
    "Easy Listening",
    "Acoustic",
    "Humour",
    "Speech",
    "Chanson",
    "Opera",
    "Chamber Music",
    "Sonata",
    "Symphony",
    "Booty Bass",
    "Primus",
    "Porn Groove",
    "Satire",
    "Slow Jam",
    "Club",
    "Tango",
    "Samba",
    "Folklore",
    "Ballad",
    "Power Ballad",
    "Rhytmic Soul",
    "Freestyle",
    "Duet",
    "Punk Rock",
    "Drum Solo",
    "Acapella",
    "Euro-House",
    "Dance Hall",
    "Goa",
    "Drum & Bass",
    "Club-House",
    "Hardcore",
    "Terror",
    "Indie",
    "BritPop",
    "Negerpunk",
    "Polsk Punk",
    "Beat",
    "Christian Gangsta",
    "Heavy Metal",
    "Black Metal",
    "Crossover",
    "Contemporary C",
    "Christian Rock",
    "Merengue",
    "Salsa",
    "Thrash Metal",
    "Anime",
    "JPop",
    "SynthPop",
];

/// A structure containing ID3v1 metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tag {
    /// The full title (ID3v1 + extension if present).
    pub title: String,
    /// The full artist (ID3v1 + extension if present).
    pub artist: String,
    /// The full album (ID3v1 + extension if present).
    pub album: String,
    /// The release year as four digits.
    ///
    /// The ID3v1 format can only represent values between 0 and 9999 inclusive.
    pub year: String,
    /// A free-form comment.
    pub comment: String,
    /// Number of the track. ID3v1.1 data.
    pub track: Option<u8>,
    /// The genre mapping is standardized up to 79, altough this implementation uses the Winamp
    /// extended genre list:
    /// https://de.wikipedia.org/wiki/Liste_der_ID3v1-Genres
    pub genre_id: u8,

    /// 1 (slow), 2, 3, 4 (fast) or None when not set. ID3v1 extended data.
    pub speed: Option<u8>,
    /// Free-form genre string. ID3v1 extended data.
    pub genre_str: Option<String>,
    /// The real start of the track, mmm:ss. ID3v1 extended data.
    pub start_time: Option<String>,
    /// The real end of the track, mmm:ss. ID3v1 extended data.
    pub end_time: Option<String>,
}

impl Tag {
    /// Creates a new empty ID3v1 tag.
    pub fn new() -> Tag {
        Tag::default()
    }

    /// Checks whether the reader contains an ID3v1 tag.
    ///
    /// The reader position will be reset back to the previous position before returning.
    pub fn is_candidate(mut reader: impl io::Read + io::Seek) -> crate::Result<bool> {
        let initial_position = reader.seek(io::SeekFrom::Current(0))?;
        reader.seek(io::SeekFrom::End(TAG_CHUNK.start))?;
        let mut buf = [0; 3];
        let nread = reader.read(&mut buf)?;
        reader.seek(io::SeekFrom::Start(initial_position))?;
        Ok(&buf[..nread] == b"TAG")
    }

    /// Seeks to and reads a ID3v1 tag from the reader.
    pub fn read_from(mut reader: impl io::Read + io::Seek) -> crate::Result<Tag> {
        let mut tag_buf = [0; 355];
        let file_len = reader.seek(io::SeekFrom::End(0))?;
        if file_len >= XTAG_CHUNK.start.abs() as u64 {
            reader.seek(io::SeekFrom::End(XTAG_CHUNK.start))?;
            reader.read_exact(&mut tag_buf)?;
        } else if file_len >= TAG_CHUNK.start.abs() as u64 {
            let l = tag_buf.len() as i64;
            reader.seek(io::SeekFrom::End(TAG_CHUNK.start))?;
            reader.read_exact(&mut tag_buf[(l + TAG_CHUNK.start) as usize..])?;
        } else {
            return Err(Error::new(
                ErrorKind::NoTag,
                "the file is too small to contain an ID3v1 tag",
            ));
        }

        let (tag, xtag) = {
            let (xtag, tag) = (&tag_buf[..227], &tag_buf[227..]);
            if &tag[0..3] != b"TAG" {
                return Err(Error::new(ErrorKind::NoTag, "no ID3v1 tag was found"));
            }
            (
                tag,
                if &xtag[0..4] == b"TAG+" {
                    Some(xtag)
                } else {
                    None
                },
            )
        };

        // Decodes a string consisting out of a base and possible extension to a String.
        // The input are one or two null-terminated ISO-8859-1 byte slices.
        fn decode_str(base: &[u8], ext: Option<&[u8]>) -> String {
            base.iter()
                .take_while(|c| **c != 0)
                .chain({
                    ext.into_iter()
                        .flat_map(|s| s.iter())
                        .take_while(|c| **c != 0)
                })
                // This works because the ISO 8859-1 code points match the unicode code
                // points. So,`c as char` will map correctly from ISO to unicode.
                .map(|c| *c as char)
                .collect()
        }
        let title = decode_str(&tag[3..33], xtag.as_ref().map(|t| &t[4..64]));
        let artist = decode_str(&tag[33..63], xtag.as_ref().map(|t| &t[64..124]));
        let album = decode_str(&tag[63..93], xtag.as_ref().map(|t| &t[124..184]));
        let year = decode_str(&tag[93..97], None);
        let (track, comment_raw) = if tag[125] == 0 && tag[126] != 0 {
            (Some(tag[126]), &tag[97..125])
        } else {
            (None, &tag[97..127])
        };
        let comment = decode_str(comment_raw, None);
        let genre_id = tag[127];
        let (speed, genre_str, start_time, end_time) = if let Some(xt) = xtag {
            let speed = if xt[184] == 0 { None } else { Some(xt[184]) };
            let genre_str = decode_str(&xt[185..215], None);
            let start_time = decode_str(&xt[185..215], None);
            let end_time = decode_str(&xt[185..215], None);
            (speed, Some(genre_str), Some(start_time), Some(end_time))
        } else {
            (None, None, None, None)
        };

        Ok(Tag {
            title,
            artist,
            album,
            year,
            comment,
            track,
            genre_id,
            speed,
            genre_str,
            start_time,
            end_time,
        })
    }

    /// Attempts to read an ID3v1 tag from the file at the indicated path.
    pub fn read_from_path(path: impl AsRef<Path>) -> crate::Result<Tag> {
        let file = fs::File::open(path)?;
        Tag::read_from(file)
    }

    /// Removes an ID3v1 tag plus possible extended data if any.
    ///
    /// The file cursor position will be reset back to the previous position before returning.
    ///
    /// Returns true if the file initially contained a tag.
    pub fn remove(file: &mut fs::File) -> crate::Result<bool> {
        let cur_pos = file.seek(io::SeekFrom::Current(0))?;
        let file_len = file.metadata()?.len();
        let has_ext_tag = if file_len >= XTAG_CHUNK.start.abs() as u64 {
            file.seek(io::SeekFrom::End(XTAG_CHUNK.start))?;
            let mut b = [0; 4];
            file.read_exact(&mut b)?;
            &b == b"TAG+"
        } else {
            false
        };
        let has_tag = if file_len >= TAG_CHUNK.start.abs() as u64 {
            file.seek(io::SeekFrom::End(TAG_CHUNK.start))?;
            let mut b = [0; 3];
            file.read_exact(&mut b)?;
            &b == b"TAG"
        } else {
            false
        };

        let truncate_to = if has_ext_tag && has_tag {
            Some(file_len - XTAG_CHUNK.start.abs() as u64)
        } else if has_tag {
            Some(file_len - TAG_CHUNK.start.abs() as u64)
        } else {
            None
        };
        file.seek(io::SeekFrom::Start(cmp::min(
            truncate_to.unwrap_or(cur_pos),
            cur_pos,
        )))?;
        if let Some(l) = truncate_to {
            file.set_len(l)?;
        }
        Ok(truncate_to.is_some())
    }

    /// Returns `genre_str`, falling back to translating `genre_id` to a string.
    pub fn genre(&self) -> Option<&str> {
        if let Some(ref g) = self.genre_str {
            if !g.is_empty() {
                return Some(g.as_str());
            }
        }
        GENRE_LIST.get(self.genre_id as usize).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn read_id3v1() {
        let file = fs::File::open("testdata/id3v1.id3").unwrap();
        let tag = Tag::read_from(file).unwrap();
        assert_eq!("Title", tag.title);
        assert_eq!("Artist", tag.artist);
        assert_eq!("Album", tag.album);
        assert_eq!("2017", tag.year);
        assert_eq!("Comment", tag.comment);
        assert_eq!(Some(1), tag.track);
        assert_eq!(31, tag.genre_id);
        assert_eq!("Trance", tag.genre().unwrap());
        assert!(tag.speed.is_none());
        assert!(tag.genre_str.is_none());
        assert!(tag.start_time.is_none());
        assert!(tag.end_time.is_none());
    }

    #[test]
    fn remove_id3v1() {
        let tmp = tempdir().unwrap();
        let tmp_name = tmp.path().join("remove_id3v1_tag");
        {
            let mut tag_file = fs::File::create(&tmp_name).unwrap();
            let mut original = fs::File::open("testdata/id3v1.id3").unwrap();
            io::copy(&mut original, &mut tag_file).unwrap();
        }
        let mut tag_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&tmp_name)
            .unwrap();
        tag_file.seek(io::SeekFrom::Start(0)).unwrap();
        assert!(Tag::remove(&mut tag_file).unwrap());
        tag_file.seek(io::SeekFrom::Start(0)).unwrap();
        assert!(!Tag::remove(&mut tag_file).unwrap());
    }
}
