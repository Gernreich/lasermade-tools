//! What the flat-part generators share (2026-11-08): the bullroarer, the buzz
//! disc, the kalimba and the slapstick were drawn by hand until then, and
//! each is now a few numbers. One table of parameters drives both the cut
//! sheet and the SolidCraft design, so the two cannot drift apart: every
//! parameter is a `--name=VALUE` flag on the generator and a parameter of the
//! same name in the design.
use crate::solidcraft::{n, Script};

#[derive(Clone, Copy, PartialEq)]
pub enum Unit {
    /// millimetres
    Len,
    /// degrees
    Ang,
    /// a count: a whole number
    Num,
    /// a plain number that need not be whole: a fraction, a ratio
    Ratio,
}

#[derive(Clone)]
pub struct Param {
    pub name: &'static str,
    pub value: f64,
    pub unit: Unit,
    pub comment: &'static str,
}

pub struct Params(pub Vec<Param>);

impl Params {
    pub fn new(table: &[(&'static str, f64, Unit, &'static str)]) -> Params {
        Params(table.iter().map(|&(name, value, unit, comment)| Param { name, value, unit, comment }).collect())
    }

    pub fn get(&self, name: &str) -> f64 {
        match self.0.iter().find(|p| p.name == name) {
            Some(p) => p.value,
            None => panic!("no parameter {name}"),
        }
    }

    /// Take every `--name=VALUE` that names a parameter (a dash in the
    /// flag reads as an underscore) and hand back the rest. `--params`
    /// prints the parameter table and exits.
    pub fn parse(&mut self, args: Vec<String>) -> Result<Vec<String>, String> {
        let mut rest = vec![];
        'next: for a in args {
            if a == "--params" {
                // the table, for the README, and nothing else
                print!("{}", self.table());
                std::process::exit(0);
            }
            if let Some((k, v)) = a.strip_prefix("--").and_then(|x| x.split_once('=')) {
                let k = k.replace('-', "_");
                if let Some(p) = self.0.iter_mut().find(|p| p.name == k) {
                    let x: f64 = v.trim().parse().map_err(|_| format!("error: {a}: {v} is not a number."))?;
                    if p.unit == Unit::Num && x.fract() != 0.0 {
                        return Err(format!("error: {a}: a count is a whole number."));
                    }
                    p.value = x;
                    continue 'next;
                }
            }
            rest.push(a);
        }
        Ok(rest)
    }

    /// The flags, for a usage line.
    pub fn flags(&self) -> String {
        self.0.iter().map(|p| format!("--{}=", p.name.replace('_', "-"))).collect::<Vec<_>>().join(" ")
    }

    /// The ones that differ from `defaults`, as flags: what the sheet's
    /// description records it was made with.
    pub fn changed(&self, defaults: &Params) -> String {
        let v: Vec<String> = self
            .0
            .iter()
            .zip(&defaults.0)
            .filter(|(p, d)| p.value != d.value)
            .map(|(p, _)| format!("--{}={}", p.name.replace('_', "-"), n(p.value)))
            .collect();
        v.join(" ")
    }

    /// Every parameter into a SolidCraft script, at its value here.
    pub fn declare(&self, s: &mut Script) {
        for p in &self.0 {
            match p.unit {
                Unit::Len => s.len(p.name, format!("{} mm", n(p.value)), p.comment),
                Unit::Ang => s.angle(p.name, format!("{} deg", n(p.value)), p.comment),
                Unit::Num | Unit::Ratio => s.num(p.name, n(p.value), p.comment),
            }
        }
    }

    /// The parameter table for a README: name, default, what it is.
    pub fn table(&self) -> String {
        let mut o = String::from("| parameter | default | what it is |\n|---|---|---|\n");
        for p in &self.0 {
            let u = match p.unit {
                Unit::Len => " mm",
                Unit::Ang => "°",
                Unit::Num | Unit::Ratio => "",
            };
            o.push_str(&format!("| `{}` | {}{u} | {} |\n", p.name, n(p.value), p.comment));
        }
        o
    }
}

// --- the cut sheet ---------------------------------------------------------------

