extern crate id3;

use id3::{Tag, Frame};
use id3::frame::Encoding;

static ID: &'static str = "TYER";
static YEAR: usize = 2014;
static YEARSTR: &'static str = "2014";
static INVALID: &'static str = "invalid";

// UTF8 {{{
#[test]
fn utf8() {
    let mut tag = Tag::with_version(4);

    tag.set_year_enc(YEAR, Encoding::UTF8);
    let frame = tag.get(ID).unwrap();
    
    assert_eq!(tag.year().unwrap(), YEAR);
    assert_eq!(&frame.content.text()[..], YEARSTR);

    let mut data: Vec<u8> = Vec::new();
    data.push(Encoding::UTF8 as u8);
    data.extend(YEARSTR.bytes());
    assert_eq!(frame.content_to_bytes(4), data);
}

#[test]
fn utf8_invalid() {
    let mut tag = Tag::with_version(4);
    let mut frame = Frame::new(ID);
    let mut data = Vec::new();
    data.push(Encoding::UTF8 as u8);
    data.extend(INVALID.bytes());
    frame.parse_data(&data[..]).unwrap();
    tag.push(frame);
    assert!(tag.year().is_none());
}
//}}}

// UTF16 {{{
#[test]
fn utf16() {
    let mut tag = Tag::with_version(4);

    tag.set_year_enc(YEAR, Encoding::UTF16);
    let frame = tag.get(ID).unwrap();

    assert_eq!(tag.year().unwrap(), YEAR);
    assert_eq!(&frame.content.text()[..], YEARSTR);

    let mut data: Vec<u8> = Vec::new();
    data.push(Encoding::UTF16 as u8);
    data.extend(id3::util::string_to_utf16(YEARSTR).into_iter());
    assert_eq!(frame.content_to_bytes(4), data);
}

#[test]
fn utf16_invalid() {
    let mut tag = Tag::with_version(4);
    let mut frame = Frame::new(ID);
    let mut data = Vec::new();
    data.push(Encoding::UTF16 as u8);
    data.extend(id3::util::string_to_utf16(INVALID).into_iter());
    frame.parse_data(&data[..]).unwrap();
    tag.push(frame);
    assert!(tag.year().is_none());
}
//}}}

// UTF16BE {{{
#[test]
fn utf16be() {
    let mut tag = Tag::with_version(4);

    tag.set_year_enc(YEAR, Encoding::UTF16BE);
    let frame = tag.get(ID).unwrap();

    assert_eq!(tag.year().unwrap(), YEAR);
    assert_eq!(&frame.content.text()[..], YEARSTR);

    let mut data: Vec<u8> = Vec::new();
    data.push(Encoding::UTF16BE as u8);
    data.extend(id3::util::string_to_utf16be(YEARSTR).into_iter());
    assert_eq!(frame.content_to_bytes(4), data);
}

#[test]
fn utf16be_invalid() {
    let mut tag = Tag::with_version(4);
    let mut frame = Frame::new(ID);
    let mut data = Vec::new();
    data.push(Encoding::UTF16BE as u8);
    data.extend(id3::util::string_to_utf16be(INVALID).into_iter());
    frame.parse_data(&data[..]).unwrap();
    tag.push(frame);
    assert!(tag.year().is_none());
}
//}}}
