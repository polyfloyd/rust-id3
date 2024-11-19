//! Read an ID3 tag from a file and print frame information to the command line.

use id3::{
    frame::{
        Chapter, Comment, EncapsulatedObject, ExtendedLink, ExtendedText, Lyrics, Picture,
        Popularimeter, SynchronisedLyrics, UniqueFileIdentifier,
    },
    Content, Tag,
};
use std::env::args;
use std::error::Error;
use std::fs::File;

fn main() -> Result<(), Box<dyn Error>> {
    let Some(path) = args().nth(1) else {
        return Err("No path specified!".into());
    };

    let file = File::open(&path)?;
    let tag = Tag::read_from2(file)?;
    let frame_count = tag.frames().count();
    println!(
        "# ID3 {version} - {frame_count} frames",
        version = tag.version()
    );
    for frame in tag.frames() {
        let id = frame.id();
        match frame.content() {
            Content::Text(value) | Content::Link(value) => {
                println!("{id}={value:?}");
            }
            Content::ExtendedText(ExtendedText { description, value }) => {
                println!("{id}:{description}={value:?}");
            }
            Content::ExtendedLink(ExtendedLink { description, link }) => {
                println!("{id}:{description}={link:?}");
            }
            Content::Comment(Comment {
                lang,
                description,
                text,
            }) => {
                println!("{id}:{description}[{lang}]={text:?}");
            }
            Content::Popularimeter(Popularimeter {
                user,
                rating,
                counter,
            }) => {
                println!("{id}:{user}[{counter}]={rating:?}");
            }
            Content::Lyrics(Lyrics {
                lang,
                description,
                text,
            }) => {
                println!("{id}:{description}[{lang}]={text:?}");
            }
            Content::SynchronisedLyrics(SynchronisedLyrics {
                lang,
                timestamp_format,
                content_type,
                description,
                content,
            }) => {
                println!(
                    "{id}:{description}[{lang}] ({timestamp_format}, {content_type})={content:?}"
                );
            }
            Content::Picture(Picture {
                mime_type,
                picture_type,
                description,
                data,
            }) => {
                let size = data.len();
                println!("{id}:{picture_type}=<image, {mime_type}, description {description:?}, {size} bytes>");
            }
            Content::EncapsulatedObject(EncapsulatedObject {
                mime_type,
                filename,
                description,
                data,
            }) => {
                let size = data.len();
                println!("{id}:{description}=<encapsulated object, {mime_type}, filename {filename:?}, {size} bytes>");
            }
            Content::Chapter(Chapter {
                element_id,
                start_time,
                end_time,
                start_offset,
                end_offset,
                frames,
            }) => {
                let chapter_frame_count = frames.len();
                println!("{id}:{element_id}=<chapter, {chapter_frame_count} frames ({start_offset}+{start_time} - {end_offset}+{end_time}>");
            }
            Content::UniqueFileIdentifier(UniqueFileIdentifier {
                owner_identifier,
                identifier,
            }) => {
                let value = identifier
                    .iter()
                    .map(|&byte| {
                        char::from_u32(byte.into())
                            .map(|c| String::from(c))
                            .unwrap_or_else(|| format!("\\x{:02X}", byte))
                    })
                    .collect::<String>();
                println!("{id}:{owner_identifier}=b\"{value}\"");
            }
            content => {
                println!("{id}={content:?}");
            }
        }
    }
    Ok(())
}
