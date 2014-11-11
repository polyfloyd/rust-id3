extern crate id3;

use id3::{ID3Tag, Frame, encoding};

static ID: &'static str = "TYER";
static YEAR: uint = 2014;
static YEARSTR: &'static str = "2014";
static INVALID: &'static str = "invalid";

// UTF8 {{{
#[test]
fn utf8() {
    let mut tag = ID3Tag::with_version(4);

    tag.set_year_enc(YEAR, encoding::UTF8);
    let frame = tag.get_frame_by_id(ID).unwrap();
    
    assert_eq!(tag.year().unwrap(), YEAR);
    assert_eq!(frame.contents.text().as_slice(), YEARSTR);

    let mut data: Vec<u8> = Vec::new();
    data.push(encoding::UTF8 as u8);
    data.extend(String::from_str(YEARSTR).into_bytes().into_iter());
    assert_eq!(frame.contents_to_bytes(), data);
}

#[test]
fn utf8_invalid() {
    let mut tag = ID3Tag::with_version(4);
    let mut frame = Frame::with_version(ID, 4);
    let mut data = Vec::new();
    data.push(encoding::UTF8 as u8);
    data.extend(String::from_str(INVALID).into_bytes().into_iter());
    frame.parse_data(data.as_slice()).unwrap();
    tag.add_frame(frame);
    assert!(tag.year().is_none());
}
//}}}

// UTF16 {{{
#[test]
fn utf16() {
    let mut tag = ID3Tag::with_version(4);

    tag.set_year_enc(YEAR, encoding::UTF16);
    let frame = tag.get_frame_by_id(ID).unwrap();

    assert_eq!(tag.year().unwrap(), YEAR);
    assert_eq!(frame.contents.text().as_slice(), YEARSTR);

    let mut data: Vec<u8> = Vec::new();
    data.push(encoding::UTF16 as u8);
    data.extend(id3::util::string_to_utf16(YEARSTR).into_iter());
    assert_eq!(frame.contents_to_bytes(), data);
}

#[test]
fn utf16_invalid() {
    let mut tag = ID3Tag::with_version(4);
    let mut frame = Frame::with_version(ID, 4);
    let mut data = Vec::new();
    data.push(encoding::UTF16 as u8);
    data.extend(id3::util::string_to_utf16(INVALID).into_iter());
    frame.parse_data(data.as_slice()).unwrap();
    tag.add_frame(frame);
    assert!(tag.year().is_none());
}
//}}}

// UTF16BE {{{
#[test]
fn utf16be() {
    let mut tag = ID3Tag::with_version(4);

    tag.set_year_enc(YEAR, encoding::UTF16BE);
    let frame = tag.get_frame_by_id(ID).unwrap();

    assert_eq!(tag.year().unwrap(), YEAR);
    assert_eq!(frame.contents.text().as_slice(), YEARSTR);

    let mut data: Vec<u8> = Vec::new();
    data.push(encoding::UTF16BE as u8);
    data.extend(id3::util::string_to_utf16be(YEARSTR).into_iter());
    assert_eq!(frame.contents_to_bytes(), data);
}

#[test]
fn utf16be_invalid() {
    let mut tag = ID3Tag::with_version(4);
    let mut frame = Frame::with_version(ID, 4);
    let mut data = Vec::new();
    data.push(encoding::UTF16BE as u8);
    data.extend(id3::util::string_to_utf16be(INVALID).into_iter());
    frame.parse_data(data.as_slice()).unwrap();
    tag.add_frame(frame);
    assert!(tag.year().is_none());
}
//}}}
