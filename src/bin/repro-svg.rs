//! Do a repository's shipped drawings and pages still come out of its generators?
//!
//!     target/release/repro-svg [REPO]
//!
//! trumpet had this twice over -- byte gates for the ribbon sheets and the block
//! sheets -- and no other repository had it at all. knotwork-soundholes ships 13
//! cut files from two generators and living-hinge 14 from one, none of them
//! checked, and for four of the knots the command that drew them was recorded
//! NOWHERE: not in the README, not in the filename, not in a comment. They were
//! recovered on 2026-09-10 by reading the parameters back out of each SVG's own
//! description -- the ribbon width is 2*HW, the cosine amplitude is AMP, the rim
//! overrun is BITE -- and every one then reproduced byte-identical. This program
//! exists so that recovery never has to happen again.
//!
//! The manifest is .repro in the repository root:
//!
//!     shipped/path.svg :: shell command writing the file to $OUT
//!
//! $OUT is a temp path; the command must put exactly that file there. Lines
//! starting with # are comments, blank lines ignored.
use lasermade_tools::{py, run, text, TempDir};
use std::collections::BTreeSet;
use std::path::Path;

fn last_line(s: &str) -> String {
    let t = s.trim();
    py::head(t.lines().last().unwrap_or("no output"), 80).to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    let repo = args.first().cloned().unwrap_or_else(|| ".".into());
    std::process::exit(check(&repo));
}

