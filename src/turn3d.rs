//! Turn a flat cut file into parts you can assemble and drag round in 3D.
//!
//! The bores have had a turnable viewer since they were first drawn, and the
//! flute and the lyre-harp grew one from their generators. The flat
//! instruments -- kalimba, slapstick, bullroarer, buzz-disc -- are hand-drawn
//! SVGs with no generator behind them, so their viewer is built from the cut
//! file itself: the same geometry flat-part-check measures, read by the same
//! parser (crate::svg), so what turns on screen is what the laser cuts.
//!
//! ```text
//! let parts = turn3d::parts("Kalimba.svg");   // outline, holes, engraving, box, ink
//! turn3d::write("kalimba-3d.html", title, &mut designs, note, by, spin);
//! ```
//!
//! A part is an outline (`o`), its holes (`h`), and the engraved lines that
//! fall on it (`e`), all in millimetres with y pointing UP -- the SVG's y is
//! flipped on the way in so a part placed in 3D reads the way it reads on the
//! sheet. Nesting decides which is which: a ring inside no other is an
//! outline, a ring inside an outline is a hole, and a ring inside a hole is an
//! outline again (an island that falls out with the hole is waste, and is
//! dropped). Blue is the engrave; violet is skip and is ignored.
//!
//! Each repository's viewer places the parts -- that is the one thing the cut
//! file does not say -- and calls write(). The Python generators that draw
//! their own parts (panpipes, the chanters) hand the same thing over as JSON
//! to the `turn3d` program. The page is self-contained apart from three.js
//! r128 off cdnjs, the version the lyre-harp viewer already loads.
//!
//! Ported from turn3d.py on 2026-11-07. The pages are byte for byte what the
//! Python wrote, and that needs Python's two kinds of number: a placing
//! matrix built from T(0, 0, 0) holds the integers 0 and 1 and prints them as
//! `0` and `1`, where a float would print `0.0`. So every number that reaches
//! the page is a [`Num`], an integer until arithmetic with a float makes it
//! one, as in Python.
use crate::py::{self, dist, max};
use crate::svg::{self, area, collect, inside, stitch, P};
use std::ops::{Add, Mul, Neg, Sub};

pub const ENGRAVE: &str = "#0000ff";
pub const SKIP: &str = "#8000ff";
pub const TOL: f64 = 0.05; // mm: two path ends this close are one corner

// --- numbers as Python has them ---------------------------------------------

/// An int or a float, with Python's promotion: int op int stays int, anything
/// with a float is a float.
#[derive(Clone, Copy, Debug)]
pub enum Num {
    I(i64),
    F(f64),
}
pub use Num::{F, I};

impl Num {
    pub fn f(self) -> f64 {
        match self {
            I(n) => n as f64,
            F(x) => x,
        }
    }
    /// truthiness
    pub fn truthy(self) -> bool {
        match self {
            I(n) => n != 0,
            F(x) => x != 0.0,
        }
    }
    /// round(v, dp): an int stays itself, a float rounds as Python's does
    pub fn round(self, dp: usize) -> Num {
        match self {
            I(n) => I(n),
            F(x) => F(py::round_dp(x, dp)),
        }
    }
    /// / always gives a float
    pub fn div(self, o: Num) -> Num {
        F(self.f() / o.f())
    }
    pub fn lt(self, o: Num) -> bool {
        match (self, o) {
            (I(a), I(b)) => a < b,
            _ => self.f() < o.f(),
        }
    }
    /// repr(): what json.dumps writes for it
    pub fn repr(self) -> String {
        match self {
            I(n) => n.to_string(),
            F(x) => py::repr(x),
        }
    }
}

