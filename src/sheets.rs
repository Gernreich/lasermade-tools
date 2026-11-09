//! Cut sheets as parts (2026-11-08), for sc-flatten and sc-compare: a sheet
//! read back as its parts (each black outline with whatever is drawn inside
//! it), a flat body matched to the part it was cut as, and a sheet written
//! back out in the house's way: one group per ink, cut order kept.
use crate::step::{ring_area, Flat, P};
use crate::svg::{self, Rx};
use geo::{Area, BooleanOps, Centroid};


/// One drawn thing: its ink, whether it is a fill engrave rather than a
/// stroke, the stroke's width, and its subpaths in millimetres.
#[derive(Clone, Debug)]
pub struct Item {
    pub ink: String,
    pub fill: bool,
    pub width: String,
    pub pts: Vec<P>,
    pub closed: bool,
    /// the element it was read from, with the matrix of the groups around it
    /// (millimetres included), and how far it has moved since: written back
    /// as it was drawn as long as its shape is the one read
    pub src: Option<(String, svg::M)>,
    pub shift: P,
    /// which element of the file it came from: a number's strokes are one
    /// element, and go where it goes
    pub el: usize,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub outline: Item,
    pub inside: Vec<Item>,
}

#[derive(Clone, Debug)]
pub struct Sheet {
    pub path: String,
    pub w: f64,
    pub h: f64,
    pub title: String,
    pub desc: String,
    /// inks in the order the file draws them
    pub inks: Vec<(String, bool, String)>,
    pub parts: Vec<Part>,
    /// what lies in no part
    pub loose: Vec<Item>,
    /// the file as read, written back untouched while nothing on it changes
    pub text: String,
    pub changed: bool,
}

impl Item {
    /// The same item moved by d.
    pub fn moved(&self, d: P) -> Item {
        Item { pts: self.pts.iter().map(|p| (p.0 + d.0, p.1 + d.1)).collect(), shift: (self.shift.0 + d.0, self.shift.1 + d.1), ..self.clone() }
    }
}

fn attr_inherit(el: roxmltree::Node, k: &str) -> Option<String> {
    let mut n = Some(el);
    while let Some(e) = n {
        if let Some(v) = e.attribute(k) {
            return Some(v.to_string());
        }
        if let Some(st) = e.attribute("style") {
            for d in st.split(';') {
                if let Some((a, b)) = d.split_once(':') {
                    if a.trim() == k {
                        return Some(b.trim().to_string());
                    }
                }
            }
        }
        n = e.parent().filter(|p| p.is_element());
    }
    None
}

