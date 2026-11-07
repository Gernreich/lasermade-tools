//! Check a flat single-sheet cut file before it is sent to the laser.
//!
//! The bore repositories have `bore_split.py --write`, which gates every net it writes.
//! The flat parts — bullroarer blades, buzz discs, anything cut from one sheet with holes
//! in it — had nothing. This is that gate. It reads the geometry rather than the drawing:
//! paths, rects, circles, ellipses, polygons and lines, flattened to polylines with every
//! enclosing transform applied, and then measured.
//!
//!     target/release/flat-part-check FILE.svg [FILE.svg ...]
//!     target/release/flat-part-check --dir DIR [--dir DIR ...]
//!     target/release/flat-part-check FILE.svg --min-edge 4 --bed 600x305
//!
//! What it checks, and the failure each one is looking for:
//!
//!   millimetre-true         a file whose user unit is not a millimetre cuts at 96/25.4
//!                           and looks right on screen the whole time
//!   fits the bed            a part wider than the bed is refused at the machine, or
//!                           worse, silently cropped
//!   cut paths are closed    an unclosed outline does not free the part, and the laser
//!                           lifts mid-run where the ends fail to meet
//!   ink is in the palette   colour is the cut order in these repositories; a colour
//!                           nothing recognises is a stage nothing runs
//!   black frees the part    the outermost path must be the last stage, or the part
//!                           moves while its holes are still being cut
//!   holes are inside        a hole outside the outline is a mark on the waste, and
//!                           usually means a transform was missed
//!   hole is big enough      the cord holes are the point of these parts
//!   edge distance           the cord hole is where a bullroarer fails: too little
//!                           material between hole and edge and it tears out
//!
//! Exit status is 1 if any file fails, so it can gate a commit.
//!
//! Not modelled: kerf. Every measurement is of the path as drawn, and the beam takes
//! its width from both sides of it — a 3.0mm hole cuts about 3.1mm and a 3.0mm wall
//! comes out about 2.9mm. Set --min-edge with that in hand rather than at the limit.
//!
//!
//! Ported from Python on 2026-11-07, keeping the same arithmetic in the same
//! order (math.dist, ** and the trigonometry as CPython does them), so every
//! figure it prints is the figure the Python printed.
use lasermade_tools::py::{self, dist, hypot, macos, max, mcos, min, msin, mtan, powf, radians};
use lasermade_tools::re::{all, findall1, first, g, rx, split, Regex};
use lasermade_tools::{die, text};
use std::collections::BTreeSet;

const BED_W: f64 = 600.0; // xTool P2S work area, mm (cut-tested 2026-10-19)
const BED_H: f64 = 305.0;
const MIN_EDGE: f64 = 3.0; // mm of material between a hole and any other edge
const MIN_HOLE: f64 = 2.0; // mm across, the smallest hole worth cutting
const FLATNESS: f64 = 0.05; // mm; curve subdivision error budget
// A path can close without saying z: Inkscape writes the return point as an ordinary
// node, and the ends coincide. The beam still travels the whole loop, so that cuts
// the same. Anything under this is the same point twice.
const CLOSE_TOL: f64 = 0.01; // mm
// Two open paths can be one cut: the bullroarer blades are drawn as a top curve and a
// bottom curve meeting at the two tips, and the part is freed by the pair. Ends this
// close are the same corner, and a laser importer welds them the same way.
const WELD_TOL: f64 = 0.05; // mm

// Colour is the cut order: blue engraves, then green, orange, cyan, black. Black is
// always the cut that frees the part. Violet means skip.
const ORDER: [&str; 5] = ["#0000ff", "#00ff00", "#ff8000", "#00ffff", "#000000"];
const SKIP: &str = "#8000ff";
const ENGRAVE: &str = "#0000ff"; // marks the part, never cuts it

type P = (f64, f64);
type M = (f64, f64, f64, f64, f64, f64);

