//! Reading a STEP file back (2026-11-08): the solid bodies SolidCraft writes,
//! each as its faces and each face's boundary loops, every edge as a
//! polyline. Enough of ISO 10303-21 for the B-reps SolidCraft and step-export
//! write: lines, circles and (rational) B-spline edges on planes, cylinders
//! and whatever else, each sampled finely enough for a cut file.
use std::collections::HashMap;

pub type V3 = [f64; 3];

/// One value in an entity's argument list.
#[derive(Clone, Debug)]
pub enum Val {
    Ref(usize),
    Num(f64),
    Str(String),
    Enum(String),
    List(Vec<Val>),
    /// a typed value or one part of a complex entity: NAME(args)
    Typed(String, Vec<Val>),
    Null,
}

impl Val {
    fn r(&self) -> Option<usize> {
        if let Val::Ref(i) = self {
            Some(*i)
        } else {
            None
        }
    }
    fn n(&self) -> Option<f64> {
        if let Val::Num(x) = self {
            Some(*x)
        } else {
            None
        }
    }
    fn list(&self) -> &[Val] {
        if let Val::List(v) = self {
            v
        } else {
            &[]
        }
    }
}

/// An entity: its type and arguments, or, for a complex entity, its parts.
#[derive(Clone, Debug)]
pub struct Ent {
    pub kind: String,
    pub args: Vec<Val>,
    pub parts: Vec<(String, Vec<Val>)>,
}

impl Ent {
    fn part(&self, name: &str) -> Option<&[Val]> {
        if self.kind == name {
            return Some(&self.args);
        }
        self.parts.iter().find(|(n, _)| n == name).map(|(_, a)| a.as_slice())
    }
}

struct Lex<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lex<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn val(&mut self) -> Result<Val, String> {
        self.ws();
        let c = *self.s.get(self.i).ok_or("unexpected end")? as char;
        match c {
            '#' => {
                self.i += 1;
                let st = self.i;
                while self.i < self.s.len() && (self.s[self.i] as char).is_ascii_digit() {
                    self.i += 1;
                }
                Ok(Val::Ref(std::str::from_utf8(&self.s[st..self.i]).unwrap().parse().map_err(|_| "bad ref")?))
            }
            '\'' => {
                self.i += 1;
                let mut out = String::new();
                loop {
                    let c = *self.s.get(self.i).ok_or("unterminated string")? as char;
                    self.i += 1;
                    if c == '\'' {
                        if self.s.get(self.i) == Some(&b'\'') {
                            out.push('\'');
                            self.i += 1;
                            continue;
                        }
                        break;
                    }
                    out.push(c);
                }
                Ok(Val::Str(out))
            }
            '.' => {
                self.i += 1;
                let st = self.i;
                while self.i < self.s.len() && self.s[self.i] != b'.' {
                    self.i += 1;
                }
                let e = std::str::from_utf8(&self.s[st..self.i]).unwrap().to_string();
                self.i += 1;
                Ok(Val::Enum(e))
            }
            '(' => {
                self.i += 1;
                Ok(Val::List(self.list_rest()?))
            }
            '$' | '*' => {
                self.i += 1;
                Ok(Val::Null)
            }
            c if c == '-' || c == '+' || c.is_ascii_digit() => {
                let st = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i] as char, '0'..='9' | '.' | 'E' | 'e' | '-' | '+') {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[st..self.i]).unwrap();
                Ok(Val::Num(t.parse().map_err(|_| format!("bad number {t}"))?))
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let name = self.ident();
                self.ws();
                if self.s.get(self.i) == Some(&b'(') {
                    self.i += 1;
                    Ok(Val::Typed(name, self.list_rest()?))
                } else {
                    Ok(Val::Enum(name))
                }
            }
            c => Err(format!("unexpected '{c}'")),
        }
    }
    fn ident(&mut self) -> String {
        let st = self.i;
        while self.i < self.s.len() && ((self.s[self.i] as char).is_ascii_alphanumeric() || self.s[self.i] == b'_') {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[st..self.i]).unwrap().to_string()
    }
    /// the rest of a list whose '(' is read
    fn list_rest(&mut self) -> Result<Vec<Val>, String> {
        let mut out = vec![];
        loop {
            self.ws();
            match self.s.get(self.i) {
                Some(b')') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b',') => {
                    self.i += 1;
                }
                None => return Err("unterminated list".into()),
                _ => out.push(self.val()?),
            }
        }
    }
}

