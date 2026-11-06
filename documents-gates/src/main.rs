//! documents-gates: every gate for the repositories under ~/Documents/GIT,
//! run in parallel.
//!
//! all-gates.sh covers the LaserMadeMusic repositories and knows nothing of the
//! ones beside them, so until 2026-11-06 the Documents repositories were gated
//! by whoever remembered to run each generator's --check by hand. This runs, for
//! every repository: what gates.txt says regenerates or tests it, doc-audit.py
//! over every markdown (with --links unless --offline), a test of every line in
//! .doc-audit-ignore and .doc-audit-generated (does it still suppress
//! anything?), the page conventions (← All projects, Credits, LICENSE, links in
//! a new tab), and finally whether the tree is clean and pushed.
//!
//! Not image metadata: that rule is the LaserMadeMusic repositories' (all-gates.sh
//! runs image-meta-check.py there), and the author kept it to them on 2026-11-06.
//!
//! One pool of jobs over every core. Regenerations go first (they write in
//! place, so one repository's run in order, and only on a clean tree); each
//! repository's read-only jobs join the queue the moment its own regeneration
//! ends, so no repository waits on another's. Clean and pushed run last, when
//! they can see everything the rest left behind. Standard library only.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Condvar, Mutex};
use std::time::{Duration, Instant};

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const EXEMPTION_FILES: [(&str, &str); 2] = [
    (".doc-audit-ignore", "--ignore-file"),
    (".doc-audit-generated", "--generated-file"),
];

struct Opts {
    root: PathBuf,
    tools: PathBuf,
    config: PathBuf,
    threads: usize,
    links: bool,
    verbose: bool,
}

struct Outcome {
    repo: String,
    gate: String,
    ok: bool,
    detail: String,
    took: Duration,
}

#[derive(PartialEq)]
enum Kind {
    Check,
    Regen,
    Nothing,
}

struct Entry {
    repo: String,
    kind: Kind,
    cmd: String,
}

enum Job {
    /// `then` is the repository's read-only work, queued the moment its regeneration ends.
    Regen { repo: String, cmds: Vec<String>, then: Vec<Job> },
    Check { repo: String, cmd: String },
    DocAudit { repo: String, md: String },
    Exemption { repo: String, file: &'static str, flag: &'static str, index: usize, entry: String, mds: Vec<String> },
    Conventions { repo: String },
    Final { repo: String },
}

