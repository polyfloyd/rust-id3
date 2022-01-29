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
