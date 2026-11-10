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
    /// the part as the laser leaves it, when a hole touches its rim or
    /// another hole: a ribbon cheek's mortice drawn as a closed slot on a
    /// straight rim cuts a notch, and the solid has the notch
    pub cut: Option<Vec<Vec<P>>>,
}

#[derive(Clone, Debug)]
pub struct Sheet {
    pub path: String,
    /// the viewBox's corner (a rosette's file centres it on the origin) and size
    pub x0: f64,
    pub y0: f64,
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
    let (x0, y0) = if vb.len() == 4 { (vb[0] * scale, vb[1] * scale) } else { (0.0, 0.0) };
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
    // a point on a's line counts as inside it: a narrow cheek's mortices are
    // drawn with their outer side on its rim
    let on_line = |q: P, r: &[P]| (0..r.len()).any(|i| seg_dist(q, r[i], r[(i + 1) % r.len()]) < 0.01);
    // (closed cuts only: an engraved stroke ending on a line stays outside)
    let holds = |a: &Item, b: &Item| b.pts.iter().all(|&q| svg::inside(q, &a.pts) || (b.closed && on_line(q, &a.pts))) && ring_area(&a.pts).abs() > ring_area(&b.pts).abs();
    let outer: Vec<usize> = black.iter().copied().filter(|&i| !black.iter().any(|&j| j != i && holds(&items[j], &items[i]))).collect();
    let mut parts: Vec<Part> = outer.iter().map(|&i| Part { outline: items[i].clone(), inside: vec![], cut: None }).collect();
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
        // the smallest outline holding its first point, or, for a closed cut
        // whose first point lies on the rim, its middle; an open stroke goes
        // by the middle of its path (a rosette's ribbon edge starts on a
        // waste piece's cut line and runs out over the board, and its first
        // point falls either side of that line by a ten-thousandth of a mm)
        let smallest = |at: P| parts.iter().enumerate().filter(|(_, p)| svg::inside(at, &p.outline.pts)).min_by(|a, b| ring_area(&a.1.outline.pts).abs().partial_cmp(&ring_area(&b.1.outline.pts).abs()).unwrap()).map(|(k, _)| k);
        let n = it.pts.len();
        let first = if it.closed || n < 2 { it.pts[0] } else { let (a, b) = (it.pts[(n - 1) / 2], it.pts[n / 2]); ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0) };
        let mut host = smallest(first);
        if host.is_none() && it.closed && it.pts.len() > 2 {
            let n = it.pts.len() as f64;
            host = smallest((it.pts.iter().map(|q| q.0).sum::<f64>() / n, it.pts.iter().map(|q| q.1).sum::<f64>() / n));
        }
        host_of.insert(it.el, host);
        match host {
            Some(k) => parts[k].inside.push(it),
            None => loose.push(it),
        }
    }
    for p in parts.iter_mut() {
        p.cut = as_cut(&drawn_rings(p));
    }
    Ok(Sheet { path: path.to_string(), x0, y0, w, h, title: text("title"), desc: text("desc"), inks, parts, loose, text: src.clone(), changed: false })
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

