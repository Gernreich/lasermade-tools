//! `sc-replay EDITED.sc [--original=ORIGINAL.sc] [--out=DIR]` (2026-11-08):
//! the cut sheets for a SolidCraft design edited by hand, drawn again by the
//! repository's own generator.
//!
//! SolidCraft evaluates both designs' parameters. Every one whose expression
//! the edit changed is an edit; every design parameter whose value moved
//! because of it, and which the generator takes as a flag, is passed to the
//! generator at its new value. A parameter is a flag when the generator's own
//! design says so in its comment ("(the generator's --scale)"), or when the
//! generator lists it under --params (the flat.rs generators: every design
//! parameter is --name=VALUE, a dash for each underscore). The commands come
//! from the repository's .repro: the line that writes the committed design
//! names the program and the arguments that pick the build, and every cut
//! sheet line with that program and those arguments is run again, the flags
//! added, into DIR (default: target/sc-replay/STEM/ in the repository) at the
//! sheet's own path.
//!
//! An edit no flag follows (a hole moved by its sketch dimension, a feature
//! added) is named and stops it: the generator cannot draw what it is not
//! told. sc-flatten draws those from the solid; sc-compare checks the two
//! against each other.
use lasermade_tools::scedit::{self, die, Param};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// One line of a .repro: its output and its command.
fn repro(repo: &Path) -> Vec<(String, String)> {
    let t = std::fs::read_to_string(repo.join(".repro")).unwrap_or_else(|e| die(format!("{}/.repro: {e}", repo.display())));
    t.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once(" :: ").map(|(a, b)| (a.trim().to_string(), b.trim().to_string())))
        .collect()
}

