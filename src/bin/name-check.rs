//! Does a cut file's NAME agree with the geometry inside it?
//!
//!     target/release/name-check [ALL-GATES.SH]
//!
//! Every other gate compares an artefact with the code that draws it. None of them
//! reads the name. That is a gap with a shape: the reproduction gate passes the
//! stem in on the command line, so a design called -1180mm reproduces perfectly
//! whatever its centreline actually is, and a folder called R62-pitch46 keeps that
//! name after the pitch is changed underneath it.
//!
//! The names are not decoration. They are what a person reads off a sheet at the
//! machine to know which bore they are holding, and the only place several of these
//! numbers appear at all.
//!
//! Designs are read from all-gates.sh's own rr lines rather than listed again here,
//! so the two cannot drift apart. Claims checked, where the name makes them:
//!
//!     bore10        the section, against "10mm square"
//!     30deg         the facet angle, against "30 degree facets"
//!     1180mm        the centreline, rounded, against "centreline 1179.9mm"
//!     R30 / R62     the radius the shape starts from
//!     R35to113      a spiral's inner and outer radius
//!     pitch46       an Archimedean spiral's rise per turn
//!     step60        a volute's step per turn
//!     3lobes        half-circles in a serpentine
//!     <shape>       the second word of the name, against what the generator says
//!
//! The generator it asks is swept-curve's ribbon_bore, run as a program: Rust
//! since 2026-11-07, built there (all-gates.sh builds it before the lanes).
use lasermade_tools::re::{all, first, g, rx};
use lasermade_tools::{die, py, text, TempDir};
use std::collections::HashSet;
use std::fmt;

/// A value as the Python held it: the claims mix whole numbers, decimals,
/// words and "nothing found".
#[derive(Clone, PartialEq)]
enum V {
    I(i64),
    F(f64),
    S(String),
    None,
}

impl fmt::Display for V {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            V::I(i) => write!(f, "{i}"),
            V::F(x) => write!(f, "{}", py::repr(*x)),
            V::S(s) => write!(f, "{s}"),
            V::None => write!(f, "None"),
        }
    }
}

fn num(v: &V) -> Option<f64> {
    match v {
        V::I(i) => Some(*i as f64),
        V::F(x) => Some(*x),
        _ => None,
    }
}

fn float(s: &str) -> f64 {
    py::float(s).unwrap_or_else(|| die(format!("could not convert string to float: '{s}'")))
}

fn int(s: &str) -> i64 {
    s.parse().unwrap_or_else(|_| die(format!("invalid literal for int(): '{s}'")))
}

