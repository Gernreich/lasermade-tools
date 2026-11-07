//! Mechanical audit for a markdown writeup and the HTML generated from it.
//!
//!     ~/LaserMadeMusic/GIT/lasermade-tools/target/release/doc-audit WRITEUP.md
//!              [--html PAGE.html]
//!              [--rebuild "~/LaserMadeMusic/GIT/lasermade-tools/target/release/md2html {md} {out}"]
//!              [--links] [--run-blocks]
//!
//! Project-agnostic. Everything it checks is a claim the document makes about
//! itself or about files on disk, so a failure is always a real inconsistency.
//! Judgment-level review (does this instruction mislead? is the order safe?) is
//! not here — that is the reading half, see the writeup-review skill.
//!
//! Ported from Python on 2026-11-07 and run against the Python over every
//! document in every repository until the two printed the same lines. The
//! one change is speed: --links fetches its URLs a few at a time rather than
//! one after another, and still reports them in the same order.
use lasermade_tools::py::{head, ints_repr, list_repr, str_repr};
use lasermade_tools::re::{all, findall1, g, has, rx, split, sub_str};
use lasermade_tools::{die, fnmatchcase, html, run, text};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Args {
    md: String,
    html: Option<String>,
    rebuild: Option<String>,
    links: bool,
    run_blocks: bool,
    ignore: String,
    ignore_file: String,
    generated_file: String,
    strict_h1: bool,
}

const USAGE: &str = "usage: doc-audit [-h] [--html HTML] [--rebuild REBUILD] [--links] [--run-blocks] [--ignore IGNORE] [--ignore-file IGNORE_FILE] [--generated-file GENERATED_FILE] [--strict-h1] md";

fn usage_error(msg: &str) -> ! {
    eprintln!("{USAGE}\ndoc-audit: error: {msg}");
    std::process::exit(2)
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        md: String::new(),
        html: None,
        rebuild: None,
        links: false,
        run_blocks: false,
        ignore: String::new(),
        ignore_file: String::new(),
        generated_file: String::new(),
        strict_h1: false,
    };
    let mut md = None;
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].clone();
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = || -> String {
            if let Some(v) = &inline {
                return v.clone();
            }
            i += 1;
            argv.get(i).cloned().unwrap_or_else(|| usage_error(&format!("argument {key}: expected one argument")))
        };
        match key.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}\n\nThe options: --html PAGE checks the generated page too; --rebuild 'CMD {{md}} {{out}}'\nregenerates it and compares; --links fetches every https URL; --run-blocks re-runs\nfenced terminal sessions and diffs them (restoring any file they write); --ignore A,B\nnames files mentioned in prose but not shipped; --ignore-file and --generated-file\nreplace the repository's .doc-audit-ignore and .doc-audit-generated; --strict-h1\nrequires exactly one <h1>.");
                std::process::exit(0);
            }
            "--html" => a.html = Some(value()),
            "--rebuild" => a.rebuild = Some(value()),
            "--links" => a.links = true,
            "--run-blocks" => a.run_blocks = true,
            "--ignore" => a.ignore = value(),
            "--ignore-file" => a.ignore_file = value(),
            "--generated-file" => a.generated_file = value(),
            "--strict-h1" => a.strict_h1 = true,
            s if s.starts_with('-') && s.len() > 1 => usage_error(&format!("unrecognized arguments: {s}")),
            _ if md.is_none() => md = Some(arg.clone()),
            _ => usage_error(&format!("unrecognized arguments: {arg}")),
        }
        i += 1;
    }
    a.md = md.unwrap_or_else(|| usage_error("the following arguments are required: md"));
    a
}

/// Python's str.strip() whitespace
fn pyws(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}
fn strip(s: &str) -> &str {
    s.trim_matches(pyws)
}
fn indented(s: &str) -> bool {
    s.starts_with("    ") || s.starts_with('\t')
}

