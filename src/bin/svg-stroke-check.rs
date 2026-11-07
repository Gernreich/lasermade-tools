//! Find SVG elements whose stroke colour is declared twice, in two places that disagree.
//!
//! An element may carry its stroke in a style="" property and again as a presentation
//! attribute stroke="". The CSS cascade says the style property wins, and browsers and
//! Inkscape both agree — but not every laser importer applies the cascade, and one that
//! reads the attribute instead puts the part in a different cut stage. Since colour IS
//! the cut order in these repositories, that is a part cut at the wrong moment.
//!
//! Usage:
//!     target/release/svg-stroke-check FILE.svg [FILE.svg ...]
//!     target/release/svg-stroke-check --dir DIR [--dir DIR ...]     # every .svg beneath
//!     target/release/svg-stroke-check ... --fix                     # drop the attribute
//!
//! --fix deletes the redundant presentation attribute, keeping the style property that
//! was winning anyway, so what the file cuts is unchanged. Path data is compared before
//! and after and the file is left alone if anything but the attribute moved.
use lasermade_tools::re::{findall1, first, g, rx, sub, sub_n, Regex};
use lasermade_tools::{text, well_formed};
use std::collections::BTreeMap;

struct Rx {
    element: Regex,
    style: Regex,
    attr: Regex,
    attr_sub: Regex,
    id: Regex,
    data: Regex,
}

