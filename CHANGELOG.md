## v1.9.0 (2023-10-24)

### Feat

- Added support for Table of contents frame (CTOC) (#116)

## v1.8.0 (2023-09-21)

### Feat

- Added support for Private Frames (PRIV)

## v1.7.0 (2023-04-13)

### Feat

- Allow disabling expensive picture decoding

## v1.6.0 (2023-01-01)

### Feat

- Add Default to Timestamp

## v1.5.1 (2022-12-08)

### Fix

- modify Content::unique() so that duplicates of frames with unknown type but identical ID are allowed
- add missing conversation method for Popularimeter in Content

## v1.5.0 (2022-11-16)

### Feat

- Adds support for reading/writing TDOR frames

## v1.4.0 (2022-10-30)

### Feat

- Add tokio support for parsing (#102)

## v1.3.0 (2022-08-05)

### Feat

- Store encoding in Frame for TXXX and GEOB (fixes #97)

## v1.2.1 (2022-06-28)

### Fix

- Support Serato GEOB (#96)

## v1.2.0 (2022-06-10)

### Feat

- Add the v1v2 module for simultaneously handling ID3v1+ID3v2 tags (fixes #92)
- Add the genre_parsed method (fixes #88)

## v1.1.4 (2022-06-09)

### Fix

- Remove dbg! prints

## v1.1.3 (2022-06-05)

### Fix

- Increase storage copy buffer size to 2^16 bytes (fixes #94)
- Require bitflags >1.3 (fixes #93)

## v1.1.2 (2022-06-01)

### Fix

- Fix reading of tags with extended headers (fixes #91)

## Version 1.1.1

* Fix wrong implementation of unsynchronization for ID3v2.3
* Permit unknown frame header flag bits to be set
* error: Include problematic data in str::Utf8Error derivative error
* Fix typos in Content docs

## Version 1.1.0

* Add partial_tag_ok
* Add helpers to decode multiple artists/genre (when a file has some) (#87)

## Version 1.0.3

* Translate text frame nulls to '/' (fixes #82)
* Fix chunk length when creating new ID3 for AIFF files (#83)

## Version 1.0.2

* Fix GRP1 frames from being erroneously rejected as invalid (#78).

## Version 1.0.1

* Fix missing description field and incorrect text encoding in SYLT content.

# Version 1.0

This is the first stable release of rust-id3! This release adds a few new features but mainly
focusses on forward compatibility to allow for easier maintenance in the future.

## Breaking changes

The functions for writing and reading tags in WAV/AIFF containers have been renamed:

* `Tag::read_from_aiff_reader(reader: impl io::Read + io::Seek)` -> `Tag::read_from_aiff(reader: impl io::Read + io::Seek)`
* `Tag::read_from_aiff(path: impl AsRef<Path>)` -> `Tag::read_from_aiff_path(path: impl AsRef<Path>)`
* `Tag::read_from_wav_reader(reader: impl io::Read + io::Seek)` -> `Tag::read_from_wav(reader: impl io::Read + io::Seek)`
* `Tag::read_from_wav(path: impl AsRef<Path>)` -> `Tag::read_from_wav_path(path: impl AsRef<Path>)`
* `Tag::write_to_aiff(&self, path: impl AsRef<Path>, version: Version)` -> `Tag::write_to_aiff_path(&self, path: impl AsRef<Path>, version: Version)`
* `Tag::write_to_wav(&self, path: impl AsRef<Path>, version: Version)` -> `Tag::write_to_wav_path(&self, path: impl AsRef<Path>, version: Version)`

The implementation for PartialEq, Eq and Hash has changed for `Frame` and `Content`. The new
implementations are implemented by Rust's derive macro.

For errors:
* The implementation for `Error::description` has been removed as it has been deprecated.
* Merge ErrorKind::UnsupportedVersion into UnsupportedFeature
* The description field changed from a `&'static str` to String to permit more useful messages

The variant names of the TimestampFormat now adhere to Rust naming conventions.

The majority of the Tag functions for mutating and retrieving frames have been moved to the new
`TagLike` trait. This trait is implemented for `Tag` and `Chapter`, making it possible to use these
functions for both types. As is required by Rust's trait rules, you must now `use id3::TagLike` to
use the functions in this trait.

## Compatibility note regarding custom frame decoders

It is a common use case to write custom frame content decoders by matching on the `Unknown` content
type. However, implementing support for such frames in rust-id3 was always a breaking change. This
was due to 2 reasons:

* `Content` gains a new variant which breaks exhaustive match expressions
* Frames that previously decoded to `Unknown` now decode to the new content type, which breaks
  custom decoders that expect it to be `Unknown`

To ensure that new frames can be added in the future without breaking compatibility, Content has
been marked as non_exhaustive and a new `Content::to_unknown` function has been added. This function
returns the `Unknown` variant if present or encodes an ad-hoc copy. This way, custom decoders will
not silently break.

## New Features
* Add support for MPEG Location Lookup Table (MLLT) frames 
* Add support for Chapter (CHAP) frames, containing frames by themselves
* Add support for Popularimeter (POPM) frames

## Miscellaneous Changes
* Prevent unrepresentable frames from being written
* Set Rust edition to 2021
* Doc tests should not unwrap()
* Fix SYLT (#74)
* Rewrite frame content encoders and decoders
* Let TagLike::remove return the removed frames
* Derive Ord and PartialOrd for eligible types
* Implement fmt::Display for Version
* Implement Extend for Tag, Chapter and FromIterator for Tag
* Implement adding frames based on `Into<Frame>`