pub fn read(path: &str) -> Result<Sheet, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let doc = svg::parse(&src).map_err(|e| format!("{path}: {e}"))?;
    let r = Rx::new();
    let root = doc.root_element();
    let scale = svg::user_scale(&r, root);
    let vb = svg::nums(&r, root.attribute("viewBox"));
    let (w, h) = if vb.len() == 4 { (vb[2] * scale, vb[3] * scale) } else { (0.0, 0.0) };
    let text = |tag: &str| root.children().find(|c| c.tag_name().name() == tag).and_then(|c| c.text()).unwrap_or("").to_string();
    let mut items = vec![];
    let mut inks: Vec<(String, bool, String)> = vec![];
    fn walk(src: &str, r: &Rx, el: roxmltree::Node, m: svg::M, scale: f64, items: &mut Vec<Item>, inks: &mut Vec<(String, bool, String)>) {
        let m = svg::mul(m, svg::parse_transform(r, el.attribute("transform")));
        for kid in el.children().filter(|k| k.is_element()) {
            let tag = kid.tag_name().name();
            if matches!(tag, "g" | "a" | "svg") {
                walk(src, r, kid, m, scale, items, inks);
                continue;
            }
            if matches!(tag, "defs" | "metadata" | "title" | "desc" | "text") {
                continue;
            }
            let stroke = svg::norm(attr_inherit(kid, "stroke").as_deref());
            let fill = svg::norm(attr_inherit(kid, "fill").as_deref());
            let (ink, is_fill) = if !stroke.is_empty() && stroke != "none" { (stroke, false) } else if !fill.is_empty() && fill != "none" { (fill, true) } else { continue };
            let width = attr_inherit(kid, "stroke-width").unwrap_or_else(|| "0.1".into());
            let key = (ink.clone(), is_fill, if is_fill { String::new() } else { width.clone() });
            if !inks.contains(&key) {
                inks.push(key);
            }
            let km = svg::mul(m, svg::parse_transform(r, kid.attribute("transform")));
            let subs = svg::shape_subpaths(r, kid);
            // an element of several subpaths is one item per subpath, each written afresh
            let el = items.last().map_or(0, |i: &Item| i.el + 1);
            let whole = (subs.len() == 1).then(|| (src[kid.range()].to_string(), svg::mul((scale, 0.0, 0.0, scale, 0.0, 0.0), m)));
            for (pts, closed) in subs {
                let mut p: Vec<P> = pts.into_iter().map(|q| svg::apply(km, q)).map(|(x, y)| (x * scale, y * scale)).collect();
                let closed = closed || (p.len() > 2 && crate::py::dist(p[0], *p.last().unwrap()) < 1e-6);
                if closed && p.len() > 1 && crate::py::dist(p[0], *p.last().unwrap()) < 1e-6 {
                    p.pop();
                }
                items.push(Item { ink: ink.clone(), fill: is_fill, width: width.clone(), pts: p, closed, src: whole.clone(), shift: (0.0, 0.0), el });
            }
        }
    }
    walk(&src, &r, root, svg::I, scale, &mut items, &mut inks);
    // the outlines: closed black strokes no other black stroke holds
    // a part is a closed cut no other closed cut holds: black frees most
    // parts, but some sheets cut a part of their own in another ink (the
    // kalimba's optional stiffeners, in orange)
    let black: Vec<usize> = (0..items.len()).filter(|&i| items[i].ink != "#0000ff" && !items[i].fill && items[i].closed && items[i].pts.len() > 2).collect();
    let holds = |a: &Item, b: &Item| b.pts.iter().all(|&q| svg::inside(q, &a.pts)) && ring_area(&a.pts).abs() > ring_area(&b.pts).abs();
    let outer: Vec<usize> = black.iter().copied().filter(|&i| !black.iter().any(|&j| j != i && holds(&items[j], &items[i]))).collect();
    let mut parts: Vec<Part> = outer.iter().map(|&i| Part { outline: items[i].clone(), inside: vec![] }).collect();
    let mut loose = vec![];
    let mut host_of: std::collections::HashMap<usize, Option<usize>> = std::collections::HashMap::new();
    for (i, it) in items.into_iter().enumerate() {
        if outer.contains(&i) {
            continue;
        }
        if let Some(h) = host_of.get(&it.el) {
            match h {
                Some(k) => parts[*k].inside.push(it),
                None => loose.push(it),
            }
            continue;
        }
        // the smallest outline holding its first point
        let at = it.pts[0];
        let host = parts.iter().enumerate().filter(|(_, p)| svg::inside(at, &p.outline.pts)).min_by(|a, b| ring_area(&a.1.outline.pts).abs().partial_cmp(&ring_area(&b.1.outline.pts).abs()).unwrap()).map(|(k, _)| k);
        host_of.insert(it.el, host);
        match host {
            Some(k) => parts[k].inside.push(it),
            None => loose.push(it),
        }
    }
    Ok(Sheet { path: path.to_string(), w, h, title: text("title"), desc: text("desc"), inks, parts, loose, text: src.clone(), changed: false })
}

// --- placing a flat body on a sheet ------------------------------------------------------

/// A rigid move of the plane, mirror included: p -> R p + t.
#[derive(Clone, Copy, Debug)]
pub struct Rigid {
    pub c: f64,
    pub s: f64,
    pub mirror: bool,
    pub t: P,
}

impl Rigid {
    pub fn rot(&self, p: P) -> P {
        let y = if self.mirror { -p.1 } else { p.1 };
        (self.c * p.0 - self.s * y, self.s * p.0 + self.c * y)
    }
    pub fn apply(&self, p: P) -> P {
        let q = self.rot(p);
        (q.0 + self.t.0, q.1 + self.t.1)
    }
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}

