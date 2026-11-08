//! A 3D page's parts as STEP solids, for a CAD program (SolidCraft, Fusion,
//! FreeCAD, ...).
//!
//!     target/release/step-export PAGE [--out=DIR] [--name=STEM]
//!
//! PAGE is a page (or the viewer.js it loads) carrying the model as one line
//! of JSON, `const DATA = {...};` or `const D = {...};`. Five kinds are read:
//!
//!   turn3d     DATA: designs of parts {o, h, e, m, w}: outline and holes as
//!              point lists, cut through a 3 mm sheet, placed by m
//!   flute      DATA, as turn3d, but each hole is {p: points} or {c: [x, y, r]}
//!              and each part has a name (square-flute's viewer.js and the two
//!              that reuse it)
//!   lyre       D with rim, hole, knot, panels: the lyre-harp page's two
//!              cheeks, its wall panels and its two laminated blocks
//!   rings      D with V, Q and rings: a stack of rings drawn as quads, one
//!              solid per ply of each ring (the trumpet bell and mouthpiece)
//!   dicebox    D with model: hex-dice-box's plates
//!
//! Each design becomes DIR/<stem>-<design>.step, or DIR/<stem>.step for a page
//! of one (the stem is the page's name unless --name gives one), with one solid body per part, assembled as the page shows it with
//! nothing exploded. DIR defaults to step/ beside the page. Curves are the
//! page's straight runs, so a round hole comes in many-sided (a {c} hole is
//! drawn with 64 sides). What the page draws without depth -- engraving, the
//! flue marks, magnet indents -- and the hardware it shows (pins, magnets) is
//! left out. Units are millimetres. Every solid's faces wind outward: each
//! is checked by its signed volume and turned inside out if it came out
//! negative.
use lasermade_tools::die;
use lasermade_tools::turn3d::J;
use std::collections::HashMap;
use std::fmt::Write as _;

type V = [f64; 3];
type P2 = [f64; 2];

