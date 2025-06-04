use crate::chunk;
use crate::frame::{
    Chapter, Comment, EncapsulatedObject, ExtendedLink, ExtendedText, Frame, InvolvedPeopleList,
    Lyrics, Picture, SynchronisedLyrics, TableOfContents, UniqueFileIdentifier,
};
use crate::storage::{plain::PlainStorage, Format, Storage};
use crate::stream;
use crate::taglike::TagLike;
use crate::v1;
use crate::StorageFile;
use crate::{Error, ErrorKind};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::iter::{FromIterator, Iterator};
use std::path::Path;

/// Denotes the version of a tag.
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Version {
    /// ID3v2.2
    Id3v22,
    /// ID3v2.3
    Id3v23,
    /// ID3v2.4
    #[default]
    Id3v24,
}

impl Version {
    /// Returns the minor version.
    ///
    /// # Example
    /// ```
    /// use id3::Version;
    ///
    /// assert_eq!(Version::Id3v24.minor(), 4);
    /// ```
    pub fn minor(self) -> u8 {
        match self {
            Version::Id3v22 => 2,
            Version::Id3v23 => 3,
            Version::Id3v24 => 4,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Version::Id3v22 => write!(f, "ID3v2.2"),
            Version::Id3v23 => write!(f, "ID3v2.3"),
            Version::Id3v24 => write!(f, "ID3v2.4"),
        }
    }
}

/// An ID3 tag containing zero or more [`Frame`]s.
#[derive(Clone, Debug, Default, Eq)]
pub struct Tag {
    /// A vector of frames included in the tag.
    frames: Vec<Frame>,
    /// ID3 Tag version
    version: Version,
}

