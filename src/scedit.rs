//! A SolidCraft design edited by hand, and the committed one it came from
//! (2026-11-08): what sc-flatten and sc-replay both need to know about it.
use std::path::{Path, PathBuf};

pub fn die(m: impl std::fmt::Display) -> ! {
    eprintln!("error: {m}");
    std::process::exit(1)
}

/// The git repository a file is in.
pub fn repo_of(p: &Path) -> Option<PathBuf> {
    let mut d = p.parent()?.to_path_buf();
    loop {
        if d.join(".git").exists() {
            return Some(d);
        }
        d = d.parent()?.to_path_buf();
    }
}

/// The repository's committed files matching a pathspec.
pub fn git_files(repo: &Path, pat: &str) -> Vec<String> {
    let o = std::process::Command::new("git").arg("-C").arg(repo).args(["ls-files", pat]).output().unwrap_or_else(|e| die(format!("git: {e}")));
    String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()
}

/// The edited design, its repository, and the committed design it was edited
/// from: given, or the committed .sc whose name begins the edited one's
/// (four-hole-ocarina-cube.sc for four-hole-ocarina-cube_bigger.sc).
pub struct Edit {
    pub edited: PathBuf,
    pub original: PathBuf,
    pub repo: PathBuf,
    pub stem: String,
}

pub fn find(edited: &str, original: Option<&str>) -> Edit {
    let ep = std::fs::canonicalize(edited).unwrap_or_else(|e| die(format!("{edited}: {e}")));
    let repo = repo_of(&ep).unwrap_or_else(|| die(format!("{edited} is in no git repository")));
    let stem = ep.file_stem().unwrap().to_string_lossy().to_string();
    let original = match original {
        Some(o) => std::fs::canonicalize(o).unwrap_or_else(|e| die(format!("{o}: {e}"))),
        None => {
            let mut c: Vec<String> = git_files(&repo, "*.sc")
                .into_iter()
                .filter(|f| {
                    let s = Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
                    s != stem && stem.starts_with(&s) && matches!(stem[s.len()..].chars().next(), Some('_' | '-' | ' ' | '.'))
                })
                .collect();
            c.sort_by_key(|f| std::cmp::Reverse(f.len()));
            let f = c.first().unwrap_or_else(|| die(format!("no committed .sc in {} begins {stem}'s name; name it with --original=", repo.display())));
            repo.join(f)
        }
    };
    Edit { edited: ep, original, repo, stem }
}

/// The committed design an edit started from. An edited file can be older
/// than the design committed now (saved before the design was rewritten), so
/// when the two's parameters do not line up name for name, this walks back
/// through the committed file's history to the newest version whose
/// parameters do: (that version on disk, its commit, its parameters). The
/// commit is None when it is the file as it stands.
pub fn base(e: &Edit, edited: &[Param], tmp: &Path) -> (PathBuf, Option<String>, Vec<Param>) {
    let names = |v: &[Param]| {
        let mut n: Vec<String> = v.iter().map(|p| p.name.clone()).collect();
        n.sort();
        n
    };
    let want = names(edited);
    // the edited file unchanged: its parameters are the ones just read
    if std::fs::read(&e.original).ok().is_some_and(|o| std::fs::read(&e.edited).ok().is_some_and(|d| d == o)) {
        return (e.original.clone(), None, edited.to_vec());
    }
    let now = parameters(&e.original, tmp);
    if names(&now) == want {
        return (e.original.clone(), None, now);
    }
    let rel = e.original.strip_prefix(&e.repo).unwrap_or(&e.original).to_string_lossy().to_string();
    let o = std::process::Command::new("git").arg("-C").arg(&e.repo).args(["log", "--format=%H", "-n", "20", "--", &rel]).output().unwrap_or_else(|er| die(format!("git: {er}")));
    for c in String::from_utf8_lossy(&o.stdout).lines() {
        let Some(text) = file_at(&e.repo, Some(c), &rel) else { continue };
        let f = tmp.join(format!("base-{}.sc", &c[..10]));
        std::fs::write(&f, text).unwrap_or_else(|er| die(format!("{}: {er}", f.display())));
        let ps = parameters(&f, tmp);
        if names(&ps) == want {
            return (f, Some(c.to_string()), ps);
        }
    }
    die(format!(
        "{} does not line up with {} or any of its last 20 committed versions, parameter for parameter: features were added or deleted. Name the design it came from with --original=.",
        e.edited.display(),
        rel
    ))
}