/// The rings' segments filed in a square grid, so that a search for the
/// nearest point looks only at the cells round it, square by square outward,
/// and stops once no cell further out can hold anything nearer. It answers
/// exactly as nearest_on_rings() and to_rings() do, distance for distance,
/// a tie going to the segment they would take (the first, rings in order);
/// on rings of thousands of segments it is hundreds of times faster
/// (2026-11-09).
pub struct RingIndex {
    segs: Vec<(P, P)>,
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl RingIndex {
    pub fn new(rings: &[Vec<P>]) -> RingIndex {
        let mut segs = vec![];
        for r in rings {
            let n = r.len();
            for i in 0..n {
                segs.push((r[i], r[(i + 1) % n]));
            }
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (a, b) in &segs {
            for q in [a, b] {
                x0 = x0.min(q.0);
                y0 = y0.min(q.1);
                x1 = x1.max(q.0);
                y1 = y1.max(q.1);
            }
        }
        if segs.is_empty() || !(x1 - x0).is_finite() || !(y1 - y0).is_finite() {
            return RingIndex { segs, x0: 0.0, y0: 0.0, cell: 1.0, nx: 1, ny: 1, cells: vec![vec![]] };
        }
        let k = ((segs.len() as f64).sqrt().ceil() as usize).clamp(1, 512);
        let cell = ((x1 - x0).max(y1 - y0) / k as f64).max(1e-6);
        let nx = (((x1 - x0) / cell).floor() as usize + 1).min(4096);
        let ny = (((y1 - y0) / cell).floor() as usize + 1).min(4096);
        let mut ix = RingIndex { segs: vec![], x0, y0, cell, nx, ny, cells: vec![vec![]; nx * ny] };
        for (i, (a, b)) in segs.iter().enumerate() {
            let (i0, i1) = (ix.col(a.0.min(b.0)), ix.col(a.0.max(b.0)));
            let (j0, j1) = (ix.row(a.1.min(b.1)), ix.row(a.1.max(b.1)));
            for j in j0..=j1 {
                for c in i0..=i1 {
                    ix.cells[j * nx + c].push(i as u32);
                }
            }
        }
        ix.segs = segs;
        ix
    }

    fn col(&self, x: f64) -> usize {
        (((x - self.x0) / self.cell).floor().max(0.0) as usize).min(self.nx - 1)
    }

    fn row(&self, y: f64) -> usize {
        (((y - self.y0) / self.cell).floor().max(0.0) as usize).min(self.ny - 1)
    }

    /// Hand `visit` every segment near p, square of cells by square, until
    /// none left unvisited can come closer than `visit`'s best so far (it
    /// returns that distance). A segment filed in several cells may be
    /// visited more than once.
    fn search(&self, p: P, mut visit: impl FnMut(usize, P, P) -> f64) {
        let (cx, cy) = (self.col(p.0) as isize, self.row(p.1) as isize);
        let (nx, ny) = (self.nx as isize, self.ny as isize);
        let mut best = f64::MAX;
        let mut k = 0isize;
        loop {
            let mut cell = |i: isize, j: isize| {
                if i >= 0 && i < nx && j >= 0 && j < ny {
                    for &s in &self.cells[(j * nx + i) as usize] {
                        let (a, b) = self.segs[s as usize];
                        best = best.min(visit(s as usize, a, b));
                    }
                }
            };
            if k == 0 {
                cell(cx, cy);
            } else {
                for i in cx - k..=cx + k {
                    cell(i, cy - k);
                    cell(i, cy + k);
                }
                for j in cy - k + 1..cy + k {
                    cell(cx - k, j);
                    cell(cx + k, j);
                }
            }
            let (lo_i, hi_i, lo_j, hi_j) = (cx - k, cx + k, cy - k, cy + k);
            if lo_i <= 0 && hi_i >= nx - 1 && lo_j <= 0 && hi_j >= ny - 1 {
                return;
            }
            // how near an unvisited cell can be
            let mut bound = f64::MAX;
            if lo_i > 0 {
                bound = bound.min(p.0 - (self.x0 + lo_i as f64 * self.cell));
            }
            if hi_i < nx - 1 {
                bound = bound.min(self.x0 + (hi_i + 1) as f64 * self.cell - p.0);
            }
            if lo_j > 0 {
                bound = bound.min(p.1 - (self.y0 + lo_j as f64 * self.cell));
            }
            if hi_j < ny - 1 {
                bound = bound.min(self.y0 + (hi_j + 1) as f64 * self.cell - p.1);
            }
            if best < bound - 1e-9 {
                return;
            }
            k += 1;
        }
    }

    /// As nearest_on_rings(p, rings).
    pub fn nearest(&self, p: P) -> P {
        let mut best = (f64::MAX, p, usize::MAX);
        self.search(p, |i, a, b| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l2 = dx * dx + dy * dy;
            let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
            let q = (a.0 + t * dx, a.1 + t * dy);
            let d = (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2);
            if d < best.0 || (d == best.0 && i < best.2) {
                best = (d, q, i);
            }
            best.0.sqrt()
        });
        best.1
    }

    /// As to_rings(p, rings).
    pub fn dist(&self, p: P) -> f64 {
        let mut best = f64::MAX;
        self.search(p, |_, a, b| {
            best = best.min(seg_dist(p, a, b));
            best
        });
        best
    }
}

/// Settle a move by closest points (ICP): each round pairs every probe
/// point with its nearest on the rings and takes the turn and shift that
/// fit those pairs best, the mirror kept.
pub fn refine(r: Rigid, probe: &[P], rings: &[Vec<P>], rounds: usize) -> Rigid {
    refine_in(r, probe, &RingIndex::new(rings), rounds)
}

/// refine, on rings already indexed
pub fn refine_in(mut r: Rigid, probe: &[P], rings: &RingIndex, rounds: usize) -> Rigid {
    for _ in 0..rounds {
        let src: Vec<P> = probe.iter().map(|&p| {
            let y = if r.mirror { -p.1 } else { p.1 };
            (p.0, y)
        }).collect();
        let dst: Vec<P> = probe.iter().map(|&p| rings.nearest(r.apply(p))).collect();
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

/// A part's rings as drawn: its outline and every closed cut inside it.
pub fn drawn_rings(p: &Part) -> Vec<Vec<P>> {
    std::iter::once(p.outline.pts.clone()).chain(p.inside.iter().filter(|i| i.closed && !i.fill && i.ink != "#0000ff").map(|i| i.pts.clone())).collect()
}

/// A part's cut rings: as drawn, or as cut where its holes touch.
pub fn part_rings(p: &Part) -> Vec<Vec<P>> {
    p.cut.clone().unwrap_or_else(|| drawn_rings(p))
}

/// Which holes (rings after the first) touch another ring: lie on it, as a
/// narrow cheek's mortice lies on its rim.
pub fn touching(rings: &[Vec<P>]) -> Vec<bool> {
    let on = |a: &[P], b: &[P]| {
        let n = b.len();
        a.iter().any(|&q| (0..n).any(|i| seg_dist(q, b[i], b[(i + 1) % n]) < 0.01))
    };
    (0..rings.len()).map(|i| i > 0 && (0..rings.len()).any(|j| j != i && on(&rings[i], &rings[j]))).collect()
}

/// The rings as the laser leaves the part, the outline less every hole,
/// when a hole touches another ring; None when none does.
pub fn as_cut(rings: &[Vec<P>]) -> Option<Vec<Vec<P>>> {
    let t = touching(rings);
    if rings.len() < 2 || !t.iter().any(|&t| t) {
        return None;
    }
    // a hole drawn on the rim can stop a ten-thousandth short of it, which
    // the subtraction keeps as a hair of material: swell each touching hole
    // by 0.005 mm, so it breaks through
    let poly = |r: &[P]| geo::MultiPolygon::new(vec![geo::Polygon::new(geo::LineString::from(r.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![])]);
    let big = (0..rings.len()).max_by(|&a, &b| ring_area(&rings[a]).abs().partial_cmp(&ring_area(&rings[b]).abs()).unwrap()).unwrap();
    let mut m = poly(&rings[big]);
    for (i, r) in rings.iter().enumerate() {
        if i != big {
            let h = if t[i] { grow(&poly(r), 0.005, false) } else { poly(r) };
            m = m.difference(&h);
        }
    }
    m.0.sort_by(|a, b| b.unsigned_area().partial_cmp(&a.unsigned_area()).unwrap());
    Some(self::rings(&m))
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
/// About n points of the rings, each ring its share by vertices but never
/// fewer than eight: a round plate's outline drawn in 800 points would
/// otherwise leave its one square hole, the only thing that says which way
/// it turns, without a point
fn sample(r: &[Vec<P>], n: usize) -> Vec<P> {
    let total: usize = r.iter().map(|x| x.len()).sum();
    if total <= n {
        return r.iter().flatten().copied().collect();
    }
    let mut out = vec![];
    for x in r {
        let k = (n * x.len() / total).max(8).min(x.len());
        out.extend((0..k).map(|i| x[i * x.len() / k]));
    }
    out
}

/// How well a flat body sits on a sheet part, and the move that puts it
/// there: (mean distance, worst distance, move). Turns are tried by lining
/// up long edges; the move then lays centroid on centroid.
///
/// Two ways within 0.005 mm of each other on the mean are one fit and its
/// near-twin (a scallop's cheek mirrors onto itself but for a few slivers
/// at its mortices): the one whose worst point sits closer is the one.
pub fn register(f: &Flat, p: &Part, only: Option<bool>) -> Option<(f64, f64, Rigid)> {
    let ways = register_all(f, p, only);
    let best = ways.first()?.0;
    ways.into_iter().filter(|w| w.0 < best + 0.005).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
}

/// Every distinct way a flat body sits on a sheet part, best first: a
/// symmetric outline (a rhombus) sits as well turned half round, and only
/// what is drawn inside it tells the ways apart.
pub fn register_all(f: &Flat, p: &Part, only: Option<bool>) -> Vec<(f64, f64, Rigid)> {
    let fr = rings(&f.region);
    let pr = part_rings(p);
    let pri = RingIndex::new(&pr);
    if fr.len() != pr.len() {
        return vec![];
    }
    let (fa, pa) = (f.region.unsigned_area(), ring_area(&pr[0]).abs() - pr[1..].iter().map(|r| ring_area(r).abs()).sum::<f64>());
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
    let pe = edges(&pr[0]);
    let fc = ring_centroid(&fr[0]); // the outline's, as the part's is: holes would pull it off
    let pc = ring_centroid(&pr[0]);
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
    if let (Some(fa), Some(pa)) = (axis(&fr[0]), axis(&pr[0])) {
        pairs.push((fa, pa));
    }
    // and an off-centre hole's offset lined up with its fellow's: a round
    // plate (the uilleann tenon) has no edge or axis to go by, and only its
    // pins and its lobed bore say which way it turns
    let off = |r: &[P], c: P| {
        let h = ring_centroid(r);
        ((h.0 - c.0).hypot(h.1 - c.1), (h.1 - c.1).atan2(h.0 - c.0), ring_area(r).abs())
    };
    let mut fh: Vec<(f64, f64, f64)> = fr[1..].iter().map(|r| off(r, fc)).filter(|h| h.0 > 1.0).collect();
    fh.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    for &(fd, fang, farea) in fh.iter().take(2) {
        for (pd, pang, parea) in pr[1..].iter().map(|r| off(r, pc)) {
            if (fd - pd).abs() < 1.0 && (farea - parea).abs() < 0.2 * farea + 1.0 {
                pairs.push((fang, pang));
            }
        }
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
                        let d = pri.dist(r.apply(x));
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
                    let r = refine_in(r, &probe, &pri, 30);
                    let ds: Vec<f64> = probe.iter().map(|&x| pri.dist(r.apply(x))).collect();
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

/// Points every `step` mm along each ring, so a long straight side counts
/// for as much as an arc of the same length sampled finely.
pub fn along(rings: &[Vec<P>], step: f64) -> Vec<P> {
    let mut out = vec![];
    for r in rings {
        let n = r.len();
        let mut carry = 0.0;
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            let l = (b.0 - a.0).hypot(b.1 - a.1);
            let mut d = carry;
            while d < l {
                out.push((a.0 + (b.0 - a.0) * d / l, a.1 + (b.1 - a.1) * d / l));
                d += step;
            }
            carry = d - l;
        }
    }
    out
}

/// A move settled on two outlines drawn alike (the same kerf on both): the
/// ICP refine on points evenly along `src`, both ways round, as long as it
/// takes a near-round part to find its turn; kept only if it fits better.
/// register_all's own refine lays an as-cut body on a drawn part, half a
/// kerf apart, and an outline that is an arc one side and straight the
/// other (a chanter's foot piece) comes out 0.07 mm over.
pub fn settle(r: Rigid, src: &[Vec<P>], dst: &[Vec<P>]) -> Rigid {
    settle_n(r, src, dst, 100)
}

/// settle, in so many ICP rounds
pub fn settle_n(r: Rigid, src: &[Vec<P>], dst: &[Vec<P>], rounds: usize) -> Rigid {
    let probe = along(src, 0.5);
    if probe.is_empty() {
        return r;
    }
    let back = along(dst, 0.5);
    let di = RingIndex::new(dst);
    let fit = |r: &Rigid| {
        let moved: Vec<Vec<P>> = src.iter().map(|x| x.iter().map(|&p| r.apply(p)).collect()).collect();
        let mi = RingIndex::new(&moved);
        let a = probe.iter().map(|&p| di.dist(r.apply(p))).sum::<f64>() / probe.len() as f64;
        let b = back.iter().map(|&p| mi.dist(p)).sum::<f64>() / back.len().max(1) as f64;
        a + b
    };
    let s = refine_in(r, &probe, &di, rounds);
    let s = if fit(&s) < fit(&r) { s } else { r };
    // then point to line: a round plate with a small polygon hole (the ghb
    // chanter's collar socket, a 72-gon round a hexagon) turns freely under
    // point-to-point rounds, its circle and the hexagon's sides sliding, and
    // stopped 0.95 degrees out with the hexagon's corners 0.05 mm off
    let q = polish(s, &probe, &di, 20);
    if fit(&q) < fit(&s) { q } else { s }
}

/// Point-to-line ICP rounds: each probe point's distance to its nearest
/// point on the rings, measured along the line between them, is made least
/// for a small turn and shift, linearised, and the turn re-normalised. It
/// settles a turn that the corners alone hold in a few rounds, where
/// point-to-point rounds creep.
pub fn polish(mut r: Rigid, probe: &[P], rings: &RingIndex, rounds: usize) -> Rigid {
    for _ in 0..rounds {
        // the normal equations of sum((p + th*perp(p - c) + t - q).n)^2
        let moved: Vec<P> = probe.iter().map(|&p| r.apply(p)).collect();
        let n = moved.len() as f64;
        let c = (moved.iter().map(|p| p.0).sum::<f64>() / n, moved.iter().map(|p| p.1).sum::<f64>() / n);
        let mut m = [[0.0f64; 3]; 3];
        let mut v = [0.0f64; 3];
        for &p in &moved {
            let q = rings.nearest(p);
            let (ex, ey) = (p.0 - q.0, p.1 - q.1);
            let l = ex.hypot(ey);
            if l < 1e-9 {
                continue;
            }
            let nn = (ex / l, ey / l);
            // a turn th about c moves p by th*(-(p.1 - c.1), p.0 - c.0)
            let j = [-(p.1 - c.1) * nn.0 + (p.0 - c.0) * nn.1, nn.0, nn.1];
            for a in 0..3 {
                for b in 0..3 {
                    m[a][b] += j[a] * j[b];
                }
                v[a] -= j[a] * l;
            }
        }
        let Some(x) = solve3(m, v) else { break };
        let (th, tx, ty) = (x[0], x[1], x[2]);
        if !(th.is_finite() && tx.is_finite() && ty.is_finite()) {
            break;
        }
        // compose: turn by th about c, then shift
        let (ct, st) = (th.cos(), th.sin());
        let rot = |p: P| (c.0 + ct * (p.0 - c.0) - st * (p.1 - c.1) + tx, c.1 + st * (p.0 - c.0) + ct * (p.1 - c.1) + ty);
        let (nc, ns) = (ct * r.c - st * r.s, st * r.c + ct * r.s);
        let o = rot(r.t);
        let next = Rigid { c: nc, s: ns, mirror: r.mirror, t: o };
        let step = th.abs() + tx.hypot(ty);
        r = next;
        if step < 1e-10 {
            break;
        }
    }
    r
}

/// The 3 x 3 system m x = v, or None when it is singular.
fn solve3(m: [[f64; 3]; 3], v: [f64; 3]) -> Option<[f64; 3]> {
    let det = |a: [[f64; 3]; 3]| a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    let d = det(m);
    if d.abs() < 1e-12 {
        return None;
    }
    let mut out = [0.0; 3];
    for k in 0..3 {
        let mut a = m;
        for i in 0..3 {
            a[i][k] = v[i];
        }
        out[k] = det(a) / d;
    }
    Some(out)
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
    let pri = RingIndex::new(&pr);
    let mut d: Vec<f64> = rings(&f.region).iter().flatten().map(|&q| pri.dist(r.apply(q))).collect();
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
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"{x0} {y0} {w} {h}\">\n<title>{}</title>\n<desc>{}</desc>\n",
        esc(&s.title),
        esc(&s.desc),
        x0 = num(s.x0),
        y0 = num(s.y0),
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

#[cfg(test)]
mod ring_index_test {
    use super::*;

    /// RingIndex against the plain searches, on rings of every density and
    /// points near, on and far from them: the same answers, bit for bit.
    #[test]
    fn same_as_brute_force() {
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..60 {
            let mut rings = vec![];
            for _ in 0..1 + case % 4 {
                let (cx, cy, r) = (rnd() * 200.0 - 100.0, rnd() * 100.0, 1.0 + rnd() * 60.0);
                let n = 3 + (rnd() * [4.0, 40.0, 400.0, 3000.0][case % 4]) as usize;
                // a wobbly circle, some points repeated, some on a grid
                let ring: Vec<P> = (0..n)
                    .map(|i| {
                        let a = i as f64 / n as f64 * std::f64::consts::TAU;
                        let w = if case % 3 == 0 { 1.0 } else { 1.0 + 0.3 * rnd() };
                        let p = (cx + r * w * a.cos(), cy + r * w * a.sin());
                        if case % 5 == 0 { ((p.0 * 4.0).round() / 4.0, (p.1 * 4.0).round() / 4.0) } else { p }
                    })
                    .collect();
                rings.push(ring);
            }
            let ix = RingIndex::new(&rings);
            for k in 0..400 {
                let spread = [1.0, 50.0, 300.0, 5000.0][k % 4];
                let p = if k % 7 == 0 { rings[0][k % rings[0].len()] } else { (rnd() * 2.0 * spread - spread, rnd() * 2.0 * spread - spread) };
                let (a, b) = (nearest_on_rings(p, &rings), ix.nearest(p));
                assert!(a.0.to_bits() == b.0.to_bits() && a.1.to_bits() == b.1.to_bits(), "case {case} point {p:?}: {a:?} vs {b:?}");
                assert_eq!(to_rings(p, &rings).to_bits(), ix.dist(p).to_bits(), "case {case} point {p:?}");
            }
        }
        let empty = RingIndex::new(&[]);
        assert_eq!(empty.nearest((1.0, 2.0)), nearest_on_rings((1.0, 2.0), &[]));
        assert_eq!(empty.dist((1.0, 2.0)), to_rings((1.0, 2.0), &[]));
    }
}