/// The nearest point of the rings to p.
pub fn nearest_on_rings(p: P, rings: &[Vec<P>]) -> P {
    let mut best = (f64::MAX, p);
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l2 = dx * dx + dy * dy;
            let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
            let q = (a.0 + t * dx, a.1 + t * dy);
            let d = (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2);
            if d < best.0 {
                best = (d, q);
            }
        }
    }
    best.1
}

/// Settle a move by closest points (ICP): each round pairs every probe
/// point with its nearest on the rings and takes the turn and shift that
/// fit those pairs best, the mirror kept.
pub fn refine(mut r: Rigid, probe: &[P], rings: &[Vec<P>], rounds: usize) -> Rigid {
    for _ in 0..rounds {
        let src: Vec<P> = probe.iter().map(|&p| {
            let y = if r.mirror { -p.1 } else { p.1 };
            (p.0, y)
        }).collect();
        let dst: Vec<P> = probe.iter().map(|&p| nearest_on_rings(r.apply(p), rings)).collect();
        let n = src.len() as f64;
        let (sx, sy) = (src.iter().map(|p| p.0).sum::<f64>() / n, src.iter().map(|p| p.1).sum::<f64>() / n);
        let (dx, dy) = (dst.iter().map(|p| p.0).sum::<f64>() / n, dst.iter().map(|p| p.1).sum::<f64>() / n);
        let (mut a, mut b) = (0.0, 0.0);
        for (s, d) in src.iter().zip(&dst) {
            let (px, py, qx, qy) = (s.0 - sx, s.1 - sy, d.0 - dx, d.1 - dy);
            a += px * qx + py * qy;
            b += px * qy - py * qx;
        }
        let ang = b.atan2(a);
        let (c, si) = (ang.cos(), ang.sin());
        let t = (dx - (c * sx - si * sy), dy - (si * sx + c * sy));
        let moved = (t.0 - r.t.0).hypot(t.1 - r.t.1) + (c - r.c).abs() + (si - r.s).abs();
        r = Rigid { c, s: si, mirror: r.mirror, t };
        if moved < 1e-9 {
            break;
        }
    }
    r
}

/// The distance from p to the nearest of the rings.
pub fn to_rings(p: P, rings: &[Vec<P>]) -> f64 {
    let mut best = f64::MAX;
    for r in rings {
        let n = r.len();
        for i in 0..n {
            best = best.min(seg_dist(p, r[i], r[(i + 1) % n]));
        }
    }
    best
}

/// Every ring of a region: each polygon's outline, then its holes.
pub fn rings(m: &geo::MultiPolygon<f64>) -> Vec<Vec<P>> {
    let mut out = vec![];
    for p in &m.0 {
        for l in std::iter::once(p.exterior()).chain(p.interiors()) {
            let mut v: Vec<P> = l.coords().map(|c| (c.x, c.y)).collect();
            if v.len() > 1 && v[0] == *v.last().unwrap() {
                v.pop();
            }
            out.push(v);
        }
    }
    out
}

/// A part's cut rings on the sheet: its outline and every closed cut inside it.
pub fn part_rings(p: &Part) -> Vec<Vec<P>> {
    std::iter::once(p.outline.pts.clone()).chain(p.inside.iter().filter(|i| i.closed && !i.fill && i.ink != "#0000ff").map(|i| i.pts.clone())).collect()
}

fn centroid(m: &geo::MultiPolygon<f64>) -> P {
    let c = m.centroid().unwrap();
    (c.x(), c.y())
}

