# rust-id3 

[![Build Status](https://travis-ci.org/jameshurst/rust-id3.svg)](https://travis-ci.org/jameshurst/rust-id3)
[![](http://meritbadge.herokuapp.com/id3)](https://crates.io/crates/id3)

A library for reading and writing ID3 metadata.

[Documentation](http://jameshurst.github.io/rust-id3/)

## Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
id3 = "0.1.11"
```

```rust
use id3::Tag;

let mut tag = Tag::read_from_path("music.mp3").unwrap();

// print the artist the hard way
println!("{}", tag.get("TALB").unwrap().content.text());

// or print it the easy way
println!("{}", tag.artist().unwrap());

tag.save().unwrap();
```

## Supported ID3 Versions

  * ID3v1 reading
  * ID3v2.2 reading/writing
  * ID3v2.3 reading/writing
  * ID3v2.4 reading/writing

## Unsupported Features

  * Grouping identity
  * Encryption

## Contributors

  * [Olivier Renaud](https://bitbucket.org/olivren) 
    * Initial ID3v1 reading code 

