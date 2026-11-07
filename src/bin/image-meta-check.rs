//! Does any tracked image carry metadata?
//!
//!     target/release/image-meta-check REPO [REPO ...]
//!
//! Every photograph here comes off a phone, and a phone writes itself into the
//! file. On 2026-10-14 the published build photos were found carrying the camera,
//! the time, its settings, a Display P3 colour profile and a GPS block -- one that
//! held only the compass bearing, not a position, though a phone set otherwise
//! writes the position there too. The author's rule since then is that no image
//! carries any metadata at all. This holds every image `git ls-files` lists to that rule.
//!
//! It reads the file's own structure rather than asking a library what it found,
//! so nothing is missed for being a tag the library does not name:
//!
//!   JPEG   any APPn segment but APP0 (the JFIF header the format needs) -- APP1 is
//!          EXIF and XMP, APP2 the ICC profile, APP13 IPTC -- and any COM comment
//!   PNG    tEXt, zTXt, iTXt, eXIf, iCCP, pHYs and tIME chunks
//!
//! A colour profile counts. Stripping one from a P3 photo would wash it out, so
//! the fix is to convert the pixels to sRGB first and then save with no profile;
//! the commit that brought this check in did exactly that to every photo.
//!
//! Exit status is 1 if any image fails, so it can gate a commit.
use std::collections::BTreeSet;

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_META: [&[u8]; 7] = [b"tEXt", b"zTXt", b"iTXt", b"eXIf", b"iCCP", b"pHYs", b"tIME"];

fn jpeg_meta(b: &[u8]) -> Vec<String> {
    let (mut found, mut i) = (Vec::new(), 2usize);
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            break;
        }
        let m = b[i + 1];
        if m == 0xD9 || m == 0xDA {
            break; // end of image, or the scan: no more headers
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        let n = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if (0xE1..=0xEF).contains(&m) || m == 0xFE {
            found.push(match m {
                0xE1 => "EXIF/XMP".to_string(),
                0xE2 => "ICC profile".to_string(),
                0xED => "IPTC".to_string(),
                0xFE => "comment".to_string(),
                _ => format!("APP{}", m - 0xE0),
            });
        }
        i += 2 + n;
    }
    found
}

fn png_meta(b: &[u8]) -> Vec<String> {
    let (mut found, mut i) = (Vec::new(), 8usize);
    while i + 8 <= b.len() {
        let n = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        let kind = &b[i + 4..i + 8];
        if PNG_META.contains(&kind) {
            found.push(String::from_utf8_lossy(kind).into_owned());
        }
        if kind == b"IEND" {
            break;
        }
        i += 12 + n;
    }
    found
}

fn check(path: &str) -> Vec<String> {
    let b = std::fs::read(path).unwrap_or_else(|e| lasermade_tools::die(format!("{path}: {e}")));
    if b.starts_with(b"\xff\xd8") {
        return jpeg_meta(&b);
    }
    if b.starts_with(PNG_SIG) {
        return png_meta(&b);
    }
    vec![format!("unrecognised format: {}", path.rsplit('.').next().unwrap())]
}

fn main() {
    let repos: Vec<String> = std::env::args().skip(1).collect();
    if repos.is_empty() {
        lasermade_tools::die("usage: image-meta-check REPO [REPO ...]");
    }
    let (mut n, mut bad) = (0, 0);
    for r in &repos {
        let files = lasermade_tools::text::git(r, &["ls-files"]).unwrap_or_default();
        for f in files.split_whitespace() {
            let l = f.to_lowercase();
            if ![".jpg", ".jpeg", ".png", ".gif", ".webp", ".tif", ".tiff", ".heic"].iter().any(|e| l.ends_with(e)) {
                continue;
            }
            n += 1;
            let p = if r.ends_with('/') { format!("{r}{f}") } else { format!("{r}/{f}") };
            let found = check(&p);
            if !found.is_empty() {
                bad += 1;
                let s: BTreeSet<String> = found.into_iter().collect();
                println!("  META  {p}: {}", s.into_iter().collect::<Vec<_>>().join(", "));
            }
        }
    }
    println!("  {n} images, {bad} with metadata");
    std::process::exit(if bad > 0 || n == 0 { 1 } else { 0 });
}