impl<'a> Tag {
    /// Creates a new ID3v2.4 tag with no frames.
    pub fn new() -> Tag {
        Tag::default()
    }

    /// Used for creating new tag with a specific version.
    pub fn with_version(version: Version) -> Tag {
        Tag {
            version,
            ..Tag::default()
        }
    }

    // Read/write functions are declared below. We adhere to the following naming conventions:
    // * <format> -> io::Read/io::Write (+ io::Seek?)
    // * <format>_path -> impl AsRef<Path>
    // * <format>_file -> &mut File

    /// Will return true if the reader is a candidate for an ID3 tag. The reader position will be
    /// reset back to the previous position before returning.
    pub fn is_candidate(mut reader: impl io::Read + io::Seek) -> crate::Result<bool> {
        let initial_position = reader.stream_position()?;
        let is_candidate = match stream::tag::locate_id3v2(&mut reader) {
            Ok(_) => true,
            Err(Error {
                kind: ErrorKind::NoTag,
                ..
            }) => false,
            Err(err) => return Err(err),
        };
        reader.seek(io::SeekFrom::Start(initial_position))?;
        Ok(is_candidate)
    }

    /// Detects the presence of an ID3v2 tag at the current position of the reader and skips it
    /// if is found. Returns true if a tag was found.
    pub fn skip(mut reader: impl io::Read + io::Seek) -> crate::Result<bool> {
        let initial_position = reader.stream_position()?;
        let range = match stream::tag::locate_id3v2(&mut reader) {
            Ok(v) => v,
            Err(Error {
                kind: ErrorKind::NoTag,
                ..
            }) => return Ok(false),
            Err(err) => return Err(err),
        };
        reader.seek(io::SeekFrom::Start(initial_position + range.end))?;
        Ok(true)
    }

    /// Removes an ID3v2 tag from the file at the specified path.
    ///
    /// Returns true if the file initially contained a tag.
    pub fn remove_from_path(path: impl AsRef<Path>) -> crate::Result<bool> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(path)?;
        Self::remove_from_file(&mut file)
    }

    /// Removes an ID3v2 tag from the specified file.
    ///
    /// Returns true if the file initially contained a tag.
    pub fn remove_from_file(mut file: impl StorageFile) -> crate::Result<bool> {
        let location = match stream::tag::locate_id3v2(&mut file) {
            Ok(l) => l,
            Err(Error {
                kind: ErrorKind::NoTag,
                ..
            }) => return Ok(false),
            Err(err) => return Err(err),
        };
        // Open the ID3 region for writing and write nothing. This removes the region in its
        // entirety.
        let mut storage = PlainStorage::new(file, location);
        storage.writer()?.flush()?;
        Ok(true)
    }

    /// Attempts to read an ID3 tag from the reader.
    #[deprecated(note = "use read_from2")]
    pub fn read_from(reader: impl io::Read) -> crate::Result<Tag> {
        stream::tag::decode(reader)
    }

    /// Attempts to read an ID3 tag from the reader.
    ///
    /// The file format is detected using header magic.
    ///
    /// In the case of both Aiff/Wav tags and a ID3 header being present, the header takes
    /// precense.
    pub fn read_from2(reader: impl io::Read + io::Seek) -> crate::Result<Tag> {
        let mut b = BufReader::new(reader);
        let probe = b.fill_buf()?;

        match Format::magic(probe) {
            Some(Format::Header) | None => stream::tag::decode(b),
            Some(Format::Aiff) => chunk::load_id3_chunk::<chunk::AiffFormat, _>(b),
            Some(Format::Wav) => chunk::load_id3_chunk::<chunk::WavFormat, _>(b),
        }
    }

    /// Attempts to read an ID3 tag via Tokio from the reader.
    #[cfg(feature = "tokio")]
    pub async fn async_read_from(
        reader: impl tokio::io::AsyncRead + std::marker::Unpin,
    ) -> crate::Result<Tag> {
        stream::tag::async_decode(reader).await
    }

    /// Attempts to read an ID3 tag from the file at the indicated path.
    pub fn read_from_path(path: impl AsRef<Path>) -> crate::Result<Tag> {
        Tag::read_from2(File::open(path)?)
    }

    /// Attempts to read an ID3 tag via Tokio from the file at the indicated path.
    #[cfg(feature = "tokio")]
    pub async fn async_read_from_path(path: impl AsRef<Path>) -> crate::Result<Tag> {
        let file = tokio::io::BufReader::new(tokio::fs::File::open(path).await?);
        stream::tag::async_decode(file).await
    }

    /// Reads an AIFF stream and returns any present ID3 tag.
    #[deprecated(note = "use read_from")]
    pub fn read_from_aiff(reader: impl io::Read + io::Seek) -> crate::Result<Tag> {
        chunk::load_id3_chunk::<chunk::AiffFormat, _>(reader)
    }

    /// Reads an AIFF file at the specified path and returns any present ID3 tag.
    #[deprecated(note = "use read_from_path")]
    pub fn read_from_aiff_path(path: impl AsRef<Path>) -> crate::Result<Tag> {
        let mut file = BufReader::new(File::open(path)?);
        chunk::load_id3_chunk::<chunk::AiffFormat, _>(&mut file)
    }

    /// Reads an AIFF file and returns any present ID3 tag.
    #[deprecated(note = "use read_from_file")]
    pub fn read_from_aiff_file(file: impl StorageFile) -> crate::Result<Tag> {
        chunk::load_id3_chunk::<chunk::AiffFormat, _>(file)
    }

    /// Reads an WAV stream and returns any present ID3 tag.
    #[deprecated(note = "use read_from")]
    pub fn read_from_wav(reader: impl io::Read + io::Seek) -> crate::Result<Tag> {
        chunk::load_id3_chunk::<chunk::WavFormat, _>(reader)
    }

    /// Reads an WAV file at the specified path and returns any present ID3 tag.
    #[deprecated(note = "use read_from_path")]
    pub fn read_from_wav_path(path: impl AsRef<Path>) -> crate::Result<Tag> {
        let mut file = BufReader::new(File::open(path)?);
        chunk::load_id3_chunk::<chunk::WavFormat, _>(&mut file)
    }

    /// Reads an WAV file and returns any present ID3 tag.
    #[deprecated(note = "use read_from_file")]
    pub fn read_from_wav_file(file: impl StorageFile) -> crate::Result<Tag> {
        chunk::load_id3_chunk::<chunk::WavFormat, _>(file)
    }

    /// Attempts to write the ID3 tag to the writer using the specified version.
    ///
    /// Note that the plain tag is written, regardless of the original contents. To safely encode a
    /// tag to an MP3 file, use `Tag::write_to_file`.
    pub fn write_to(&self, writer: impl io::Write, version: Version) -> crate::Result<()> {
        stream::tag::Encoder::new()
            .version(version)
            .encode(self, writer)
    }

    /// Attempts to write the ID3 tag from the file at the indicated path. If the specified path is
    /// the same path which the tag was read from, then the tag will be written to the padding if
    /// possible.
    pub fn write_to_file(&self, file: impl StorageFile, version: Version) -> crate::Result<()> {
        stream::tag::Encoder::new()
            .version(version)
            .write_to_file(self, file)?;
        Ok(())
    }

    /// Conventience function for [`write_to_file`].
    pub fn write_to_path(&self, path: impl AsRef<Path>, version: Version) -> crate::Result<()> {
        let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        self.write_to_file(file, version)
    }

    /// Overwrite WAV file ID3 chunk in a file
    #[deprecated(note = "use write_to_path")]
    pub fn write_to_aiff_path(
        &self,
        path: impl AsRef<Path>,
        version: Version,
    ) -> crate::Result<()> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(path)?;
        chunk::write_id3_chunk_file::<chunk::AiffFormat>(&mut file, self, version)?;
        file.flush()?;
        Ok(())
    }

    /// Overwrite AIFF file ID3 chunk in a file. The file must be opened read/write.
    #[deprecated(note = "use write_to_file")]
    pub fn write_to_aiff_file(
        &self,
        file: impl StorageFile,
        version: Version,
    ) -> crate::Result<()> {
        chunk::write_id3_chunk_file::<chunk::AiffFormat>(file, self, version)
    }

    /// Overwrite WAV file ID3 chunk
    #[deprecated(note = "use write_to_path")]
    pub fn write_to_wav_path(&self, path: impl AsRef<Path>, version: Version) -> crate::Result<()> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .truncate(false)
            .open(path)?;
        chunk::write_id3_chunk_file::<chunk::WavFormat>(&mut file, self, version)?;
        file.flush()?;
        Ok(())
    }

    /// Overwrite AIFF file ID3 chunk in a file. The file must be opened read/write.
    #[deprecated(note = "use write_to_file")]
    pub fn write_to_wav_file(&self, file: impl StorageFile, version: Version) -> crate::Result<()> {
        chunk::write_id3_chunk_file::<chunk::WavFormat>(file, self, version)
    }

    /// Returns version of the read tag.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Returns an iterator over the all frames in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Content, Frame, Tag, TagLike};
    ///
    /// let mut tag = Tag::new();
    ///
    /// tag.add_frame(Frame::with_content("TPE1", Content::Text("".to_string())));
    /// tag.add_frame(Frame::with_content("APIC", Content::Text("".to_string())));
    ///
    /// assert_eq!(tag.frames().count(), 2);
    /// ```
    pub fn frames(&'a self) -> impl Iterator<Item = &'a Frame> + 'a {
        self.frames.iter()
    }

    /// Returns an iterator over the extended texts in the tag.
    pub fn extended_texts(&'a self) -> impl Iterator<Item = &'a ExtendedText> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().extended_text())
    }

    /// Returns an iterator over the extended links in the tag.
    pub fn extended_links(&'a self) -> impl Iterator<Item = &'a ExtendedLink> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().extended_link())
    }

    /// Returns an iterator over the [General Encapsulated Object (GEOB)](https://id3.org/id3v2.3.0#General_encapsulated_object) frames in the tag.
    pub fn encapsulated_objects(&'a self) -> impl Iterator<Item = &'a EncapsulatedObject> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().encapsulated_object())
    }
    /// Returns an iterator over the comments in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::{Content, Comment};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let frame = Frame::with_content("COMM", Content::Comment(Comment {
    ///     lang: "eng".to_owned(),
    ///     description: "key1".to_owned(),
    ///     text: "value1".to_owned()
    /// }));
    /// tag.add_frame(frame);
    ///
    /// let frame = Frame::with_content("COMM", Content::Comment(Comment {
    ///     lang: "eng".to_owned(),
    ///     description: "key2".to_owned(),
    ///     text: "value2".to_owned()
    /// }));
    /// tag.add_frame(frame);
    ///
    /// assert_eq!(tag.comments().count(), 2);
    /// ```
    pub fn comments(&'a self) -> impl Iterator<Item = &'a Comment> + 'a {
        self.frames().filter_map(|frame| frame.content().comment())
    }

    /// Returns an iterator over the lyrics frames in the tag.
    pub fn lyrics(&'a self) -> impl Iterator<Item = &'a Lyrics> + 'a {
        self.frames().filter_map(|frame| frame.content().lyrics())
    }

    /// Returns an iterator over the synchronised lyrics frames in the tag.
    pub fn synchronised_lyrics(&'a self) -> impl Iterator<Item = &'a SynchronisedLyrics> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().synchronised_lyrics())
    }

    /// Returns an iterator over the pictures in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::{Content, Picture, PictureType};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let picture = Picture {
    ///     mime_type: String::new(),
    ///     picture_type: PictureType::Other,
    ///     description: String::new(),
    ///     data: Vec::new(),
    /// };
    /// tag.add_frame(Frame::with_content("APIC", Content::Picture(picture.clone())));
    /// tag.add_frame(Frame::with_content("APIC", Content::Picture(picture.clone())));
    ///
    /// assert_eq!(tag.pictures().count(), 1);
    /// ```
    pub fn pictures(&'a self) -> impl Iterator<Item = &'a Picture> + 'a {
        self.frames().filter_map(|frame| frame.content().picture())
    }

    /// Returns an iterator over the Unique File Identifiers (ufid) in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Frame, Tag, TagLike};
    /// use id3::frame::{Content, UniqueFileIdentifier};
    ///
    /// let mut tag = Tag::new();
    ///
    /// let unique_file_identifier = UniqueFileIdentifier {
    ///     owner_identifier: String::from("http://www.id3.org/dummy/ufid.html"),
    ///     identifier: "7FZo5fMqyG5Ys1dm8F1FHa".into(),
    /// };
    /// tag.add_frame(Frame::with_content("UFID", Content::UniqueFileIdentifier(unique_file_identifier.clone())));
    /// tag.add_frame(Frame::with_content("UFID", Content::UniqueFileIdentifier(unique_file_identifier.clone())));
    ///
    /// assert_eq!(tag.unique_file_identifiers().count(), 1);
    /// ```
    pub fn unique_file_identifiers(
        &'a self,
    ) -> impl Iterator<Item = &'a UniqueFileIdentifier> + 'a {
        self.frames()
            .filter_map(|frame| frame.content().unique_file_identifier())
    }

    /// Returns an iterator over all chapters (CHAP) in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{Chapter, Content, Frame};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Chapter{
    ///     element_id: "01".to_string(),
    ///     start_time: 1000,
    ///     end_time: 2000,
    ///     start_offset: 0xff,
    ///     end_offset: 0xff,
    ///     frames: Vec::new(),
    /// });
    /// tag.add_frame(Chapter{
    ///     element_id: "02".to_string(),
    ///     start_time: 2000,
    ///     end_time: 3000,
    ///     start_offset: 0xff,
    ///     end_offset: 0xff,
    ///     frames: Vec::new(),
    /// });
    /// assert_eq!(2, tag.chapters().count());
    /// ```
    pub fn chapters(&self) -> impl Iterator<Item = &Chapter> {
        self.frames().filter_map(|frame| frame.content().chapter())
    }

    /// Returns an iterator over all tables of contents (CTOC) in the tag.
    ///
    /// # Example
    /// ```
    /// use id3::{Tag, TagLike};
    /// use id3::frame::{Chapter, TableOfContents, Content, Frame};
    ///
    /// let mut tag = Tag::new();
    /// tag.add_frame(Chapter{
    ///     element_id: "chap01".to_string(),
    ///     start_time: 1000,
    ///     end_time: 2000,
    ///     start_offset: 0xff,
    ///     end_offset: 0xff,
    ///     frames: Vec::new(),
    /// });
    /// tag.add_frame(TableOfContents{
    ///     element_id: "internalTable01".to_string(),
    ///     top_level: false,
    ///     ordered: false,
    ///     elements: Vec::new(),
    ///     frames: Vec::new(),
    /// });
    /// tag.add_frame(TableOfContents{
    ///     element_id: "01".to_string(),
    ///     top_level: true,
    ///     ordered: true,
    ///     elements: vec!["internalTable01".to_string(),"chap01".to_string()],
    ///     frames: Vec::new(),
    /// });
    /// assert_eq!(2, tag.tables_of_contents().count());
    /// ```
    pub fn tables_of_contents(&self) -> impl Iterator<Item = &TableOfContents> {
        self.frames()
            .filter_map(|frame| frame.content().table_of_contents())
    }

    /// Returns an iterator over all involved people lists (`IPLS` in ID3v2.3, `TIPL` and `TMCL` in
    /// ID3v2.4) in the tag.
    ///
    /// # Examples
    ///
    /// ## `IPLS` frame (ID3v2.3)
    ///
    /// ```
    /// use id3::{Frame, Tag, TagLike, Version};
    /// use id3::frame::{Content, InvolvedPeopleList, InvolvedPeopleListItem};
    ///
    /// let mut tag = Tag::with_version(Version::Id3v23);
    ///
    /// let frame = Frame::with_content("IPLS", Content::InvolvedPeopleList(InvolvedPeopleList {
    ///     items: vec![
    ///         InvolvedPeopleListItem {
    ///             involvement: "drums (drum set)".to_string(),
    ///             involvee: "Gene Krupa".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "piano".to_string(),
    ///             involvee: "Hank Jones".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "tenor saxophone".to_string(),
    ///             involvee: "Frank Socolow".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "tenor saxophone".to_string(),
    ///             involvee: "Eddie Wasserman".to_string(),
    ///         },
    ///     ],
    /// }));
    /// tag.add_frame(frame);
    /// assert_eq!(1, tag.involved_people_lists().count());
    /// assert_eq!(4, tag.involved_people_lists().flat_map(|list| list.items.iter()).count());
    /// ```
    ///
    /// ## `TIPL`/`TMCL` frames (ID3v2.4)
    /// ```
    /// use id3::{Frame, Tag, TagLike, Version};
    /// use id3::frame::{Content, InvolvedPeopleList, InvolvedPeopleListItem};
    ///
    /// let mut tag = Tag::with_version(Version::Id3v24);
    ///
    /// let frame = Frame::with_content("TMCL", Content::InvolvedPeopleList(InvolvedPeopleList {
    ///     items: vec![
    ///         InvolvedPeopleListItem {
    ///             involvement: "drums (drum set)".to_string(),
    ///             involvee: "Gene Krupa".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "piano".to_string(),
    ///             involvee: "Hank Jones".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "tenor saxophone".to_string(),
    ///             involvee: "Frank Socolow".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "tenor saxophone".to_string(),
    ///             involvee: "Eddie Wasserman".to_string(),
    ///         },
    ///     ],
    /// }));
    /// tag.add_frame(frame);
    ///
    /// let frame = Frame::with_content("TIPL", Content::InvolvedPeopleList(InvolvedPeopleList {
    ///     items: vec![
    ///         InvolvedPeopleListItem {
    ///             involvement: "executive producer".to_string(),
    ///             involvee: "Ken Druker".to_string(),
    ///         },
    ///         InvolvedPeopleListItem {
    ///             involvement: "arranger".to_string(),
    ///             involvee: "Gerry Mulligan".to_string(),
    ///         },
    ///     ],
    /// }));
    /// tag.add_frame(frame);
    /// assert_eq!(2, tag.involved_people_lists().count());
    /// assert_eq!(6, tag.involved_people_lists().flat_map(|list| list.items.iter()).count());
    ///
    /// ```
    pub fn involved_people_lists(&self) -> impl Iterator<Item = &InvolvedPeopleList> {
        self.frames()
            .filter_map(|frame| frame.content().involved_people_list())
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Tag) -> bool {
        self.frames.len() == other.frames.len()
            && self.frames().all(|frame| other.frames.contains(frame))
    }
}