/// The house inks, in the order they run: blue engraves, then green, orange
/// and cyan cut inside a part, and black frees it.
pub const INKS: [&str; 5] = ["#0000ff", "#00ff00", "#ff8000", "#00ffff", "#000000"];

pub struct Sheet {
    pub w: f64,
    pub h: f64,
    pub items: Vec<(&'static str, String)>,
}

/// A number for a cut file: to the micrometre, no trailing zeros.
pub fn f(v: f64) -> String {
    let s = format!("{:.4}", if v.abs() < 5e-5 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

impl Sheet {
    pub fn new(w: f64, h: f64) -> Sheet {
        Sheet { w, h, items: vec![] }
    }
    pub fn rect(&mut self, ink: &'static str, x: f64, y: f64, w: f64, h: f64) {
        self.items.push((ink, format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>", f(x), f(y), f(w), f(h))));
    }
    pub fn circle(&mut self, ink: &'static str, cx: f64, cy: f64, d: f64) {
        self.items.push((ink, format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\"/>", f(cx), f(cy), f(d / 2.0))));
    }
    pub fn path(&mut self, ink: &'static str, d: String) {
        self.items.push((ink, format!("<path d=\"{d}\"/>")));
    }
    /// A closed polyline.
    pub fn poly(&mut self, ink: &'static str, pts: &[(f64, f64)]) {
        let p: Vec<String> = pts.iter().map(|(x, y)| format!("{},{}", f(*x), f(*y))).collect();
        self.path(ink, format!("M {} Z", p.join(" L ")));
    }
    /// The file: one group per ink, in cut order. Blue is an engrave, so it
    /// is drawn as a stroke too: these sheets engrave lines, not fills.
    pub fn svg(&self, title: &str, desc: &str) -> String {
        let mut o = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">\n<title>{title}</title>\n<desc>{desc}</desc>\n",
            f(self.w),
            f(self.h),
            f(self.w),
            f(self.h)
        );
        for ink in INKS {
            let mine: Vec<&String> = self.items.iter().filter(|(i, _)| *i == ink).map(|(_, s)| s).collect();
            if mine.is_empty() {
                continue;
            }
            o.push_str(&format!("  <g fill=\"none\" stroke=\"{ink}\" stroke-width=\"0.2\">\n"));
            for s in mine {
                o.push_str(&format!("    {s}\n"));
            }
            o.push_str("  </g>\n");
        }
        o.push_str("</svg>\n");
        o
    }
}

/// `--out=DIR` and `--solidcraft=DIR` from the leftover flags; anything
/// else is an error naming what the generator takes.
pub struct Outs {
    pub out: Option<String>,
    pub solidcraft: Option<String>,
    pub no_write: bool,
}

pub fn outs(rest: Vec<String>, flags: &str) -> Result<Outs, String> {
    let mut o = Outs { out: None, solidcraft: None, no_write: false };
    for a in rest {
        if let Some(v) = a.strip_prefix("--out=") {
            o.out = Some(v.into());
        } else if let Some(v) = a.strip_prefix("--solidcraft=") {
            o.solidcraft = Some(v.into());
        } else if a == "--no-write" {
            o.no_write = true;
        } else {
            return Err(format!("error: {a}? It takes {flags} --out=DIR, --solidcraft=DIR and --no-write."));
        }
    }
    Ok(o)
}

/// The validation block every generator prints.
pub struct Checks {
    pub ok: bool,
}

impl Checks {
    pub fn new() -> Checks {
        println!("validation");
        Checks { ok: true }
    }
    pub fn note(&mut self, good: bool, what: &str, detail: String) {
        self.ok &= good;
        println!("  {} {what}: {detail}", if good { "ok  " } else { "FAIL" });
    }
    pub fn done(&self) -> i32 {
        println!("validation {}", if self.ok { "passed" } else { "FAILED" });
        if self.ok {
            0
        } else {
            1
        }
    }
}

impl Default for Checks {
    fn default() -> Self {
        Self::new()
    }
}

pub fn write(path: &str, s: &str) -> Result<(), String> {
    if let Some(d) = std::path::Path::new(path).parent() {
        if !d.as_os_str().is_empty() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
    }
    std::fs::write(path, s).map_err(|e| format!("{path}: {e}"))?;
    println!("wrote {path}");
    Ok(())
}