/// A committed file's bytes: at a commit, or as it stands.
pub fn file_at(repo: &Path, commit: Option<&str>, rel: &str) -> Option<Vec<u8>> {
    match commit {
        None => std::fs::read(repo.join(rel)).ok(),
        Some(c) => {
            let o = std::process::Command::new("git").arg("-C").arg(repo).args(["show", &format!("{c}:{rel}")]).output().ok()?;
            o.status.success().then_some(o.stdout)
        }
    }
}

/// One parameter of a design as SolidCraft evaluates it.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub expression: String,
    pub value: f64,
    pub comment: String,
    /// user, feature or sketch
    pub source: String,
    pub uses: Vec<String>,
}

/// Every parameter of a design, evaluated.
pub fn parameters(sc: &Path, tmp: &Path) -> Vec<Param> {
    parameters_step(sc, tmp, None)
}

/// parameters(), and the design built as a STEP file in the same run when
/// `step` names one: SolidCraft builds the design either way, so asking for
/// both at once saves building it twice.
pub fn parameters_step(sc: &Path, tmp: &Path, step: Option<&Path>) -> Vec<Param> {
    let script = tmp.join("parameters.list.json");
    let out = tmp.join("parameters.out.json");
    std::fs::write(&script, "[{\"command\":\"parameters.list\",\"params\":{}}]").unwrap();
    let mut cmd = std::process::Command::new("solidcraft-cli");
    cmd.arg("run").arg(&script).arg("--in").arg(sc);
    if let Some(s) = step {
        cmd.arg("--out").arg(s);
    }
    let r = cmd
        .args(["--quiet", "--results", "--json-out"])
        .arg(&out)
        .output()
        .unwrap_or_else(|e| die(format!("solidcraft-cli: {e}")));
    if !r.status.success() {
        die(format!("SolidCraft could not read {}: {}", sc.display(), String::from_utf8_lossy(&r.stderr).trim()));
    }
    let text = std::fs::read_to_string(&out).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    let j = json::parse(&text).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    let list = j.idx(0).get("parameters");
    let mut v = vec![];
    for p in list.items() {
        v.push(Param {
            name: p.get("name").str().to_string(),
            expression: p.get("expression").str().to_string(),
            value: p.get("value").num(),
            comment: p.get("comment").str().to_string(),
            source: p.get("source").str().to_string(),
            uses: p.get("uses").items().iter().map(|u| u.str().to_string()).collect(),
        });
    }
    v
}

/// A scratch directory of its own, removed when dropped.
pub struct Tmp(pub PathBuf);