fn sh(repo: &Path, cmd: &str, out: &Path) -> (bool, String) {
    let r = std::process::Command::new("sh").arg("-c").arg(cmd).current_dir(repo).env("OUT", out).output().unwrap_or_else(|e| die(format!("sh: {e}")));
    (r.status.success(), format!("{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr)))
}

fn same(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// A value as a flag takes it: the shortest that reads back the same.
fn num(v: f64) -> String {
    if v == v.round() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    format!("{v}")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut edited, mut original, mut out) = (None, None, None);
    for a in &args {
        if let Some(v) = a.strip_prefix("--original=") {
            original = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--out=") {
            out = Some(v.to_string());
        } else if a.starts_with('-') {
            die(format!("{a}? It takes EDITED.sc, --original=ORIGINAL.sc and --out=DIR."));
        } else {
            edited = Some(a.clone());
        }
    }
    let edited = edited.unwrap_or_else(|| die("which .sc file? sc-replay EDITED.sc [--original=ORIGINAL.sc] [--out=DIR]"));
    let e = scedit::find(&edited, original.as_deref());
    let out = std::path::PathBuf::from(out.unwrap_or_else(|| e.repo.join("target/sc-replay").join(&e.stem).to_string_lossy().to_string()));
    println!("sc-replay {}\n  from {}", e.edited.display(), e.original.display());
    let tmp = scedit::Tmp::new("sc-replay");
    let new = scedit::parameters(&e.edited, &tmp.0);
    let (_, commit, old) = scedit::base(&e, &new, &tmp.0);
    if let Some(c) = &commit {
        println!("  it was edited from {} as committed in {}, not as it stands; the generator draws as it stands", e.original.file_name().unwrap().to_string_lossy(), &c[..7]);
    }
    let oldby: HashMap<&str, &Param> = old.iter().map(|p| (p.name.as_str(), p)).collect();
    let newby: HashMap<&str, &Param> = new.iter().map(|p| (p.name.as_str(), p)).collect();
    let added: Vec<&str> = new.iter().filter(|p| !oldby.contains_key(p.name.as_str())).map(|p| p.name.as_str()).collect();
    let gone: Vec<&str> = old.iter().filter(|p| !newby.contains_key(p.name.as_str())).map(|p| p.name.as_str()).collect();
    if !added.is_empty() || !gone.is_empty() {
        die(format!(
            "the edit adds or removes parameters ({}{}{}): features were added or deleted, which no generator flag says. sc-flatten draws it from the solid.",
            added.iter().map(|a| format!("+{a}")).collect::<Vec<_>>().join(" "),
            if added.is_empty() || gone.is_empty() { "" } else { " " },
            gone.iter().map(|a| format!("-{a}")).collect::<Vec<_>>().join(" ")
        ));
    }
    let edits: Vec<&Param> = new.iter().filter(|p| oldby[p.name.as_str()].expression != p.expression).collect();
    if edits.is_empty() {
        println!("  no parameter's expression changed: the committed sheets are the sheets");
    }
    for p in &edits {
        println!("  edited    {} ({}): {} -> {}", p.name, p.source, oldby[p.name.as_str()].expression, p.expression);
    }

    // the generator, from the line that writes the committed design
    let lines = repro(&e.repo);
    let ostem = e.original.file_stem().unwrap().to_string_lossy().to_string();
    let design_out = format!("{ostem}.solidcraft.json");
    let (_, dcmd) = lines
        .iter()
        .find(|(o, _)| o.ends_with(&format!("/{design_out}")) || *o == design_out)
        .unwrap_or_else(|| die(format!("{}/.repro has no line writing {design_out}, so which generator drew it is unknown", e.repo.display())));
    let words: Vec<&str> = dcmd.split_whitespace().collect();
    let program = words[0];
    let picks: Vec<&str> = words[1..].iter().copied().filter(|w| !w.starts_with("--solidcraft=") && *w != "--no-write" && !w.starts_with("--out=")).collect();
    if !e.repo.join(program).exists() {
        die(format!("{program} is not built: cargo build --release in {}", e.repo.display()));
    }

    // which parameters are flags: the generator's own design says, and so does --params
    let fresh = tmp.0.join("design").join(&design_out);
    std::fs::create_dir_all(fresh.parent().unwrap()).unwrap();
    let (ok, _) = sh(&e.repo, dcmd, &fresh);
    let mut flag_of: HashMap<String, String> = HashMap::new();
    if ok {
        if let Ok(t) = std::fs::read_to_string(&fresh) {
            let re = fancy_regex::Regex::new(r"\(the generator's (--[a-z0-9-]+)\)").unwrap();
            if let Ok(j) = scedit::json::parse(&t) {
                let steps = if !j.items().is_empty() { j.items().to_vec() } else if !j.get("commands").items().is_empty() { j.get("commands").items().to_vec() } else { j.get("steps").items().to_vec() };
                for st in steps {
                    if st.get("command").str() != "parameters.add" {
                        continue;
                    }
                    let p = st.get("params");
                    if let Ok(Some(c)) = re.captures(p.get("comment").str()) {
                        flag_of.insert(p.get("name").str().to_string(), c[1].to_string());
                    }
                }
            }
        }
    }
    let (ok, table) = sh(&e.repo, &format!("{program} --params"), &tmp.0.join("x"));
    if ok && table.starts_with("| parameter") {
        for l in table.lines().skip(2) {
            if let Some(n) = l.split('`').nth(1) {
                flag_of.entry(n.to_string()).or_insert_with(|| format!("--{}", n.replace('_', "-")));
            }
        }
    }

    // the flags: every flagged parameter whose value the edit moved
    let mut flags: Vec<(String, String, f64)> = vec![];
    for p in &new {
        if let Some(f) = flag_of.get(&p.name) {
            let o = oldby[p.name.as_str()];
            if !same(o.value, p.value) {
                flags.push((p.name.clone(), f.clone(), p.value));
            }
        }
    }
    // does every edit reach a flag? walk the uses from each flag back
    let uses: HashMap<&str, &Vec<String>> = new.iter().map(|p| (p.name.as_str(), &p.uses)).collect();
    let mut reached: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = flags.iter().map(|f| f.0.clone()).collect();
    while let Some(n) = stack.pop() {
        if reached.insert(n.clone()) {
            if let Some(u) = uses.get(n.as_str()) {
                stack.extend(u.iter().cloned());
            }
        }
    }
    let lost: Vec<&&Param> = edits.iter().filter(|p| !reached.contains(&p.name)).collect();
    if !lost.is_empty() {
        let names: Vec<String> = lost.iter().map(|p| format!("{} ({})", p.name, p.source)).collect();
        die(format!(
            "the generator takes no flag that follows {}. It has flags for {}. sc-flatten draws such an edit from the solid.",
            names.join(", "),
            if flag_of.is_empty() { "nothing".to_string() } else { let mut v: Vec<&String> = flag_of.keys().collect(); v.sort(); v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ") }
        ));
    }
    let fl: Vec<String> = flags.iter().map(|(_, f, v)| format!("{f}={}", num(*v))).collect();
    for (n, f, v) in &flags {
        println!("  flag      {f}={} (the design's {n}: {} -> {})", num(*v), num(oldby[n.as_str()].value), num(*v));
    }

    // every cut sheet the program draws for this build, drawn again
    let sheets: Vec<&(String, String)> = lines
        .iter()
        .filter(|(o, c)| {
            let w: Vec<&str> = c.split_whitespace().collect();
            o.ends_with(".svg") && !o.split('/').any(|p| p == "previews") && w[0] == program && picks.iter().all(|p| w.contains(p)) && !c.contains("--solidcraft=")
        })
        .collect();
    if sheets.is_empty() {
        die(format!("{}/.repro has no cut sheet drawn by {program} {}", e.repo.display(), picks.join(" ")));
    }
    let mut bad = 0;
    let mut runs: HashMap<String, (bool, String)> = HashMap::new();
    for (o, c) in sheets {
        let dest = out.join(o);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap_or_else(|er| die(format!("{}: {er}", dest.display())));
        let cmd = format!("{c} {}", fl.join(" "));
        // a generator that writes every sheet of a build at once runs once per directory
        let key = format!("{cmd}\u{0}{}", dest.parent().unwrap().display());
        let (ok, text) = runs.entry(key).or_insert_with(|| sh(&e.repo, &cmd, &dest)).clone();
        if ok && dest.exists() {
            println!("  wrote     {}", dest.display());
        } else if dest.exists() {
            // drawn, then refused by its own checks: still worth comparing
            bad += 1;
            println!("  wrote     {}, but the generator's own checks fail:", dest.display());
            for l in text.lines().filter(|l| l.contains("FAIL")).take(12) {
                println!("            {}", l.trim());
            }
        } else {
            bad += 1;
            println!("  FAILED    {o}: {}", cmd.trim());
            for l in text.lines().filter(|l| l.contains("FAIL") || l.starts_with("error")).take(12) {
                println!("            {}", l.trim());
            }
            if ok {
                println!("            (the generator ran but wrote no {o}: it writes nothing while a check fails)");
            }
        }
    }
    if bad > 0 {
        std::process::exit(1);
    }
}