/// Every entity of the DATA section, by its number.
pub fn parse(text: &str) -> Result<HashMap<usize, Ent>, String> {
    let data = text.find("DATA;").ok_or("no DATA section")? + 5;
    let end = text[data..].find("ENDSEC;").map_or(text.len(), |e| data + e);
    let body = &text[data..end];
    let mut out = HashMap::new();
    // statements end at ';' outside strings
    let (mut st, mut in_str) = (0usize, false);
    let b = body.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'\'' {
            in_str = !in_str;
        }
        if c == b';' && !in_str {
            let stmt = body[st..i].trim();
            st = i + 1;
            let Some(eq) = stmt.find('=') else { continue };
            let id: usize = match stmt[..eq].trim().trim_start_matches('#').parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            let mut lx = Lex { s: stmt[eq + 1..].as_bytes(), i: 0 };
            lx.ws();
            let ent = if lx.s.get(lx.i) == Some(&b'(') {
                lx.i += 1;
                let parts = lx.list_rest()?.into_iter().filter_map(|v| if let Val::Typed(n, a) = v { Some((n, a)) } else { None }).collect();
                Ent { kind: String::new(), args: vec![], parts }
            } else {
                let kind = lx.ident();
                lx.ws();
                lx.i += 1; // '('
                Ent { kind, args: lx.list_rest()?, parts: vec![] }
            };
            out.insert(id, ent);
        }
    }
    Ok(out)
}

/// A face: its surface, if a plane its point and normal, and its loops.
#[derive(Clone, Debug)]
pub struct Face {
    pub plane: Option<(V3, V3)>,
    pub loops: Vec<Vec<V3>>,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub name: String,
    pub faces: Vec<Face>,
}

struct Rd<'a> {
    e: &'a HashMap<usize, Ent>,
    tol: f64,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}
fn d3(a: V3, b: V3) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