macro_rules! op {
    ($tr:ident, $f:ident, $o:tt) => {
        impl $tr for Num {
            type Output = Num;
            fn $f(self, o: Num) -> Num {
                match (self, o) {
                    (I(a), I(b)) => I(a $o b),
                    _ => F(self.f() $o o.f()),
                }
            }
        }
    };
}
op!(Add, add, +);
op!(Sub, sub, -);
op!(Mul, mul, *);
impl Neg for Num {
    type Output = Num;
    fn neg(self) -> Num {
        match self {
            I(a) => I(-a),
            F(x) => F(-x),
        }
    }
}
impl From<f64> for Num {
    fn from(x: f64) -> Num {
        F(x)
    }
}
impl From<i64> for Num {
    fn from(n: i64) -> Num {
        I(n)
    }
}
impl From<i32> for Num {
    fn from(n: i32) -> Num {
        I(n as i64)
    }
}

/// max() / min() of a run, keeping the first of equals, as Python's do
fn nmax(v: &[Num]) -> Num {
    v[1..].iter().fold(v[0], |a, &b| if a.lt(b) { b } else { a })
}
fn nmin(v: &[Num]) -> Num {
    v[1..].iter().fold(v[0], |a, &b| if b.lt(a) { b } else { a })
}

// --- a JSON value, in Python's dict order -----------------------------------

#[derive(Clone, Debug)]
pub enum J {
    N(Num),
    S(String),
    B(bool),
    Null,
    A(Vec<J>),
    O(Vec<(String, J)>),
}

impl J {
    pub fn get(&self, k: &str) -> Option<&J> {
        match self {
            J::O(kv) => kv.iter().find(|(a, _)| a == k).map(|(_, v)| v),
            _ => None,
        }
    }
    /// d[k] = v: replaced where it is, else added at the end
    pub fn set(&mut self, k: &str, v: J) {
        if let J::O(kv) = self {
            match kv.iter_mut().find(|(a, _)| a == k) {
                Some(e) => e.1 = v,
                None => kv.push((k.to_string(), v)),
            }
        }
    }
    pub fn arr(&self) -> &[J] {
        match self {
            J::A(v) => v,
            _ => &[],
        }
    }
    pub fn num(&self) -> Num {
        match self {
            J::N(n) => *n,
            _ => panic!("not a number: {self:?}"),
        }
    }
    pub fn nums(&self) -> Vec<Num> {
        self.arr().iter().map(J::num).collect()
    }
    pub fn pts(&self) -> Vec<[Num; 2]> {
        self.arr().iter().map(|q| [q.arr()[0].num(), q.arr()[1].num()]).collect()
    }
    pub fn str(&self) -> &str {
        match self {
            J::S(s) => s,
            _ => "",
        }
    }
    pub fn obj(kv: Vec<(&str, J)>) -> J {
        J::O(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    /// json.dumps(self, separators=(',', ':')), ensure_ascii as by default
    pub fn dump(&self, out: &mut String) {
        match self {
            J::N(n) => out.push_str(&n.repr()),
            J::B(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Null => out.push_str("null"),
            J::S(s) => dump_str(s, out),
            J::A(v) => {
                out.push('[');
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    x.dump(out);
                }
                out.push(']');
            }
            J::O(kv) => {
                out.push('{');
                for (i, (k, x)) in kv.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    dump_str(k, out);
                    out.push(':');
                    x.dump(out);
                }
                out.push('}');
            }
        }
    }

    /// json.dump(self, f, indent=N): ',' and ': ' with each item on its own
    /// line, an empty list or object as [] or {}
    pub fn dump_indent(&self, indent: usize, level: usize, out: &mut String) {
        let pad = |out: &mut String, l: usize| {
            out.push('\n');
            out.push_str(&" ".repeat(indent * l));
        };
        match self {
            J::A(v) if !v.is_empty() => {
                out.push('[');
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, level + 1);
                    x.dump_indent(indent, level + 1, out);
                }
                pad(out, level);
                out.push(']');
            }
            J::O(kv) if !kv.is_empty() => {
                out.push('{');
                for (i, (k, x)) in kv.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, level + 1);
                    dump_str(k, out);
                    out.push_str(": ");
                    x.dump_indent(indent, level + 1, out);
                }
                pad(out, level);
                out.push('}');
            }
            _ => self.dump(out),
        }
    }

    /// Read what json.dumps wrote: ints stay ints, objects keep their order.
    pub fn parse(s: &str) -> Result<J, String> {
        let b = s.as_bytes();
        let mut i = 0;
        let v = parse_at(b, &mut i)?;
        ws(b, &mut i);
        if i != b.len() {
            return Err(format!("JSON: extra data at byte {i}"));
        }
        Ok(v)
    }
}

