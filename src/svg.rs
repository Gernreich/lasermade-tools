//! How a cut file is read: every stroked shape flattened to a polyline in
//! millimetres, with every enclosing transform applied, and the measures the
//! checks and the 3D pages share -- area, inside, stitching open paths into
//! rings. flat-part-check gates with it and turn3d builds parts with it.
//!
//! Ported from flat-part-check.py on 2026-11-07 with the same arithmetic in
//! the same order (math.dist, ** and the trigonometry as CPython does them),
//! so every figure it gives is the figure the Python gave.
use crate::die;
use crate::py::{self, dist, hypot, macos, max, mcos, min, msin, mtan, powf, radians};
use crate::re::{all, findall1, first, g, rx, split, Regex};

pub const FLATNESS: f64 = 0.05; // mm; curve subdivision error budget

pub type P = (f64, f64);

pub type M = (f64, f64, f64, f64, f64, f64);

/// #00FF00, #0f0 and 'lime' are one ink.
pub fn norm(c: Option<&str>) -> String {
    let c = c.unwrap_or("").trim().to_lowercase();
    let named = [
        ("black", "#000000"), ("lime", "#00ff00"), ("blue", "#0000ff"), ("cyan", "#00ffff"),
        ("aqua", "#00ffff"), ("red", "#ff0000"), ("white", "#ffffff"), ("green", "#008000"), ("none", "none"),
    ];
    if let Some((_, v)) = named.iter().find(|(k, _)| *k == c) {
        return v.to_string();
    }
    if c.len() == 4 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_digit() || ('a'..='f').contains(&h)) {
        return format!("#{}", c[1..].chars().flat_map(|h| [h, h]).collect::<String>());
    }
    c
}

pub fn float(s: &str) -> f64 {
    py::float(s).unwrap_or_else(|| die(format!("could not convert string to float: {}", py::str_repr(s))))
}

pub const I: M = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

pub fn mul(m: M, n: M) -> M {
    let (a, b, c, d, e, f) = m;
    let (aa, bb, cc, dd, ee, ff) = n;
    (a * aa + c * bb, b * aa + d * bb, a * cc + c * dd, b * cc + d * dd, a * ee + c * ff + e, b * ee + d * ff + f)
}

pub fn apply(m: M, p: P) -> P {
    let (a, b, c, d, e, f) = m;
    (a * p.0 + c * p.1 + e, b * p.0 + d * p.1 + f)
}

pub struct Rx {
    pub func: Regex,
    pub sep: Regex,
    pub tokens: Regex,
    pub number: Regex,
    pub stroke: Regex,
}

pub fn parse_transform(r: &Rx, s: Option<&str>) -> M {
    let mut m = I;
    for c in all(&r.func, s.unwrap_or("")) {
        let (name, args) = (g(&c, 1), g(&c, 2));
        let v: Vec<f64> = split(&r.sep, args.trim()).into_iter().filter(|x| !x.is_empty()).map(float).collect();
        let at = |i: usize| *v.get(i).unwrap_or_else(|| die(format!("{name}(): too few arguments")));
        match name {
            "translate" => m = mul(m, (1.0, 0.0, 0.0, 1.0, at(0), if v.len() > 1 { v[1] } else { 0.0 })),
            "scale" => m = mul(m, (at(0), 0.0, 0.0, if v.len() > 1 { v[1] } else { v[0] }, 0.0, 0.0)),
            "matrix" if v.len() == 6 => m = mul(m, (v[0], v[1], v[2], v[3], v[4], v[5])),
            "rotate" => {
                let t = radians(at(0));
                let rot = (mcos(t), msin(t), -msin(t), mcos(t), 0.0, 0.0);
                if v.len() == 3 {
                    m = mul(mul(m, (1.0, 0.0, 0.0, 1.0, v[1], v[2])), rot);
                    m = mul(m, (1.0, 0.0, 0.0, 1.0, -v[1], -v[2]));
                } else {
                    m = mul(m, rot);
                }
            }
            "skewX" => m = mul(m, (1.0, 0.0, mtan(radians(at(0))), 1.0, 0.0, 0.0)),
            "skewY" => m = mul(m, (1.0, mtan(radians(at(0))), 0.0, 1.0, 0.0, 0.0)),
            _ => {}
        }
    }
    m
}

