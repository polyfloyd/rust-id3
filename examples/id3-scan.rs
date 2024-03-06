use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

use id3::v1v2::FormatVersion;

struct AppArgs {
    root: PathBuf,
}

fn cli_parser() -> bpaf::OptionParser<AppArgs> {
    let root = bpaf::positional::<PathBuf>("root").help("Directory path to scan");
    bpaf::Parser::to_options(bpaf::construct!(AppArgs { root }))
}

enum Id3v1 {
    // We don't distinguish v1.0 and v1.1 (track number or comment)
    Id3v1_1,
    Id3v1_2a, // TAG+, which this crate supports
    Id3v1_2b, // EXT
}

// We have an id3v1 tag, does it use id3v1.2?
fn id3v1_sub_version(file: &File) -> std::io::Result<Id3v1> {
    let len = file.metadata()?.len();
    if let Some(off) = len.checked_sub(355) {
        // TAG+ at -355: 1998 or earlier
        // https://web.archive.org/web/19981205202300/http://www.fortunecity.com/underworld/sonic/3/id3tag.html
        let mut sig = [0; 4];
        file.read_exact_at(&mut sig, off)?;
        if &sig == b"TAG+" {
            return Ok(Id3v1::Id3v1_2a);
        }
    }
    if let Some(off) = len.checked_sub(256) {
        // There is another Id3v1.2 at http://www.birdcagesoft.com/ID3v12.txt
        // EXT at -256: 2002-2003
        let mut sig = [0; 3];
        file.read_exact_at(&mut sig, off)?;
        if &sig == b"EXT" {
            return Ok(Id3v1::Id3v1_2b);
        }
    }
    Ok(Id3v1::Id3v1_1)
}

fn describe_id3_version(file: &File) -> id3::Result<&'static str> {
    Ok(match id3::v1v2::is_candidate(file)? {
        FormatVersion::None => "no tags",
        FormatVersion::Id3v1 => match id3v1_sub_version(file)? {
            Id3v1::Id3v1_1 => "id3v1.1",
            Id3v1::Id3v1_2a => "id3v1.2a",
            Id3v1::Id3v1_2b => "id3v1.2b",
        },
        FormatVersion::Id3v2 => "id3v2",
        FormatVersion::Both => "id3v1+id3v2",
    })
}

fn main() {
    let args = cli_parser().run();
    for dent in walkdir::WalkDir::new(args.root)
        .same_file_system(true)
        .into_iter()
        // Ignore hidden files, don't recurse into hidden directories
        .filter_entry(|dent| !dent.file_name().as_bytes().starts_with(b"."))
    {
        // Ignore walkdir errors (permissions, concurrent modification…)
        let Ok(dent) = dent else { continue };
        // Regular files only, no symlinks
        if !dent.file_type().is_file() {
            continue;
        }
        let path = dent.into_path();
        let Some(ext) = path.extension() else {
            continue;
        };
        if !matches!(ext.as_bytes(), b"mp3" | b"MP3") {
            continue;
        }
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                println!("{}: open error {}", path.display(), err);
                continue;
            }
        };
        match describe_id3_version(&file) {
            Ok(desc) => {
                println!("{}: {}", path.display(), desc)
            }
            Err(err) => println!("{}: error {}", path.display(), err),
        }
    }
}