/// json.dumps(s): a string in quotes, ensure_ascii as by default
pub fn dump_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\n' | b'\r') {
        *i += 1;
    }
}

fn parse_at(b: &[u8], i: &mut usize) -> Result<J, String> {
    ws(b, i);
    let err = |i: usize, what: &str| Err(format!("JSON: {what} at byte {i}"));
    match b.get(*i) {
        None => err(*i, "unexpected end"),
        Some(b'{') => {
            *i += 1;
            let mut kv = vec![];
            ws(b, i);
            if b.get(*i) == Some(&b'}') {
                *i += 1;
                return Ok(J::O(kv));
            }
            loop {
                ws(b, i);
                let k = match parse_at(b, i)? {
                    J::S(s) => s,
                    _ => return err(*i, "a key that is not a string"),
                };
                ws(b, i);
                if b.get(*i) != Some(&b':') {
                    return err(*i, "':' expected");
                }
                *i += 1;
                let v = parse_at(b, i)?;
                kv.push((k, v));
                ws(b, i);
                match b.get(*i) {
                    Some(b',') => *i += 1,
                    Some(b'}') => {
                        *i += 1;
                        return Ok(J::O(kv));
                    }
                    _ => return err(*i, "',' or '}' expected"),
                }
            }
        }
        Some(b'[') => {
            *i += 1;
            let mut v = vec![];
            ws(b, i);
            if b.get(*i) == Some(&b']') {
                *i += 1;
                return Ok(J::A(v));
            }
            loop {
                v.push(parse_at(b, i)?);
                ws(b, i);
                match b.get(*i) {
                    Some(b',') => *i += 1,
                    Some(b']') => {
                        *i += 1;
                        return Ok(J::A(v));
                    }
                    _ => return err(*i, "',' or ']' expected"),
                }
            }
        }
        Some(b'"') => {
            *i += 1;
            let mut s = String::new();
            let mut units: Vec<u16> = vec![];
            loop {
                let c = match b.get(*i) {
                    None => return err(*i, "unterminated string"),
                    Some(c) => *c,
                };
                if c == b'\\' {
                    let e = *b.get(*i + 1).ok_or("JSON: bad escape")?;
                    *i += 2;
                    if e == b'u' {
                        let h = std::str::from_utf8(&b[*i..*i + 4]).map_err(|e| e.to_string())?;
                        units.push(u16::from_str_radix(h, 16).map_err(|e| e.to_string())?);
                        *i += 4;
                        continue;
                    }
                    s.push_str(&String::from_utf16_lossy(&units));
                    units.clear();
                    s.push(match e {
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'/' => '/',
                        b'\\' => '\\',
                        b'"' => '"',
                        _ => return err(*i, "bad escape"),
                    });
                    continue;
                }
                s.push_str(&String::from_utf16_lossy(&units));
                units.clear();
                if c == b'"' {
                    *i += 1;
                    return Ok(J::S(s));
                }
                // a run of plain bytes, which may be UTF-8
                let st = *i;
                while *i < b.len() && b[*i] != b'"' && b[*i] != b'\\' {
                    *i += 1;
                }
                s.push_str(std::str::from_utf8(&b[st..*i]).map_err(|e| e.to_string())?);
            }
        }
        Some(b't') if b[*i..].starts_with(b"true") => {
            *i += 4;
            Ok(J::B(true))
        }
        Some(b'f') if b[*i..].starts_with(b"false") => {
            *i += 5;
            Ok(J::B(false))
        }
        Some(b'n') if b[*i..].starts_with(b"null") => {
            *i += 4;
            Ok(J::Null)
        }
        Some(_) => {
            let st = *i;
            while *i < b.len() && matches!(b[*i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                *i += 1;
            }
            let t = std::str::from_utf8(&b[st..*i]).unwrap();
            if t.is_empty() {
                return err(st, "unexpected character");
            }
            if t.contains(['.', 'e', 'E']) {
                t.parse::<f64>().map(|x| J::N(F(x))).map_err(|e| format!("JSON: {t}: {e}"))
            } else {
                t.parse::<i64>().map(|n| J::N(I(n))).map_err(|e| format!("JSON: {t}: {e}"))
            }
        }
    }
}

// --- the cut file's parts ---------------------------------------------------

pub type Pt = [Num; 2];

#[derive(Clone, Debug)]
pub struct Part {
    pub o: Vec<Pt>,
    pub h: Vec<Vec<Pt>>,
    pub e: Vec<Vec<Pt>>,
    pub ink: String,
    /// (x0, y0, x1, y1)
    pub bx: [Num; 4],
}

impl Part {
    /// the outline as plain floats
    pub fn of(&self) -> Vec<P> {
        fl(&self.o)
    }
}

pub fn fl(p: &[Pt]) -> Vec<P> {
    p.iter().map(|q| (q[0].f(), q[1].f())).collect()
}
fn nm(p: &[P]) -> Vec<Pt> {
    p.iter().map(|&(x, y)| [F(x), F(y)]).collect()
}

/// Drop points that sit on the line between their neighbours.
fn simplify(p: Vec<P>, eps: f64) -> Vec<P> {
    if p.len() < 4 {
        return p;
    }
    let mut out = vec![p[0]];
    for i in 1..p.len() - 1 {
        let (a, b, c) = (*out.last().unwrap(), p[i], p[i + 1]);
        let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if cross.abs() > eps * max(dist(a, c), 1e-9) {
            out.push(b);
        }
    }
    out.push(*p.last().unwrap());
    out
}

fn ring(p: &[P]) -> Vec<P> {
    let mut r = p.to_vec();
    r.push(p[0]);
    r
}

/// The cut file's geometry, read as flat-part-check reads it:
/// (colour, points, closed) in millimetres.
pub fn geometry(svg_path: &str) -> Result<(Vec<svg::Geo>, f64), String> {
    let src = std::fs::read_to_string(svg_path).map_err(|e| format!("{svg_path}: {e}"))?;
    let doc = svg::parse(&src).map_err(|e| format!("{svg_path}: {e}"))?;
    let r = svg::Rx::new();
    let root = doc.root_element();
    let vb = root.attribute("viewBox").map(|v| v.split_whitespace().nth(2).map(svg::float).unwrap_or(f64::NAN)).unwrap_or(f64::NAN);
    Ok((collect(&r, root), vb))
}

/// -> the cut file's parts, largest first, y up, in millimetres.
pub fn parts(svg_path: &str) -> Vec<Part> {
    parts_eps(svg_path, 0.02)
}

/// parts(), each ring's points dropped where they lie within eps of the
/// line through their neighbours: 0.02 mm for a 3D view, 0 for a design
/// that must be the cut file's own. (At 0.02 the tip of a knotwork
/// septafoil's 11.6 degree waste piece went, a point 0.057 mm past its
/// neighbour and that close to the line back through it.)
pub fn parts_eps(svg_path: &str, eps: f64) -> Vec<Part> {
    let (got, _) = geometry(svg_path).unwrap_or_else(|e| crate::die(e));
    let mut cuts: Vec<(String, Vec<Vec<P>>)> = vec![];
    let mut engr: Vec<Vec<P>> = vec![];
    for (c, pts, _) in got {
        if c == SKIP {
            continue;
        }
        let flip: Vec<P> = pts.iter().map(|&(x, y)| (x, -y)).collect();
        if c == ENGRAVE {
            engr.push(flip);
            continue;
        }
        match cuts.iter_mut().find(|(k, _)| *k == c) {
            Some(e) => e.1.push(flip),
            None => cuts.push((c, vec![flip])),
        }
    }
    let mut rings: Vec<(String, Vec<P>)> = vec![];
    for (c, paths) in cuts {
        let closed: Vec<Vec<P>> = paths.iter().filter(|p| dist(p[0], *p.last().unwrap()) <= TOL).cloned().collect();
        let rest: Vec<Vec<P>> = paths.iter().filter(|p| dist(p[0], *p.last().unwrap()) > TOL).cloned().collect();
        let joined = if rest.is_empty() { vec![] } else { stitch(rest, TOL).0 };
        for p in closed.into_iter().chain(joined) {
            let p = if p[0] == *p.last().unwrap() { p[..p.len() - 1].to_vec() } else { p };
            rings.push((c.clone(), simplify(p, eps)));
        }
    }
    let key = |r: &(String, Vec<P>)| -area(&ring(&r.1)).abs();
    rings.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap());

    // The smallest ring that contains ring i, or None. Most of its vertices,
    // not its first: parts laid edge to edge share a cut line, and a first
    // vertex on that line reads as inside the neighbour.
    let closed: Vec<Vec<P>> = rings.iter().map(|r| ring(&r.1)).collect();
    let holder = |i: usize| -> Option<usize> {
        let pts = &rings[i].1;
        let mut best = None;
        for (j, rj) in closed.iter().enumerate().take(i) {
            if pts.iter().filter(|&&q| inside(q, rj)).count() * 2 > pts.len() {
                best = Some(j);
            }
        }
        best
    };
    let mut depth: Vec<usize> = vec![];
    let mut owner: Vec<Option<usize>> = vec![];
    for i in 0..rings.len() {
        let j = holder(i);
        depth.push(match j {
            None => 0,
            Some(j) => depth[j] + 1,
        });
        owner.push(j);
    }
    let mut out: Vec<Part> = vec![];
    let mut index: Vec<Option<usize>> = vec![None; rings.len()];
    for (i, (c, p)) in rings.iter().enumerate() {
        if depth[i] == 0 {
            index[i] = Some(out.len());
            out.push(Part { o: nm(p), h: vec![], e: vec![], ink: c.clone(), bx: [I(0); 4] });
        } else if depth[i] % 2 == 1 {
            if let Some(k) = owner[i].and_then(|o| index[o]) {
                out[k].h.push(nm(p));
            }
        }
    }
    for e in engr {
        for prt in out.iter_mut() {
            let o = ring(&prt.of());
            if inside(e[0], &o) || inside(e[e.len() / 2], &o) {
                prt.e.push(nm(&e));
                break;
            }
        }
    }
    for prt in out.iter_mut() {
        let xs: Vec<Num> = prt.o.iter().map(|q| q[0]).collect();
        let ys: Vec<Num> = prt.o.iter().map(|q| q[1]).collect();
        prt.bx = [nmin(&xs), nmin(&ys), nmax(&xs), nmax(&ys)];
    }
    out
}