/// Everything inside a ``` block is quoted content, not markdown. A shell comment
/// there is not a heading — writeups quote sessions containing '# ...' lines,
/// which the renderer ignores and a naive regex reads as an <h1>. Same toggle rule
/// md2html uses, so the two agree on what is code. Filename mentions are
/// deliberately NOT stripped: `node foo.js` inside a block is a real reference.
///
/// An INDENTED code block is quoted content too (CommonMark: it needs a blank
/// line before it and cannot interrupt a paragraph, and four spaces under a list
/// is list continuation, not code). tools/CLAUDE.md quoted a gate's output four
/// spaces in, and the counts check read "393 checks, 0 failed" as a claim.
///
/// Stripped lines are BLANKED, not deleted, so every line number the checks
/// report is the line number in the file, and a removed block cannot make
/// neighbours of two words that were never adjacent.
fn strip_fences(t: &str) -> String {
    let lines: Vec<&str> = t.split('\n').collect();
    let list = rx(r"^\s*(?:[-*+]|\d+\.)\s");
    let mut incode = vec![false; lines.len()];
    let mut prev = "";
    for (i, ln) in lines.iter().enumerate() {
        if !strip(ln).is_empty() && !indented(ln) {
            prev = ln;
            continue;
        }
        if strip(ln).is_empty() {
            continue;
        }
        let mut j = i as isize - 1;
        while j >= 0 && strip(lines[j as usize]).is_empty() {
            j -= 1;
        }
        let opens = j < 0
            || (!indented(lines[j as usize]) && strip(lines[j as usize]) == strip(prev) && !has(&list, prev));
        if opens || (i > 0 && incode[i - 1]) {
            incode[i] = true;
        }
    }
    let mut out = Vec::with_capacity(lines.len());
    let mut inblock = false;
    for (i, ln) in lines.iter().enumerate() {
        if ln.starts_with("```") {
            inblock = !inblock;
            out.push("");
            continue;
        }
        out.push(if inblock || incode[i] { "" } else { ln });
    }
    out.join("\n")
}

struct Audit {
    notes: Vec<String>,
    fails: Vec<String>,
}

impl Audit {
    fn ok(&mut self, label: &str, good: bool, detail: &str) {
        let line = format!("{} {label}{}", if good { "  ✓" } else { "  ✗" }, if detail.is_empty() { String::new() } else { format!("  {detail}") });
        if good {
            self.notes.push(line)
        } else {
            self.fails.push(line)
        }
    }
}

fn exists(p: &Path) -> bool {
    p.exists()
}

/// Every directory under root (root included) that pathlib's rglob would
/// search: it does not descend through a symlinked directory.
fn dirs_under(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let mut i = 0;
    while i < out.len() {
        if let Ok(rd) = std::fs::read_dir(&out[i]) {
            let mut kids: Vec<PathBuf> = rd
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.path())
                .collect();
            kids.sort();
            out.extend(kids);
        }
        i += 1;
    }
    out
}

fn mtime(p: &Path) -> std::io::Result<f64> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(p)?;
    Ok(m.mtime() as f64 + m.mtime_nsec() as f64 * 1e-9)
}

/// urllib.request.urlopen's answer for a URL: the status it ends on after
/// following 301, 302, 303 and 307 (Python 3.9 does not follow 308), or the
/// error. Fetched with curl.
fn http_status(u: &str) -> String {
    let mut url = u.to_string();
    for _ in 0..=10 {
        let o = std::process::Command::new("curl")
            .args(["-s", "-o", "/dev/null", "-A", "Mozilla/5.0", "--max-time", "25", "-w", "%{http_code} %{redirect_url}", "--"])
            .arg(&url)
            .output();
        let Ok(o) = o else { return "curl not found".into() };
        let out = String::from_utf8_lossy(&o.stdout).into_owned();
        let (code, next) = out.split_once(' ').unwrap_or((out.as_str(), ""));
        if code == "000" || code.is_empty() {
            return format!("<urlopen error curl exit {}>", o.status.code().unwrap_or(-1));
        }
        if matches!(code, "301" | "302" | "303" | "307") && !next.is_empty() {
            url = next.trim().to_string();
            continue;
        }
        return code.to_string();
    }
    "HTTP Error 310: too many redirects".into()
}