impl Rd<'_> {
    fn ent(&self, i: usize) -> Result<&Ent, String> {
        self.e.get(&i).ok_or_else(|| format!("#{i} is missing"))
    }
    fn pt(&self, i: usize) -> Result<V3, String> {
        let e = self.ent(i)?;
        let a = e.part("CARTESIAN_POINT").ok_or_else(|| format!("#{i} is not a point"))?;
        let c: Vec<f64> = a[1].list().iter().filter_map(|v| v.n()).collect();
        Ok([c[0], c[1], *c.get(2).unwrap_or(&0.0)])
    }
    fn dir(&self, i: usize) -> Result<V3, String> {
        let e = self.ent(i)?;
        let a = e.part("DIRECTION").ok_or_else(|| format!("#{i} is not a direction"))?;
        let c: Vec<f64> = a[1].list().iter().filter_map(|v| v.n()).collect();
        Ok(unit([c[0], c[1], *c.get(2).unwrap_or(&0.0)]))
    }
    fn vertex(&self, i: usize) -> Result<V3, String> {
        let a = self.ent(i)?.part("VERTEX_POINT").ok_or("not a vertex")?;
        self.pt(a[1].r().ok_or("vertex without point")?)
    }
    /// (origin, z axis, x axis) of an AXIS2_PLACEMENT_3D
    fn placement(&self, i: usize) -> Result<(V3, V3, V3), String> {
        let a = self.ent(i)?.part("AXIS2_PLACEMENT_3D").ok_or("not a placement")?;
        let o = self.pt(a[1].r().ok_or("placement without origin")?)?;
        let z = a[2].r().map_or(Ok([0.0, 0.0, 1.0]), |r| self.dir(r))?;
        let mut x = a[3].r().map_or(Ok([1.0, 0.0, 0.0]), |r| self.dir(r))?;
        // make x square to z
        let k = dot(x, z);
        x = unit(sub(x, [z[0] * k, z[1] * k, z[2] * k]));
        Ok((o, z, x))
    }
    /// The curve from a to b, as points from a to b.
    fn curve(&self, i: usize, a: V3, b: V3) -> Result<Vec<V3>, String> {
        let e = self.ent(i)?;
        if e.part("LINE").is_some() {
            return Ok(vec![a, b]);
        }
        let mut pts = if let Some(c) = e.part("CIRCLE") {
            let (o, z, x) = self.placement(c[1].r().ok_or("circle without placement")?)?;
            let r = c[2].n().ok_or("circle without radius")?;
            let y = cross(z, x);
            let ang = |p: V3| {
                let d = sub(p, o);
                dot(d, y).atan2(dot(d, x))
            };
            let (t0, mut t1) = (ang(a), ang(b));
            // anticlockwise about z from a to b; a whole circle when they meet
            while t1 <= t0 + 1e-12 {
                t1 += std::f64::consts::TAU;
            }
            let n = arc_steps(r, t1 - t0, self.tol);
            let mut v: Vec<V3> = (0..=n)
                .map(|k| {
                    let t = t0 + (t1 - t0) * k as f64 / n as f64;
                    let (c, s) = (t.cos(), t.sin());
                    [o[0] + r * (c * x[0] + s * y[0]), o[1] + r * (c * x[1] + s * y[1]), o[2] + r * (c * x[2] + s * y[2])]
                })
                .collect();
            // the edge may run clockwise: then it is the other way round
            if d3(v[n / 2], a) < 1e-9 {
                v.reverse();
            }
            v
        } else if e.part("B_SPLINE_CURVE").is_some() || e.part("B_SPLINE_CURVE_WITH_KNOTS").is_some() {
            self.nurbs(e)?
        } else if let Some(t) = e.part("TRIMMED_CURVE") {
            return self.curve(t[1].r().ok_or("trimmed without basis")?, a, b);
        } else {
            return Err(format!("#{i}: a {} edge, which this reader does not follow", if e.kind.is_empty() { "complex" } else { &e.kind }));
        };
        // run it from a: a closed curve may start anywhere on it
        if d3(pts[0], a) > d3(*pts.last().unwrap(), a) {
            pts.reverse();
        }
        let n = pts.len();
        pts[0] = a;
        pts[n - 1] = b;
        Ok(pts)
    }
    fn nurbs(&self, e: &Ent) -> Result<Vec<V3>, String> {
        // the simple form carries everything in one list; the complex one in parts
        let (deg, cps, mults, knots, weights) = if !e.kind.is_empty() {
            let a = &e.args;
            let deg = a[1].n().ok_or("degree")? as usize;
            let cps: Result<Vec<V3>, String> = a[2].list().iter().map(|v| self.pt(v.r().ok_or("control point")?)).collect();
            let mults: Vec<usize> = a[6].list().iter().filter_map(|v| v.n()).map(|x| x as usize).collect();
            let knots: Vec<f64> = a[7].list().iter().filter_map(|v| v.n()).collect();
            (deg, cps?, mults, knots, None)
        } else {
            let b = e.part("B_SPLINE_CURVE").ok_or("no B_SPLINE_CURVE")?;
            let k = e.part("B_SPLINE_CURVE_WITH_KNOTS").ok_or("no knots")?;
            let deg = b[0].n().ok_or("degree")? as usize;
            let cps: Result<Vec<V3>, String> = b[1].list().iter().map(|v| self.pt(v.r().ok_or("control point")?)).collect();
            let mults: Vec<usize> = k[0].list().iter().filter_map(|v| v.n()).map(|x| x as usize).collect();
            let knots: Vec<f64> = k[1].list().iter().filter_map(|v| v.n()).collect();
            let w = e.part("RATIONAL_B_SPLINE_CURVE").map(|r| r[0].list().iter().filter_map(|v| v.n()).collect::<Vec<f64>>());
            (deg, cps?, mults, knots, w)
        };
        let mut kv = vec![];
        for (m, k) in mults.iter().zip(&knots) {
            kv.extend(std::iter::repeat(*k).take(*m));
        }
        let w = weights.unwrap_or_else(|| vec![1.0; cps.len()]);
        let (t0, t1) = (kv[deg], kv[kv.len() - deg - 1]);
        let spans = knots.len().max(2) - 1;
        let n = 48 * spans;
        Ok((0..=n).map(|i| de_boor(deg, &kv, &cps, &w, t0 + (t1 - t0) * i as f64 / n as f64)).collect())
    }
    fn face(&self, i: usize) -> Result<Face, String> {
        let a = self.ent(i)?.part("ADVANCED_FACE").or_else(|| self.e[&i].part("FACE_SURFACE")).ok_or_else(|| format!("#{i} is not a face"))?;
        let surf = self.ent(a[2].r().ok_or("face without surface")?)?;
        let plane = match surf.part("PLANE") {
            Some(p) => {
                let (o, z, _) = self.placement(p[1].r().ok_or("plane without placement")?)?;
                Some((o, z))
            }
            None => None,
        };
        let mut loops = vec![];
        for bnd in a[1].list() {
            let fb = self.ent(bnd.r().ok_or("bound")?)?;
            let fa = fb.part("FACE_BOUND").or_else(|| fb.part("FACE_OUTER_BOUND")).ok_or("not a bound")?;
            let lp = self.ent(fa[1].r().ok_or("bound without loop")?)?;
            let Some(la) = lp.part("EDGE_LOOP") else { continue }; // a vertex loop: nothing to cut
            let mut pts: Vec<V3> = vec![];
            for oe in la[1].list() {
                let o = self.ent(oe.r().ok_or("edge")?)?.part("ORIENTED_EDGE").ok_or("not an oriented edge")?.to_vec();
                let fwd = !matches!(&o[4], Val::Enum(s) if s == "F");
                let ec = self.ent(o[3].r().ok_or("oriented edge without edge")?)?.part("EDGE_CURVE").ok_or("not an edge curve")?.to_vec();
                let (va, vb) = (self.vertex(ec[1].r().ok_or("v")?)?, self.vertex(ec[2].r().ok_or("v")?)?);
                let mut seg = self.curve(ec[3].r().ok_or("curve")?, va, vb)?;
                if !fwd {
                    seg.reverse();
                }
                if pts.is_empty() {
                    pts.extend(seg);
                } else {
                    pts.extend(seg.into_iter().skip(1));
                }
            }
            if pts.len() > 1 && d3(pts[0], *pts.last().unwrap()) < 1e-6 {
                pts.pop();
            }
            loops.push(pts);
        }
        Ok(Face { plane, loops })
    }
}