fn ring_centroid(p: &[P]) -> P {
    let m = geo::MultiPolygon::new(vec![geo::Polygon::new(geo::LineString::from(p.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![])]);
    centroid(&m)
}

/// A ring's principal axis, from its area moments (exact for the polygon,
/// whatever its points' spacing); None when it has no one long way (a
/// square, a circle).
fn axis(r: &[P]) -> Option<f64> {
    let n = r.len();
    let (mut a, mut cx, mut cy, mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (r[i], r[(i + 1) % n]);
        let k = p.0 * q.1 - q.0 * p.1;
        a += k;
        cx += (p.0 + q.0) * k;
        cy += (p.1 + q.1) * k;
        xx += (p.0 * p.0 + p.0 * q.0 + q.0 * q.0) * k;
        yy += (p.1 * p.1 + p.1 * q.1 + q.1 * q.1) * k;
        xy += (p.0 * q.1 + 2.0 * p.0 * p.1 + 2.0 * q.0 * q.1 + q.0 * p.1) * k;
    }
    if a.abs() < 1e-12 {
        return None;
    }
    // a clockwise ring sums every term negative: the same moments, signed
    if a < 0.0 {
        (a, cx, cy, xx, yy, xy) = (-a, -cx, -cy, -xx, -yy, -xy);
    }
    let a = a / 2.0;
    let (cx, cy) = (cx / (6.0 * a), cy / (6.0 * a));
    // second moments about the centroid
    let ixx = xx / 12.0 - a * cx * cx;
    let iyy = yy / 12.0 - a * cy * cy;
    let ixy = xy / 24.0 - a * cx * cy;
    if (ixx - iyy).abs() < 1e-6 * (ixx + iyy).abs() && ixy.abs() < 1e-6 * (ixx + iyy).abs() {
        return None;
    }
    Some(0.5 * (2.0 * ixy).atan2(ixx - iyy))
}

/// Thin a ring's points to at most n, evenly.
fn sample(r: &[Vec<P>], n: usize) -> Vec<P> {
    let all: Vec<P> = r.iter().flatten().copied().collect();
    if all.len() <= n {
        return all;
    }
    (0..n).map(|i| all[i * all.len() / n]).collect()
}

/// How well a flat body sits on a sheet part, and the move that puts it
/// there: (mean distance, worst distance, move). Turns are tried by lining
/// up long edges; the move then lays centroid on centroid.
pub fn register(f: &Flat, p: &Part, only: Option<bool>) -> Option<(f64, f64, Rigid)> {
    register_all(f, p, only).into_iter().next()
}

/// Every distinct way a flat body sits on a sheet part, best first: a
/// symmetric outline (a rhombus) sits as well turned half round, and only
/// what is drawn inside it tells the ways apart.
pub fn register_all(f: &Flat, p: &Part, only: Option<bool>) -> Vec<(f64, f64, Rigid)> {
    let fr = rings(&f.region);
    let pr = part_rings(p);
    if fr.len() != pr.len() {
        return vec![];
    }
    let (fa, pa) = (f.region.unsigned_area(), ring_area(&p.outline.pts).abs() - pr[1..].iter().map(|r| ring_area(r).abs()).sum::<f64>());
    if (fa - pa).abs() > 0.05 * fa + 2.0 {
        return vec![];
    }
    let edges = |r: &[P]| -> Vec<(f64, f64)> {
        let n = r.len();
        let mut e: Vec<(f64, f64)> = (0..n).map(|i| {
            let (a, b) = (r[i], r[(i + 1) % n]);
            ((b.0 - a.0).hypot(b.1 - a.1), (b.1 - a.1).atan2(b.0 - a.0))
        }).collect();
        e.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        e
    };
    let fe = edges(&fr[0]);
    let pe = edges(&p.outline.pts);
    let fc = ring_centroid(&fr[0]); // the outline's, as the part's is: holes would pull it off
    let pc = ring_centroid(&p.outline.pts);
    let probe = sample(&fr, 160);
    let mut found: Vec<(f64, f64, Rigid)> = vec![];
    let mut tried: Vec<(bool, f64)> = vec![];
    // the turns to try: long edges lined up, and the principal axes lined up,
    // for an outline that is all curve (the bullroarers) and has no long
    // edge to line up, or whose curves the solid and the sheet cut into
    // different lengths
    let mut pairs: Vec<(f64, f64)> = vec![];
    for &(fl, fang) in fe.iter().take(3) {
        for &(pl, pang) in pe.iter().take(12) {
            if (fl - pl).abs() <= 1.0 {
                pairs.push((fang, pang));
            }
        }
    }
    if let (Some(fa), Some(pa)) = (axis(&fr[0]), axis(&p.outline.pts)) {
        pairs.push((fa, pa));
    }
    for &(fang, pang) in &pairs {
        {
            for mirror in [false, true] {
                if only.is_some_and(|m| m != mirror) {
                    continue;
                }
                for flip in [0.0, std::f64::consts::PI] {
                    let a = if mirror { pang + fang + flip } else { pang - fang + flip };
                    let a = a.rem_euclid(std::f64::consts::TAU);
                    if tried.iter().any(|&(m, b)| m == mirror && ((a - b).abs() < 1e-4 || (std::f64::consts::TAU - (a - b).abs()) < 1e-4)) {
                        continue;
                    }
                    tried.push((mirror, a));
                    let mut r = Rigid { c: a.cos(), s: a.sin(), mirror, t: (0.0, 0.0) };
                    let q = r.rot(fc);
                    r.t = (pc.0 - q.0, pc.1 - q.1);
                    let mut sum = 0.0;
                    let mut worst: f64 = 0.0;
                    for &x in &probe {
                        let d = to_rings(r.apply(x), &pr);
                        sum += d;
                        worst = worst.max(d);
                        if worst > 4.0 {
                            break;
                        }
                    }
                    let dbg = std::env::var("SC_REG_DEBUG").is_ok();
                    if worst > 4.0 {
                        if dbg {
                            eprintln!("  turn {:.1} deg{}: a probe point {worst:.2} mm off, dropped", a.to_degrees(), if mirror { " mirrored" } else { "" });
                        }
                        continue;
                    }
                    let mean = sum / probe.len() as f64;
                    if mean > 2.0 {
                        if dbg {
                            eprintln!("  turn {:.1} deg{}: mean {mean:.2} mm, dropped", a.to_degrees(), if mirror { " mirrored" } else { "" });
                        }
                        continue;
                    }
                    // settle it, then measure again
                    let r = refine(r, &probe, &pr, 30);
                    let ds: Vec<f64> = probe.iter().map(|&x| to_rings(r.apply(x), &pr)).collect();
                    let mean = ds.iter().sum::<f64>() / ds.len() as f64;
                    let worst = ds.iter().cloned().fold(0.0, f64::max);
                    // the same way found from another pair of edges
                    if !found.iter().any(|(_, _, q)| q.mirror == r.mirror && (q.c - r.c).abs() < 1e-6 && (q.s - r.s).abs() < 1e-6 && (q.t.0 - r.t.0).abs() < 1e-4 && (q.t.1 - r.t.1).abs() < 1e-4) {
                        found.push((mean, worst, r));
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    found
}

// --- the kerf ---------------------------------------------------------------------------

/// The region grown by `burn` (outlines out, holes in), corners mitred.
/// Round, the outside corners are arcs, as Boxes.py's burn draws them;
/// else they are mitred.
pub fn grow(m: &geo::MultiPolygon<f64>, burn: f64, round: bool) -> geo::MultiPolygon<f64> {
    use geo::algorithm::buffer::{Buffer, BufferStyle, LineJoin};
    if burn.abs() < 1e-9 {
        return m.clone();
    }
    let join = if round { LineJoin::Round(0.005) } else { LineJoin::Miter(2.0 * (1.0f64 / 20.0).asin()) };
    m.buffer_with_style(BufferStyle::new(burn).line_join(join))
}

/// The kerf a sheet says it is drawn for ("drawn for a 0.13mm kerf",
/// "carry a 0.14mm kerf"), if it says.
pub fn stated_kerf(desc: &str) -> Option<f64> {
    let re = fancy_regex::Regex::new(r"([0-9]*\.?[0-9]+)\s*mm kerf").unwrap();
    re.captures(desc).ok().flatten().and_then(|c| c[1].parse().ok())
}

/// How much the sheet's cut lines stand off the body's, set where the
/// registration puts it: the burn. The median over every corner of the
/// body's rings, so the few places a design and its sheet differ (fingers
/// overlapping at a corner, say) do not move it.
pub fn burn_of(f: &Flat, p: &Part, r: Rigid) -> f64 {
    let pr = part_rings(p);
    let mut d: Vec<f64> = rings(&f.region).iter().flatten().map(|&q| to_rings(r.apply(q), &pr)).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    d[d.len() / 2]
}

pub fn overlap(a: &[P], b: &[P]) -> f64 {
    let pa = geo::Polygon::new(geo::LineString::from(a.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![]);
    let pb = geo::Polygon::new(geo::LineString::from(b.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![]);
    pa.intersection(&pb).unsigned_area()
}

// --- writing ----------------------------------------------------------------------------

pub fn num(v: f64) -> String {
    let s = format!("{:.3}", if v.abs() < 5e-4 { 0.0 } else { v });
    s
}

pub fn d_of(pts: &[P], closed: bool) -> String {
    let mut o = String::new();
    for (i, p) in pts.iter().enumerate() {
        o.push_str(&format!("{}{},{} ", if i == 0 { "M " } else { "L " }, num(p.0), num(p.1)));
    }
    if closed {
        o.push('Z');
    }
    o.trim_end().to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A sheet's text: one group per ink, in the order the original drew them.
pub fn write(s: &Sheet) -> String {
    if !s.changed {
        return s.text.clone();
    }
    let mut o = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n<title>{}</title>\n<desc>{}</desc>\n",
        esc(&s.title),
        esc(&s.desc),
        w = num(s.w),
        h = num(s.h)
    );
    let all: Vec<&Item> = s.parts.iter().flat_map(|p| std::iter::once(&p.outline).chain(p.inside.iter())).chain(s.loose.iter()).collect();
    for (ink, fill, width) in &s.inks {
        let mine: Vec<&&Item> = all.iter().filter(|i| &i.ink == ink && i.fill == *fill && (*fill || &i.width == width)).collect();
        if mine.is_empty() {
            continue;
        }
        if *fill {
            o.push_str(&format!("  <g fill=\"{ink}\" stroke=\"none\">\n"));
        } else {
            o.push_str(&format!("  <g fill=\"none\" stroke=\"{ink}\" stroke-width=\"{width}\">\n"));
        }
        let mut done: Vec<*const u8> = vec![];
        for it in mine {
            match &it.src {
                Some((el, m)) => {
                    // one element may be several items; write it once
                    if done.contains(&el.as_ptr()) {
                        continue;
                    }
                    done.push(el.as_ptr());
                    let m = crate::svg::mul((1.0, 0.0, 0.0, 1.0, it.shift.0, it.shift.1), *m);
                    if m == (1.0, 0.0, 0.0, 1.0, 0.0, 0.0) {
                        o.push_str(&format!("    {el}\n"));
                    } else {
                        o.push_str(&format!("    <g transform=\"matrix({} {} {} {} {} {})\">{el}</g>\n", m.0, m.1, m.2, m.3, num(m.4), num(m.5)));
                    }
                }
                None => o.push_str(&format!("    <path d=\"{}\"/>\n", d_of(&it.pts, it.closed))),
            }
        }
        o.push_str("  </g>\n");
    }
    o.push_str("</svg>\n");
    o
}

/// A sheet part as a flat body, to register one sheet's parts on another's.
pub fn part_flat(p: &Part, name: &str) -> Flat {
    Flat {
        name: name.to_string(),
        u: [1.0, 0.0, 0.0],
        v: [0.0, 1.0, 0.0],
        n: [0.0, 0.0, 1.0],
        thickness: 0.0,
        region: crate::step::face_region(&part_rings(p)),
        pockets: vec![],
    }
}

/// How many items of each ink a part carries, inks in order.
pub fn ink_counts(p: &Part) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = vec![];
    for it in std::iter::once(&p.outline).chain(p.inside.iter()) {
        let k = format!("{}{}", it.ink, if it.fill { " fill" } else { "" });
        match v.iter_mut().find(|(a, _)| *a == k) {
            Some(e) => e.1 += 1,
            None => v.push((k, 1)),
        }
    }
    v.sort();
    v
}