fn main() {
    let o = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let start = Instant::now();

    let conf = o.config.clone();
    let entries = match read_config(&conf) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("documents-gates: {}: {e}", conf.display());
            std::process::exit(2);
        }
    };
    let found = discover(&o.root);
    let mut outcomes = Vec::new();

    // The list is a claim both ways: every repository found is named, every one named is found.
    let named: Vec<&str> = {
        let mut v: Vec<&str> = entries.iter().map(|e| e.repo.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    for r in &found {
        if !named.contains(&r.as_str()) {
            outcomes.push(fail(r, "named in gates.txt", "found under the root but not in gates.txt"));
        }
    }
    for r in &named {
        if !found.iter().any(|f| f == r) {
            outcomes.push(fail(r, "exists", "named in gates.txt but not a git repository under the root"));
        }
    }

    // One pool for regenerating and reading. A repository's read-only jobs wait
    // only for its own regeneration (they read what it writes), never for another
    // repository's: queued behind the slowest regeneration of all, the first run
    // spent 21s of its 27 waiting on coprime-intervals.
    let t1 = Instant::now();
    let tools = o.tools.to_string_lossy().into_owned();
    let mut jobs = Vec::new();
    let mut reads_now = Vec::new();
    for r in &found {
        let mut reads = Vec::new();
        for e in entries.iter().filter(|e| &e.repo == r) {
            match e.kind {
                Kind::Check => reads.push(Job::Check { repo: r.clone(), cmd: e.cmd.replace("{tools}", &tools) }),
                Kind::Nothing if o.verbose => outcomes.push(Outcome {
                    repo: r.clone(),
                    gate: "nothing to regenerate".into(),
                    ok: true,
                    detail: e.cmd.clone(),
                    took: Duration::ZERO,
                }),
                _ => {}
            }
        }
        let dir = o.root.join(r);
        let mds = tracked(&dir, "*.md");
        for md in &mds {
            reads.push(Job::DocAudit { repo: r.clone(), md: md.clone() });
        }
        for (file, flag) in EXEMPTION_FILES {
            if let Ok(text) = std::fs::read_to_string(dir.join(file)) {
                for (index, line) in text.lines().enumerate() {
                    let entry = line.split('#').next().unwrap_or("").trim();
                    if !entry.is_empty() {
                        reads.push(Job::Exemption {
                            repo: r.clone(),
                            file,
                            flag,
                            index,
                            entry: entry.to_string(),
                            mds: mds.clone(),
                        });
                    }
                }
            }
        }
        reads.push(Job::Conventions { repo: r.clone() });
        let cmds: Vec<String> = entries
            .iter()
            .filter(|e| &e.repo == r && e.kind == Kind::Regen)
            .map(|e| e.cmd.replace("{tools}", &tools))
            .collect();
        if cmds.is_empty() {
            reads_now.extend(reads);
        } else {
            jobs.push(Job::Regen { repo: r.clone(), cmds, then: reads });
        }
    }
    jobs.extend(reads_now); // regenerations first: they are the long poles
    outcomes.extend(pool(jobs, &o));
    let p1 = t1.elapsed();

    // Then clean and pushed, which must see whatever the rest left behind.
    let t2 = Instant::now();
    let jobs = found.iter().map(|r| Job::Final { repo: r.clone() }).collect();
    outcomes.extend(pool(jobs, &o));
    let p2 = t2.elapsed();

    report(outcomes, &o, start.elapsed(), [p1, p2], found.len());
}

fn parse_args() -> Result<Opts, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set")?;
    let mut o = Opts {
        root: PathBuf::from(&home).join("Documents/GIT"),
        tools: Path::new(MANIFEST_DIR).parent().unwrap().to_path_buf(),
        config: Path::new(MANIFEST_DIR).join("gates.txt"),
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        links: true,
        verbose: false,
    };
    for a in std::env::args().skip(1) {
        if let Some(v) = a.strip_prefix("--root=") {
            o.root = PathBuf::from(v);
        } else if let Some(v) = a.strip_prefix("--config=") {
            o.config = PathBuf::from(v);
        } else if let Some(v) = a.strip_prefix("--tools=") {
            o.tools = PathBuf::from(v);
        } else if let Some(v) = a.strip_prefix("--jobs=").or_else(|| a.strip_prefix("-j")) {
            o.threads = v.parse().ok().filter(|&n| n > 0).ok_or(format!("not a job count: {a}"))?;
        } else if a == "--offline" {
            o.links = false;
        } else if a == "-v" || a == "--verbose" {
            o.verbose = true;
        } else if a == "-h" || a == "--help" {
            println!(
                "documents-gates [--root=DIR] [--config=FILE] [--tools=DIR] [--jobs=N|-jN] [--offline] [-v]\n\
                 Runs every gate for the git repositories under DIR (default ~/Documents/GIT),\n\
                 as listed in FILE (default gates.txt beside Cargo.toml).\n\
                 --offline skips the link checks; -v prints every gate, not just the failures.\n\
                 Exits 0 when every gate passes, 1 when any fails, 2 on a usage or config error."
            );
            std::process::exit(0);
        } else {
            return Err(format!("documents-gates: not a flag: {a} (try --help)"));
        }
    }
    if !o.tools.join("doc-audit.py").is_file() {
        return Err(format!("documents-gates: no doc-audit.py in {}", o.tools.display()));
    }
    Ok(o)
}