impl FromIterator<Frame> for Tag {
    fn from_iter<I: IntoIterator<Item = Frame>>(iter: I) -> Self {
        Self {
            frames: Vec::from_iter(iter),
            ..Self::default()
        }
    }
}

impl Extend<Frame> for Tag {
    fn extend<I: IntoIterator<Item = Frame>>(&mut self, iter: I) {
        self.frames.extend(iter)
    }
}

impl TagLike for Tag {
    fn frames_vec(&self) -> &Vec<Frame> {
        &self.frames
    }

    fn frames_vec_mut(&mut self) -> &mut Vec<Frame> {
        &mut self.frames
    }
}

impl From<v1::Tag> for Tag {
    fn from(tag_v1: v1::Tag) -> Tag {
        let mut tag = Tag::new();
        if let Some(genre) = tag_v1.genre() {
            tag.set_genre(genre.to_string());
        }
        if !tag_v1.title.is_empty() {
            tag.set_title(tag_v1.title);
        }
        if !tag_v1.artist.is_empty() {
            tag.set_artist(tag_v1.artist);
        }
        if !tag_v1.album.is_empty() {
            tag.set_album(tag_v1.album);
        }
        if !tag_v1.year.is_empty() {
            tag.set_text("TYER", tag_v1.year);
        }
        if !tag_v1.comment.is_empty() {
            tag.add_frame(Comment {
                lang: "eng".to_string(),
                description: "".to_string(),
                text: tag_v1.comment,
            });
        }
        if let Some(track) = tag_v1.track {
            tag.set_track(u32::from(track));
        }
        tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taglike::TagLike;
    use std::error::Error;
    use std::fs;
    use std::process::Command;
    use std::{self, io::Read, io::Seek};
    use tempfile::tempdir;

    fn ffprobe(file: impl AsRef<Path>) -> Result<String, Box<dyn Error>> {
        let output = Command::new("ffprobe")
            .arg(file.as_ref())
            .output()
            .map_err(|err| match err.kind() {
                io::ErrorKind::NotFound => io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{}. Is ffprobe present in $PATH?", err),
                ),
                _ => err,
            })?;
        if !output.status.success() {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::Other,
                "ffprobe output status failure",
            )));
        }
        Ok(String::from_utf8(output.stderr)?)
    }

    #[test]
    fn remove_id3v2() {
        let tmp = tempdir().unwrap();
        let tmp_name = tmp.path().join("remove_id3v2_tag");
        {
            let mut tag_file = fs::File::create(&tmp_name).unwrap();
            let mut original = fs::File::open("testdata/id3v24.id3").unwrap();
            io::copy(&mut original, &mut tag_file).unwrap();
        }
        let mut tag_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&tmp_name)
            .unwrap();
        tag_file.seek(io::SeekFrom::Start(0)).unwrap();
        assert!(Tag::remove_from_file(&mut tag_file).unwrap());
        tag_file.seek(io::SeekFrom::Start(0)).unwrap();
        assert!(!Tag::remove_from_file(&mut tag_file).unwrap());
    }

    // https://github.com/polyfloyd/rust-id3/issues/39
    #[test]
    fn test_issue_39() {
        // Create temp file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::copy("testdata/quiet.mp3", &tmp).unwrap();
        // Generate sample tag
        let mut tag = Tag::new();
        tag.set_title("Title");
        tag.set_artist("Artist");
        tag.write_to_path(&tmp, Version::Id3v24).unwrap();
        // Check with ffprobe
        let output = ffprobe(&tmp).unwrap();
        // This bug shows as different messages in ffprobe
        assert!(!output.contains("Estimating duration from bitrate, this may be inaccurate"));
        assert!(!output.contains("bytes of junk at"));
        // Also show in console too for manual double check
        println!("{}", output);
    }

    #[test]
    fn github_issue_82() {
        let mut tag = Tag::new();
        tag.set_artist("artist 1\0artist 2\0artist 3");
        assert_eq!(tag.artist(), Some("artist 1\0artist 2\0artist 3"));
        let mut buf = Vec::new();
        tag.write_to(&mut buf, Version::Id3v22).unwrap();
        let tag = Tag::read_from2(io::Cursor::new(buf)).unwrap();
        assert_eq!(tag.artist(), Some("artist 1\0artist 2\0artist 3"));
    }

    #[test]
    fn github_issue_86a() {
        // File has frame header flag bits set that are not known to the standard.
        let _tag = Tag::read_from_path("testdata/github-issue-86a.id3").unwrap();
    }

    #[test]
    fn github_issue_86c() {
        // Unsynchronized bytes on frame boundary exposed that the unsync scheme was applied on the
        // wrong level.
        let _tag = Tag::read_from_path("testdata/github-issue-86b.id3").unwrap();
    }

    #[test]
    fn github_issue_91() {
        // Presence of extended header revealed bad calculation of remaining tag length.
        let _tag = Tag::read_from_path("testdata/github-issue-91.id3").unwrap();
    }

    #[test]
    fn github_issue_147() {
        // Tag contains a broken IPLS frame with an odd value count. We need to handle this
        // gracefully without failing to parse the entire tag, because these issue is apparently
        // widespread.
        let _tag = Tag::read_from_path("testdata/github-issue-147.id3").unwrap();
    }

    #[test]
    fn github_issue_156a() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/github-issue-156a.id3", &tmp).unwrap();

        let tag = crate::partial_tag_ok(Tag::read_from_path(&tmp)).unwrap();
        tag.write_to_path(&tmp, Version::Id3v23).unwrap();
    }

    #[test]
    fn github_issue_156b() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/github-issue-156b.id3", &tmp).unwrap();

        let tag = Tag::read_from_path(&tmp).unwrap();
        tag.write_to_path(&tmp, Version::Id3v23).unwrap();
    }

    #[test]
    fn aiff_read_and_write() {
        // Copy
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/aiff/quiet.aiff", &tmp).unwrap();

        // Read
        let mut tag = Tag::read_from2(&tmp).unwrap();
        assert_eq!(tag.title(), Some("Title"));
        assert_eq!(tag.album(), Some("Album"));

        // Edit
        tag.set_title("NewTitle");
        tag.set_album("NewAlbum");

        // Write
        tag.write_to_path(&tmp, Version::Id3v24).unwrap();

        // Check if not corrupted with ffprobe
        let output = ffprobe(&tmp).unwrap();
        assert!(!output.contains("Input/output error"));
        // Also show in console too for manual double check
        println!("{}", output);

        // Check written data
        tag = Tag::read_from_path(&tmp).unwrap();
        assert_eq!(tag.title(), Some("NewTitle"));
        assert_eq!(tag.album(), Some("NewAlbum"));
    }

    #[test]
    fn aiff_read_padding() {
        let tag = Tag::read_from_path("testdata/aiff/padding.aiff").unwrap();

        assert_eq!(tag.title(), Some("TEST TITLE"));
        assert_eq!(tag.artist(), Some("TEST ARTIST"));
    }

    #[test]
    fn wav_read_tagless() {
        use crate::ErrorKind;

        let error = Tag::read_from_path("testdata/wav/tagless.wav").unwrap_err();

        assert!(
            matches!(error.kind, ErrorKind::NoTag),
            "unexpected error kind: {:?}",
            error.kind
        );
    }

    #[test]
    fn wav_read_tag_mid() {
        let tag = Tag::read_from_path("testdata/wav/tagged-mid.wav").unwrap();

        assert_eq!(tag.title(), Some("Some Great Song"));
        assert_eq!(tag.artist(), Some("Some Great Band"));
        if cfg!(feature = "decode_picture") {
            assert!(tag.pictures().next().is_some())
        }
    }

    #[test]
    fn wav_read_tag_end() {
        let tag = Tag::read_from_path("testdata/wav/tagged-end.wav").unwrap();

        assert_eq!(tag.title(), Some("Some Great Song"));
        assert_eq!(tag.artist(), Some("Some Great Band"));
        if cfg!(feature = "decode_picture") {
            assert!(tag.pictures().next().is_some())
        }
    }

    #[test]
    fn wav_read_tagless_corrupted() {
        use crate::ErrorKind;

        let error = Tag::read_from_path("testdata/wav/tagless-corrupted.wav").unwrap_err();

        // With this file, we reach EOF before the expected chunk end.
        assert!(
            matches!(error.kind, ErrorKind::Io(ref error) if error.kind() == io::ErrorKind::UnexpectedEof),
            "unexpected error kind: {:?}",
            error.kind
        );

        let error = Tag::read_from_path("testdata/wav/tagless-corrupted-2.wav").unwrap_err();

        // With this file, the RIFF chunk size is zero.
        assert!(
            matches!(error.kind, ErrorKind::InvalidInput),
            "unexpected error kind: {:?}",
            error.kind
        );
    }

    #[test]
    fn wav_read_tag_corrupted() {
        use crate::ErrorKind;

        let error = Tag::read_from_path("testdata/wav/tagged-mid-corrupted.wav").unwrap_err();

        assert!(
            matches!(error.kind, ErrorKind::NoTag),
            "unexpected error kind: {:?}",
            error.kind
        );
    }

    #[test]
    fn wav_read_trailing_data() {
        use crate::ErrorKind;

        let error = Tag::read_from_path("testdata/wav/tagless-trailing-data.wav").unwrap_err();

        assert!(
            matches!(error.kind, ErrorKind::NoTag),
            "unexpected error kind: {:?}",
            error.kind
        );
    }

    #[test]
    fn wav_write_tagged_end() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/wav/tagged-end.wav", &tmp).unwrap();

        edit_and_check_wav_tag(&tmp, &tmp).unwrap();
    }

    #[test]
    fn wav_write_tagged_mid() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/wav/tagged-mid.wav", &tmp).unwrap();

        edit_and_check_wav_tag(&tmp, &tmp).unwrap();

        let mut file = File::open(&tmp).unwrap();

        check_trailing_data(&mut file, b"data\x12\0\0\0here is some music");
    }

    #[test]
    fn wav_write_tagless() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/wav/tagless.wav", &tmp).unwrap();

        edit_and_check_wav_tag("testdata/wav/tagged-mid.wav", &tmp).unwrap();
    }

    #[test]
    fn wav_write_trailing_data() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/wav/tagless-trailing-data.wav", &tmp).unwrap();

        edit_and_check_wav_tag("testdata/wav/tagged-mid.wav", &tmp).unwrap();

        let mut file = File::open(&tmp).unwrap();

        check_trailing_data(
            &mut file,
            b", and here is some trailing data that should be preserved.",
        );
    }

    #[test]
    fn wav_write_corrupted() {
        use crate::ErrorKind;

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/wav/tagless-corrupted.wav", &tmp).unwrap();

        let error = edit_and_check_wav_tag("testdata/wav/tagged-mid.wav", &tmp).unwrap_err();

        // With this file, we reach EOF before the expected chunk end.
        assert!(
            matches!(error.kind, ErrorKind::Io(ref error) if error.kind() == io::ErrorKind::UnexpectedEof),
            "unexpected error kind: {:?}",
            error.kind
        );

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy("testdata/wav/tagless-corrupted-2.wav", &tmp).unwrap();

        let error = edit_and_check_wav_tag("testdata/wav/tagged-mid.wav", &tmp).unwrap_err();

        // With this file, the RIFF chunk size is zero.
        assert!(
            matches!(error.kind, ErrorKind::InvalidInput),
            "unexpected error kind: {:?}",
            error.kind
        );
    }

    fn edit_and_check_wav_tag(from: impl AsRef<Path>, to: impl AsRef<Path>) -> crate::Result<()> {
        let from = from.as_ref();
        let to = to.as_ref();

        // Read
        let mut tag = Tag::read_from_path(from)?;

        // Edit
        tag.set_title("NewTitle");
        tag.set_album("NewAlbum");
        tag.set_genre("New Wave");
        tag.set_disc(20);
        tag.set_duration(500);
        tag.set_year(2020);

        // Write
        tag.write_to_path(to, Version::Id3v24)?;

        // Check written data
        tag = Tag::read_from_path(to)?;
        assert_eq!(tag.title(), Some("NewTitle"));
        assert_eq!(tag.album(), Some("NewAlbum"));
        assert_eq!(tag.genre(), Some("New Wave"));
        assert_eq!(tag.disc(), Some(20));
        assert_eq!(tag.duration(), Some(500));
        assert_eq!(tag.year(), Some(2020));

        Ok(())
    }

    fn check_trailing_data<const N: usize>(file: &mut File, data: &[u8; N]) {
        let mut trailing_data = [0; N];
        file.seek(io::SeekFrom::End(-(N as i64))).unwrap();

        file.read_exact(&mut trailing_data).unwrap();

        assert_eq!(&trailing_data, data)
    }

    #[test]
    fn check_read_version() {
        assert_eq!(
            Tag::read_from_path("testdata/id3v22.id3")
                .unwrap()
                .version(),
            Version::Id3v22
        );
        assert_eq!(
            Tag::read_from_path("testdata/id3v23.id3")
                .unwrap()
                .version(),
            Version::Id3v23
        );
        assert_eq!(
            Tag::read_from_path("testdata/id3v24.id3")
                .unwrap()
                .version(),
            Version::Id3v24
        );
    }

    #[test]
    fn test_sylt() {
        let tag = Tag::read_from_path("testdata/SYLT.mp3").unwrap();
        let lyrics = tag.synchronised_lyrics().next().unwrap();
        assert_eq!(lyrics.description, "Description");
    }

    #[test]
    fn test_issue_84() {
        // Read multiple tags from the file
        let tag = Tag::read_from_path("testdata/multi-tags.mp3").unwrap();
        let genres = tag.genres();
        let artists = tag.artists();

        assert_eq!(genres, Some(vec!["Pop/Trip-Hop"]));
        assert_eq!(artists, Some(vec!["First", "Secondary"]));
    }

    /// Serato writes its GEOB tags twice with different encoding.
    #[test]
    fn test_serato_geob() {
        let tag = Tag::read_from_path("testdata/geob_serato.id3").unwrap();
        let count = tag.encapsulated_objects().count();
        assert_eq!(count, 14);
        tag.write_to_path("testdata/geob_serato.id3", Version::Id3v24)
            .unwrap();
        let tag = Tag::read_from_path("testdata/geob_serato.id3").unwrap();
        assert_eq!(count, tag.encapsulated_objects().count());
    }

    /// Read an IPLS frame with UTF-16 encording in an ID3v2.3 tag written by MusicBrainz Picard
    /// 2.12.3.
    #[test]
    fn test_ipls_id3v23_utf16() {
        let tag = Tag::read_from_path("testdata/picard-2.12.3-id3v23-utf16.id3").unwrap();
        assert_eq!(tag.version(), Version::Id3v23);
        let count = tag.involved_people_lists().count();
        assert_eq!(count, 1);
        let ipls = tag.get("IPLS").unwrap();
        let involved_people = ipls
            .content()
            .involved_people_list()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.involvement.as_str(), item.involvee.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            &involved_people,
            &[
                ("double bass", "Israel Crosby"),
                ("drums (drum set)", "Vernell Fournier"),
                ("piano", "Ahmad Jamal"),
                ("producer", "Dave Usher")
            ]
        );

        // Now write the tag. Then check if it can be parsed and results in the same input.
        let mut buffer = Vec::new();
        tag.write_to(&mut buffer, Version::Id3v23).unwrap();
        let new_tag = Tag::read_from2(io::Cursor::new(&buffer)).unwrap();

        let new_involved_people = new_tag
            .get("IPLS")
            .unwrap()
            .content()
            .involved_people_list()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.involvement.as_str(), item.involvee.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(&involved_people, &new_involved_people,);
    }

    /// Read `TIPL` and `TMCL` frames with UTF-8 encording in an ID3v2.4 tag written by MusicBrainz
    /// Picard 2.12.3.
    #[test]
    fn test_ipls_id3v24_utf8() {
        let tag = Tag::read_from_path("testdata/picard-2.12.3-id3v24-utf8.id3").unwrap();
        assert_eq!(tag.version(), Version::Id3v24);
        let count = tag.involved_people_lists().count();
        assert_eq!(count, 2);

        let tipl = tag.get("TIPL").unwrap();
        let involved_people = tipl
            .content()
            .involved_people_list()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.involvement.as_str(), item.involvee.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(&involved_people, &[("producer", "Dave Usher")]);

        let tmcl = tag.get("TMCL").unwrap();
        let musician_credits = tmcl
            .content()
            .involved_people_list()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.involvement.as_str(), item.involvee.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            &musician_credits,
            &[
                ("double bass", "Israel Crosby"),
                ("drums (drum set)", "Vernell Fournier"),
                ("piano", "Ahmad Jamal")
            ]
        );

        // Now write the tag. Then check if it can be parsed and results in the same input.
        let mut buffer = Vec::new();
        tag.write_to(&mut buffer, Version::Id3v24).unwrap();
        let new_tag = Tag::read_from2(io::Cursor::new(&buffer)).unwrap();

        let new_involved_people = new_tag
            .get("TIPL")
            .unwrap()
            .content()
            .involved_people_list()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.involvement.as_str(), item.involvee.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(&involved_people, &new_involved_people,);

        let new_musician_credits = new_tag
            .get("TMCL")
            .unwrap()
            .content()
            .involved_people_list()
            .unwrap()
            .items
            .iter()
            .map(|item| (item.involvement.as_str(), item.involvee.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(&musician_credits, &new_musician_credits,);
    }
}
