#rust-id3 [![Build Status](https://travis-ci.org/jamesrhurst/rust-id3.svg)](https://travis-ci.org/jamesrhurst/rust-id3)

An ID3 tag reader/writer. The `Tag` struct implements the [AudioTag](https://github.com/jamesrhurst/rust-audiotag) trait for reading, writing, and modification of common metadata elements.

Documentation is available via Rust CI at [http://www.rust-ci.org/jamesrhurst/rust-id3/doc/id3/](http://www.rust-ci.org/jamesrhurst/rust-id3/doc/id3/).

##Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies.id3]
git = "https://github.com/jamesrhurst/rust-id3"
```

```rust
use id3::Tag;

let mut tag = Tag::read_from_path("music.mp3").unwrap();

// print the artist the hard way
println!("{}", tag.get("TALB").unwrap().contents.text());

// or print it the easy way
println!("{}", tag.artist().unwrap());

tag.save().unwrap();
```

##Supported ID3 Versions

  * ID3v1 reading
  * ID3v2.2 reading/writing
  * ID3v2.3 reading/writing
  * ID3v2.4 reading/writing

##Unsupported Features

  * Unsynchronization
  * Grouping identity
  * Encryption

##Contributors

  * [Olivier Renaud](https://bitbucket.org/olivren) 
    * Initial ID3v1 reading code 

