
#rust-id3


An ID3 tag reader/writer. The `ID3Tag` struct implements the [AudioTag](https://github.com/jamesrhurst/rust-audiotag) trait for reading, writing, and modification of common metadata elements.

##Usage

Add the dependency to your `Cargo.toml`:

```
[dependencies.id3]
git = "https://github.com/jamesrhurst/rust-id3"
```

```rust
extern crate id3;
use id3::{AudioTag, ID3Tag};

fn main() {
	let tag = AudioTag::load("music.mp3");

	// Some things modifying the tag

	tag.save();
}
```

##Unsupported Features

  * Unsynchronization
  * Grouping identity
  * Encryption

