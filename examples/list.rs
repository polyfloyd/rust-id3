use id3::{Tag, TagLike};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tag = Tag::read_from_path(
        std::env::args_os()
            .nth(1)
            .ok_or("first argument is the path to read informaation from")?,
    )?;

    // Get a bunch of frames...
    if let Some(artist) = tag.artist() {
        println!("artist: {}", artist);
    }
    if let Some(title) = tag.title() {
        println!("title: {}", title);
    }
    if let Some(album) = tag.album() {
        println!("album: {}", album);
    }

    // Get frames before getting their content for more complex tags.
    if let Some(artist) = tag.get("TPE1").and_then(|frame| frame.content().text()) {
        println!("artist: {}", artist);
    }
    Ok(())
}
