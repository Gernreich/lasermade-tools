//! sc-slim DESIGN.sc: rewrite a SolidCraft design as the design alone, so it
//! can be committed and checked byte for byte (2026-11-08).
//!
//! A .sc that `solidcraft-cli run --save` writes is a zip of document.json
//! (the parameters and the timeline), container.json and cache/, the built
//! bodies and how long each feature took to build. The timings change every
//! run, and the cache is most of the file (9.6 MB of a panpipes design's
//! 9.7). SolidCraft rebuilds a design without it when it opens one, as fast
//! as with it, so this keeps document.json and container.json only, stored
//! uncompressed with a fixed date (git compresses them anyway).
//!
//!     solidcraft-cli run X.solidcraft.json --save X.sc && sc-slim X.sc
use lasermade_tools::die;
use std::process::Command;

const KEEP: [&str; 2] = ["document.json", "container.json"];

fn crc32(b: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else { die("usage: sc-slim DESIGN.sc") };
    let files: Vec<(&str, Vec<u8>)> = KEEP
        .iter()
        .map(|name| {
            let o = Command::new("unzip").args(["-p", path, name]).output().unwrap_or_else(|e| die(format!("unzip: {e}")));
            if !o.status.success() || o.stdout.is_empty() {
                die(format!("{path}: no {name} in it; is it a SolidCraft design?"));
            }
            (*name, o.stdout)
        })
        .collect();
    let (time, date) = (0u16, (1 << 5) | 1); // 1980-01-01 00:00
    let mut out: Vec<u8> = vec![];
    let mut central: Vec<u8> = vec![];
    for (name, data) in &files {
        let (crc, len, at) = (crc32(data), data.len() as u32, out.len() as u32);
        let head = |sig: u32, v: &mut Vec<u8>, central: bool| {
            v.extend(sig.to_le_bytes());
            if central {
                v.extend(20u16.to_le_bytes()); // made by
            }
            v.extend(20u16.to_le_bytes()); // needed
            v.extend(0u16.to_le_bytes()); // flags
            v.extend(0u16.to_le_bytes()); // stored
            v.extend(time.to_le_bytes());
            v.extend((date as u16).to_le_bytes());
            v.extend(crc.to_le_bytes());
            v.extend(len.to_le_bytes());
            v.extend(len.to_le_bytes());
            v.extend((name.len() as u16).to_le_bytes());
            v.extend(0u16.to_le_bytes()); // extra
        };
        head(0x0403_4b50, &mut out, false);
        out.extend(name.as_bytes());
        out.extend(data);
        head(0x0201_4b50, &mut central, true);
        central.extend(0u16.to_le_bytes()); // comment
        central.extend(0u16.to_le_bytes()); // disk
        central.extend(0u16.to_le_bytes()); // internal attributes
        central.extend(0u32.to_le_bytes()); // external attributes
        central.extend(at.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let (cd_at, cd_len, n) = (out.len() as u32, central.len() as u32, files.len() as u16);
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]); // this disk, the directory's disk
    out.extend(n.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend(cd_len.to_le_bytes());
    out.extend(cd_at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    std::fs::write(path, out).unwrap_or_else(|e| die(format!("{path}: {e}")));
}