/// The part moved so (cx, cy) -- default its bounding-box centre -- is the origin.
pub fn centred(prt: &Part, c: Option<(Num, Num)>) -> Part {
    let [x0, y0, x1, y1] = prt.bx;
    let (cx, cy) = c.unwrap_or(((x0 + x1).div(I(2)), (y0 + y1).div(I(2))));
    let mv = |p: &Vec<Pt>| p.iter().map(|q| [q[0] - cx, q[1] - cy]).collect::<Vec<Pt>>();
    Part {
        o: mv(&prt.o),
        h: prt.h.iter().map(mv).collect(),
        e: prt.e.iter().map(mv).collect(),
        ink: prt.ink.clone(),
        bx: [x0 - cx, y0 - cy, x1 - cx, y1 - cy],
    }
}

// --- placing: 4x4 matrices, row-major, that take the part's sheet plane -----
// (x, y, z in 0..3mm) to where it sits in the instrument.

pub type Mat = Vec<Vec<Num>>;

pub fn matmul(a: &Mat, b: &Mat) -> Mat {
    (0..4)
        .map(|i| (0..4).map(|j| (0..4).fold(I(0), |s, k| s + a[i][k] * b[k][j])).collect())
        .collect()
}

#[allow(non_snake_case)]
pub fn T(x: impl Into<Num>, y: impl Into<Num>, z: impl Into<Num>) -> Mat {
    vec![
        vec![I(1), I(0), I(0), x.into()],
        vec![I(0), I(1), I(0), y.into()],
        vec![I(0), I(0), I(1), z.into()],
        vec![I(0), I(0), I(0), I(1)],
    ]
}