fn read_config(path: &Path) -> Result<Vec<Entry>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, "::").map(str::trim).collect();
        let kind = match parts.get(1) {
            Some(&"check") => Kind::Check,
            Some(&"regen") => Kind::Regen,
            Some(&"none") => Kind::Nothing,
            _ => return Err(format!("line {}: want `repo :: check|regen|none :: command`", n + 1)),
        };
        match parts.get(2) {
            Some(cmd) if !cmd.is_empty() && !parts[0].is_empty() => out.push(Entry {
                repo: parts[0].to_string(),
                kind,
                cmd: cmd.to_string(),
            }),
            _ => return Err(format!("line {}: no repository or no command", n + 1)),
        }
    }
    if out.is_empty() {
        return Err("names no repository".into());
    }
    Ok(out)
}

fn discover(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().join(".git").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Run the jobs over `o.threads` workers sharing one queue. A job may hand back
/// more jobs, which join the queue; a worker stops only when the queue is empty
/// and no other worker is still busy, since a busy one may yet add work.
fn pool(jobs: Vec<Job>, o: &Opts) -> Vec<Outcome> {
    let state = Mutex::new((VecDeque::from(jobs), 0usize)); // (queue, workers busy)
    let wake = Condvar::new();
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|s| {
        for _ in 0..o.threads {
            let tx = tx.clone();
            let (state, wake) = (&state, &wake);
            s.spawn(move || loop {
                let job = {
                    let mut g = state.lock().unwrap();
                    loop {
                        if let Some(j) = g.0.pop_front() {
                            g.1 += 1;
                            break Some(j);
                        }
                        if g.1 == 0 {
                            break None;
                        }
                        g = wake.wait(g).unwrap();
                    }
                };
                let Some(job) = job else {
                    wake.notify_all();
                    break;
                };
                let (outs, then) = run(job, o);
                for out in outs {
                    tx.send(out).unwrap();
                }
                let mut g = state.lock().unwrap();
                g.0.extend(then);
                g.1 -= 1;
                drop(g);
                wake.notify_all();
            });
        }
        drop(tx);
    });
    rx.into_iter().collect()
}

fn run(job: Job, o: &Opts) -> (Vec<Outcome>, Vec<Job>) {
    let t = Instant::now();
    let mut then = Vec::new();
    let mut out = match job {
        Job::Regen { repo, cmds, then: reads } => {
            then = reads;
            regen(&repo, &cmds, o)
        }
        Job::Check { repo, cmd } => {
            let (ok, text) = sh(&o.root.join(&repo), &cmd);
            vec![result(&repo, &format!("runs: {cmd}"), ok, if ok { String::new() } else { tail(&text, 3) })]
        }
        Job::DocAudit { repo, md } => {
            let mut extra = vec![];
            if o.links {
                extra.push("--links".to_string());
            }
            let gate = format!("doc-audit {md}");
            match doc_audit(o, &o.root.join(&repo), &md, &extra) {
                Ok((0, _)) => vec![result(&repo, &gate, true, String::new())],
                Ok((n, crosses)) => vec![fail(&repo, &gate, &format!("{n} failed: {}", clip(&crosses, 400)))],
                Err(e) => vec![fail(&repo, &gate, &e)],
            }
        }
        Job::Exemption { repo, file, flag, index, entry, mds } => exemption(o, &repo, file, flag, index, &entry, &mds),
        Job::Conventions { repo } => conventions(&o.root.join(&repo), &repo),
        Job::Final { repo } => {
            let dir = o.root.join(&repo);
            let mut v = Vec::new();
            let (ok, status) = git(&dir, &["status", "--porcelain"]);
            let changed: Vec<&str> = status.lines().collect();
            v.push(result(
                &repo,
                "clean",
                ok && changed.is_empty(),
                if ok { list(&changed, 6) } else { tail(&status, 2) },
            ));
            let (ok, ahead) = git(&dir, &["rev-list", "--count", "@{u}..HEAD"]);
            let ahead = ahead.trim();
            v.push(match (ok, ahead) {
                (true, "0") => result(&repo, "pushed", true, String::new()),
                (true, n) => fail(&repo, "pushed", &format!("{n} commit(s) not pushed")),
                (false, _) => fail(&repo, "pushed", "no upstream branch"),
            });
            v
        }
    };
    let took = t.elapsed();
    for x in &mut out {
        if x.took.is_zero() {
            x.took = took;
        }
    }
    (out, then)
}