fn rxs() -> Rx {
    let drawable = "path|rect|circle|ellipse|line|polyline|polygon|text|g|use";
    Rx {
        element: rx(&format!(r"(?i)<(?:{drawable})\b[^>]*>")),
        style: rx(r#"(?si)style="[^"]*?(?<![-\w])stroke:\s*([^;"\s]+)"#),
        attr: rx(r#"(?si)(?:^|\s)stroke="([^"]+)""#),
        attr_sub: rx(r#"\s+stroke="[^"]+""#),
        id: rx(r#"\bid="([^"]+)""#),
        data: rx(r#"\bd="([^"]+)""#),
    }
}

/// #00FF00 and #0f0 are the same ink; so are 'black' and '#000000'.
fn norm(c: &str) -> String {
    let c = c.trim().to_lowercase();
    let named = [
        ("black", "#000000"), ("white", "#ffffff"), ("red", "#ff0000"), ("lime", "#00ff00"),
        ("blue", "#0000ff"), ("cyan", "#00ffff"), ("aqua", "#00ffff"), ("magenta", "#ff00ff"),
        ("fuchsia", "#ff00ff"), ("yellow", "#ffff00"), ("green", "#008000"),
    ];
    if let Some((_, v)) = named.iter().find(|(k, _)| *k == c) {
        return v.to_string();
    }
    let b = c.as_bytes();
    if b.len() == 4 && b[0] == b'#' && c[1..].chars().all(|h| h.is_ascii_digit() || ('a'..='f').contains(&h)) {
        return format!("#{}", c[1..].chars().flat_map(|h| [h, h]).collect::<String>());
    }
    c
}

type Row = (String, String, String);

/// -> (conflicts, duplicates). Conflicts disagree; duplicates say the same thing.
fn scan(r: &Rx, t: &str) -> (Vec<Row>, Vec<Row>) {
    let (mut conflicts, mut duplicates) = (Vec::new(), Vec::new());
    for m in r.element.find_iter(t) {
        let e = m.unwrap().as_str();
        let (Some(sty), Some(att)) = (first(&r.style, e), first(&r.attr, e)) else { continue };
        let pid = first(&r.id, e).map(|c| g(&c, 1).to_string()).unwrap_or_else(|| "(no id)".into());
        let row = (pid, norm(g(&sty, 1)), norm(g(&att, 1)));
        if row.1 != row.2 {
            conflicts.push(row)
        } else {
            duplicates.push(row)
        }
    }
    (conflicts, duplicates)
}

fn fix(r: &Rx, t: &str) -> Result<String, String> {
    let out = sub(&r.element, t, |c| {
        let e = g(c, 0);
        if first(&r.style, e).is_some() && first(&r.attr, e).is_some() {
            sub_n(&r.attr_sub, e, 1, |_| String::new())
        } else {
            e.to_string()
        }
    });
    if findall1(&r.data, &out, 1) != findall1(&r.data, t, 1) {
        return Err("path data changed — refusing to write".into());
    }
    if let Err(e) = well_formed(&out) {
        return Err(format!("result does not parse ({e}) — refusing to write"));
    }
    Ok(out)
}

const USAGE: &str = "usage: svg-stroke-check [-h] [--dir DIR] [--fix] [--quiet] [files ...]";

fn usage_error(msg: &str) -> ! {
    eprintln!("{USAGE}\nsvg-stroke-check: error: {msg}");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let (mut do_fix, mut quiet) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}\n\npositional arguments:\n  files\n\noptional arguments:\n  -h, --help  show this help message and exit\n  --dir DIR\n  --fix\n  --quiet     list only files with findings");
                return;
            }
            "--fix" => do_fix = true,
            "--quiet" => quiet = true,
            "--dir" => {
                i += 1;
                dirs.push(args.get(i).cloned().unwrap_or_else(|| usage_error("argument --dir: expected one argument")));
            }
            s if s.starts_with("--dir=") => dirs.push(s[6..].to_string()),
            s if s.starts_with('-') && s.len() > 1 => usage_error(&format!("unrecognized arguments: {s}")),
            s => files.push(text::pathstr(s)),
        }
        i += 1;
    }
    let mut targets = files;
    for d in &dirs {
        targets.extend(text::rglob_ext(&text::pathstr(d), ".svg"));
    }
    if targets.is_empty() {
        usage_error("no SVG files given");
    }
    let r = rxs();
    let (mut n_conf, mut n_dup, mut n_files, mut n_fixed, mut n_unread) = (0, 0, 0, 0, 0);
    for f in &targets {
        // AN UNREADABLE FILE IS A FINDING, not a skip: the tally and the exit
        // status are both built from the counts, so a corrupt SVG that only
        // printed a line left "0 conflicting" standing and the harness read
        // that as ok. The one file the tool could not look at is the one it
        // should refuse to pass.
        let t = match text::read_strict(f) {
            Ok(t) => t,
            Err(e) => {
                println!("  UNREADABLE {f}: {e}");
                n_unread += 1;
                continue;
            }
        };
        let (conflicts, duplicates) = scan(&r, &t);
        if conflicts.is_empty() && duplicates.is_empty() {
            if !quiet {
                println!("  ok    {f}");
            }
            continue;
        }
        n_files += 1;
        n_conf += conflicts.len();
        n_dup += duplicates.len();
        println!("\n  {f}");
        if !conflicts.is_empty() {
            let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
            for (_, s, a) in &conflicts {
                *pairs.entry((s.clone(), a.clone())).or_default() += 1;
            }
            println!("    {} CONFLICTING — style wins, attribute is a different colour", conflicts.len());
            for ((s, a), n) in &pairs {
                println!("      x{n:<4} style {s}  vs  attribute {a}   (cuts as {s})");
            }
            for (pid, _, _) in conflicts.iter().take(8) {
                println!("        {pid}");
            }
            if conflicts.len() > 8 {
                println!("        … and {} more", conflicts.len() - 8);
            }
        }
        if !duplicates.is_empty() {
            println!("    {} redundant but agreeing (harmless, still noise)", duplicates.len());
        }
        if do_fix {
            match fix(&r, &t) {
                Err(e) => println!("    NOT FIXED: {e}"),
                Ok(out) => {
                    std::fs::write(f, out).unwrap_or_else(|e| lasermade_tools::die(format!("{f}: {e}")));
                    n_fixed += 1;
                    println!("    fixed — redundant stroke=\"\" attributes removed");
                }
            }
        }
    }
    println!(
        "\n  {} file(s) scanned, {n_files} with findings: {n_conf} conflicting, {n_dup} redundant-but-agreeing{}{}",
        targets.len() - n_unread,
        if n_unread > 0 { format!(", {n_unread} unreadable") } else { String::new() },
        if do_fix { format!(", {n_fixed} file(s) rewritten") } else { String::new() }
    );
    std::process::exit(if n_conf > 0 || n_unread > 0 { 1 } else { 0 });
}