/// A solid: faces, each a list of loops (outer first), each loop a ring of
/// points anticlockwise about the face's outward normal (holes clockwise).
struct Solid {
    name: String,
    faces: Vec<Vec<Vec<V>>>,
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V) -> V {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Twice the signed area of a ring, y up: positive is anticlockwise.
fn area2(r: &[P2]) -> f64 {
    (0..r.len())
        .map(|i| {
            let (p, q) = (r[i], r[(i + 1) % r.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

/// A ring without its closing repeat or doubled points.
fn clean(r: Vec<P2>) -> Vec<P2> {
    let close = |a: P2, b: P2| (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-6;
    let mut out: Vec<P2> = vec![];
    for p in r {
        if out.last().map_or(true, |&q| !close(q, p)) {
            out.push(p);
        }
    }
    while out.len() > 1 && close(out[0], out[out.len() - 1]) {
        out.pop();
    }
    out
}

/// A flat part: outline and holes through a sheet `t` thick from z0, placed
/// by the 3x4 matrix m.
fn plate(name: String, o: Vec<P2>, h: Vec<Vec<P2>>, t: f64, z0: f64, m: &[Vec<f64>]) -> Option<Solid> {
    let mut o = clean(o);
    if o.len() < 3 {
        return None;
    }
    if area2(&o) < 0.0 {
        o.reverse();
    }
    let rings: Vec<Vec<P2>> = std::iter::once(o)
        .chain(h.into_iter().map(clean).filter(|r| r.len() >= 3).map(|mut r| {
            if area2(&r) > 0.0 {
                r.reverse();
            }
            r
        }))
        .collect();
    let at = |p: P2, z: f64| -> V { [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * z + m[i][3]) };
    let (zb, zt) = (z0, z0 + t);
    let mut faces = vec![
        rings.iter().map(|r| r.iter().map(|&p| at(p, zt)).collect()).collect(),
        rings.iter().map(|r| r.iter().rev().map(|&p| at(p, zb)).collect()).collect(),
    ];
    for r in &rings {
        for i in 0..r.len() {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            faces.push(vec![vec![at(a, zb), at(b, zb), at(b, zt), at(a, zt)]]);
        }
    }
    Some(Solid { name, faces })
}

/// Six times the signed volume enclosed by the faces.
fn volume6(s: &Solid) -> f64 {
    let mut v = 0.0;
    for f in &s.faces {
        for l in f {
            for i in 1..l.len().saturating_sub(1) {
                v += dot(l[0], cross(l[i], l[i + 1]));
            }
        }
    }
    v
}

/// The STEP file being written: numbered entities, with points, directions
/// and vertices shared.
struct Step {
    body: String,
    n: usize,
    dirs: HashMap<String, usize>,
}

fn f(x: f64) -> String {
    let s = format!("{:.4}", if x.abs() < 5e-5 { 0.0 } else { x });
    let s = s.trim_end_matches('0');
    if s.ends_with('.') { format!("{s}0") } else { s.to_string() }
}

impl Step {
    fn add(&mut self, e: String) -> usize {
        self.n += 1;
        writeln!(self.body, "#{}={};", self.n, e).unwrap();
        self.n
    }
    fn point(&mut self, p: V) -> usize {
        self.add(format!("CARTESIAN_POINT('',({},{},{}))", f(p[0]), f(p[1]), f(p[2])))
    }
    fn dir(&mut self, d: V) -> usize {
        let k = format!("({},{},{})", f(d[0]), f(d[1]), f(d[2]));
        if let Some(&i) = self.dirs.get(&k) {
            return i;
        }
        let i = self.add(format!("DIRECTION('',{k})"));
        self.dirs.insert(k, i);
        i
    }

    /// One solid as a MANIFOLD_SOLID_BREP; returns its entity number.
    fn solid(&mut self, s: &Solid) -> usize {
        type K = (i64, i64, i64);
        let key = |p: V| -> K { ((p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64, (p[2] * 1e4).round() as i64) };
        let mut verts: HashMap<K, (usize, usize)> = HashMap::new(); // key -> (point, vertex)
        let mut edges: HashMap<(K, K), usize> = HashMap::new();
        let mut face_ids = vec![];
        for fc in &s.faces {
            // the plane's normal by Newell, over every loop
            let mut nrm = [0.0; 3];
            for l in fc {
                for i in 0..l.len() {
                    let (a, b) = (l[i], l[(i + 1) % l.len()]);
                    nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
                    nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
                    nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
                }
            }
            let nrm = norm(nrm);
            let l0 = &fc[0];
            let xd = l0.iter().skip(1).map(|&p| sub(p, l0[0])).find(|d| dot(*d, *d) > 1e-12).unwrap_or([1.0, 0.0, 0.0]);
            let xd = norm(cross(cross(nrm, xd), nrm));
            let mut bounds = vec![];
            for (li, l) in fc.iter().enumerate() {
                let mut oes = vec![];
                for i in 0..l.len() {
                    let (pa, pb) = (l[i], l[(i + 1) % l.len()]);
                    let (a, b) = (key(pa), key(pb));
                    if a == b {
                        continue;
                    }
                    for (k, p) in [(a, pa), (b, pb)] {
                        if !verts.contains_key(&k) {
                            let cp = self.point(p);
                            let vp = self.add(format!("VERTEX_POINT('',#{cp})"));
                            verts.insert(k, (cp, vp));
                        }
                    }
                    let (e, same) = if let Some(&e) = edges.get(&(b, a)) {
                        (e, false)
                    } else if let Some(&e) = edges.get(&(a, b)) {
                        (e, true)
                    } else {
                        let d = self.dir(norm(sub(pb, pa)));
                        let vec = self.add(format!("VECTOR('',#{d},1.0)"));
                        let line = self.add(format!("LINE('',#{},#{vec})", verts[&a].0));
                        let e = self.add(format!("EDGE_CURVE('',#{},#{},#{line},.T.)", verts[&a].1, verts[&b].1));
                        edges.insert((a, b), e);
                        (e, true)
                    };
                    oes.push(self.add(format!("ORIENTED_EDGE('',*,*,#{e},{})", if same { ".T." } else { ".F." })));
                }
                let list = oes.iter().map(|x| format!("#{x}")).collect::<Vec<_>>().join(",");
                let lp = self.add(format!("EDGE_LOOP('',({list}))"));
                let kind = if li == 0 { "FACE_OUTER_BOUND" } else { "FACE_BOUND" };
                bounds.push(self.add(format!("{kind}('',#{lp},.T.)")));
            }
            let org = verts[&key(l0[0])].0;
            let zd = self.dir(nrm);
            let xid = self.dir(xd);
            let ax = self.add(format!("AXIS2_PLACEMENT_3D('',#{org},#{zd},#{xid})"));
            let plane = self.add(format!("PLANE('',#{ax})"));
            let list = bounds.iter().map(|x| format!("#{x}")).collect::<Vec<_>>().join(",");
            face_ids.push(self.add(format!("ADVANCED_FACE('',({list}),#{plane},.T.)")));
        }
        let list = face_ids.iter().map(|x| format!("#{x}")).collect::<Vec<_>>().join(",");
        let shell = self.add(format!("CLOSED_SHELL('',({list}))"));
        self.add(format!("MANIFOLD_SOLID_BREP('{}',#{shell})", esc(&s.name)))
    }
}

fn esc(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).collect::<String>().replace('\'', "''")
}

fn write_step(path: &str, title: &str, solids: Vec<Solid>) -> usize {
    let mut st = Step { body: String::new(), n: 0, dirs: HashMap::new() };
    let mut ids = vec![];
    for mut s in solids {
        if volume6(&s) < 0.0 {
            for fc in s.faces.iter_mut() {
                for l in fc.iter_mut() {
                    l.reverse();
                }
            }
        }
        ids.push(st.solid(&s));
    }
    let t = esc(title);
    // the product and its units around the solids (AP214)
    let app = st.add("APPLICATION_CONTEXT('automotive_design')".into());
    st.add(format!("APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app})"));
    let pc = st.add(format!("PRODUCT_CONTEXT('',#{app},'mechanical')"));
    let prod = st.add(format!("PRODUCT('{t}','{t}','',(#{pc}))"));
    let pdf = st.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{prod})"));
    let pdc = st.add(format!("PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"));
    let pd = st.add(format!("PRODUCT_DEFINITION('design','',#{pdf},#{pdc})"));
    let pds = st.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));
    let mm = st.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))".into());
    let rad = st.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))".into());
    let sr = st.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())".into());
    let unc = st.add(format!("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#{mm},'distance_accuracy_value','')"));
    let ctx = st.add(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc}))GLOBAL_UNIT_ASSIGNED_CONTEXT((#{mm},#{rad},#{sr}))REPRESENTATION_CONTEXT('',''))"
    ));
    let o = st.point([0.0; 3]);
    let z = st.dir([0.0, 0.0, 1.0]);
    let x = st.dir([1.0, 0.0, 0.0]);
    let ax = st.add(format!("AXIS2_PLACEMENT_3D('',#{o},#{z},#{x})"));
    let items = std::iter::once(ax).chain(ids.iter().copied()).map(|x| format!("#{x}")).collect::<Vec<_>>().join(",");
    let rep = st.add(format!("ADVANCED_BREP_SHAPE_REPRESENTATION('{t}',({items}),#{ctx})"));
    st.add(format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));
    let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string();
    // no time stamp in FILE_NAME, so the same page always gives the same file
    let out = format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('{t}'),'2;1');\nFILE_NAME('{}','',(''),(''),'lasermade-tools step-export','','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 1 1 1 1 }}'));\nENDSEC;\nDATA;\n{}ENDSEC;\nEND-ISO-10303-21;\n",
        esc(&name),
        st.body
    );
    std::fs::write(path, out).unwrap_or_else(|e| die(format!("{path}: {e}")));
    ids.len()
}