/// Regenerate in place, in order, on a tree that was clean before. Each command
/// must exit 0 and leave the tree clean: anything it changed was stale.
fn regen(repo: &str, cmds: &[String], o: &Opts) -> Vec<Outcome> {
    let dir = o.root.join(repo);
    let gate = |c: &str| format!("regenerates: {c}");
    let (ok, status) = git(&dir, &["status", "--porcelain"]);
    if !ok || !status.trim().is_empty() {
        return cmds
            .iter()
            .map(|c| fail(repo, &gate(c), "not run: the tree was not clean before, and regenerating would write over that work"))
            .collect();
    }
    let mut out = Vec::new();
    let mut dirty = false;
    for c in cmds {
        if dirty {
            out.push(fail(repo, &gate(c), "not run: the command before it left the tree dirty"));
            continue;
        }
        let t = Instant::now();
        let (ok, text) = sh(&dir, c);
        let (_, status) = git(&dir, &["status", "--porcelain"]);
        let changed: Vec<&str> = status.lines().collect();
        dirty = !changed.is_empty();
        let mut r = if !ok {
            fail(repo, &gate(c), &tail(&text, 3))
        } else if dirty {
            fail(repo, &gate(c), &format!("stale, rewritten in place: {}", list(&changed, 6)))
        } else {
            result(repo, &gate(c), true, String::new())
        };
        r.took = t.elapsed();
        out.push(r);
    }
    out
}