/// Steps for an arc of radius r through angle a, each chord within tol.
fn arc_steps(r: f64, a: f64, tol: f64) -> usize {
    let step = if r <= tol { 1.0 } else { 2.0 * (1.0 - tol / r).acos() };
    ((a / step).ceil() as usize).max(2)
}

fn de_boor(p: usize, kv: &[f64], cps: &[V3], w: &[f64], t: f64) -> V3 {
    let n = cps.len();
    let mut k = p;
    while k + 1 < n && t >= kv[k + 1] {
        k += 1;
    }
    let mut d: Vec<[f64; 4]> = (0..=p).map(|j| {
        let c = cps[j + k - p];
        let wj = w[j + k - p];
        [c[0] * wj, c[1] * wj, c[2] * wj, wj]
    }).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let den = kv[i + p + 1 - r] - kv[i];
            let al = if den.abs() < 1e-15 { 0.0 } else { (t - kv[i]) / den };
            for c in 0..4 {
                d[j][c] = (1.0 - al) * d[j - 1][c] + al * d[j][c];
            }
        }
    }
    let q = d[p];
    [q[0] / q[3], q[1] / q[3], q[2] / q[3]]
}

/// The middle of every body's corners together.
pub fn centre(bs: &[Body]) -> V3 {
    let v: Vec<V3> = bs.iter().flat_map(|b| b.faces.iter().flat_map(|f| f.loops.iter().flatten().copied())).collect();
    let k = v.len().max(1) as f64;
    [v.iter().map(|p| p[0]).sum::<f64>() / k, v.iter().map(|p| p[1]).sum::<f64>() / k, v.iter().map(|p| p[2]).sum::<f64>() / k]
}

/// Every named solid in the file, with its faces.
pub fn bodies(text: &str, tol: f64) -> Result<Vec<Body>, String> {
    let e = parse(text)?;
    let rd = Rd { e: &e, tol };
    let mut ids: Vec<&usize> = e.keys().collect();
    ids.sort();
    let mut out = vec![];
    for &id in ids {
        let Some(a) = e[&id].part("MANIFOLD_SOLID_BREP") else { continue };
        let name = if let Val::Str(s) = &a[0] { s.clone() } else { String::new() };
        let shell = rd.ent(a[1].r().ok_or("brep without shell")?)?;
        let sa = shell.part("CLOSED_SHELL").ok_or("not a closed shell")?;
        let faces: Result<Vec<Face>, String> = sa[1].list().iter().map(|f| rd.face(f.r().ok_or("face ref")?)).collect();
        let faces = faces.map_err(|er| format!("{name}: {er}"))?;
        out.push(Body { name, faces });
    }
    Ok(out)
}

// --- a body as a flat part ------------------------------------------------------------

pub type P = (f64, f64);

/// A body as the laser sees it: lying on its face, the outline and holes it
/// cuts through, and the pockets it is engraved with.
#[derive(Clone, Debug)]
pub struct Flat {
    pub name: String,
    /// the frame: x and y in the face, the normal, and the world origin's depth
    pub u: V3,
    pub v: V3,
    pub n: V3,
    pub thickness: f64,
    /// the through-cut region: rings, the first of each polygon its outline
    pub region: geo::MultiPolygon<f64>,
    /// pockets: (depth from the face they open on, which face: 0 the low, 1 the high, their region)
    pub pockets: Vec<(f64, u8, geo::MultiPolygon<f64>)>,
}