// --- reading the pages --------------------------------------------------------

fn num(j: &J) -> f64 {
    j.num().f()
}
fn p2s(j: &J) -> Vec<P2> {
    j.arr().iter().map(|q| [num(&q.arr()[0]), num(&q.arr()[1])]).collect()
}
fn mat(j: &J) -> Vec<Vec<f64>> {
    j.arr().iter().map(|r| r.arr().iter().map(num).collect()).collect()
}
fn arr<'a>(j: &'a J, k: &str) -> &'a [J] {
    j.get(k).map(J::arr).unwrap_or(&[])
}
const IDENT: [[f64; 4]; 3] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
fn ident() -> Vec<Vec<f64>> {
    IDENT.iter().map(|r| r.to_vec()).collect()
}

/// turn3d and flute pages: designs of parts.
fn designs(data: &J) -> Vec<(String, Vec<Solid>)> {
    let J::O(ds) = data else { die("DATA is not an object") };
    ds.iter()
        .map(|(name, d)| {
            let solids = arr(d, "parts")
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let h = arr(p, "h")
                        .iter()
                        .map(|h| match h {
                            J::O(_) => match h.get("c") {
                                Some(c) => {
                                    let c: Vec<f64> = c.arr().iter().map(num).collect();
                                    (0..64)
                                        .map(|k| {
                                            let a = k as f64 * std::f64::consts::TAU / 64.0;
                                            [c[0] + c[2] * a.cos(), c[1] + c[2] * a.sin()]
                                        })
                                        .collect()
                                }
                                None => p2s(h.get("p").unwrap_or_else(|| die("a hole without p or c"))),
                            },
                            _ => p2s(h),
                        })
                        .collect();
                    let label = match p.get("name") {
                        Some(n) => format!("{} {}", i + 1, n.str()),
                        None => format!("part {} ({})", i + 1, p.get("w").map(J::str).unwrap_or("ply")),
                    };
                    plate(label, p2s(p.get("o").unwrap_or_else(|| die("a part without o"))), h, 3.0, 0.0, &mat(p.get("m").unwrap()))
                })
                .collect();
            (name.clone(), solids)
        })
        .collect()
}