/// Segment count for a curve, from its control polygon length.
pub fn steps(pts: &[P]) -> usize {
    let mut n = 0.0;
    for i in 0..pts.len() - 1 {
        n += dist(pts[i], pts[i + 1]);
    }
    4usize.max(160usize.min((n / FLATNESS).sqrt() as usize + 4))
}

pub fn cubic(p0: P, p1: P, p2: P, p3: P) -> Vec<P> {
    let n = steps(&[p0, p1, p2, p3]);
    let mut out = Vec::with_capacity(n);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        out.push((
            powf(u, 3.0) * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + powf(t, 3.0) * p3.0,
            powf(u, 3.0) * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + powf(t, 3.0) * p3.1,
        ));
    }
    out
}

pub fn quad(p0: P, p1: P, p2: P) -> Vec<P> {
    let k = 2.0 / 3.0;
    cubic(p0, (p0.0 + k * (p1.0 - p0.0), p0.1 + k * (p1.1 - p0.1)), (p2.0 + k * (p1.0 - p2.0), p2.1 + k * (p1.1 - p2.1)), p2)
}

/// Endpoint parametrisation, F.6.5 of the SVG spec.
pub fn arc(p0: P, rx: f64, ry: f64, rot: f64, large: i64, sweep: i64, p1: P) -> Vec<P> {
    if p0 == p1 {
        return vec![];
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return vec![p1];
    }
    let t = radians(rot);
    let (cos, sin) = (mcos(t), msin(t));
    let (dx, dy) = ((p0.0 - p1.0) / 2.0, (p0.1 - p1.1) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    // F.6.6: grow the radii if they cannot span the chord
    let lam = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let co = max(0.0, num / den).sqrt() * (if large == sweep { -1.0 } else { 1.0 });
    let (cx1, cy1) = (co * rx * y1 / ry, -co * ry * x1 / rx);
    let cx = cos * cx1 - sin * cy1 + (p0.0 + p1.0) / 2.0;
    let cy = sin * cx1 + cos * cy1 + (p0.1 + p1.1) / 2.0;

    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let mut d = hypot(ux, uy) * hypot(vx, vy);
        if d == 0.0 {
            d = 1e-12;
        }
        let a = macos(max(-1.0, min(1.0, (ux * vx + uy * vy) / d)));
        if ux * vy - uy * vx < 0.0 { -a } else { a }
    };
    let th0 = ang(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dth = ang((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if sweep == 0 && dth > 0.0 {
        dth -= 2.0 * std::f64::consts::PI;
    } else if sweep != 0 && dth < 0.0 {
        dth += 2.0 * std::f64::consts::PI;
    }
    let n = 6usize.max(240usize.min((dth.abs() * max(rx, ry) / max(FLATNESS, 1e-6)) as usize + 6));
    (1..=n)
        .map(|i| {
            let th = th0 + dth * i as f64 / n as f64;
            let (ex, ey) = (rx * mcos(th), ry * msin(th));
            (cos * ex - sin * ey + cx, sin * ex + cos * ey + cy)
        })
        .collect()
}

pub type Sub = (Vec<P>, bool);

/// -> [(points, closed)], one entry per subpath, in user units.
pub fn flatten_path(r: &Rx, d: Option<&str>) -> Vec<Sub> {
    let t: Vec<String> = findall1(&r.tokens, d.unwrap_or(""), 0);
    let mut subs: Vec<Sub> = Vec::new();
    let mut pts: Vec<P> = Vec::new();
    let mut closed = false;
    let (mut cur, mut start): (P, P) = ((0.0, 0.0), (0.0, 0.0));
    let mut cmd: Option<char> = None;
    let (mut prev_c, mut prev_q): (Option<P>, Option<P>) = (None, None);
    let mut i = 0;

    fn end(subs: &mut Vec<Sub>, pts: &mut Vec<P>, closed: &mut bool) {
        if pts.len() > 1 {
            subs.push((std::mem::take(pts), *closed));
        }
        pts.clear();
        *closed = false;
    }
    let is_letter = |s: &str| s.len() == 1 && s.chars().all(|c| c.is_ascii_alphabetic());

    while i < t.len() {
        if is_letter(&t[i]) {
            let c = t[i].chars().next().unwrap();
            cmd = Some(c);
            i += 1;
            if c == 'Z' || c == 'z' {
                if !pts.is_empty() {
                    if pts[0] != *pts.last().unwrap() {
                        pts.push(pts[0]);
                    }
                    closed = true;
                    end(&mut subs, &mut pts, &mut closed);
                }
                cur = start;
                continue;
            }
        }
        let Some(cm) = cmd else { break };
        let rel = cm.is_ascii_lowercase();
        let up = cm.to_ascii_uppercase();
        let mut num = |k: usize| -> Vec<f64> {
            if i + k > t.len() {
                die(format!("path data ends inside a {cm} command"));
            }
            let v = t[i..i + k].iter().map(|x| float(x)).collect();
            i += k;
            v
        };
        let at = |cur: P, x: f64, y: f64| if rel { (cur.0 + x, cur.1 + y) } else { (x, y) };
        match up {
            'M' => {
                let v = num(2);
                end(&mut subs, &mut pts, &mut closed);
                cur = at(cur, v[0], v[1]);
                start = cur;
                pts = vec![cur];
                cmd = Some(if rel { 'l' } else { 'L' });
            }
            'L' => {
                let v = num(2);
                cur = at(cur, v[0], v[1]);
                pts.push(cur);
            }
            'H' => {
                let v = num(1);
                cur = if rel { (cur.0 + v[0], cur.1) } else { (v[0], cur.1) };
                pts.push(cur);
            }
            'V' => {
                let v = num(1);
                cur = if rel { (cur.0, cur.1 + v[0]) } else { (cur.0, v[0]) };
                pts.push(cur);
            }
            'C' | 'S' => {
                let (p1, x2, y2, x, y);
                if up == 'C' {
                    let v = num(6);
                    p1 = at(cur, v[0], v[1]);
                    (x2, y2, x, y) = (v[2], v[3], v[4], v[5]);
                } else {
                    let v = num(4);
                    (x2, y2, x, y) = (v[0], v[1], v[2], v[3]);
                    p1 = match prev_c {
                        Some(pc) => (2.0 * cur.0 - pc.0, 2.0 * cur.1 - pc.1),
                        None => cur,
                    };
                }
                let p2 = at(cur, x2, y2);
                let p3 = at(cur, x, y);
                pts.extend(cubic(cur, p1, p2, p3));
                cur = p3;
                prev_c = Some(p2);
                prev_q = None;
                continue;
            }
            'Q' | 'T' => {
                let (p1, x, y);
                if up == 'Q' {
                    let v = num(4);
                    p1 = at(cur, v[0], v[1]);
                    (x, y) = (v[2], v[3]);
                } else {
                    let v = num(2);
                    (x, y) = (v[0], v[1]);
                    p1 = match prev_q {
                        Some(pq) => (2.0 * cur.0 - pq.0, 2.0 * cur.1 - pq.1),
                        None => cur,
                    };
                }
                let p2 = at(cur, x, y);
                pts.extend(quad(cur, p1, p2));
                cur = p2;
                prev_q = Some(p1);
                prev_c = None;
                continue;
            }
            'A' => {
                let v = num(7);
                let p1 = at(cur, v[5], v[6]);
                pts.extend(arc(cur, v[0], v[1], v[2], v[3] as i64, v[4] as i64, p1));
                cur = p1;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        prev_c = None;
        prev_q = None;
    }
    end(&mut subs, &mut pts, &mut closed);
    subs
}

pub fn nums(r: &Rx, s: Option<&str>) -> Vec<f64> {
    findall1(&r.number, s.unwrap_or(""), 0).iter().map(|x| float(x)).collect()
}

/// -> [(points, closed)] for one drawable element, in user units.
pub fn shape_subpaths(r: &Rx, el: roxmltree::Node) -> Vec<Sub> {
    let tag = el.tag_name().name();
    let get = |k: &str| el.attribute(k).map(float).unwrap_or(0.0);
    match tag {
        "path" => flatten_path(r, el.attribute("d")),
        "rect" => {
            let (x, y, w, h) = (get("x"), get("y"), get("width"), get("height"));
            if w <= 0.0 || h <= 0.0 {
                return vec![];
            }
            vec![(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)], true)]
        }
        "circle" | "ellipse" => {
            let (cx, cy) = (get("cx"), get("cy"));
            let (rx, ry) = if tag == "circle" { (get("r"), get("r")) } else { (get("rx"), get("ry")) };
            if rx <= 0.0 || ry <= 0.0 {
                return vec![];
            }
            let tau = 2.0 * std::f64::consts::PI;
            let n = 24usize.max(360usize.min((tau * max(rx, ry) / max(FLATNESS, 1e-6)) as usize + 8));
            let p = (0..=n)
                .map(|i| (cx + rx * mcos(tau * i as f64 / n as f64), cy + ry * msin(tau * i as f64 / n as f64)))
                .collect();
            vec![(p, true)]
        }
        "polygon" | "polyline" => {
            let v = nums(r, el.attribute("points"));
            let mut p: Vec<P> = v.chunks_exact(2).map(|c| (c[0], c[1])).collect();
            if p.len() < 2 {
                return vec![];
            }
            if tag == "polygon" && p[0] != *p.last().unwrap() {
                p.push(p[0]);
            }
            vec![(p, tag == "polygon")]
        }
        "line" => vec![(vec![(get("x1"), get("y1")), (get("x2"), get("y2"))], false)],
        _ => vec![],
    }
}

/// An element's own stroke: the style property, else the attribute.
pub fn own_stroke<'a>(r: &Rx, el: roxmltree::Node<'a, 'a>) -> Option<String> {
    let style = el.attribute("style").unwrap_or("");
    match first(&r.stroke, style) {
        Some(m) => Some(g(&m, 1).to_string()),
        None => el.attribute("stroke").map(String::from),
    }
}

/// The stroke colour this element cuts in, or None if it is not a cut.
///
/// STROKE IS INHERITED. An element with no stroke of its own takes its
/// ancestors', which is ordinary SVG and what most generators here emit: a
/// <g stroke="#000000"> around a run of bare <path d="...">. Reading only the
/// element's own attributes made every such path "not a cut", so a file full
/// of geometry came out as "no stroked geometry" and stopped after two checks.
/// Found 2026-09-09 by running this over the bore cut files, which is a thing
/// nobody had done: 69 shipped files across knotwork-soundholes, living-hinge
/// and the ribbon bores were getting 2 checks each where bullroarer got 9.
/// The count was the tell -- 13 files and 26 checks against 5 files and 45.
pub fn ink(r: &Rx, el: roxmltree::Node, inherited: Option<&str>) -> Option<String> {
    let own = own_stroke(r, el);
    let c = norm(own.as_deref().or(inherited));
    // a filled shape with no stroke still cuts on some importers, but in these
    // repositories every cut is stroked; treat it as artwork, not geometry
    if c == "none" || c.is_empty() {
        return None;
    }
    Some(c)
}

pub type Geo = (String, Vec<P>, bool);

/// -> [(colour, points, closed)] in millimetres, transforms applied.
pub fn collect(r: &Rx, root: roxmltree::Node) -> Vec<Geo> {
    let mut out = Vec::new();
    let scale = user_scale(r, root);
    fn walk(r: &Rx, el: roxmltree::Node, m: M, stroke: Option<String>, scale: f64, out: &mut Vec<Geo>) {
        let m = mul(m, parse_transform(r, el.attribute("transform")));
        let stroke = own_stroke(r, el).or(stroke);
        for kid in el.children().filter(|k| k.is_element()) {
            let tag = kid.tag_name().name();
            if matches!(tag, "g" | "a" | "svg") {
                walk(r, kid, m, stroke.clone(), scale, out);
                continue;
            }
            if matches!(tag, "defs" | "metadata" | "title" | "desc") {
                continue;
            }
            let Some(c) = ink(r, kid, stroke.as_deref()) else { continue };
            let km = mul(m, parse_transform(r, kid.attribute("transform")));
            for (pts, closed) in shape_subpaths(r, kid) {
                let p = pts.into_iter().map(|q| apply(km, q)).map(|(x, y)| (x * scale, y * scale)).collect();
                out.push((c.clone(), p, closed));
            }
        }
    }
    walk(r, root, I, None, scale, &mut out);
    out
}

/// Millimetres per user unit, from width/height against the viewBox.
pub fn user_scale(r: &Rx, root: roxmltree::Node) -> f64 {
    let vb = nums(r, root.attribute("viewBox"));
    let w = root.attribute("width").unwrap_or("");
    if vb.len() == 4 && vb[2] != 0.0 && w.trim().ends_with("mm") {
        return nums(r, Some(w))[0] / vb[2];
    }
    1.0
}

pub fn area(p: &[P]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len().saturating_sub(1) {
        a += p[i].0 * p[i + 1].1 - p[i + 1].0 * p[i].1;
    }
    a.abs() / 2.0
}

/// Ray casting, over the edges as a cycle.
///
/// Taking the edges as poly[i]..poly[i+1] and stopping leaves out the edge that
/// closes the ring. On a path whose ends only nearly meet — Inkscape writes the
/// return point as an ordinary node, so they differ in the ninth decimal — that
/// missing edge is a real one, and a ray passing between the two endpoint heights
/// loses a crossing and reports inside as outside. BuzzDisc1's outline starts at
/// 3 o'clock, exactly the height of its two cord holes, so it hit this every time.
pub fn inside(pt: P, poly: &[P]) -> bool {
    let (x, y) = pt;
    let mut hit = false;
    let n = poly.len();
    for i in 0..n {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % n];
        if (y1 > y) != (y2 > y) {
            let mut dy = y2 - y1;
            if dy == 0.0 {
                dy = 1e-12;
            }
            let xx = x1 + (y - y1) * (x2 - x1) / dy;
            if x < xx {
                hit = !hit;
            }
        }
    }
    hit
}

/// Join open polylines whose ends meet. -> (rings, leftovers, joins made).
///
/// A profile is not always one path. Drawn as two curves sharing their endpoints it
/// cuts exactly the same ring, so judging each half on its own reports two open
/// paths and no outline — which is what this said about four of the five bullroarer
/// blades before it could stitch.
pub fn stitch(paths: Vec<Vec<P>>, tol: f64) -> (Vec<Vec<P>>, Vec<Vec<P>>, usize) {
    let mut chains = paths;
    let mut joins = 0;
    loop {
        let mut done = true;
        'outer: for i in 0..chains.len() {
            for j in 0..chains.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (&chains[i], &chains[j]);
                let (a0, a1, b0, b1) = (a[0], *a.last().unwrap(), b[0], *b.last().unwrap());
                if dist(a0, a1) <= tol {
                    continue; // already a ring
                }
                let joined: Vec<P> = if dist(a1, b0) <= tol {
                    a.iter().chain(&b[1..]).copied().collect()
                } else if dist(a1, b1) <= tol {
                    a.iter().copied().chain(b.iter().rev().skip(1).copied()).collect()
                } else if dist(a0, b1) <= tol {
                    b.iter().chain(&a[1..]).copied().collect()
                } else if dist(a0, b0) <= tol {
                    b.iter().rev().copied().chain(a[1..].iter().copied()).collect()
                } else {
                    continue;
                };
                chains[i] = joined;
                chains.remove(j);
                joins += 1;
                done = false;
                break 'outer;
            }
        }
        if done {
            break;
        }
    }
    let rings: Vec<Vec<P>> = chains.iter().filter(|c| c.len() > 3 && dist(c[0], *c.last().unwrap()) <= tol).cloned().collect();
    let rest = chains.into_iter().filter(|c| !rings.contains(c)).collect();
    (rings, rest, joins)
}

impl Rx {
    pub fn new() -> Rx {
        Rx {
            func: rx(r"(\w+)\s*\(([^)]*)\)"),
            sep: rx(r"[\s,]+"),
            tokens: rx(r"[MmZzLlHhVvCcSsQqTtAa]|-?\d*\.?\d+(?:[eE][-+]?\d+)?"),
            number: rx(r"-?\d*\.?\d+(?:[eE][-+]?\d+)?"),
            stroke: rx(r"(?<![-\w])stroke\s*:\s*([^;]+)"),
        }
    }
}

impl Default for Rx {
    fn default() -> Rx {
        Rx::new()
    }
}

/// Read an SVG file, as ElementTree would, or the parser's complaint.
pub fn parse(src: &str) -> Result<roxmltree::Document<'_>, String> {
    let opt = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    roxmltree::Document::parse_with_options(src, opt).map_err(|e| e.to_string())
}