#[allow(non_snake_case)]
pub fn Rx(t: f64) -> Mat {
    let (c, s) = (F(py::mcos(t)), F(py::msin(t)));
    vec![vec![I(1), I(0), I(0), I(0)], vec![I(0), c, -s, I(0)], vec![I(0), s, c, I(0)], vec![I(0), I(0), I(0), I(1)]]
}

#[allow(non_snake_case)]
pub fn Ry(t: f64) -> Mat {
    let (c, s) = (F(py::mcos(t)), F(py::msin(t)));
    vec![vec![c, I(0), s, I(0)], vec![I(0), I(1), I(0), I(0)], vec![-s, I(0), c, I(0)], vec![I(0), I(0), I(0), I(1)]]
}

#[allow(non_snake_case)]
pub fn Rz(t: f64) -> Mat {
    let (c, s) = (F(py::mcos(t)), F(py::msin(t)));
    vec![vec![c, -s, I(0), I(0)], vec![s, c, I(0), I(0)], vec![I(0), I(0), I(1), I(0)], vec![I(0), I(0), I(0), I(1)]]
}

pub fn chain(ms: &[Mat]) -> Mat {
    ms.iter().fold(T(0, 0, 0), |out, m| matmul(&out, m))
}

fn rpts(p: &[Pt], dp: usize) -> J {
    J::A(p.iter().map(|q| J::A(vec![J::N(q[0].round(dp)), J::N(q[1].round(dp))])).collect())
}

