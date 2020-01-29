# rust-id3

[![Build Status](https://travis-ci.org/jameshurst/rust-id3.svg)](https://travis-ci.org/jameshurst/rust-id3)
[![](http://meritbadge.herokuapp.com/id3)](https://crates.io/crates/id3)
[![Docs](https://docs.rs/id3/badge.svg)](https://docs.rs/id3)

A library for reading and writing ID3 metadata.

## Usage
```rust
fn main() {
  let tag = id3::Tag::read_from_path("testdata/id3v24.id3").unwrap();

  // print the artist the hard way
  println!("{}", tag.get("TPE1").unwrap().content().text().unwrap());

  // or print it the easy way
  println!("{}", tag.artist().unwrap());
}
```

## Supported ID3 Versions

  * ID3v1 reading
  * ID3v2.2 reading/writing
  * ID3v2.3 reading/writing
  * ID3v2.4 reading/writing

## Unsupported Features

  * Grouping identity
  * Encryption