/// Does this one exemption still suppress anything? Run every markdown in the
/// repository against a COPY of the file with this line removed; the line is
/// live if any document then fails. The copy, never the repository's own file.
fn exemption(o: &Opts, repo: &str, file: &str, flag: &str, index: usize, entry: &str, mds: &[String]) -> Vec<Outcome> {
    let dir = o.root.join(repo);
    let gate = format!("{file} still needs: {entry}");
    let text = match std::fs::read_to_string(dir.join(file)) {
        Ok(t) => t,
        Err(e) => return vec![fail(repo, &gate, &e.to_string())],
    };
    let without: String = text
        .lines()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, l)| format!("{l}\n"))
        .collect();
    let tmp = std::env::temp_dir().join(format!(
        "documents-gates-{}-{}{}-{}",
        std::process::id(),
        repo,
        file,
        index
    ));
    if let Err(e) = std::fs::write(&tmp, without) {
        return vec![fail(repo, &gate, &format!("could not write the copy: {e}"))];
    }
    let arg = format!("{flag}={}", tmp.display());
    let mut live = Ok(false);
    for md in mds {
        match doc_audit(o, &dir, md, &[arg.clone()]) {
            Ok((n, _)) if n > 0 => {
                live = Ok(true);
                break;
            }
            Ok(_) => {}
            Err(e) => {
                live = Err(e);
                break;
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    match live {
        Ok(true) => vec![result(repo, &gate, true, String::new())],
        Ok(false) => vec![fail(repo, &gate, "suppresses nothing in any document; delete the line")],
        Err(e) => vec![fail(repo, &gate, &e)],
    }
}

/// The page conventions every one of these repositories follows.
fn conventions(dir: &Path, repo: &str) -> Vec<Outcome> {
    let mut v = Vec::new();
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap_or_default();
    let top: String = readme.lines().take(8).collect::<Vec<_>>().join("\n");
    v.push(result(
        repo,
        "README opens with ← All projects",
        top.contains("All projects](https://gernreich.github.io"),
        "no link to https://gernreich.github.io in its first eight lines".into(),
    ));
    v.push(result(
        repo,
        "README has a Credits section",
        readme.lines().any(|l| l.trim() == "## Credits"),
        "no `## Credits` heading".into(),
    ));
    v.push(result(repo, "LICENSE", dir.join("LICENSE").is_file(), "no LICENSE file".into()));

    let page = ["index.html", "docs/index.html"].iter().map(|p| dir.join(p)).find(|p| p.is_file());
    v.push(match page {
        Some(p) => {
            let t = std::fs::read_to_string(&p).unwrap_or_default();
            result(repo, "page opens with ← All projects", t.contains("All projects"), format!("{} has no ← All projects", p.display()))
        }
        None => fail(repo, "page opens with ← All projects", "no index.html or docs/index.html"),
    });

    let mut bad = Vec::new();
    for f in tracked(dir, "*.html") {
        let n = std::fs::read_to_string(dir.join(&f)).map(|t| same_tab_links(&t)).unwrap_or(0);
        if n > 0 {
            bad.push(format!("{f} ({n})"));
        }
    }
    v.push(result(
        repo,
        "page links open in a new tab",
        bad.is_empty(),
        format!("links without target=\"_blank\": {}", bad.join(", ")),
    ));
    for x in &mut v {
        if x.ok {
            x.detail.clear();
        }
    }
    v
}

/// Links that would open in the same tab. In-page #anchors, ?query links (a
/// page switching its own mode) and data: downloads are meant to; a link
/// written inside a script string (href=\"…\") counts like any other.
fn same_tab_links(html: &str) -> usize {
    let mut n = 0;
    let mut i = 0;
    while let Some(p) = html[i..].find("<a") {
        let s = i + p;
        i = s + 2;
        match html.as_bytes().get(s + 2) {
            Some(b' ' | b'\n' | b'\t' | b'\r') => {}
            _ => continue,
        }
        let Some(e) = html[s..].find('>') else { break };
        let tag = &html[s..=s + e];
        i = s + e + 1;
        let Some(h) = tag.find("href=") else { continue };
        let value = tag[h + 5..].trim_start_matches(['\\', '"', '\'']);
        if value.starts_with('#') || value.starts_with('?') || value.starts_with("data:") || tag.contains("target=") {
            continue;
        }
        n += 1;
    }
    n
}

/// doc-audit.py on one document: (failed count, the ✗ lines). Fail-closed: a
/// run that prints no tally is an error, never a pass.
fn doc_audit(o: &Opts, dir: &Path, md: &str, extra: &[String]) -> Result<(usize, String), String> {
    let mut c = Command::new("python3");
    c.arg(o.tools.join("doc-audit.py")).arg(md).args(extra).current_dir(dir);
    let out = c.output().map_err(|e| format!("could not run doc-audit.py: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let tally = text
        .lines()
        .rev()
        .find(|l| l.contains(" passed,") && l.contains(" failed"))
        .ok_or_else(|| format!("no tally from doc-audit.py: {}", tail(&text, 2)))?;
    let failed = tally
        .split(',')
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or_else(|| format!("unreadable tally: {}", tally.trim()))?;
    Ok((failed, grep(&text, "✗")))
}

fn tracked(dir: &Path, pattern: &str) -> Vec<String> {
    let (ok, text) = git(dir, &["ls-files", pattern]);
    if ok {
        text.lines().map(String::from).collect()
    } else {
        vec![]
    }
}

fn sh(dir: &Path, cmd: &str) -> (bool, String) {
    match Command::new("sh").arg("-c").arg(cmd).current_dir(dir).output() {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr),
        ),
        Err(e) => (false, e.to_string()),
    }
}

fn git(dir: &Path, args: &[&str]) -> (bool, String) {
    match Command::new("git").arg("-C").arg(dir).args(args).output() {
        Ok(o) if o.status.success() => (true, String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => (false, String::from_utf8_lossy(&o.stderr).into_owned()),
        Err(e) => (false, e.to_string()),
    }
}

fn result(repo: &str, gate: &str, ok: bool, detail: String) -> Outcome {
    Outcome { repo: repo.into(), gate: gate.into(), ok, detail, took: Duration::ZERO }
}

fn fail(repo: &str, gate: &str, detail: &str) -> Outcome {
    result(repo, gate, false, detail.into())
}

fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    clip(&lines[lines.len().saturating_sub(n)..].join(" | "), 400)
}

fn grep(text: &str, needle: &str) -> String {
    text.lines().filter(|l| l.contains(needle)).map(str::trim).collect::<Vec<_>>().join(" | ")
}

fn list(items: &[&str], n: usize) -> String {
    let mut s = items.iter().take(n).map(|l| l.trim()).collect::<Vec<_>>().join(", ");
    if items.len() > n {
        s += &format!(" and {} more", items.len() - n);
    }
    s
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn report(mut outcomes: Vec<Outcome>, o: &Opts, total: Duration, phases: [Duration; 2], repos: usize) {
    outcomes.sort_by(|a, b| (a.repo.to_lowercase(), &a.gate).cmp(&(b.repo.to_lowercase(), &b.gate)));
    let mut by_repo: BTreeMap<String, Vec<&Outcome>> = BTreeMap::new();
    for x in &outcomes {
        by_repo.entry(x.repo.to_lowercase()).or_default().push(x);
    }
    for list in by_repo.values() {
        let failed = list.iter().filter(|x| !x.ok).count();
        let name = &list[0].repo;
        let verdict = if failed == 0 { "ok".to_string() } else { format!("FAIL {failed}") };
        println!("{name:<24} {verdict:<7} {} gates", list.len());
        for x in list {
            if !x.ok || o.verbose {
                let mark = if x.ok { "ok  " } else { "FAIL" };
                let detail = if x.detail.is_empty() { String::new() } else { format!(" -- {}", x.detail) };
                println!("    {mark} {}{detail}", x.gate);
            }
        }
    }
    let slowest = outcomes.iter().max_by_key(|x| x.took);
    println!();
    println!(
        "{} gates in {repos} repos, ran in {:.1}s on {} threads (regenerate and read {:.1}s, clean and pushed {:.1}s){}{}",
        outcomes.len(),
        total.as_secs_f64(),
        o.threads,
        phases[0].as_secs_f64(),
        phases[1].as_secs_f64(),
        if o.links { "" } else { ", links not checked (--offline)" },
        slowest.map(|x| format!("; slowest {} in {} ({:.1}s)", x.gate, x.repo, x.took.as_secs_f64())).unwrap_or_default(),
    );
    let failing: Vec<&Outcome> = outcomes.iter().filter(|x| !x.ok).collect();
    if !failing.is_empty() {
        println!("FAILING GATES:");
        for x in &failing {
            println!("  {}: {}", x.repo, x.gate);
        }
    }
    println!("GATES FAILING: {}", failing.len());
    std::process::exit(if failing.is_empty() { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_tab_links_counts_only_links_that_leave_the_page() {
        let html = r##"<a href="#top">a</a> <a href="?test">b</a> <a href="x.html">c</a>
            <a href="https://e.org" target="_blank" rel="noopener">d</a> <abbr title="x">e</abbr>
            <a download="c.csv" href="data:text/csv;base64,AA">f</a> "<a href=\"index.html#g1\">g</a>"
            <a class="k" href='y.html'>h</a>"##;
        // x.html, index.html#g1 inside a script string, y.html
        assert_eq!(same_tab_links(html), 3);
    }

    #[test]
    fn config_rejects_a_line_without_a_kind() {
        let p = std::env::temp_dir().join(format!("documents-gates-test-{}", std::process::id()));
        std::fs::write(&p, "repo :: python3 x.py\n").unwrap();
        assert!(read_config(&p).is_err());
        std::fs::write(&p, "# c\nrepo :: check :: python3 x.py --check\nother :: none :: why\n").unwrap();
        let e = read_config(&p).unwrap();
        assert_eq!(e.len(), 2);
        assert!(e[0].kind == Kind::Check && e[0].cmd == "python3 x.py --check");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn tail_keeps_the_last_nonempty_lines() {
        assert_eq!(tail("a\n\nb\nc\n\n", 2), "b | c");
    }
}