/// One part ready for the page: geometry, where it goes, and which way it
/// comes out when the explode slider is drawn.
pub fn placed_raw(o: &[Pt], h: &[Vec<Pt>], e: &[Vec<Pt>], m: &Mat, explode: &[Num], wood: &str) -> J {
    J::obj(vec![
        ("o", rpts(o, 2)),
        ("h", J::A(h.iter().map(|x| rpts(x, 2)).collect())),
        ("e", J::A(e.iter().map(|x| rpts(x, 2)).collect())),
        ("m", J::A(m[..3].iter().map(|row| J::A(row.iter().map(|v| J::N(v.round(4))).collect())).collect())),
        ("x", J::A(explode.iter().map(|v| J::N(v.round(3))).collect())),
        ("w", J::S(wood.into())),
    ])
}

pub fn placed(prt: &Part, m: &Mat, explode: [Num; 3], wood: &str) -> J {
    placed_raw(&prt.o, &prt.h, &prt.e, m, &explode, wood)
}

/// The no-explode default: (0, 0, 0).
pub const STILL: [Num; 3] = [I(0), I(0), I(0)];

pub fn nums(v: &[Num]) -> J {
    J::A(v.iter().map(|&n| J::N(n)).collect())
}

/// A cord: a run of xyz points, a loop if the last is the first.
pub fn cord(pts: &[[Num; 3]]) -> J {
    J::A(pts.iter().map(|p| nums(p)).collect())
}

// --- the page ----------------------------------------------------------------

const PAGE: &str = include_str!("turn3d-page.html");
const SPIN: &str = "<label><input type=\"checkbox\" id=\"spin\"> Spin it</label>";