/// #00FF00, #0f0 and 'lime' are one ink.
fn norm(c: Option<&str>) -> String {
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

fn float(s: &str) -> f64 {
    py::float(s).unwrap_or_else(|| die(format!("could not convert string to float: {}", py::str_repr(s))))
}

// --- transforms -------------------------------------------------------------
// (a, b, c, d, e, f) as in SVG: x' = a x + c y + e,  y' = b x + d y + f

const I: M = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

fn mul(m: M, n: M) -> M {
    let (a, b, c, d, e, f) = m;
    let (aa, bb, cc, dd, ee, ff) = n;
    (a * aa + c * bb, b * aa + d * bb, a * cc + c * dd, b * cc + d * dd, a * ee + c * ff + e, b * ee + d * ff + f)
}

fn apply(m: M, p: P) -> P {
    let (a, b, c, d, e, f) = m;
    (a * p.0 + c * p.1 + e, b * p.0 + d * p.1 + f)
}

struct Rx {
    func: Regex,
    sep: Regex,
    tokens: Regex,
    number: Regex,
    stroke: Regex,
}

fn parse_transform(r: &Rx, s: Option<&str>) -> M {
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

// --- path flattening --------------------------------------------------------

/// Segment count for a curve, from its control polygon length.
fn steps(pts: &[P]) -> usize {
    let mut n = 0.0;
    for i in 0..pts.len() - 1 {
        n += dist(pts[i], pts[i + 1]);
    }
    4usize.max(160usize.min((n / FLATNESS).sqrt() as usize + 4))
}

fn cubic(p0: P, p1: P, p2: P, p3: P) -> Vec<P> {
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

fn quad(p0: P, p1: P, p2: P) -> Vec<P> {
    let k = 2.0 / 3.0;
    cubic(p0, (p0.0 + k * (p1.0 - p0.0), p0.1 + k * (p1.1 - p0.1)), (p2.0 + k * (p1.0 - p2.0), p2.1 + k * (p1.1 - p2.1)), p2)
}

/// Endpoint parametrisation, F.6.5 of the SVG spec.
fn arc(p0: P, rx: f64, ry: f64, rot: f64, large: i64, sweep: i64, p1: P) -> Vec<P> {
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

type Sub = (Vec<P>, bool);

/// -> [(points, closed)], one entry per subpath, in user units.
fn flatten_path(r: &Rx, d: Option<&str>) -> Vec<Sub> {
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

// --- shapes -----------------------------------------------------------------

fn nums(r: &Rx, s: Option<&str>) -> Vec<f64> {
    findall1(&r.number, s.unwrap_or(""), 0).iter().map(|x| float(x)).collect()
}

/// -> [(points, closed)] for one drawable element, in user units.
fn shape_subpaths(r: &Rx, el: roxmltree::Node) -> Vec<Sub> {
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
fn own_stroke<'a>(r: &Rx, el: roxmltree::Node<'a, 'a>) -> Option<String> {
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
fn ink(r: &Rx, el: roxmltree::Node, inherited: Option<&str>) -> Option<String> {
    let own = own_stroke(r, el);
    let c = norm(own.as_deref().or(inherited));
    // a filled shape with no stroke still cuts on some importers, but in these
    // repositories every cut is stroked; treat it as artwork, not geometry
    if c == "none" || c.is_empty() {
        return None;
    }
    Some(c)
}

type Geo = (String, Vec<P>, bool);

/// -> [(colour, points, closed)] in millimetres, transforms applied.
fn collect(r: &Rx, root: roxmltree::Node) -> Vec<Geo> {
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
fn user_scale(r: &Rx, root: roxmltree::Node) -> f64 {
    let vb = nums(r, root.attribute("viewBox"));
    let w = root.attribute("width").unwrap_or("");
    if vb.len() == 4 && vb[2] != 0.0 && w.trim().ends_with("mm") {
        return nums(r, Some(w))[0] / vb[2];
    }
    1.0
}

// --- measurement ------------------------------------------------------------

fn bbox<'a>(p: impl IntoIterator<Item = &'a P>) -> (f64, f64, f64, f64) {
    let mut it = p.into_iter();
    let f = *it.next().unwrap_or_else(|| die("min() arg is an empty sequence"));
    let (mut x0, mut y0, mut x1, mut y1) = (f.0, f.1, f.0, f.1);
    for q in it {
        x0 = min(x0, q.0);
        y0 = min(y0, q.1);
        x1 = max(x1, q.0);
        y1 = max(y1, q.1);
    }
    (x0, y0, x1, y1)
}

fn area(p: &[P]) -> f64 {
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
fn inside(pt: P, poly: &[P]) -> bool {
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

fn centroid(p: &[P]) -> P {
    let (mut sx, mut sy) = (0.0, 0.0);
    for q in p {
        sx += q.0;
        sy += q.1;
    }
    (sx / p.len() as f64, sy / p.len() as f64)
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let (ax, ay) = a;
    let (dx, dy) = (b.0 - ax, b.1 - ay);
    let l = dx * dx + dy * dy;
    let t = if l == 0.0 { 0.0 } else { max(0.0, min(1.0, ((p.0 - ax) * dx + (p.1 - ay) * dy) / l)) };
    dist(p, (ax + t * dx, ay + t * dy))
}

/// Join open polylines whose ends meet. -> (rings, leftovers, joins made).
///
/// A profile is not always one path. Drawn as two curves sharing their endpoints it
/// cuts exactly the same ring, so judging each half on its own reports two open
/// paths and no outline — which is what this said about four of the five bullroarer
/// blades before it could stitch.
fn stitch(paths: Vec<Vec<P>>, tol: f64) -> (Vec<Vec<P>>, Vec<Vec<P>>, usize) {
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

/// Least distance between two polylines. Sampled at the vertices of each.
fn poly_dist(a: &[P], b: &[P]) -> f64 {
    let mut d: Option<f64> = None;
    let mut take = |x: f64| d = Some(match d { None => x, Some(v) => min(v, x) });
    for &p in a {
        for i in 0..b.len().saturating_sub(1) {
            take(seg_dist(p, b[i], b[i + 1]));
        }
    }
    let first_min = d.unwrap_or_else(|| die("min() arg is an empty sequence"));
    let mut e: Option<f64> = None;
    for &p in b {
        for i in 0..a.len().saturating_sub(1) {
            let x = seg_dist(p, a[i], a[i + 1]);
            e = Some(match e { None => x, Some(v) => min(v, x) });
        }
    }
    min(first_min, e.unwrap_or_else(|| die("min() arg is an empty sequence")))
}

// --- the gate ---------------------------------------------------------------

type Row = (String, bool, String);

fn check(r: &Rx, path: &str, bed: &[f64], min_edge: f64, min_hole: f64, weld: f64) -> (Vec<Row>, Vec<String>) {
    let (mut rows, mut notes): (Vec<Row>, Vec<String>) = (Vec::new(), Vec::new());
    let mut row = |name: &str, ok: bool, detail: String| rows.push((name.to_string(), ok, detail));

    let src = match text::read_strict(path) {
        Ok(s) => s,
        Err(e) => {
            row("the file parses as SVG", false, e);
            return (rows, notes);
        }
    };
    let opt = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = match roxmltree::Document::parse_with_options(&src, opt) {
        Ok(d) => d,
        Err(e) => {
            row("the file parses as SVG", false, e.to_string());
            return (rows, notes);
        }
    };
    let root = doc.root_element();

    let (w, h) = (root.attribute("width").unwrap_or(""), root.attribute("height").unwrap_or(""));
    let vb = nums(r, root.attribute("viewBox"));
    let mm = w.trim().ends_with("mm") && h.trim().ends_with("mm");
    let scale = user_scale(r, root);
    row(
        "millimetre-true",
        mm && (scale - 1.0).abs() < 0.002,
        format!("width=\"{w}\" height=\"{h}\"")
            + &if vb.len() != 4 {
                "  no viewBox".to_string()
            } else if mm {
                format!("  1 unit = {scale:.4}mm")
            } else {
                "  size is not in mm, so a user unit means nothing".to_string()
            },
    );

    let geo = collect(r, root);
    if geo.is_empty() {
        row("the file draws something", false, "no stroked geometry".into());
        return (rows, notes);
    }

    let (x0, y0, x1, y1) = bbox(geo.iter().flat_map(|(_, p, _)| p.iter()));
    let (ww, hh) = (x1 - x0, y1 - y0);
    row("fits the bed", ww <= bed[0] && hh <= bed[1], format!("{ww:.1} x {hh:.1}mm on a {:.0} x {:.0} bed", bed[0], bed[1]));

    let cuts: Vec<&Geo> = geo.iter().filter(|(c, _, _)| c != SKIP).collect();
    let inks: Vec<String> = cuts.iter().map(|(c, _, _)| c.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    // Blue ENGRAVES -- the palette starts with it -- and an engraved line is not
    // a cut: it is meant to be open, it frees nothing, and it is not a hole. It
    // stays in the palette check and comes out of the geometry below. The
    // knotwork rosettes draw <g id="cut"> and <g id="engrave">, and before this
    // 25 engraved polylines were reported as open cut paths and 15 as holes
    // outside the outline.
    let cuts: Vec<&Geo> = cuts.into_iter().filter(|(c, _, _)| c != ENGRAVE).collect();
    let strange: Vec<&String> = inks.iter().filter(|c| !ORDER.contains(&c.as_str())).collect();
    row(
        "ink is in the palette",
        strange.is_empty(),
        inks.join(", ")
            + &if strange.is_empty() {
                String::new()
            } else {
                format!("   unknown: {}", strange.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
            },
    );

    // A subpath counts as closed if it says z or if its ends meet: the laser cuts the
    // same loop either way. What is left over is stitched to its own colour, because a
    // profile drawn as two curves meeting at the tips is one ring cut in two goes.
    let shut = |p: &[P], cl: bool| cl || dist(p[0], *p.last().unwrap()) <= CLOSE_TOL;

    let mut closed: Vec<(String, Vec<P>)> =
        cuts.iter().filter(|(_, p, cl)| shut(p, *cl) && p.len() > 3).map(|(c, p, _)| (c.clone(), p.clone())).collect();
    let welded = cuts.iter().filter(|(_, p, cl)| !cl && shut(p, *cl)).count();
    let (mut loose, mut joins): (Vec<(String, Vec<P>)>, usize) = (Vec::new(), 0);
    let colours: BTreeSet<&String> = cuts.iter().map(|(c, _, _)| c).collect();
    for c in colours {
        let segs: Vec<Vec<P>> = cuts.iter().filter(|(cc, p, cl)| cc == c && !shut(p, *cl)).map(|(_, p, _)| p.clone()).collect();
        if segs.is_empty() {
            continue;
        }
        let (rings, rest, n) = stitch(segs, weld);
        joins += n;
        closed.extend(rings.into_iter().map(|q| (c.clone(), q)));
        loose.extend(rest.into_iter().map(|q| (c.clone(), q)));
    }

    let mut made = if joins == 0 { String::new() } else { format!("   ({joins} join(s) stitched)") };
    if welded > 0 {
        made += &format!("   ({welded} closed by coincidence, no z)");
    }
    row(
        "cut paths are closed",
        loose.is_empty(),
        if loose.is_empty() {
            "all closed".to_string()
        } else {
            let l = &loose[0].1;
            format!("{} open, first at {:.1},{:.1}mm, gap {:.3}mm", loose.len(), l[0].0, l[0].1, dist(l[0], *l.last().unwrap()))
        } + &made,
    );
    if closed.is_empty() {
        row("something is cut out", false, "no closed cut path".into());
        return (rows, notes);
    }

    let mut outer_i = 0;
    for i in 1..closed.len() {
        if area(&closed[i].1) > area(&closed[outer_i].1) {
            outer_i = i;
        }
    }
    let (outline_ink, outline) = (&closed[outer_i].0, &closed[outer_i].1);
    let holes: Vec<&(String, Vec<P>)> = closed.iter().enumerate().filter(|(i, _)| *i != outer_i).map(|(_, h)| h).collect();

    row(
        "black frees the part",
        outline_ink == "#000000",
        format!("outline cuts in {outline_ink}")
            + if outline_ink == "#000000" {
                ""
            } else if !loose.is_empty() {
                "  — black is the freeing cut; the open path above may be the real outline"
            } else {
                "  — black is the freeing cut"
            },
    );

    let idx = |c: &str| ORDER.iter().position(|o| *o == c);
    if let Some(oi) = idx(outline_ink) {
        let late: Vec<&String> = holes.iter().map(|(c, _)| c).filter(|c| idx(c).map(|k| k > oi).unwrap_or(false)).collect();
        let names: BTreeSet<&str> = late.iter().map(|s| s.as_str()).collect();
        row(
            "holes are cut before the outline",
            late.is_empty(),
            if late.is_empty() {
                "all earlier".into()
            } else {
                format!("{} after it: {}", late.len(), names.into_iter().collect::<Vec<_>>().join(", "))
            },
        );
    }

    let out_of = holes.iter().filter(|(_, p)| !inside(centroid(p), outline)).count();
    row(
        "holes are inside the outline",
        out_of == 0,
        if out_of == 0 { format!("{} hole(s)", holes.len()) } else { format!("{out_of} outside") },
    );

    let fold_min = |v: &[f64]| v[1..].iter().fold(v[0], |a, &b| min(a, b));
    if !holes.is_empty() {
        let small: Vec<f64> = holes
            .iter()
            .map(|(_, p)| {
                let (a, b, c, d) = bbox(p.iter());
                min(c - a, d - b)
            })
            .collect();
        let s = fold_min(&small);
        row("hole is big enough", s >= min_hole, format!("smallest {s:.2}mm across, floor {min_hole:.2}mm"));

        let mut edges = Vec::new();
        for (k, (_, p)) in holes.iter().enumerate() {
            let mut e = poly_dist(p, outline);
            for (j, (_, q)) in holes.iter().enumerate() {
                if j != k {
                    e = min(e, poly_dist(p, q));
                }
            }
            edges.push(e);
        }
        let e = fold_min(&edges);
        row("edge distance", e >= min_edge, format!("least {e:.2}mm of material, floor {min_edge:.2}mm"));
        notes.push(format!("{} hole(s); least edge {e:.2}mm, smallest hole {s:.2}mm across", holes.len()));
    } else {
        notes.push("no holes".into());
    }

    notes.push(format!("{ww:.1} x {hh:.1}mm, {} cut path(s), ink {}", cuts.len(), inks.join(", ")));
    (rows, notes)
}

const USAGE: &str = "usage: flat-part-check [-h] [--dir DIR] [--bed BED] [--min-edge MIN_EDGE] [--min-hole MIN_HOLE] [--weld WELD] [--quiet] [files ...]";

fn usage_error(msg: &str) -> ! {
    eprintln!("{USAGE}\nflat-part-check: error: {msg}");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let mut bed_s = format!("{BED_W:.0}x{BED_H:.0}");
    let (mut min_edge, mut min_hole, mut weld, mut quiet) = (MIN_EDGE, MIN_HOLE, WELD_TOL, false);
    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        let (key, inline) = match a.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        let mut value = || -> String {
            if let Some(v) = &inline {
                return v.clone();
            }
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| usage_error(&format!("argument {key}: expected one argument")))
        };
        let fl = |k: &str, v: String| py::float(&v).unwrap_or_else(|| usage_error(&format!("argument {k}: invalid float value: {}", py::str_repr(&v))));
        match key.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}\n\nPre-cut gate for flat single-sheet parts.\n\npositional arguments:\n  files\n\noptional arguments:\n  -h, --help           show this help message and exit\n  --dir DIR            every .svg beneath DIR (previews/ and dot-directories skipped)\n  --bed BED            work area in mm, default {BED_W:.0}x{BED_H:.0}\n  --min-edge MIN_EDGE  mm of material a hole must leave, default {}\n  --min-hole MIN_HOLE  mm across a hole must measure, default {}\n  --weld WELD          mm within which two path ends are one corner, default {}\n  --quiet              only the failures", py::repr(MIN_EDGE), py::repr(MIN_HOLE), py::repr(WELD_TOL));
                return;
            }
            "--dir" => dirs.push(value()),
            "--bed" => bed_s = value(),
            "--min-edge" => min_edge = fl("--min-edge", value()),
            "--min-hole" => min_hole = fl("--min-hole", value()),
            "--weld" => weld = fl("--weld", value()),
            "--quiet" => quiet = true,
            s if s.starts_with('-') && s.len() > 1 => usage_error(&format!("unrecognized arguments: {s}")),
            _ => files.push(text::pathstr(&a)),
        }
        i += 1;
    }
    for d in &dirs {
        files.extend(text::rglob_ext(&text::pathstr(d), ".svg").into_iter().filter(|f| {
            let parts = text::parts(f);
            !parts.iter().any(|p| p == "previews") && !parts.iter().any(|p| p.starts_with('.'))
        }));
    }
    if files.is_empty() {
        usage_error("no files given");
    }
    let r = Rx {
        func: rx(r"(\w+)\s*\(([^)]*)\)"),
        sep: rx(r"[\s,]+"),
        tokens: rx(r"[MmZzLlHhVvCcSsQqTtAa]|-?\d*\.?\d+(?:[eE][-+]?\d+)?"),
        number: rx(r"-?\d*\.?\d+(?:[eE][-+]?\d+)?"),
        stroke: rx(r"(?<![-\w])stroke\s*:\s*([^;]+)"),
    };
    let bed: Vec<f64> = split(&rx("[x,]"), &bed_s).into_iter().map(float).collect();
    if bed.len() < 2 {
        die(format!("--bed wants WxH, not {bed_s}"));
    }
    let (mut checks, mut failed, mut bad_files) = (0, 0, 0);
    for f in &files {
        let (rows, notes) = check(&r, f, &bed, min_edge, min_hole, weld);
        let n = rows.iter().filter(|(_, ok, _)| !ok).count();
        checks += rows.len();
        failed += n;
        bad_files += (n > 0) as usize;
        if quiet && n == 0 {
            continue;
        }
        println!("\n{f}");
        for (name, ok, detail) in &rows {
            println!("  {}  {name:<34} {detail}", if *ok { "pass" } else { "FAIL" });
        }
        for line in &notes {
            println!("        {line}");
        }
    }
    println!(
        "\n  {} file(s), {checks} checks, {failed} failed{}",
        files.len(),
        if failed > 0 { format!(", {bad_files} file(s) bad") } else { String::new() }
    );
    std::process::exit(if failed > 0 { 1 } else { 0 });
}