/// The lyre-harp page: what its script builds, piece for piece.
fn lyre(d: &J) -> Vec<Solid> {
    let t = num(d.get("t").unwrap());
    let half = num(d.get("bore").unwrap()) / 2.0;
    let rim = p2s(d.get("rim").unwrap());
    let hole = p2s(d.get("hole").unwrap());
    let knot: Vec<Vec<P2>> = arr(d, "knot").iter().map(p2s).collect();
    let id = ident();
    let mut out = vec![];
    let mut front = vec![hole.clone()];
    front.extend(knot);
    out.extend(plate("front cheek".into(), rim.clone(), front, t, half, &id));
    out.extend(plate("back cheek".into(), rim, vec![hole], t, -half - t, &id));
    for (key, layers, what) in [("hitch", "hitch_layers", "hitch block"), ("tune", "tune_layers", "tuning block")] {
        let loop_ = p2s(d.get(key).unwrap());
        for k in 0..num(d.get(layers).unwrap()) as usize {
            out.extend(plate(format!("{what} layer {}", k + 1), loop_.clone(), vec![], t, half - (k as f64 + 1.0) * t, &id));
        }
    }
    for (q, dir) in arr(d, "panels").iter().zip(arr(d, "panels_dir")) {
        let (len, x, y) = (num(q.get("len").unwrap()), num(q.get("x").unwrap()), num(q.get("y").unwrap()));
        let a = num(dir.get("dy").unwrap()).atan2(num(dir.get("dx").unwrap()));
        let (c, s) = (a.cos(), a.sin());
        let m = vec![vec![c, -s, 0.0, x], vec![s, c, 0.0, y], vec![0.0, 0.0, 1.0, 0.0]];
        let r = vec![[-len / 2.0, -t / 2.0], [len / 2.0, -t / 2.0], [len / 2.0, t / 2.0], [-len / 2.0, t / 2.0]];
        let name = format!("{} panel {}", q.get("wall").map(J::str).unwrap_or(""), q.get("tag").map(J::str).unwrap_or(""));
        out.extend(plate(name, r, vec![], 2.0 * half, -half, &m));
    }
    out
}