/// Centre, radius, size and half-extents of a design as placed, from its outline vertices.
fn bounds(design: &J) -> (J, J, J, J) {
    let (mut xs, mut ys, mut zs): (Vec<Num>, Vec<Num>, Vec<Num>) = (vec![], vec![], vec![]);
    for p in design.get("parts").map(J::arr).unwrap_or(&[]) {
        let m: Vec<Vec<Num>> = p.get("m").unwrap().arr().iter().map(J::nums).collect();
        for q in p.get("o").unwrap().arr() {
            let (x, y) = (q.arr()[0].num(), q.arr()[1].num());
            for z in [I(0), I(3)] {
                xs.push(m[0][0] * x + m[0][1] * y + m[0][2] * z + m[0][3]);
                ys.push(m[1][0] * x + m[1][1] * y + m[1][2] * z + m[1][3]);
                zs.push(m[2][0] * x + m[2][1] * y + m[2][2] * z + m[2][3]);
            }
        }
    }
    for c in design.get("cords").map(J::arr).unwrap_or(&[]) {
        for q in c.arr() {
            let v = q.nums();
            xs.push(v[0]);
            ys.push(v[1]);
            zs.push(v[2]);
        }
    }
    let all = [&xs, &ys, &zs];
    let c: Vec<Num> = all.iter().map(|v| (nmin(v) + nmax(v)).div(I(2))).collect();
    let e: Vec<Num> = all.iter().map(|v| (nmax(v) - nmin(v)).div(I(2))).collect();
    let size = all
        .iter()
        .map(|v| nmax(v) - nmin(v))
        .filter(|d| F(0.5).lt(*d))
        .map(|d| format!("{:.0}", d.f()))
        .collect::<Vec<_>>()
        .join(" × ")
        + "mm";
    let r = nmax(&e).round(2);
    (nums(&c.iter().map(|v| v.round(2)).collect::<Vec<_>>()), J::N(r), J::S(size), nums(&e.iter().map(|v| v.round(2)).collect::<Vec<_>>()))
}

/// designs: name -> {'parts': [placed(...)], 'cords': [cord(...)], 'axis':
/// [ox,oy,oz,dx,dy,dz], 'ex': mm the explode slider moves a part per unit of
/// its 'x', 'view': [yaw, pitch]}; a third 'view' number, the distance in
/// radii it once set, is ignored: the view opens fitted to its frame.
pub fn write(path: &str, title: &str, designs: &mut Vec<(String, J)>, note: &str, by: &str, spin: bool) -> String {
    for (_, d) in designs.iter_mut() {
        let (c, r, size, e) = bounds(d);
        d.set("c", c);
        d.set("r", r);
        d.set("size", size);
        d.set("e", e);
    }
    let mut data = String::new();
    J::O(designs.clone()).dump(&mut data);
    let exhide = designs.iter().any(|(_, d)| d.get("ex").map(|x| x.num().truthy()).unwrap_or(true));
    // one part has nothing to come apart: the slider stays in the page,
    // since the script reads it, but out of sight
    let page = PAGE
        .replace("__DATA__", &data)
        .replace("__TITLE__", title)
        .replace("__NOTE__", note)
        .replace("__BY__", by)
        .replace("__SPIN__", if spin { SPIN } else { "" })
        .replace("__EXHIDE__", if exhide { "" } else { " hidden" });
    std::fs::write(path, &page).unwrap_or_else(|e| crate::die(format!("{path}: {e}")));
    page
}

/// The design list a viewer builds up, in order.
pub fn design(kv: Vec<(&str, J)>) -> J {
    J::obj(kv)
}

/// `--out=PATH` from the arguments, else the default beside the viewer.
pub fn out_arg(default: &str) -> String {
    std::env::args().skip(1).filter_map(|a| a.strip_prefix("--out=").map(String::from)).next().unwrap_or_else(|| default.to_string())
}

/// os.path.relpath(path) for the "wrote" line
pub fn relpath(path: &str) -> String {
    let cwd = std::env::current_dir().unwrap();
    let p = std::path::Path::new(path);
    let abs = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let norm = |p: &std::path::Path| -> Vec<String> {
        let mut v: Vec<String> = vec![];
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    v.pop();
                }
                std::path::Component::Normal(s) => v.push(s.to_string_lossy().into()),
                _ => {}
            }
        }
        v
    };
    let (a, b) = (norm(&abs), norm(&cwd));
    let k = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = vec!["..".into(); b.len() - k];
    out.extend(a[k..].iter().cloned());
    if out.is_empty() {
        ".".into()
    } else {
        out.join("/")
    }
}