fn ring(p: &[P]) -> geo::LineString<f64> {
    let mut v: Vec<geo::Coord<f64>> = p.iter().map(|&(x, y)| geo::Coord { x, y }).collect();
    if let (Some(a), Some(b)) = (v.first().copied(), v.last().copied()) {
        if a != b {
            v.push(a);
        }
    }
    geo::LineString::from(v)
}

pub fn ring_area(p: &[P]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1).sum::<f64>() / 2.0
}

/// A face's loops as one region: the largest loop its outline, the rest holes.
pub fn face_region(loops: &[Vec<P>]) -> geo::MultiPolygon<f64> {
    use geo::BooleanOps;
    if loops.is_empty() {
        return geo::MultiPolygon::new(vec![]);
    }
    let big = (0..loops.len()).max_by(|&a, &b| ring_area(&loops[a]).abs().partial_cmp(&ring_area(&loops[b]).abs()).unwrap()).unwrap();
    let mut r = geo::MultiPolygon::new(vec![geo::Polygon::new(ring(&loops[big]), vec![])]);
    for (i, l) in loops.iter().enumerate() {
        if i != big {
            r = r.difference(&geo::MultiPolygon::new(vec![geo::Polygon::new(ring(l), vec![])]));
        }
    }
    r
}

/// Lay a body down: find the direction it is thin in, and take its faces
/// square to that as the laser sees them. With `centre` (the whole
/// instrument's), the normal points away from it, so every plate is seen
/// from outside and a plate and its mirror twin read as mirror images.
pub fn flatten(b: &Body, centre: Option<V3>) -> Result<Flat, String> {
    use geo::BooleanOps;
    let verts: Vec<V3> = b.faces.iter().flat_map(|f| f.loops.iter().flatten().copied()).collect();
    let mut normals: Vec<V3> = vec![];
    for f in &b.faces {
        if let Some((_, z)) = f.plane {
            let z = if z[0] < -1e-9 || (z[0].abs() < 1e-9 && (z[1] < -1e-9 || (z[1].abs() < 1e-9 && z[2] < 0.0))) { [-z[0], -z[1], -z[2]] } else { z };
            if !normals.iter().any(|m| d3(*m, z) < 1e-7) {
                normals.push(z);
            }
        }
    }
    let extent = |n: V3| {
        let d: Vec<f64> = verts.iter().map(|&p| dot(p, n)).collect();
        (d.iter().cloned().fold(f64::MAX, f64::min), d.iter().cloned().fold(f64::MIN, f64::max))
    };
    let n = *normals.iter().min_by(|a, b| {
        let (ea, eb) = (extent(**a), extent(**b));
        (ea.1 - ea.0).partial_cmp(&(eb.1 - eb.0)).unwrap()
    }).ok_or_else(|| format!("{}: no flat face", b.name))?;
    let n = match centre {
        Some(c) => {
            let k = verts.len() as f64;
            let m = [verts.iter().map(|p| p[0]).sum::<f64>() / k, verts.iter().map(|p| p[1]).sum::<f64>() / k, verts.iter().map(|p| p[2]).sum::<f64>() / k];
            if dot(n, sub(m, c)) < 0.0 { [-n[0], -n[1], -n[2]] } else { n }
        }
        None => n,
    };
    let (lo, hi) = extent(n);
    // a frame tied to the world, so the same plate edited lies the same way
    let wx = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let k = dot(wx, n);
    let u = unit(sub(wx, [n[0] * k, n[1] * k, n[2] * k]));
    let v = cross(n, u);
    let flat = |p: V3| (dot(p, u), dot(p, v));
    let mut low = geo::MultiPolygon::new(vec![]);
    let mut high = geo::MultiPolygon::new(vec![]);
    let mut pockets = vec![];
    for f in &b.faces {
        let Some((o, z)) = f.plane else { continue };
        if (dot(z, n).abs() - 1.0).abs() > 1e-7 {
            continue;
        }
        let d = dot(o, n);
        let loops: Vec<Vec<P>> = f.loops.iter().map(|l| l.iter().map(|&p| flat(p)).collect()).collect();
        let r = face_region(&loops);
        if (d - lo).abs() < 1e-6 {
            low = low.union(&r);
        } else if (d - hi).abs() < 1e-6 {
            high = high.union(&r);
        } else {
            let (depth, side) = if d - lo < hi - d { (d - lo, 0) } else { (hi - d, 1) };
            pockets.push((depth, side, r));
        }
    }
    let region = low.union(&high);
    Ok(Flat { name: b.name.clone(), u, v, n, thickness: hi - lo, region, pockets })
}