impl Tmp {
    pub fn new(tag: &str) -> Tmp {
        let d = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap_or_else(|e| die(format!("{}: {e}", d.display())));
        Tmp(d)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Just enough JSON to read what solidcraft-cli writes.
pub mod json {
    #[derive(Clone, Debug)]
    pub enum J {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<J>),
        Obj(Vec<(String, J)>),
    }

    static NULL: J = J::Null;

    impl J {
        pub fn get(&self, k: &str) -> &J {
            match self {
                J::Obj(v) => v.iter().find(|(a, _)| a == k).map_or(&NULL, |(_, b)| b),
                _ => &NULL,
            }
        }
        pub fn idx(&self, i: usize) -> &J {
            match self {
                J::Arr(v) => v.get(i).unwrap_or(&NULL),
                _ => &NULL,
            }
        }
        pub fn items(&self) -> &[J] {
            match self {
                J::Arr(v) => v,
                _ => &[],
            }
        }
        pub fn str(&self) -> &str {
            match self {
                J::Str(s) => s,
                _ => "",
            }
        }
        pub fn num(&self) -> f64 {
            match self {
                J::Num(x) => *x,
                _ => f64::NAN,
            }
        }
    }

    pub fn parse(t: &str) -> Result<J, String> {
        let b: Vec<char> = t.chars().collect();
        let mut i = 0;
        let v = val(&b, &mut i)?;
        Ok(v)
    }

    fn ws(b: &[char], i: &mut usize) {
        while *i < b.len() && b[*i].is_whitespace() {
            *i += 1;
        }
    }

    fn val(b: &[char], i: &mut usize) -> Result<J, String> {
        ws(b, i);
        match b.get(*i) {
            None => Err("unexpected end of JSON".into()),
            Some('{') => {
                *i += 1;
                let mut v = vec![];
                loop {
                    ws(b, i);
                    if b.get(*i) == Some(&'}') {
                        *i += 1;
                        return Ok(J::Obj(v));
                    }
                    let J::Str(k) = val(b, i)? else { return Err("a key that is not a string".into()) };
                    ws(b, i);
                    *i += 1; // ':'
                    v.push((k, val(b, i)?));
                    ws(b, i);
                    if b.get(*i) == Some(&',') {
                        *i += 1;
                    }
                }
            }
            Some('[') => {
                *i += 1;
                let mut v = vec![];
                loop {
                    ws(b, i);
                    if b.get(*i) == Some(&']') {
                        *i += 1;
                        return Ok(J::Arr(v));
                    }
                    v.push(val(b, i)?);
                    ws(b, i);
                    if b.get(*i) == Some(&',') {
                        *i += 1;
                    }
                }
            }
            Some('"') => {
                *i += 1;
                let mut s = String::new();
                while let Some(&c) = b.get(*i) {
                    *i += 1;
                    match c {
                        '"' => return Ok(J::Str(s)),
                        '\\' => {
                            let e = *b.get(*i).ok_or("unterminated string")?;
                            *i += 1;
                            match e {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                'r' => s.push('\r'),
                                'b' => s.push('\u{8}'),
                                'f' => s.push('\u{c}'),
                                'u' => {
                                    let h: String = b[*i..*i + 4].iter().collect();
                                    *i += 4;
                                    let mut c = u32::from_str_radix(&h, 16).map_err(|_| "bad \\u")?;
                                    if (0xD800..0xDC00).contains(&c) && b.get(*i) == Some(&'\\') {
                                        let l: String = b[*i + 2..*i + 6].iter().collect();
                                        *i += 6;
                                        let lo = u32::from_str_radix(&l, 16).map_err(|_| "bad \\u")?;
                                        c = 0x10000 + ((c - 0xD800) << 10) + (lo - 0xDC00);
                                    }
                                    s.push(char::from_u32(c).unwrap_or('?'));
                                }
                                o => s.push(o),
                            }
                        }
                        c => s.push(c),
                    }
                }
                Err("unterminated string".into())
            }
            Some('t') => {
                *i += 4;
                Ok(J::Bool(true))
            }
            Some('f') => {
                *i += 5;
                Ok(J::Bool(false))
            }
            Some('n') => {
                *i += 4;
                Ok(J::Null)
            }
            Some(_) => {
                let st = *i;
                while *i < b.len() && matches!(b[*i], '0'..='9' | '-' | '+' | '.' | 'e' | 'E') {
                    *i += 1;
                }
                let t: String = b[st..*i].iter().collect();
                t.parse().map(J::Num).map_err(|_| format!("bad number '{t}'"))
            }
        }
    }
}