/// Read the design table. Takes a path so this check can be tested against a
/// COPY of all-gates.sh with a name deliberately falsified, instead of editing
/// the live harness in place -- which is how the first attempt at proving it
/// can fail left five stems corrupted mid-loop.
fn designs(path: &str) -> Vec<(String, Vec<String>)> {
    let t = text::read_strict(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    // 'rrc' as well as 'rr': --port-per-cheek writes two cheek sheets, so it
    // has its own helper in all-gates.sh, and a pattern anchored to 'rr '
    // alone silently dropped those designs out of this table. The line has
    // carried a SUFFIX field since 2026-09-13.
    let r = rx(r"^rrc? (\S+/\S+) (\S+) (\S+) (--\S.*)");
    let (mut rows, mut seen) = (Vec::new(), HashSet::new());
    for ln in t.lines() {
        if let Some(m) = first(&r, ln.trim()) {
            // One row per STEM: every claim is a property of the stem and
            // identical across its port variants, and checking it three times
            // would overstate what was examined.
            if seen.insert(g(&m, 2).to_string()) {
                rows.push((g(&m, 2).to_string(), g(&m, 4).split_whitespace().map(String::from).collect()));
            }
        }
    }
    rows
}

fn main() {
    let gates = std::env::args().nth(1).unwrap_or_else(|| {
        let exe = std::env::current_exe().unwrap_or_default();
        // target/release/name-check -> the repository's all-gates.sh
        let here = exe.ancestors().nth(3).map(|p| p.to_path_buf()).unwrap_or_default();
        here.join("all-gates.sh").to_string_lossy().into_owned()
    });
    std::process::exit(check(&gates));
}

fn check(gates: &str) -> i32 {
    let rows = designs(gates);
    // A check that parses nothing passes. Say what was read, and refuse to
    // report agreement on an empty list.
    if rows.is_empty() {
        println!("  FAIL  no rr lines found in all-gates.sh");
        return 1;
    }
    let swept = format!("{}/LaserMadeMusic/GIT/swept-curve", std::env::var("HOME").unwrap_or_default());
    let head_rx = rx(r"ribbon bore, (\S+)\s+(\d+)mm square, ([\d.]+) degree facets");
    let cl_rx = rx(r"centreline ([\d.]+)mm");
    let mut bad = 0;
    let t = TempDir::new("name-check");
    for (stem, sw) in &rows {
        let out = std::process::Command::new(format!("{swept}/target/release/ribbon_bore"))
            .args(sw)
            .arg(format!("--out={}/x.svg", t.0.display()))
            .current_dir(&swept)
            .output()
            .map(|o| text::decode(&o.stdout))
            .unwrap_or_default();
        let (Some(head), Some(cl)) = (first(&head_rx, &out), first(&cl_rx, &out)) else {
            println!("  FAIL  {stem}: the generator printed no report");
            bad += 1;
            continue;
        };
        let (shape, bore, deg, mm) = (g(&head, 1).to_string(), int(g(&head, 2)), float(g(&head, 3)), float(g(&cl, 1)));
        // The shape line is the one AFTER the header, not stdout line 1:
        // --merge-lead prints before the report.
        let lines: Vec<&str> = out.split('\n').collect();
        let desc = lines.iter().position(|l| *l == g(&head, 0)).map(|i| lines.get(i + 1).copied().unwrap_or("")).unwrap_or("");
        let need = |p: &str| first(&rx(p), stem).unwrap_or_else(|| die(format!("{stem}: the name has no {p}")));
        let mut claims: Vec<(&str, V, V)> = vec![
            ("bore", V::I(int(g(&need(r"bore(\d+)"), 1))), V::I(bore)),
            ("facet angle", V::S(g(&need(r"-([\d.]+)deg"), 1).to_string()), V::F(deg)),
        ];
        if let Some(m) = first(&rx(r"-(\d+)mm$"), stem) {
            claims.push(("centreline", V::I(int(g(&m, 1))), V::I(py::round(mm))));
        }
        // radii: a span (R35to113) or a single start radius (R30, R62, R94)
        if let Some(m) = first(&rx(r"R(\d+)to(\d+)"), stem) {
            let got: Vec<i64> = all(&rx(r"R([\d.]+)"), desc).iter().map(|c| py::round(float(g(c, 1)))).collect();
            claims.push(("inner R", V::I(int(g(&m, 1))), got.first().map(|&x| V::I(x)).unwrap_or(V::None)));
            claims.push(("outer R", V::I(int(g(&m, 2))), got.get(1).map(|&x| V::I(x)).unwrap_or(V::None)));
        } else if let Some(m) = first(&rx(r"-R([\d.]+)"), stem) {
            let got = first(&rx(r"R([\d.]+)"), desc).map(|c| V::F(float(g(&c, 1)))).unwrap_or(V::None);
            claims.push(("radius", V::S(g(&m, 1).to_string()), got));
        }
        let rounded = |p: &str| first(&rx(p), desc).map(|c| V::I(py::round(float(g(&c, 1))))).unwrap_or(V::None);
        if let Some(m) = first(&rx(r"pitch(\d+)"), stem) {
            claims.push(("pitch", V::I(int(g(&m, 1))), rounded(r"rising ([\d.]+)mm a turn")));
        }
        if let Some(m) = first(&rx(r"step(\d+)"), stem) {
            claims.push(("step", V::I(int(g(&m, 1))), rounded(r"stepping ([\d.]+)mm a turn")));
        }
        if let Some(m) = first(&rx(r"-(\d+)lobes"), stem) {
            // "half-circles" is the OPEN serpentine's word; a closed one's
            // description says "5 lobes". Either spelling counts.
            let got = first(&rx(r"(\d+) (?:half-circles|lobes)"), desc).map(|c| V::I(int(g(&c, 1)))).unwrap_or(V::None);
            claims.push(("lobes", V::I(int(g(&m, 1))), got));
        }
        // The shape is everything between an optional 'ribbon-' and '-bore':
        // double-spiral and the rest have hyphens of their own.
        let sh = first(&rx(r"^(?:ribbon-)?(.+?)-bore\d"), stem).map(|c| g(&c, 1).to_string()).unwrap_or_else(|| die(format!("{stem}: no shape in the name")));
        claims.push(("shape", V::S(sh), V::S(shape)));
        let mut wrong = 0;
        let decimal = rx(r"^[\d.]+$");
        for (what, said, got) in &claims {
            // A NAME IS HELD TO THE PRECISION IT STATES, and no further. -R72
            // names a radius of 71.754, rounded in the name on purpose;
            // -R128.572 and -27.6923deg name a ring that cannot be written any
            // shorter. Round the generator's figure to the name's own
            // precision and compare there.
            let ok = match said {
                V::S(s) if first(&decimal, s).is_some() => {
                    let dp = s.split_once('.').map(|(_, f)| f.len()).unwrap_or(0);
                    num(got).map(|x| py::round_dp(x, dp) == float(s)).unwrap_or(false)
                }
                _ => match (num(said), num(got)) {
                    (Some(a), Some(b)) => a == b,
                    _ => said == got,
                },
            };
            if !ok {
                println!("  FAIL  {stem}: name says {what} {said}, the generator says {got}");
                bad += 1;
                wrong += 1;
            }
        }
        if wrong == 0 {
            println!("  ok    {:56} {} claims", py::head(stem, 56), claims.len());
        } else {
            println!("  ----  {:56} {wrong} of {} wrong", py::head(stem, 56), claims.len());
        }
    }
    if bad > 0 {
        println!("\n  {} designs, {bad} name(s) disagreeing", rows.len());
    } else {
        println!("\n  {} designs, every name agrees", rows.len());
    }
    if bad > 0 { 1 } else { 0 }
}
