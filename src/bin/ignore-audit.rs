//! Does every .doc-audit-ignore entry still suppress something?
//!
//!     target/release/ignore-audit
//!
//! Each line of an ignore file is a standing claim -- "this name is prose, not a
//! file here" -- and every gate in this repository checks artefacts, code or
//! documents. Nothing checks the SUPPRESSIONS. An exemption that has stopped doing
//! anything is worse than no exemption at all: it is an assertion nobody re-reads,
//! sitting in the one list whose entire job is to be trusted, and the longer such a
//! list runs on the less anyone questions any line of it.
//!
//! Method: for each entry, run every document in the repository against a COPY of
//! the ignore file with that one line removed, and see whether anything starts
//! failing. If nothing does, the entry is dead weight. The copy matters -- editing
//! the repository's own file in a loop is how check 24 left five design stems
//! corrupted when it was interrupted.
//!
//! Found two on 2026-09-11: trumpet exempted parts/LICENSE and tools/LICENSE from
//! the orphan check, which never needed it, because that check matches a tracked
//! file by BASENAME as well as by path and the README's own mention of LICENSE
//! already covered every copy.
//!
//! Both suppression files, not just one. .doc-audit-ignore exempts a NAME and
//! .doc-audit-generated exempts a DIRECTORY -- six of those lines once exempted
//! 378 tracked files in trumpet, so it is much the blunter instrument, and it
//! went unaudited when the named exemptions were first audited.
use lasermade_tools::{run, text, TempDir};
use std::path::{Path, PathBuf};

const FILES: [(&str, &str); 2] = [(".doc-audit-ignore", "--ignore-file"), (".doc-audit-generated", "--generated-file")];

/// (count of failing checks, the documents whose audit did not complete).
///
/// A doc-audit that CRASHED prints no ✗ lines, so a broken audit would come back
/// as a clean document and every exemption would be measured against nothing.
/// doc-audit exits 0 when it passes and 1 when it fails, so anything else, or a
/// status that disagrees with the marks printed, means it did not finish.
fn failures(audit: &Path, repo: &Path, flag: &str, list: &Path) -> (usize, Vec<String>) {
    let docs = text::git(repo, &["ls-files", "*.md"]).unwrap_or_default();
    let (mut n, mut broke) = (0, Vec::new());
    for d in docs.split_whitespace() {
        let p = Path::new(d);
        let dir = repo.join(p.parent().unwrap_or(Path::new("")));
        let r = std::process::Command::new(audit).arg(p.file_name().unwrap()).arg(flag).arg(list).current_dir(dir).output();
        let (code, out) = match r {
            Ok(o) => (o.status.code(), text::decode(&o.stdout)),
            Err(_) => (None, String::new()),
        };
        let hits = out.matches('✗').count();
        if !matches!(code, Some(0) | Some(1)) || (code == Some(1)) != (hits > 0) {
            broke.push(d.to_string());
        }
        n += hits;
    }
    (n, broke)
}

fn main() {
    let exe = std::env::current_exe().unwrap_or_default();
    let audit = exe.with_file_name("doc-audit");
    if !audit.exists() {
        lasermade_tools::die(format!("ignore-audit: no {} (cargo build --release)", audit.display()));
    }
    let root = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("LaserMadeMusic/GIT");
    let (mut dead, mut total, mut skipped, mut broken) = (0, 0, 0, 0);
    let t = TempDir::new("ignore-audit");
    let mut names: Vec<String> = std::fs::read_dir(&root).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.sort();
    let mut probes: Vec<(String, &str, String, PathBuf, PathBuf, &str)> = Vec::new();
    for name in &names {
        let repo = root.join(name);
        for (which, flag) in FILES {
            let f = repo.join(which);
            if !f.is_file() {
                continue;
            }
            let body = text::read(&f).unwrap_or_default();
            let lines: Vec<&str> = body.split('\n').collect();
            let entries: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
            // With the full list a repository must be clean, or "removing this line
            // changes nothing" cannot mean anything. A SKIP IS A FAILURE, and its
            // entries were never audited.
            let (n, broke) = failures(&audit, &repo, flag, &f);
            if !broke.is_empty() {
                println!(
                    "  ERROR {name} {which}: doc-audit did not finish on {}{}",
                    broke.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
                    if broke.len() > 3 { " ..." } else { "" }
                );
                broken += entries.len();
                continue;
            }
            if n > 0 {
                println!("  SKIP  {name} {which}: {n} check(s) fail with the full list, so its {} exemption(s) go unaudited", entries.len());
                skipped += entries.len();
                continue;
            }
            total += entries.len();
            for e in entries {
                // A copy of its own per entry, so the runs can go at once.
                let tmp = t.0.join(format!("list{}", probes.len()));
                let kept: Vec<&str> = lines.iter().filter(|l| l.trim() != e).copied().collect();
                std::fs::write(&tmp, kept.join("\n")).unwrap();
                probes.push((name.clone(), which, e.to_string(), repo.clone(), tmp, flag));
            }
        }
    }
    // Every entry's documents re-audited at once, then reported in the order the
    // loop above found them. The method is unchanged, one line removed per run.
    let jobs: Vec<(PathBuf, PathBuf, PathBuf, String)> = probes.iter().map(|(_, _, _, repo, tmp, flag)| (audit.clone(), repo.clone(), tmp.clone(), flag.to_string())).collect();
    let results = run::pool(jobs, run::cores(), |(audit, repo, tmp, flag)| failures(&audit, &repo, &flag, &tmp).0);
    for ((name, which, e, _, _, _), n) in probes.iter().zip(results) {
        if n == 0 {
            println!("  DEAD  {name} {which}: {e} suppresses nothing");
            dead += 1;
        }
    }
    println!(
        "  {total} exemptions audited, {dead} suppressing nothing{}{}",
        if skipped > 0 { format!(", {skipped} unaudited behind a failing list") } else { String::new() },
        if broken > 0 { format!(", {broken} behind an audit that crashed") } else { String::new() }
    );
    std::process::exit(if dead > 0 || skipped > 0 || broken > 0 { 1 } else { 0 });
}