/// A stack of rings drawn as quads: each ring's top face (f 0), outside
/// (f 1) and bore (f 2), between two levels, its walls upright. The page
/// draws only the first ring's underside, the rest being hidden by the ring
/// below, so every ring is rebuilt here from its top face and walls, and
/// split into its plies, `rise` / 3 laminations of 3 mm ply.
fn rings(d: &J) -> Vec<Solid> {
    let vs: Vec<V> = arr(d, "V")
        .iter()
        .map(|v| {
            let v: Vec<f64> = v.arr().iter().map(num).collect();
            [v[0], v[1], v[2]]
        })
        .collect();
    let rise = num(d.get("rise").unwrap());
    let plies = (rise / 3.0).round().max(1.0) as usize;
    let mut by: std::collections::BTreeMap<i64, Vec<(i64, Vec<V>)>> = Default::default();
    for q in arr(d, "Q") {
        let s = num(q.get("s").unwrap()) as i64;
        let f = num(q.get("f").unwrap()) as i64;
        let l: Vec<V> = q.get("v").unwrap().arr().iter().map(|i| vs[num(i) as usize]).collect();
        by.entry(s).or_default().push((f, l));
    }
    let mut out = vec![];
    for (s, qs) in by {
        let zs: Vec<f64> = qs.iter().flat_map(|(_, l)| l.iter().map(|p| p[2])).collect();
        let (zb, zt) = (zs.iter().cloned().fold(f64::MAX, f64::min), zs.iter().cloned().fold(f64::MIN, f64::max));
        let lvl = |z: f64| (z - zb).abs() < 1e-6 || (z - zt).abs() < 1e-6;
        if !zs.iter().all(|&z| lvl(z)) || ((zt - zb) - rise).abs() > 1e-6 {
            die(format!("ring {}: not one rise between two levels", s + 1));
        }
        for (f, l) in &qs {
            if *f == 0 && !l.iter().all(|p| (p[2] - zt).abs() < 1e-6) {
                die(format!("ring {}: a top face off the top", s + 1));
            }
            if (*f == 1 || *f == 2) && !(0..4).any(|i| {
                let (a, b) = (l[i], l[(i + 1) % 4]);
                (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
            }) {
                die(format!("ring {}: a wall that is not upright", s + 1));
            }
        }
        for k in 0..plies {
            let (b, t) = (zb + rise * k as f64 / plies as f64, zb + rise * (k + 1) as f64 / plies as f64);
            let at = |p: V| -> V { [p[0], p[1], if (p[2] - zt).abs() < 1e-6 { t } else { b }] };
            let mut faces = vec![];
            for (f, l) in &qs {
                match f {
                    0 => {
                        faces.push(vec![l.iter().map(|&p| at(p)).collect()]);
                        faces.push(vec![l.iter().rev().map(|&p| [p[0], p[1], b]).collect()]);
                    }
                    1 | 2 => faces.push(vec![l.iter().map(|&p| at(p)).collect()]),
                    _ => {}
                }
            }
            let name = if plies > 1 { format!("ring {} ply {}", s + 1, k + 1) } else { format!("ring {}", s + 1) };
            out.push(Solid { name, faces });
        }
    }
    out
}

/// hex-dice-box: its plates, each `T` thick.
fn dicebox(d: &J) -> Vec<Solid> {
    let t = num(d.get("T").unwrap());
    arr(d, "model")
        .iter()
        .filter_map(|p| {
            let h = arr(p, "holes").iter().map(p2s).collect();
            plate(p.get("name").map(J::str).unwrap_or("part").into(), p2s(p.get("outline").unwrap()), h, t, 0.0, &mat(p.get("m").unwrap()))
        })
        .collect()
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let page = args.iter().find(|a| !a.starts_with("--")).unwrap_or_else(|| die("usage: step-export PAGE [--out=DIR]"));
    let src = std::fs::read_to_string(page).unwrap_or_else(|e| die(format!("{page}: {e}")));
    let line = src
        .lines()
        .find_map(|l| l.trim().strip_prefix("const DATA = ").or_else(|| l.trim().strip_prefix("const D = ")))
        .unwrap_or_else(|| die(format!("{page}: no `const DATA = ` or `const D = ` line")));
    let data = J::parse(line.trim_end().trim_end_matches(';')).unwrap_or_else(|e| die(e));
    let pp = std::path::Path::new(page);
    let dir = args.iter().find_map(|a| a.strip_prefix("--out=")).map(String::from).unwrap_or_else(|| {
        let d = pp.parent().map(|p| p.to_string_lossy().to_string()).filter(|s| !s.is_empty()).unwrap_or(".".into());
        format!("{d}/step")
    });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(e));
    let stem = args.iter().find_map(|a| a.strip_prefix("--name=")).map(String::from)
        .unwrap_or_else(|| pp.file_stem().unwrap().to_string_lossy().to_string());
    // rings before lyre: the bell's data names its rim too
    let files: Vec<(String, String, Vec<Solid>)> = if data.get("Q").is_some() && data.get("rings").is_some() {
        vec![(stem.clone(), stem.clone(), rings(&data))]
    } else if data.get("rim").is_some() && data.get("panels").is_some() {
        vec![(stem.clone(), stem.clone(), lyre(&data))]
    } else if data.get("model").is_some() {
        vec![(stem.clone(), stem.clone(), dicebox(&data))]
    } else if data.get("Q").is_some() {
        die(format!("{page}: a bore drawn as surfaces, not parts; nothing to export"))
    } else {
        designs(&data).into_iter().map(|(n, s)| (format!("{stem}-{}", slug(&n)), format!("{stem} {n}"), s)).collect()
    };
    for (file, title, solids) in files {
        let path = format!("{dir}/{file}.step");
        let n = write_step(&path, &title, solids);
        println!("{path}  {n} solids");
    }
}