fn check(repo: &str) -> i32 {
    let man = Path::new(repo).join(".repro");
    let mans = man.to_string_lossy().into_owned();
    if !man.exists() {
        println!("  no .repro in {repo}");
        return 2;
    }
    let body = text::read_strict(&man).unwrap_or_else(|e| lasermade_tools::die(format!("{mans}: {e}")));
    let mut rows: Vec<(String, String)> = Vec::new();
    for (n, line) in body.split_inclusive('\n').enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, cmd)) = line.split_once("::") else {
            println!("  {mans}:{}: no :: separator", n + 1);
            return 2;
        };
        rows.push((name.trim().to_string(), cmd.trim().to_string()));
    }
    // A command of "!" means: this file ships without a generator, we know, and
    // the reason is in the comment above it. It still has to be LISTED, so a new
    // unreproducible file is a decision someone writes down rather than a
    // silence. Display-only renderings are the honest case; a cut file should
    // never be one.
    let acked: BTreeSet<String> = rows.iter().filter(|(_, c)| c == "!").map(|(n, _)| n.clone()).collect();
    rows.retain(|(_, c)| c != "!");
    // A manifest that parses nothing passes. Say what was read, and treat a
    // shipped drawing or page that no line claims as a failure rather than a
    // silence -- pages too since 2026-09-10, for a page whose source is named
    // differently from it, or which has no markdown at all, falls outside
    // doc-audit's page-currency check entirely.
    //
    // A RUST REPOSITORY IS BUILT ONCE, HERE, before any row runs, and its rows
    // then run target/release/<program> directly. As `cargo run` rows, one in
    // thirty failed "No such file or directory" under the full gate: cargo
    // re-links the finished binary on every run, so with the rows running at
    // once one cargo could pull the program out from under another starting
    // it. Building here also means the rows run the code as it stands.
    if Path::new(repo).join("Cargo.toml").exists() {
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo = std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(|d| Path::new(d).join("cargo"))
            .find(|p| p.is_file())
            .unwrap_or_else(|| Path::new(&home).join(".cargo/bin/cargo"));
        match std::process::Command::new(cargo).args(["build", "-q", "--release", "--locked"]).current_dir(repo).output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let all = text::decode(&o.stdout) + &text::decode(&o.stderr);
                println!("  cargo build failed: {}", last_line(&all));
                return 2;
            }
            Err(e) => {
                println!("  cargo build failed: {}", py::head(&e.to_string(), 80));
                return 2;
            }
        }
    }
    let mut shipped: BTreeSet<String> = BTreeSet::new();
    let root = Path::new(repo);
    for p in text::walk(root) {
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
        let parts: Vec<&str> = rel.split('/').collect();
        // .git anywhere, and target/ at the top: cargo's build output
        if parts.contains(&".git") || parts[0] == "target" {
            continue;
        }
        if p.is_dir() {
            continue;
        }
        // and the STEP exports of the 3D views, since 2026-11-07, and the
        // SolidCraft designs and their scripts, since 2026-11-08
        let kinds = [".svg", ".html", ".step", ".sc", ".solidcraft.json"];
        if kinds.iter().any(|k| rel.ends_with(k)) {
            shipped.insert(rel);
        }
    }
    let (mut same, mut bad) = (0, 0);
    // EACH ROW ITS OWN DIRECTORY, since 2026-10-18. Most commands write every
    // sheet they draw into $(dirname $OUT), and with one directory for the
    // whole manifest a row whose generator failed could find the file an
    // earlier row had left there and pass on it. Separate directories close
    // that, and let the rows run at once. Results are still reported in
    // manifest order.
    let t = TempDir::new("repro-svg");
    let mut jobs: Vec<(usize, String, String)> = Vec::new();
    for (i, (name, cmd)) in rows.iter().enumerate() {
        if root.join(name).exists() {
            let base = Path::new(name).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let out = t.0.join(i.to_string()).join(base).to_string_lossy().into_owned();
            jobs.push((i, cmd.clone(), out));
        }
    }
    let repo_dir = root.to_path_buf();
    let results = run::pool(jobs.clone(), run::cores(), move |(_, cmd, out)| {
        std::fs::create_dir_all(Path::new(&out).parent().unwrap()).unwrap();
        run::shell(&cmd, &repo_dir, &[("OUT", &out)], None)
    });
    let mut by_row = std::collections::HashMap::new();
    for ((i, _, out), r) in jobs.into_iter().zip(results) {
        by_row.insert(i, (out, r));
    }
    for (i, (name, _)) in rows.iter().enumerate() {
        let Some((out, r)) = by_row.get(&i) else {
            println!("  MISSING   {name}");
            bad += 1;
            continue;
        };
        if !Path::new(out).exists() {
            let msg = match r {
                Ok(r) => last_line(&(r.stdout.clone() + &r.stderr)),
                Err(e) => py::head(&e.to_string(), 80).to_string(),
            };
            println!("  NOT DRAWN {name}: {msg}");
            bad += 1;
        } else if std::fs::read(root.join(name)).ok() == std::fs::read(out).ok() {
            same += 1;
        } else {
            println!("  DIFFERS   {name}");
            bad += 1;
        }
    }
    let claimed: BTreeSet<&String> = rows.iter().map(|(n, _)| n).collect();
    let unclaimed: Vec<&String> = shipped.iter().filter(|s| !claimed.contains(s) && !acked.contains(*s)).collect();
    for u in &unclaimed {
        println!("  UNCLAIMED {u}");
    }
    // AND AN ACKNOWLEDGEMENT FOR A FILE THAT HAS GONE. A "!" line is a standing
    // claim that a particular file ships without a generator; a deleted preview
    // left its exemption behind, an assertion nobody re-reads, in a list whose
    // whole job is to be trusted.
    let stale: Vec<&String> = acked.iter().filter(|a| !root.join(a).exists()).collect();
    for a in &stale {
        println!("  STALE !   {a}: acknowledged, and not there any more");
    }
    let tail = if acked.is_empty() { String::new() } else { format!(", {} shipped without a generator", acked.len()) };
    let lost = if stale.is_empty() { String::new() } else { format!(", {} acknowledged and gone", stale.len()) };
    let failing = bad > 0 || !unclaimed.is_empty() || !stale.is_empty();
    if failing {
        println!("  {same} reproduce, {bad} failing, {} unclaimed{tail}{lost}", unclaimed.len());
    } else {
        println!("  {same} reproduce{tail}");
    }
    if failing { 1 } else { 0 }
}