fn main() {
    let a = parse_args();
    let md_s = text::pathstr(&a.md);
    let md = PathBuf::from(&md_s);
    let md_parent = md.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_path_buf() }).unwrap_or_else(|| PathBuf::from("."));
    let md_name = md.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();

    // Resolve file references against the repository root, not the document's own
    // directory. A document that sits in a subdirectory (coupons/README.md)
    // legitimately names files one level up, and its siblings are documented by the
    // writeups above it. For a document at the repo root this is the same directory.
    let mut root = md_parent.clone();
    if let Some(top) = text::git(&md_parent, &["rev-parse", "--show-toplevel"]) {
        let top = top.trim();
        if !top.is_empty() {
            root = PathBuf::from(top);
        }
    }
    // git ls-files reports paths relative to the repository root, so identify the
    // document the same way.
    let md_rel = match (std::fs::canonicalize(&md), std::fs::canonicalize(&root)) {
        (Ok(m), Ok(r)) => m.strip_prefix(&r).map(|p| p.to_string_lossy().into_owned()).unwrap_or(md_name.clone()),
        _ => md_name.clone(),
    };
    let src = text::read_strict(&md).unwrap_or_else(|e| die(format!("doc-audit: {md_s}: {e}")));
    let page: Option<String> = a.html.as_ref().map(|h| text::read_strict(h).unwrap_or_else(|e| die(format!("doc-audit: {h}: {e}"))));
    let mut au = Audit { notes: Vec::new(), fails: Vec::new() };

    let prose_src = strip_fences(&src);

    // ── 1. files the document names, and files nobody names ──────────────────────
    let mut prose_only: HashSet<String> = a.ignore.split(',').map(strip).filter(|x| !x.is_empty()).map(String::from).collect();
    // A repository may legitimately name files it does not ship, and states those
    // exceptions once, in .doc-audit-ignore at the root: one name per line, # for
    // comments. --ignore-file points that somewhere else, so ignore-audit can test
    // the list against a COPY with one line removed.
    let ignore_file = if a.ignore_file.is_empty() { root.join(".doc-audit-ignore") } else { PathBuf::from(&a.ignore_file) };
    if ignore_file.exists() {
        for ln in text::read(&ignore_file).unwrap_or_default().split('\n') {
            let ln = strip(ln.split('#').next().unwrap());
            if !ln.is_empty() {
                prose_only.insert(ln.to_string());
            }
        }
    }
    // Directories of machine output, documented collectively, are declared in
    // .doc-audit-generated: one path per line. A line holding * ? or [ is a glob
    // over tracked paths, for machine output loose beside hand-written files.
    let generated_file = if a.generated_file.is_empty() { root.join(".doc-audit-generated") } else { PathBuf::from(&a.generated_file) };
    let (mut generated_dirs, mut generated_globs): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    if generated_file.exists() {
        for ln in text::read(&generated_file).unwrap_or_default().split('\n') {
            let ln = strip(ln.split('#').next().unwrap()).trim_end_matches('/');
            if !ln.is_empty() && ln.chars().any(|c| "*?[".contains(c)) {
                generated_globs.push(ln.to_string());
            } else if !ln.is_empty() {
                generated_dirs.push(format!("{ln}/"));
            }
        }
    }
    let no_urls = sub_str(&rx(r"https?://\S+"), &src, " "); // a URL's tail is not a local file
    // The extension must end the name, or ".json" matched ".js" inside it.
    let refs: Vec<String> = findall1(&rx(r"[A-Za-z0-9_][A-Za-z0-9_.-]*\.(?:svg|js|py|md|html|png|jpg|jpeg|css|zip)(?![A-Za-z0-9])"), &no_urls, 0)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    // A document may name a file that lives in a subdirectory. Accept either:
    // exists at ROOT, or exists anywhere below it under that name.
    let dirs: Vec<PathBuf> = dirs_under(&root).into_iter().filter(|d| !d.components().any(|c| c.as_os_str() == ".git")).collect();
    let somewhere = |name: &str| -> bool {
        if root.join(name).exists() {
            return true;
        }
        let base = Path::new(name).file_name().map(|s| s.to_os_string()).unwrap_or_default();
        dirs.iter().any(|d| d.join(&base).exists())
    };
    let missing: Vec<&String> = refs.iter().filter(|r| !somewhere(r) && !prose_only.contains(*r)).collect();
    au.ok("every file the document names exists", missing.is_empty(), &if missing.is_empty() { String::new() } else { list_repr(&missing) });

    // A name and a path are different claims, and only one of them is a URL. An
    // <img src> resolves against the page's own directory and nowhere else, so link
    // and image targets are resolved from the DOCUMENT's directory, not from ROOT.
    let link_target = rx(r#"<img\b[^>]*?\ssrc="([^"]*)"|<a\b[^>]*?\shref="([^"]*)"|!\[[^\]]*\]\(\s*<?([^)>\s]+)|(?<!!)\[[^\]]*\]\(\s*<?([^)>\s]+)"#);
    let skip_scheme = ["http://", "https://", "data:", "mailto:", "tel:", "//", "#", "{"];
    // A LINK INSIDE BACKTICKS IS QUOTED SYNTAX, NOT A LINK: blank code spans too.
    let link_src = sub_str(&rx(r"`[^`\n]*`"), &prose_src, " ");
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    for m in all(&link_target, &link_src) {
        let u = (1..=4).map(|i| m.get(i)).find(|x| x.is_some()).flatten().map(|x| x.as_str()).unwrap_or("");
        let u = strip(u.split('#').next().unwrap().split('?').next().unwrap());
        if !u.is_empty() && !skip_scheme.iter().any(|s| u.starts_with(s)) && !exists(&md_parent.join(u)) && !prose_only.contains(u) {
            unresolved.insert(u.to_string());
        }
    }
    let unresolved: Vec<String> = unresolved.into_iter().collect();
    au.ok("every link and image path resolves from the document", unresolved.is_empty(), &if unresolved.is_empty() { String::new() } else { list_repr(&unresolved) });

    // ONLY THE GIT CALL IS ALLOWED TO FAIL QUIETLY.
    let tracked: Option<Vec<String>> = text::git(&root, &["ls-files"]).map(|s| s.split_whitespace().map(String::from).collect());
    if tracked.is_none() {
        au.notes.push("  – not a git repo, skipped the orphan check".into());
    }
    if let Some(tracked) = &tracked {
        // "Somewhere" is the whole repository: a file named in any of its
        // documents is documented.
        let mut docs = src.clone();
        for t in tracked {
            if t.ends_with(".md") && *t != md_rel {
                if let Some(s) = text::read(root.join(t)) {
                    docs.push_str(&s);
                }
            }
        }
        let tracked_set: HashSet<&String> = tracked.iter().collect();
        // A generated page is the rendering of its markdown; README.md needs no
        // introduction; dot-directories are plumbing; declared generated
        // directories are documented by the directory.
        let mut skip: HashSet<String> = ["LICENSE", ".gitignore", "README.md"].iter().map(|s| s.to_string()).collect();
        skip.insert(md_rel.clone());
        if let Some(h) = &a.html {
            skip.insert(Path::new(h).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        }
        for t in tracked {
            let generated = t.ends_with(".html") && tracked_set.contains(&format!("{}.md", &t[..t.len() - 5]));
            let infra = t.split('/').any(|p| p.starts_with('.'));
            let machine = generated_dirs.iter().any(|d| t.starts_with(d.as_str())) || generated_globs.iter().any(|gl| fnmatchcase(t, gl));
            if generated || infra || machine {
                skip.insert(t.clone());
            }
        }
        let named = |f: &str| docs.contains(f) || docs.contains(Path::new(f).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default().as_str());
        let orphans: Vec<&String> = tracked.iter().filter(|f| !named(f) && !skip.contains(*f)).collect();
        au.ok("every tracked file is mentioned somewhere", orphans.is_empty(), &if orphans.is_empty() { String::new() } else { list_repr(&orphans) });

        // A file deleted from the index but left in the working tree is invisible to
        // every check above. Genuinely new untracked files are not reported.
        let others: Vec<String> = text::decode(
            &std::process::Command::new("git").args(["ls-files", "--others", "--exclude-standard"]).current_dir(&root).output().map(|o| o.stdout).unwrap_or_default(),
        )
        .split_whitespace()
        .map(String::from)
        .collect();
        let mut left: Vec<String> = Vec::new();
        if !others.is_empty() {
            let seen: HashSet<String> = text::decode(
                &std::process::Command::new("git")
                    .args(["log", "--all", "--diff-filter=D", "--name-only", "--format="])
                    .current_dir(&root)
                    .output()
                    .map(|o| o.stdout)
                    .unwrap_or_default(),
            )
            .split_whitespace()
            .map(String::from)
            .collect();
            left = others
                .into_iter()
                .filter(|f| seen.contains(f) && !prose_only.contains(f) && !prose_only.contains(&Path::new(f).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()))
                .collect();
            left.sort();
        }
        au.ok("no file deleted from the index is still on disk", left.is_empty(), &if left.is_empty() { String::new() } else { list_repr(&left) });

        // A named file and a shown file are different things. Per document, not
        // pooled: an image a document names is an image that document should show.
        // A full-size photograph offered behind its own thumbnail is shown.
        let last = |s: &str| s.rsplit('/').next().unwrap().to_string();
        let mut shown: HashSet<String> = HashSet::new();
        for p in [r#"<img[^>]+src="([^"]+)""#, r"!\[[^\]]*\]\(([^)\s]+)", r#"<a[^>]+href="([^"]+)"[^>]*>\s*<img"#] {
            shown.extend(findall1(&rx(p), &src, 1).iter().map(|s| last(s)));
        }
        let raster = [".jpg", ".jpeg", ".png", ".gif", ".webp"];
        let named_here: BTreeSet<String> = refs.iter().filter(|r| raster.iter().any(|e| r.to_lowercase().ends_with(e))).map(|r| last(r)).collect();
        let unshown: Vec<&String> = named_here.iter().filter(|n| !shown.contains(*n) && !prose_only.contains(*n)).collect();
        au.ok("every image is displayed, not just named", unshown.is_empty(), &if unshown.is_empty() { String::new() } else { list_repr(&unshown) });
    }

    // ── 2. in-page anchors ───────────────────────────────────────────────────────
    // GitHub's rule: drop anything that is not a letter, digit, space or hyphen,
    // then turn spaces into hyphens. An em dash therefore leaves a double hyphen.
    let (r_marks, r_keep, r_ws) = (rx(r"`|\*\*|\*"), rx(r"[^a-z0-9 \-]"), rx(r"\s"));
    let slug = |t: &str| sub_str(&r_ws, &sub_str(&r_keep, &sub_str(&r_marks, t, "").to_lowercase(), ""), "-");
    let heads: Vec<(usize, String)> = all(&rx(r"(?m)^(#{1,6})\s+(.*)$"), &prose_src).iter().map(|m| (g(m, 1).len(), g(m, 2).to_string())).collect();
    let ids: HashSet<String> = heads.iter().map(|(_, t)| slug(t)).collect();
    let dangling: Vec<String> = findall1(&rx(r"\]\(#([a-z0-9-]+)\)"), &prose_src, 1).into_iter().filter(|m| !ids.contains(m)).collect::<BTreeSet<_>>().into_iter().collect();
    au.ok("in-page links resolve to a heading", dangling.is_empty(), &if dangling.is_empty() { String::new() } else { list_repr(&dangling) });

    // ── 3. heading structure ─────────────────────────────────────────────────────
    let lv: Vec<usize> = heads.iter().map(|(h, _)| *h).collect();
    let skips: Vec<String> = lv.windows(2).filter(|w| w[1] as isize - w[0] as isize > 1).map(|w| format!("({}, {})", w[0], w[1])).collect();
    au.ok("heading levels never skip a level", skips.is_empty(), &if skips.is_empty() { String::new() } else { format!("[{}]", skips.join(", ")) });
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, t) in &heads {
        *counts.entry(t.as_str()).or_default() += 1;
    }
    let dupes: Vec<&str> = counts.iter().filter(|(_, n)| **n > 1).map(|(t, _)| *t).collect();
    au.ok("no two headings share a title", dupes.is_empty(), &if dupes.is_empty() { String::new() } else { list_repr(&dupes) });

    // ── 4. counts the prose claims about its own lists ───────────────────────────
    let words = ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];
    let item = rx(r"^\s*(?:\d+\.|[-*+])\s");
    let bold = rx(r"(?m)^\*\*(?:`[^`]*`|[^*`])+\*\*");
    let heading = rx(r"(?m)^#{1,6}\s");
    // The lookbehind keeps a numeral whole: "2,868 rules" is not "868 rules".
    for m in all(&rx(r"(?<![\d,.])\b(One|Two|Three|Four|Five|Six|Seven|Eight|Nine|Ten|\d+)\s+(things|pitfalls|rules|steps|reasons|ways|checks|traps)\b"), &prose_src) {
        let m0 = m.get(0).unwrap();
        let word = g(&m, 1);
        // A document that quotes the pattern as an example is not making the claim.
        let before = prose_src[..m0.start()].chars().last();
        let after = prose_src[m0.end()..].chars().next();
        if before.map(|c| "\"“`".contains(c)).unwrap_or(true) && after.map(|c| "\"”`".contains(c)).unwrap_or(true) {
            continue;
        }
        // A NUMERAL is a measured total, not prose introducing a list.
        if word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let claimed = words.iter().position(|w| *w == word.to_lowercase()).map(|i| i + 1).unwrap();
        // Count the list that FOLLOWS the claim, and every item in it.
        let tail = split(&heading, &prose_src[m0.end()..])[0];
        let (mut items, mut seen) = (0usize, false);
        for ln in tail.split('\n') {
            if has(&item, ln) {
                items += 1;
                seen = true;
            } else if seen && strip(ln).is_empty() {
                continue; // a blank line between items keeps the list open
            } else if seen && !(ln.starts_with(' ') || ln.starts_with('\t')) {
                break; // unindented prose ends the list
            }
        }
        if items == 0 {
            // some lists are bold-lead paragraphs rather than markdown list items
            items = all(&bold, tail).len();
        }
        if items > 0 {
            au.ok(&format!("\"{}\" matches the list under it", m0.as_str()), items == claimed, &format!("claimed {claimed}, found {items}"));
        }
    }

    // ── 5. prose hygiene ─────────────────────────────────────────────────────────
    let prose = sub_str(&rx(r"https?://\S+"), &prose_src, "");
    // A code span is a quotation, not prose; a NON-SPACE placeholder keeps words on
    // either side of it from becoming neighbours.
    let spanless = sub_str(&rx(r"`[^`]*`"), &prose, "\x00");
    let dbl = findall1(&rx(r"\b([A-Za-z]{3,})\s+\1\b"), &spanless, 1);
    au.ok("no doubled words", dbl.is_empty(), &if dbl.is_empty() { String::new() } else { list_repr(&dbl) });
    // Balanced per PARAGRAPH, not per line: a code span may wrap across a line
    // break. The line given is where the paragraph starts.
    let (mut para, mut start, mut odd) = (0usize, 1usize, Vec::new());
    let plines: Vec<&str> = prose.split('\n').chain(std::iter::once("")).collect();
    for (i, l) in plines.iter().enumerate() {
        if !strip(l).is_empty() {
            if para == 0 {
                start = i + 1;
            }
            para += l.matches('`').count();
        } else {
            if para % 2 == 1 {
                odd.push(start);
            }
            para = 0;
        }
    }
    au.ok("code spans balanced in every paragraph", odd.is_empty(), &if odd.is_empty() { String::new() } else { format!("paragraphs starting at {}", ints_repr(&odd)) });

    // ── 6. the generated page ────────────────────────────────────────────────────
    if let Some(page) = page.as_ref().filter(|p| !p.is_empty()) {
        let hl: Vec<String> = findall1(&rx(r"<h([1-6])"), page, 1);
        let h1 = hl.iter().filter(|x| *x == "1").count();
        if a.strict_h1 {
            au.ok("page has exactly one <h1>", h1 == 1, &format!("found {h1}"));
        } else if h1 > 1 {
            au.notes.push(format!("  – {h1} <h1> elements; fine for a chaptered page, pass --strict-h1 if you want one"));
        }
        let pids: HashSet<String> = findall1(&rx(r#"id="([^"]+)""#), page, 1).into_iter().collect();
        let broken: Vec<String> = findall1(&rx(r##"href="#([^"]+)""##), page, 1).into_iter().filter(|l| !pids.contains(l)).collect::<BTreeSet<_>>().into_iter().collect();
        au.ok("no broken anchors in the page", broken.is_empty(), &list_repr(&broken));
        let alt = rx(r#"alt="[^"]+""#);
        let bad: Vec<usize> = findall1(&rx(r"(?s)<figure>(.*?)</figure>"), page, 1)
            .iter()
            .enumerate()
            .filter(|(_, f)| (f.contains("<svg") && !f.contains("aria-label")) || (f.contains("<img") && !has(&alt, f)))
            .map(|(i, _)| i + 1)
            .collect();
        au.ok("every figure has a text alternative", bad.is_empty(), &if bad.is_empty() { String::new() } else { format!("figures {}", ints_repr(&bad)) });
        // MARKDOWN THAT THE RENDERER DECLINED TO CONVERT reaches the page as literal
        // text, and every check above still passes it. Search the page's TEXT, not
        // its markup: <pre> and <code> quote markdown on purpose, so both come out.
        let body = sub_str(&rx(r"(?s)<(pre|code)\b[^>]*>.*?</\1>"), page, " ");
        let body = html::unescape(&sub_str(&rx(r"<[^>]+>"), &body, " "));
        let leftover = findall1(&rx(r"!?\[[^\]\n]*\]\([^)\s]+\)"), &body, 0);
        let detail = if leftover.is_empty() {
            String::new()
        } else {
            list_repr(&leftover[..leftover.len().min(3)]) + &if leftover.len() > 3 { format!(" and {} more", leftover.len() - 3) } else { String::new() }
        };
        au.ok("no markdown left unrendered in the page", leftover.is_empty(), &detail);
        au.ok("wide tables scroll inside their own box", page.matches("<table>").count() == 0 || page.contains("overflow-x:auto"), "");
        au.ok("print stylesheet present", page.contains("@media print"), "");
        au.ok("respects a dark theme", page.contains("prefers-color-scheme"), "");
        au.ok("page declares a language", has(&rx(r"<html[^>]+lang="), page), "");
        if let Some(rebuild) = &a.rebuild {
            let tmp = lasermade_tools::TempDir::new("doc-audit");
            let out = tmp.0.join("page.html");
            let cmd = rebuild.replace("{md}", &md_s).replace("{out}", &out.to_string_lossy());
            let _ = run::shell(&cmd, Path::new("."), &[], None);
            let current = text::read_strict(&out).map(|s| s == *page).unwrap_or(false);
            au.ok("page is current with the markdown", current, "");
        } else {
            // Without --rebuild the page cannot be regenerated and compared, but
            // md2html INLINES an SVG figure, so a regenerated SVG leaves the page
            // stale while the markdown is untouched. Modification times catch
            // that, and the detail says plainly it is the weaker test.
            let mut srcs = vec![md.clone()];
            srcs.extend(findall1(&rx(r"!\[[^\]]*\]\(([^)\s]+\.svg)\)"), &src, 1).iter().map(|m| md_parent.join(m)));
            let srcs: Vec<PathBuf> = srcs.into_iter().filter(|p| p.exists()).collect();
            if let Ok(ht) = mtime(Path::new(a.html.as_ref().unwrap())) {
                let mut stale: BTreeSet<String> = BTreeSet::new();
                let mut failed = false;
                for p in &srcs {
                    match mtime(p) {
                        Ok(t) if t > ht + 1.0 => {
                            stale.insert(p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                        }
                        Ok(_) => {}
                        Err(_) => failed = true,
                    }
                }
                if !failed {
                    let stale: Vec<String> = stale.into_iter().collect();
                    au.ok(
                        "page is no older than its sources",
                        stale.is_empty(),
                        &if stale.is_empty() {
                            format!("mtime only, {} source(s); pass --rebuild to compare content", srcs.len())
                        } else {
                            format!("newer than the page: {}", list_repr(&stale))
                        },
                    );
                }
            }
        }
    }

    // ── 7. fenced blocks presented as terminal sessions ──────────────────────────
    let blocks: Vec<(String, String)> = all(&rx(r"```\n\$ ([^\n]+)\n((?:.*\n)*?)```"), &src).iter().map(|m| (g(m, 1).to_string(), g(m, 2).to_string())).collect();
    // A second shape: a tagged fence holding one command and its output commented
    // out. DELIBERATELY NARROW -- the fence is tagged, it holds exactly one command
    // line, and every remaining line is a comment -- because a fence of commands
    // annotated with # commentary would redraw cut files if it were run. A quoted
    // line ending in ... is compared as a prefix.
    let cblocks: Vec<(String, String)> = all(&rx(r"```(?:bash|sh|console)\n([^\n#][^\n]*)\n((?:#[^\n]*\n)+)```"), &src).iter().map(|m| (g(m, 1).to_string(), g(m, 2).to_string())).collect();
    if a.run_blocks && (!blocks.is_empty() || !cblocks.is_empty()) {
        run_blocks(&mut au, &root, &blocks, &cblocks);
    }

    // ── 8. external links ────────────────────────────────────────────────────────
    // Some sites answer every script 403 however real the page. .doc-audit-links
    // lists those URLs: each must still answer 403, and one that answers 200
    // again fails until it is taken off the list.
    let links_file = root.join(".doc-audit-links");
    let refuses: HashSet<String> = if links_file.exists() {
        text::read(&links_file).unwrap_or_default().split('\n').map(strip).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect()
    } else {
        HashSet::new()
    };
    if a.links {
        let urls: Vec<String> = findall1(&rx(r#"https://[^)\s"<>]+"#), &src, 0).into_iter().collect::<BTreeSet<_>>().into_iter().collect();
        let codes = run::pool(urls.clone(), 8, |u: String| http_status(&u));
        for (u, code) in urls.iter().zip(codes) {
            let label = head(u, 66);
            if refuses.contains(u) {
                au.ok(label, code == "403", &if code == "200" { "answers 200 now: take it off .doc-audit-links".to_string() } else { format!("HTTP {code}") });
            } else {
                au.ok(label, code == "200", &if code != "200" { format!("HTTP {code}") } else { String::new() });
            }
        }
    }

    for line in &au.notes {
        println!("{line}");
    }
    println!();
    for line in &au.fails {
        println!("{line}");
    }
    println!("\n  {} passed, {} failed", au.notes.len(), au.fails.len());
    std::process::exit(if au.fails.is_empty() { 0 } else { 1 });
}

/// Run the quoted sessions and compare. A quoted command is often a generator,
/// which rewrites the very figure the document displays, and auditing must not
/// mutate the tree it audits: snapshot ROOT, run the blocks against the real
/// tree for fidelity, then put back anything they touched. Files over 8 MiB are
/// tracked by size and mtime instead of content -- that cannot restore one, but
/// it can tell whether restoring is even called for.
fn run_blocks(au: &mut Audit, root: &Path, blocks: &[(String, String)], cblocks: &[(String, String)]) {
    const STASH_CAP: u64 = 8 * 1024 * 1024;
    let walk = || -> Vec<PathBuf> {
        text::walk(root)
            .into_iter()
            .filter(|p| p.is_file() && !p.components().any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".DS_Store"))
            .collect()
    };
    let stamp = |p: &Path| -> std::io::Result<(u64, i64)> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(p)?;
        Ok((m.size(), m.mtime() * 1_000_000_000 + m.mtime_nsec()))
    };
    let mut before: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    let mut big: BTreeMap<PathBuf, Option<(u64, i64)>> = BTreeMap::new();
    for p in walk() {
        match std::fs::metadata(&p) {
            Ok(m) if m.len() <= STASH_CAP => match std::fs::read(&p) {
                Ok(b) => {
                    before.insert(p, b);
                }
                Err(_) => {
                    big.insert(p, None);
                }
            },
            Ok(_) => {
                let s = stamp(&p).ok();
                big.insert(p, s);
            }
            Err(_) => {
                big.insert(p, None);
            }
        }
    }
    let live = |cmd: &str| -> Result<String, String> {
        match run::shell(cmd, root, &[], Some(Duration::from_secs(60))) {
            Ok(r) if r.timed_out => Err(format!("Command {} timed out after 60 seconds", str_repr(cmd))),
            Ok(r) => Ok(r.stdout),
            Err(e) => Err(e.to_string()),
        }
    };
    for (cmd, body) in blocks {
        let got = match live(cmd) {
            Ok(s) => s,
            Err(e) => {
                au.ok(&format!("`$ {cmd}` runs"), false, &e);
                continue;
            }
        };
        let want: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        let have: Vec<&str> = got.trim_end_matches('\n').split('\n').take(want.len()).collect();
        au.ok(&format!("quoted output of `$ {}` is verbatim", head(cmd, 44)), want == have, if want == have { "" } else { "block differs from a live run" });
    }
    let spaces = rx(r"[ \t]+");
    let hash = rx(r"^#\s?");
    for (cmd, body) in cblocks {
        let got = match live(cmd) {
            Ok(s) => s,
            Err(e) => {
                au.ok(&format!("`{cmd}` runs"), false, &e);
                continue;
            }
        };
        // Runs of spaces are collapsed on both sides: the tool column-pads, the
        // guide retypes it with two, and the numbers are what the line is quoted for.
        let sq = |s: &str| strip(&sub_str(&spaces, s, " ")).to_string();
        let want: Vec<String> = body.trim_end_matches('\n').split('\n').map(|l| sq(&sub_str(&hash, l, ""))).collect();
        let have: Vec<String> = got.trim_end_matches('\n').split('\n').map(sq).take(want.len()).collect();
        let bad = want.iter().zip(&have).find(|(w, h)| {
            !(if w.ends_with("...") {
                let n = w.chars().count() - 3;
                h.starts_with(&sq(&w.chars().take(n).collect::<String>()))
            } else {
                h == w
            })
        });
        let agree = bad.is_none() && have.len() == want.len();
        let detail = if agree { String::new() } else { format!("document has {}, the run does not", bad.map(|(w, _)| str_repr(w)).unwrap_or_else(|| "None".into())) };
        au.ok(&format!("quoted output of `{}` is verbatim", head(cmd, 44)), agree, &detail);
    }

    let rel = |p: &Path| p.strip_prefix(root).map(|q| q.to_string_lossy().into_owned()).unwrap_or_default();
    let (mut put_back, mut unrestorable): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for p in walk() {
        // changed, or newly created
        if let Some(was) = big.get(&p) {
            match stamp(&p) {
                Ok(s) if was.is_some() && Some(s) != *was => unrestorable.push(rel(&p)),
                Err(_) => unrestorable.push(rel(&p)),
                _ => {}
            }
            continue;
        }
        let Some(keep) = before.get(&p) else {
            let _ = std::fs::remove_file(&p);
            put_back.push(rel(&p) + " (created)");
            continue;
        };
        if std::fs::read(&p).ok().as_ref() != Some(keep) {
            let _ = std::fs::write(&p, keep);
            put_back.push(rel(&p));
        }
    }
    for (p, keep) in &before {
        // or deleted outright
        if !p.exists() {
            let _ = std::fs::write(p, keep);
            put_back.push(rel(p) + " (deleted)");
        }
    }
    for p in big.keys() {
        if !p.exists() {
            unrestorable.push(rel(p) + " (deleted)");
        }
    }
    put_back.sort();
    unrestorable.sort();
    if !put_back.is_empty() {
        au.notes.push(format!("  – blocks wrote to the tree; restored {}", put_back.join(", ")));
    }
    if !unrestorable.is_empty() {
        au.fails.push(format!("  ✗ a block changed a file too large to restore: {}", unrestorable.join(", ")));
    }
}
