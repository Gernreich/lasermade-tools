//! `sc-flatten EDITED.sc [--original=ORIGINAL.sc] [--out=DIR]` (2026-11-08):
//! the cut sheets for a SolidCraft design edited by hand, worked back from
//! the solid itself.
//!
//! SolidCraft builds the edited design and the committed one it came from
//! (as committed then, if the edit is older than the design now) as STEP
//! files. Every body is laid down on its thin side: its outline and the
//! holes it is cut through, and the pockets engraved in it. Each body of the
//! committed design is found among the parts of the repository's cut sheets
//! (by shape, any turn or mirror; mirror twins held to the way the sheet
//! lays most plates), which says which sheet it is on, where, which way up,
//! in which inks and with how much kerf. A body the edit left alone keeps its
//! part exactly as drawn, and a sheet with none changed is written back byte
//! for byte. A changed body is drawn afresh from the solid: the outline grown
//! by the kerf the sheet states (else the median its parts show), round or
//! mitred at the corners as the sheet is, each hole offset as its committed
//! hole was, pockets in the fill inks they were engraved in, set where the
//! old part sat. What the design does not model (engraved numbers, lines, a
//! rosette's interlace) is carried over, each mark following the nearest
//! corner or hole; parts the design has no body for stay as committed. Parts
//! that no longer fit are packed again, onto more sheets if need be.
//! `--redraw-all` redraws every body, unchanged or not, to check this
//! against the committed sheets.
//!
//! The sheets are written under DIR (default: target/sc-flatten/STEM/ in the
//! repository) at the paths the committed sheets have. sc-replay draws the
//! same edit through the repository's own generator; sc-compare checks the
//! two against each other.
use lasermade_tools::scedit;
use lasermade_tools::sheets::{self, Item, Part, Sheet};
use lasermade_tools::step::{self, Flat, P};
use std::collections::{HashMap, HashSet};

fn die(m: String) -> ! {
    scedit::die(m)
}

/// The repository's committed files matching a pathspec, at a commit or as they stand.
fn files_at(repo: &std::path::Path, commit: Option<&str>, pat: &str) -> Vec<String> {
    match commit {
        None => scedit::git_files(repo, pat),
        Some(c) => {
            let o = std::process::Command::new("git").arg("-C").arg(repo).args(["ls-tree", "-r", "--name-only", c]).output().expect("git");
            let ext = pat.trim_start_matches('*');
            String::from_utf8_lossy(&o.stdout).lines().filter(|f| f.ends_with(ext)).map(String::from).collect()
        }
    }
}

/// SolidCraft builds the design; its bodies, flat.
fn flats(sc: &str, tmp: &std::path::Path, tag: &str) -> Vec<Flat> {
    let out = tmp.join(format!("{tag}.step"));
    let r = std::process::Command::new("solidcraft-cli").args(["run", sc, "--out"]).arg(&out).arg("--quiet").output().unwrap_or_else(|e| die(format!("solidcraft-cli: {e}")));
    if !r.status.success() {
        die(format!("SolidCraft could not build {sc}: {}", String::from_utf8_lossy(&r.stderr).trim()));
    }
    flats_of(sc, &out)
}

/// The bodies of a design SolidCraft has built as a STEP file, flat.
fn flats_of(sc: &str, out: &std::path::Path) -> Vec<Flat> {
    let t = std::fs::read_to_string(out).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));
    let bs = step::bodies(&t, 0.005).unwrap_or_else(|e| die(format!("{sc}: {e}")));
    let c = step::centre(&bs);
    bs.iter().map(|b| step::flatten(b, Some(c)).unwrap_or_else(|e| die(e))).collect()
}

/// Rings the same, wherever the body sits.
fn same_shape(a: &Flat, b: &Flat) -> bool {
    use geo::{Area, Centroid};
    let (ra, rb) = (sheets::rings(&a.region), sheets::rings(&b.region));
    if ra.len() != rb.len() || (a.region.unsigned_area() - b.region.unsigned_area()).abs() > 1e-4 || (a.thickness - b.thickness).abs() > 1e-6 || a.pockets.len() != b.pockets.len() {
        return false;
    }
    let (ca, cb) = (a.region.centroid().unwrap(), b.region.centroid().unwrap());
    let d = (cb.x() - ca.x(), cb.y() - ca.y());
    ra.iter().flatten().all(|&p| sheets::to_rings((p.0 + d.0, p.1 + d.1), &rb) < 1e-4)
}

fn bbox(pts: &[P]) -> (f64, f64, f64, f64) {
    pts.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)))
}

/// The directions a part's sides run in: the directions its outline runs
/// most in, counting only segments longer than `seg` (the flanks of the
/// fingers, a ply long, do not count; their tips and the edge between them
/// do), a direction counting once its segments add up to `keep`.
fn side_directions(pts: &[P], seg: f64, keep: f64) -> Vec<f64> {
    let n = pts.len();
    // (angle in [0, 180) degrees, length)
    let mut a: Vec<(f64, f64)> = (0..n)
        .filter_map(|i| {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            let l = (q.0 - p.0).hypot(q.1 - p.1);
            (l > seg).then(|| ((q.1 - p.1).atan2(q.0 - p.0).to_degrees().rem_euclid(180.0), l))
        })
        .collect();
    a.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    // runs of directions within 3 degrees, the turn from 180 back to 0 included
    let mut groups: Vec<(f64, f64)> = vec![]; // (weighted sum of angles, weight)
    for &(ang, l) in &a {
        match groups.last_mut() {
            Some(g) if ang - g.0 / g.1 < 3.0 => {
                g.0 += ang * l;
                g.1 += l;
            }
            _ => groups.push((ang * l, l)),
        }
    }
    if groups.len() > 1 {
        let (f, l) = (groups[0], *groups.last().unwrap());
        if f.0 / f.1 + 180.0 - l.0 / l.1 < 3.0 {
            let g = groups.pop().unwrap();
            groups[0] = (f.0 + (g.0 - 180.0 * g.1), f.1 + g.1);
        }
    }
    groups.iter().filter(|g| g.1 > keep).map(|g| (g.0 / g.1).to_radians()).collect()
}

/// The part's outermost line on each side of each direction: n . p <= h.
fn bounds(pts: &[P], dirs: &[f64]) -> Vec<(P, f64)> {
    let mut v = vec![];
    for &t in dirs {
        for sg in [1.0, -1.0] {
            let nv = (-t.sin() * sg, t.cos() * sg);
            v.push((nv, pts.iter().map(|p| nv.0 * p.0 + nv.1 * p.1).fold(f64::MIN, f64::max)));
        }
    }
    v
}

fn cross_lines(a: (P, f64), b: (P, f64)) -> Option<P> {
    let ((p, h), (q, k)) = (a, b);
    let den = p.0 * q.1 - p.1 * q.0;
    (den.abs() > 1e-9).then(|| ((h * q.1 - k * p.1) / den, (p.0 * k - q.0 * h) / den))
}

/// A part's corners: where its bounds meet, each with the two bounds that
/// make it, so the same corner can be found on the part redrawn. Finger
/// counts and the bevels at a corner change; the bounds do not.
fn corners(pts: &[P], lines: &[(P, f64)]) -> Vec<(P, usize, usize)> {
    const BOX: usize = usize::MAX;
    let (x0, y0, x1, y1) = bbox(pts);
    // the polygon as its corners, each with the lines of the edges into and out of it
    let bx = [(x0 - 10.0, y0 - 10.0), (x1 + 10.0, y0 - 10.0), (x1 + 10.0, y1 + 10.0), (x0 - 10.0, y1 + 10.0)];
    let mut poly: Vec<(P, usize, usize)> = bx.iter().map(|&p| (p, BOX, BOX)).collect();
    for (li, &(nv, h)) in lines.iter().enumerate() {
        let f = |p: P| nv.0 * p.0 + nv.1 * p.1 - h;
        let mut out: Vec<(P, usize, usize)> = vec![];
        let m = poly.len();
        for k in 0..m {
            let (p, q) = (poly[k], poly[(k + 1) % m]);
            let (fp, fq) = (f(p.0), f(q.0));
            if fp <= 0.0 {
                out.push(p);
            }
            if (fp < 0.0) != (fq < 0.0) && fp != fq {
                let u = fp / (fp - fq);
                let x = (p.0 .0 + u * (q.0 .0 - p.0 .0), p.0 .1 + u * (q.0 .1 - p.0 .1));
                // the edge p..q runs along p's outgoing line
                if fp <= 0.0 {
                    out.push((x, p.2, li));
                } else {
                    out.push((x, li, p.2));
                }
            }
        }
        poly = out;
    }
    poly.into_iter().filter(|c| c.1 != BOX && c.2 != BOX).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut edited, mut original, mut out) = (None, None, None);
    let mut redraw_all = false;
    for a in &args {
        if a == "--redraw-all" {
            redraw_all = true;
        } else if let Some(v) = a.strip_prefix("--original=") {
            original = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--out=") {
            out = Some(v.to_string());
        } else if a.starts_with('-') {
            die(format!("{a}? It takes EDITED.sc, --original=ORIGINAL.sc, --out=DIR and --redraw-all."));
        } else {
            edited = Some(a.clone());
        }
    }
    let edited = edited.unwrap_or_else(|| die("which .sc file? sc-flatten EDITED.sc [--original=ORIGINAL.sc] [--out=DIR]".into()));
    let e = scedit::find(&edited, original.as_deref());
    let (ep, repo, stem) = (e.edited.clone(), e.repo.clone(), e.stem.clone());
    let out = std::path::PathBuf::from(out.unwrap_or_else(|| repo.join("target/sc-flatten").join(&stem).to_string_lossy().to_string()));
    let tmp = scedit::Tmp::new("sc-flatten");
    println!("sc-flatten {}\n  from {}", ep.display(), e.original.display());
    // the version of the design the edit started from, and the sheets of that day
    // the edited design's parameters, and the design built, in one run
    let est = tmp.0.join("edited.step");
    let (base, commit, _) = scedit::base(&e, &scedit::parameters_step(&ep, &tmp.0, Some(&est)), &tmp.0);
    if let Some(c) = &commit {
        println!("  it was edited from {} as committed in {}, so its sheets are read as they were then", e.original.file_name().unwrap().to_string_lossy(), &c[..7]);
    }
    // an unchanged design (--redraw-all on the committed file) is built once
    let same = std::fs::read(&base).ok().is_some_and(|b| std::fs::read(&ep).ok().is_some_and(|d| d == b));
    let old = if same { flats_of(&ep.to_string_lossy(), &est) } else { flats(&base.to_string_lossy(), &tmp.0, "original") };
    let new = if same { old.clone() } else { flats_of(&ep.to_string_lossy(), &est) };
    for f in &old {
        if !new.iter().any(|g| g.name == f.name) {
            die(format!("the edit removed or renamed body '{}'; sc-flatten redraws bodies, it does not add or drop them", f.name));
        }
    }
    if new.len() != old.len() {
        die(format!("the edit has {} bodies and the original {}; sc-flatten redraws bodies, it does not add or drop them", new.len(), old.len()));
    }
    // the repository's cut sheets
    let mut sheets_v: Vec<Sheet> = files_at(&repo, commit.as_deref(), "*.svg")
        .into_iter()
        .filter(|f| !f.split('/').any(|p| p == "previews" || p.starts_with('.')))
        .filter_map(|f| {
            let at = match &commit {
                None => repo.join(&f),
                Some(_) => {
                    let t = tmp.0.join("sheets").join(&f);
                    std::fs::create_dir_all(t.parent().unwrap()).ok()?;
                    std::fs::write(&t, scedit::file_at(&repo, commit.as_deref(), &f)?).ok()?;
                    t
                }
            };
            sheets::read(&at.to_string_lossy()).ok().map(|mut s| {
                s.path = f;
                s
            })
        })
        .filter(|s| !s.parts.is_empty())
        .collect();
    // a body no sheet draws whole, whose holes are what the sheets cut: a
    // knotwork rosette's board is the soundboard the waste comes out of, and
    // its cut file draws only the waste. Each hole is then a body of its own,
    // in the original and the edit alike
    let seen = |f: &Flat| sheets_v.iter().any(|s| s.parts.iter().any(|p| sheets::register(f, p, None).is_some_and(|r| r.0 < 0.2)));
    let unseen: Vec<String> = old.iter().filter(|f| sheets::rings(&f.region).len() > 1 && !seen(f)).map(|f| f.name.clone()).collect();
    let as_holes = |v: Vec<Flat>| -> Vec<Flat> {
        v.into_iter()
            .flat_map(|f| {
                if !unseen.contains(&f.name) {
                    return vec![f];
                }
                sheets::rings(&f.region)
                    .into_iter()
                    .skip(1)
                    .enumerate()
                    .map(|(k, h)| Flat {
                        name: format!("{}, hole {}", f.name, k + 1),
                        region: geo::MultiPolygon::new(vec![geo::Polygon::new(geo::LineString::from(h.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![])]),
                        pockets: vec![],
                        ..f.clone()
                    })
                    .collect()
            })
            .collect()
    };
    let (old, new) = (as_holes(old), as_holes(new));
    if !unseen.is_empty() {
        println!("  {} drawn by {} holes alone, as the sheets cut it", unseen.join(", "), if unseen.len() == 1 { "its" } else { "their" });
    }
    let newby: HashMap<&str, &Flat> = new.iter().map(|f| (f.name.as_str(), f)).collect();
    // every committed body to the part it was cut as. A plate and its mirror
    // twin are one shape, so shape alone cannot say which is which; the
    // sheet's way of laying plates (inside face up, say) can: most plates
    // match one way round, mirrored or not, and the twins are held to it.
    let match_all = |only: Option<bool>| {
        let mut v: Vec<(f64, usize, usize, usize, f64, sheets::Rigid)> = vec![];
        for (bi, f) in old.iter().enumerate() {
            for (si, s) in sheets_v.iter().enumerate() {
                for (pi, p) in s.parts.iter().enumerate() {
                    if let Some((mean, _, r)) = sheets::register(f, p, only) {
                        if mean < 0.2 {
                            // and the part's cut onto the body: a part with a
                            // notch the body lacks (a torus's round-port
                            // cheek has six mortices more than its square
                            // one) fits it as well the other way only.
                            // Both ways on every corner, not the register's
                            // sample of them: a plate with 64 holes gives its
                            // outline a handful of the 160 sample points, and
                            // a back plate 5 mm short at one end (the panpipes'
                            // strip edge) fitted its full-length twin as well
                            let fr: Vec<Vec<P>> = sheets::rings(&f.region).iter().map(|ring| ring.iter().map(|&q| r.apply(q)).collect()).collect();
                            let pr = sheets::part_rings(p);
                            let (fi, pix) = (sheets::RingIndex::new(&fr), sheets::RingIndex::new(&pr));
                            let nf = fr.iter().map(|x| x.len()).sum::<usize>().max(1) as f64;
                            let fd: Vec<f64> = fr.iter().flatten().map(|&q| pix.dist(q)).collect();
                            let bd: Vec<f64> = pr.iter().flatten().map(|&q| fi.dist(q)).collect();
                            let fwd = fd.iter().sum::<f64>() / nf;
                            let n = pr.iter().map(|x| x.len()).sum::<usize>().max(1) as f64;
                            let back = bd.iter().sum::<f64>() / n;
                            // and the worst corner either way: a 2 mm notch at
                            // one end of a 455 mm wall barely moves a mean
                            let worst = fd.iter().chain(&bd).cloned().fold(0.0, f64::max);
                            v.push(((fwd + back) / 2.0, bi, si, pi, worst, r));
                        }
                    }
                }
            }
        }
        v
    };
    let mut cands = match_all(None);
    let mirrored = cands.iter().filter(|c| c.5.mirror).count();
    let way = mirrored * 2 > cands.len();
    // the sheet's way is preferred, not required: a body with no match that
    // way keeps its others (the top plate lies outside face up), and a pair
    // of plates that are the same shape (piece 3 of a trumpet coil) needs
    // one of each to have a part apiece
    let held = match_all(Some(way));
    cands.extend(held);
    // a build's sheets are one set: name-sheet1, name-sheet2, ... (the
    // panpipes, every build in one folder), or else a folder of its own
    // (a trumpet build, one sheet per piece, beside concept walks whose
    // pieces are the same shapes). When one set holds every body of the
    // design, the design is that build, and parts that look the same on
    // another build's sheets are not its
    // (a sheet of other stock is the build's too: the panpipes' strip edge
    // is cut from 1.5 mm, "panpipes-E3-strip-1.5mm-sheet1")
    let by_name = |si: usize| -> String {
        let re = fancy_regex::Regex::new(r"(-\d+(\.\d+)?mm)?-sheet\d+").unwrap();
        re.replace_all(&sheets_v[si].path, "").to_string()
    };
    let by_folder = |si: usize| -> String {
        let p = &sheets_v[si].path;
        p.rsplit_once('/').map_or(String::new(), |x| x.0.to_string())
    };
    // or one variant of a build: a ribbon bore's folder holds its cheeks and
    // panels unported, ported, ported both ends, ... ("-ported-square-narrow-
    // panels", "-ported-square-narrow-cheek-x2"): the sheets that differ only
    // in the part kind are one set, and the design is one of them
    let kind_re = fancy_regex::Regex::new(r"-(panels|cheek(-[a-z0-9]+)?)(-cut-files)?\.svg$").unwrap();
    let by_variant = |si: usize| -> String { kind_re.replace(&sheets_v[si].path, "").to_string() };
    let variants = sheets_v.iter().any(|s| kind_re.is_match(&s.path).unwrap_or(false));
    let sets: Vec<&dyn Fn(usize) -> String> = if variants { vec![&by_variant, &by_name, &by_folder] } else { vec![&by_name, &by_folder] };
    for set_of in sets {
        let mut cover: HashMap<String, HashSet<usize>> = HashMap::new();
        for c in &cands {
            cover.entry(set_of(c.2)).or_default().insert(c.1);
        }
        if std::env::var("SC_SETS").is_ok() {
            let mut ks: Vec<&String> = cover.keys().collect();
            ks.sort();
            for k in ks {
                let miss: Vec<&str> = (0..old.len()).filter(|bi| !cover[k].contains(bi)).map(|bi| old[bi].name.as_str()).collect();
                eprintln!("  set {k}: {} of {} bodies; lacks {}", cover[k].len(), old.len(), miss.iter().take(8).cloned().collect::<Vec<_>>().join(", "));
            }
        }
        let mut whole: Vec<(String, f64)> = cover
            .iter()
            .filter(|(_, b)| b.len() == old.len())
            .map(|(k, _)| {
                // the set's fit: each body's best mean on it, and each of its
                // parts' best body, a part no body fits counting 1 mm. Bodies
                // alone miss a part the design lacks among parts it has
                // twins of: the panpipes' notch-edge tubes have three walls
                // notched, and a plain design's walls all take the plain
                // twins, so the notch set fitted as well as the plain one
                let fit: f64 = (0..old.len())
                    .map(|bi| cands.iter().filter(|c| c.1 == bi && set_of(c.2) == *k).map(|c| c.0).fold(f64::MAX, f64::min))
                    .sum();
                let parts: f64 = (0..sheets_v.len())
                    .filter(|&si| set_of(si) == *k)
                    .flat_map(|si| (0..sheets_v[si].parts.len()).map(move |pi| (si, pi)))
                    .map(|(si, pi)| {
                        let b = cands.iter().filter(|c| c.2 == si && c.3 == pi).map(|c| c.0).fold(f64::MAX, f64::min);
                        if b == f64::MAX { 1.0 } else { b }
                    })
                    .sum();
                (k.clone(), fit + parts)
            })
            .collect();
        // sets that fit alike (the panpipes' plain and chamfer rafts are one
        // outline, the chamfer only engraved) go to the one named like the
        // design: panpipes-plain-edge.sc is the "panpipes-E3-plain" sheets,
        // coil-10x10x30-1.5t.sc the sheets in .../coil-10x10x30-1.5t/cut-files
        let design_words: HashSet<String> = e.original.file_stem().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
        let named = |k: &str| std::cmp::Reverse(k.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| design_words.contains(*w)).count());
        let best_fit = whole.iter().map(|w| w.1).fold(f64::MAX, f64::min);
        let near = |f: f64| f <= best_fit + 1e-6 * (1.0 + best_fit.abs());
        if std::env::var("SC_SETS").is_ok() {
            for w in &whole {
                eprintln!("  whole set {}: fit {:.6} near {} named {}", w.0, w.1, near(w.1), named(&w.0).0);
            }
        }
        whole.sort_by(|a, b| near(b.1).cmp(&near(a.1)).then_with(|| if near(a.1) { named(&a.0).cmp(&named(&b.0)) } else { std::cmp::Ordering::Equal }).then(a.1.partial_cmp(&b.1).unwrap()).then(a.0.cmp(&b.0)));
        if let Some((w, _)) = whole.first() {
            let w = w.clone();
            cands.retain(|c| set_of(c.2) == w);
            break;
        }
    }
    // a part common to several designs (the ocarinas' bird) is on every one of
    // their sheets: take it from the sheets that hold most of this design
    let mut held: HashMap<usize, Vec<usize>> = HashMap::new();
    for c in &cands {
        let v = held.entry(c.2).or_default();
        if !v.contains(&c.1) {
            v.push(c.1);
        }
    }
    let rank = |si: usize| std::cmp::Reverse(held.get(&si).map_or(0, |v| v.len()));
    // the sheet's way only breaks near-ties: a body and a part that fit
    // clearly better turned over keep that (a nearly square trumpet plate
    // fits its own part the sheet's way too, a corner out of place)
    let mut best_at: HashMap<(usize, usize, usize), f64> = HashMap::new();
    for c in &cands {
        let e = best_at.entry((c.1, c.2, c.3)).or_insert(f64::MAX);
        *e = e.min(c.0);
    }
    cands.retain(|c| c.0 < best_at[&(c.1, c.2, c.3)] + 0.05);
    // and a body takes no part it fits clearly worse than its best one (the
    // preferences below come before the fit, so they must only choose
    // among fits that are equally good)
    let mut best_body: HashMap<usize, f64> = HashMap::new();
    for c in &cands {
        let e = best_body.entry(c.1).or_insert(f64::MAX);
        *e = e.min(c.0);
    }
    cands.retain(|c| c.0 < best_body[&c.1] + 0.1);
    let theirs = |c: &(f64, usize, usize, usize, f64, sheets::Rigid)| c.5.mirror != way;
    // a body named for its piece ("piece 3 wall 1") belongs on that piece's
    // sheet ("...-03of12-..."): pieces of one bore share wall shapes, and
    // near-twins differ by a finger
    let piece_re = fancy_regex::Regex::new(r"\bpiece (\d+)\b").unwrap();
    let sheet_re = fancy_regex::Regex::new(r"(\d+)of\d+").unwrap();
    let num = |re: &fancy_regex::Regex, t: &str| re.captures(t).ok().flatten().and_then(|c| c[1].parse::<usize>().ok());
    let elsewhere = |c: &(f64, usize, usize, usize, f64, sheets::Rigid)| match (num(&piece_re, &old[c.1].name), num(&sheet_re, &sheets_v[c.2].path)) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    };
    // the sheet's way is for near-ties only: a fit 0.02 mm worse than the
    // body's best is not one (a torus's bottom cheek fits the round-port
    // sheet the sheet's way at 0.115 mm, its own square-port part turned
    // over at 0.077)
    let worse = |c: &(f64, usize, usize, usize, f64, sheets::Rigid)| c.0 > best_body[&c.1] + 0.02;
    // nor is a fit whose worst corner is half a millimetre further out than
    // the body's best fit's: the panpipes' notch-edge tubes have two 455 mm
    // walls, one notched at an end, and the sheet's way round chose the
    // other one for each
    let mut best_worst: HashMap<usize, f64> = HashMap::new();
    for c in &cands {
        let e = best_worst.entry(c.1).or_insert(f64::MAX);
        *e = e.min(c.4);
    }
    let farther = |c: &(f64, usize, usize, usize, f64, sheets::Rigid)| c.4 > best_worst[&c.1] + 0.5;
    cands.sort_by(|a, b| elsewhere(a).cmp(&elsewhere(b)).then(rank(a.2).cmp(&rank(b.2))).then(worse(a).cmp(&worse(b))).then(farther(a).cmp(&farther(b))).then(theirs(a).cmp(&theirs(b))).then(a.0.partial_cmp(&b.0).unwrap()));
    if let Ok(w) = std::env::var("SC_CANDS") {
        for c in cands.iter().filter(|c| old[c.1].name == w) {
            eprintln!("  cand {:.4} worst {:.3} {} part {} mirror {} rank {} theirs {}", c.0, c.4, sheets_v[c.2].path.rsplit('/').next().unwrap_or(""), c.3, c.5.mirror, rank(c.2).0, theirs(c));
        }
    }
    let mut body_at: Vec<Option<(usize, usize, f64, f64, sheets::Rigid)>> = vec![None; old.len()];
    // a part on a sheet named "-x2-" is cut twice (a ribbon bore's two
    // cheeks, one drawing), so it takes two bodies
    let cap: Vec<usize> = {
        let re = fancy_regex::Regex::new(r"-x(\d+)-").unwrap();
        sheets_v.iter().map(|sh| re.captures(sh.path.rsplit('/').next().unwrap_or("")).ok().flatten().and_then(|c| c[1].parse().ok()).unwrap_or(1)).collect()
    };
    let mut taken: Vec<(usize, usize)> = vec![];
    for &(mean, bi, si, pi, worst, r) in &cands {
        if body_at[bi].is_some() || taken.iter().filter(|t| **t == (si, pi)).count() >= cap[si] {
            continue;
        }
        body_at[bi] = Some((si, pi, mean, worst, r));
        taken.push((si, pi));
    }
    // a body left without a part, when every part it fits went to another
    // body that fits elsewhere too (identical walls on three trumpet pieces'
    // sheets): move those along, as a matching does, until it has one
    fn augment(bi: usize, cands: &[(f64, usize, usize, usize, f64, sheets::Rigid)], body_at: &mut Vec<Option<(usize, usize, f64, f64, sheets::Rigid)>>, seen: &mut HashSet<(usize, usize)>, cap: &[usize]) -> bool {
        for c in cands.iter().filter(|c| c.1 == bi) {
            let at = (c.2, c.3);
            if !seen.insert(at) {
                continue;
            }
            let holders: Vec<usize> = (0..body_at.len()).filter(|&k| body_at[k].is_some_and(|b| (b.0, b.1) == at)).collect();
            if holders.len() < cap[at.0] || augment(holders[0], cands, body_at, seen, cap) {
                body_at[bi] = Some((c.2, c.3, c.0, c.4, c.5));
                return true;
            }
        }
        false
    }
    for bi in 0..old.len() {
        if body_at[bi].is_none() {
            augment(bi, &cands, &mut body_at, &mut HashSet::new(), &cap);
        }
    }
    if std::env::var("SC_CANDS").is_ok() {
        for (bi, b) in body_at.iter().enumerate() {
            if let Some(b) = b {
                if b.2 > best_body[&bi] + 0.003 {
                    eprintln!("  off-best {}: took {:.4} on {} part {}, best {:.4}", old[bi].name, b.2, sheets_v[b.0].path.rsplit('/').next().unwrap_or(""), b.1, best_body[&bi]);
                    for c in cands.iter().filter(|c| c.1 == bi).take(6) {
                        eprintln!("      cand {:.4} {} part {} mirror {} rank {} worse {} theirs {}", c.0, sheets_v[c.2].path.rsplit('/').next().unwrap_or(""), c.3, c.5.mirror, rank(c.2).0, worse(c), theirs(c));
                    }
                }
            }
        }
    }
    // twins (a plate and its mirror) fit each other's parts nearly as well:
    // swap any two bodies whose parts suit them better the other way round
    let fit = |bi: usize, si: usize, pi: usize| cands.iter().find(|c| c.1 == bi && c.2 == si && c.3 == pi).copied();
    loop {
        let mut swapped = false;
        for i in 0..old.len() {
            for j in i + 1..old.len() {
                let (Some(a), Some(b)) = (body_at[i], body_at[j]) else { continue };
                let (Some(x), Some(y)) = (fit(i, b.0, b.1), fit(j, a.0, a.1)) else { continue };
                if x.0 + y.0 < a.2 + b.2 - 1e-6 {
                    body_at[i] = Some((x.2, x.3, x.0, x.4, x.5));
                    body_at[j] = Some((y.2, y.3, y.0, y.4, y.5));
                    swapped = true;
                }
            }
        }
        if !swapped {
            break;
        }
    }
    let lost: Vec<&str> = old.iter().zip(&body_at).filter(|(_, b)| b.is_none()).map(|(f, _)| f.name.as_str()).collect();
    if !lost.is_empty() {
        // a design with no sheets of its own (a trumpet concept walk drawn
        // only as a page): the folder its solidcraft/ sits in holds no cut
        // sheet; parts elsewhere that look like some of its pieces are not it
        let home = e.original.parent().and_then(|d| d.parent()).and_then(|d| d.strip_prefix(&repo).ok()).map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
        let own = sheets_v.iter().filter(|s| home.is_empty() || s.path.starts_with(&format!("{home}/"))).count();
        if own == 0 {
            die(format!(
                "{} has no cut sheets: none is committed under {}/, and {} of its {} bodies are on no sheet elsewhere (those that are belong to other builds with the same pieces). There is nothing to redraw it from.",
                e.original.file_name().unwrap().to_string_lossy(),
                if home.is_empty() { "." } else { home.as_str() },
                lost.len(),
                old.len()
            ));
        }
        die(format!("no part of any cut sheet in {} is {}: the committed design and the committed sheets disagree, or the sheets draw it some way sc-flatten cannot read", repo.display(), lost.join(", ")));
    }
    // the kerf each sheet is drawn for: what it says, else the median of
    // its parts' burns, as one part's own can be thrown by its shape; and
    // whether it rounds its outside corners (Boxes.py does) or mitres them,
    // whichever redraws its parts the better
    let mut burns: HashMap<usize, Vec<f64>> = HashMap::new();
    for (bi, f) in old.iter().enumerate() {
        let (si, pi, _, _, r) = body_at[bi].unwrap();
        burns.entry(si).or_default().push(sheets::burn_of(f, &sheets_v[si].parts[pi], r));
    }
    let burn_on: HashMap<usize, f64> = burns
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let b = sheets::stated_kerf(&sheets_v[k].desc).map_or((v[v.len() / 2] * 1e4).round() / 1e4, |k| k / 2.0);
            (k, b)
        })
        .collect();
    // a near-symmetric part (a scallop's cheek, five lobes and a mirror)
    // sits almost as well turned or flipped: the plain fit's mean carries
    // the kerf, which drowns the slivers that tell the twins apart. With the
    // burn known, grow the body by it and take the way whose cut lies on the
    // part's line both ways round, as the redraw will be judged
    for bi in 0..old.len() {
        let Some((si, pi, mean, worst, r0)) = body_at[bi] else { continue };
        let part = &sheets_v[si].parts[pi];
        // (the stored fit is the two-way one; the ways carry the plain mean)
        let all_ways = sheets::register_all(&old[bi], part, None);
        let Some(top) = all_ways.first().map(|w| w.0) else { continue };
        let ways: Vec<sheets::Rigid> = all_ways.into_iter().filter(|w| w.0 < top + 0.01).map(|w| w.2).collect();
        if ways.len() < 2 {
            continue;
        }
        let pr = sheets::part_rings(part);
        let g = sheets::rings(&sheets::grow(&old[bi].region, burn_on[&si], false));
        let pockets: Vec<Vec<P>> = old[bi].pockets.iter().flat_map(|(_, _, m)| sheets::rings(m)).collect();
        let fills: Vec<Vec<P>> = part.inside.iter().filter(|it| it.fill && it.closed).map(|it| it.pts.clone()).collect();
        let (pri, fli) = (sheets::RingIndex::new(&pr), sheets::RingIndex::new(&fills));
        let score = |r: &sheets::Rigid| {
            let gr: Vec<Vec<P>> = g.iter().map(|ring| ring.iter().map(|&q| r.apply(q)).collect()).collect();
            let n1 = gr.iter().map(|x| x.len()).sum::<usize>().max(1) as f64;
            let n2 = pr.iter().map(|x| x.len()).sum::<usize>().max(1) as f64;
            let gri = sheets::RingIndex::new(&gr);
            let cut = gr.iter().flatten().map(|&q| pri.dist(q)).sum::<f64>() / n1 + pr.iter().flatten().map(|&q| gri.dist(q)).sum::<f64>() / n2;
            // and the pockets on the part's fills: a symmetric plate whose
            // engraving alone says which way round it lies (the square
            // flute's flue, red one half and magenta the other)
            let pk: Vec<P> = pockets.iter().flatten().map(|&q| r.apply(q)).collect();
            let on = if pk.is_empty() || fills.is_empty() { 0.0 } else { pk.iter().map(|&q| fli.dist(q)).sum::<f64>() / pk.len() as f64 };
            cut + on
        };
        let best = ways.into_iter().min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()).unwrap();
        if std::env::var("SC_REG_DEBUG").is_ok() {
            eprintln!("  twins {}: kept {:.4}, best {:.4}", old[bi].name, score(&r0), score(&best));
        }
        let best = if score(&best) < score(&r0) - 1e-6 { best } else { r0 };
        body_at[bi] = Some((si, pi, mean, worst, best));
    }
    // near-twins in a run (a chanter's mouthpiece plates, each a step
    // narrower than the last) fit their neighbours' parts almost as well,
    // and before the burn was known the half-kerf in every fit could take
    // a run two parts along. With it known, grow each body by it and swap
    // any two whose parts suit them better the other way round, both ways
    // measured, until none do
    {
        let grown: Vec<Option<Vec<Vec<P>>>> = (0..old.len()).map(|bi| body_at[bi].map(|b| sheets::rings(&sheets::grow(&old[bi].region, burn_on[&b.0], false)))).collect();
        let two_way = |bi: usize, si: usize, pi: usize, r: &sheets::Rigid| {
            let g = grown[bi].as_ref().unwrap();
            let pr = sheets::part_rings(&sheets_v[si].parts[pi]);
            // settled first: an unsettled move is a few hundredths out, as
            // much as twin panels differ
            let r = sheets::settle_n(*r, g, &pr, 20);
            let gr: Vec<Vec<P>> = g.iter().map(|x| x.iter().map(|&q| r.apply(q)).collect()).collect();
            let n1 = gr.iter().map(|x| x.len()).sum::<usize>().max(1) as f64;
            let n2 = pr.iter().map(|x| x.len()).sum::<usize>().max(1) as f64;
            let (pri, gri) = (sheets::RingIndex::new(&pr), sheets::RingIndex::new(&gr));
            gr.iter().flatten().map(|&q| pri.dist(q)).sum::<f64>() / n1 + pr.iter().flatten().map(|&q| gri.dist(q)).sum::<f64>() / n2
        };
        // of every way the body was found on that part, the one fitting best
        let fit = |bi: usize, si: usize, pi: usize| {
            cands.iter().filter(|c| c.1 == bi && c.2 == si && c.3 == pi).map(|c| (two_way(bi, si, pi, &c.5), *c)).min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        };
        let mut now: Vec<f64> = (0..old.len()).map(|bi| body_at[bi].map_or(0.0, |b| two_way(bi, b.0, b.1, &b.4))).collect();
        for _ in 0..20 {
            let mut swapped = false;
            for i in 0..old.len() {
                for j in i + 1..old.len() {
                    let (Some(a), Some(b)) = (body_at[i], body_at[j]) else { continue };
                    if a.0 != b.0 || (a.0, a.1) == (b.0, b.1) {
                        continue;
                    }
                    let (Some((sx, x)), Some((sy, y))) = (fit(i, b.0, b.1), fit(j, a.0, a.1)) else { continue };
                    // (a clear gain: the twins of a ribbon bore differ by less)
                    if sx + sy < now[i] + now[j] - 0.005 {
                        if std::env::var("SC_REG_DEBUG").is_ok() {
                            eprintln!("  swap {} and {}: {:.4} + {:.4} -> {:.4} + {:.4}", old[i].name, old[j].name, now[i], now[j], sx, sy);
                        }
                        body_at[i] = Some((x.2, x.3, x.0, x.4, x.5));
                        body_at[j] = Some((y.2, y.3, y.0, y.4, y.5));
                        (now[i], now[j]) = (sx, sy);
                        swapped = true;
                    }
                }
            }
            if !swapped {
                break;
            }
        }
    }
    // and settle each move with the body grown by the kerf, so the two
    // outlines are drawn alike
    for bi in 0..old.len() {
        let Some((si, pi, mean, worst, r)) = body_at[bi] else { continue };
        let g = sheets::rings(&sheets::grow(&old[bi].region, burn_on[&si], false));
        let r = sheets::settle(r, &g, &sheets::part_rings(&sheets_v[si].parts[pi]));
        body_at[bi] = Some((si, pi, mean, worst, r));
    }
    let mut round_on: HashMap<usize, bool> = HashMap::new();
    for (&si, &burn) in &burn_on {
        // every part on the sheet has its say, by its points: the biggest
        // alone can be thrown by a misfit of its own (a chanter's strip
        // half a kerf long at its sole) and take the small parts' corners
        // the wrong way with it
        let on: Vec<usize> = (0..old.len()).filter(|&b| body_at[b].unwrap().0 == si).collect();
        let fit = |round: bool| {
            // both ways, each its own mean: the sheet's lines onto the grown
            // outline (a round corner's many close points cannot outvote),
            // and the grown outline onto the sheet's (a mitre's spike at a
            // pointed end, the bullroarer's, shows)
            let (mut a, mut na, mut b, mut nb) = (0.0, 0usize, 0.0, 0usize);
            for &bi in &on {
                let (_, pi, _, _, r) = body_at[bi].unwrap();
                let pr = sheets::part_rings(&sheets_v[si].parts[pi]);
                let g: Vec<Vec<P>> = sheets::rings(&sheets::grow(&old[bi].region, burn, round)).iter().map(|x| x.iter().map(|&q| r.apply(q)).collect()).collect();
                let (gi, pri) = (sheets::RingIndex::new(&g), sheets::RingIndex::new(&pr));
                for &q in pr.iter().flatten() {
                    a += gi.dist(q);
                    na += 1;
                }
                for &q in g.iter().flatten() {
                    b += pri.dist(q);
                    nb += 1;
                }
            }
            a / na.max(1) as f64 + b / nb.max(1) as f64
        };
        round_on.insert(si, fit(true) < fit(false));
    }
    // redraw
    let mut touched: Vec<usize> = vec![];
    let mut changed_names: HashMap<usize, Vec<String>> = HashMap::new();
    let mut report = vec![];
    let mut carried_total = 0;
    for (bi, f) in old.iter().enumerate() {
        let (si, pi, mean, _worst, r) = body_at[bi].unwrap();
        if !touched.contains(&si) {
            touched.push(si);
        }
        let g = newby[f.name.as_str()];
        if !redraw_all && same_shape(f, g) {
            report.push(format!("  same      {:24} {} (part sits within {:.3} mm)", f.name, sheets_v[si].path, mean));
            continue;
        }
        changed_names.entry(si).or_default().push(f.name.clone());
        let oldp = sheets_v[si].parts[pi].clone();
        let burn = burn_on[&si];
        // the old part's place: its outline's centroid on the sheet
        let oc = {
            use geo::Centroid;
            let c = f.region.centroid().unwrap();
            r.apply((c.x(), c.y()))
        };
        let mut rn = r;
        {
            use geo::Centroid;
            let c = g.region.centroid().unwrap();
            let q = r.rot((c.x(), c.y()));
            rn.t = (oc.0 - q.0, oc.1 - q.1);
        }
        // the outline grows by the sheet's burn; each hole by its own, as
        // the committed sheet drew the hole it was (some generators put the
        // kerf on outlines only)
        let round = round_on[&si];
        let poly = |r: &[P]| geo::MultiPolygon::new(vec![geo::Polygon::new(geo::LineString::from(r.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![])]);
        let perim = |r: &[P]| (0..r.len()).map(|i| (r[(i + 1) % r.len()].0 - r[i].0).hypot(r[(i + 1) % r.len()].1 - r[i].1)).sum::<f64>();
        let cen = |r: &[P]| {
            let n = r.len() as f64;
            (r.iter().map(|q| q.0).sum::<f64>() / n, r.iter().map(|q| q.1).sum::<f64>() / n)
        };
        let near = |c: P, rs: &[Vec<P>]| rs.iter().min_by(|a, b| {
            let (x, y) = (cen(a), cen(b));
            ((x.0 - c.0).hypot(x.1 - c.1)).partial_cmp(&((y.0 - c.0).hypot(y.1 - c.1))).unwrap()
        }).cloned();
        let (old_body_holes, sheet_holes): (Vec<Vec<P>>, Vec<Vec<P>>) = (
            sheets::rings(&f.region).into_iter().skip(1).collect(),
            oldp.inside.iter().filter(|i| i.closed && !i.fill && i.ink != "#0000ff").map(|i| i.pts.clone()).collect(),
        );
        let new_rings = sheets::rings(&g.region);
        let mut gr: Vec<Vec<P>> = sheets::rings(&sheets::grow(&poly(&new_rings[0]), burn, round)).into_iter().take(1).collect();
        if oldp.cut.is_some() {
            // the committed part drew its notches as slots on a straight rim
            // (a narrow ribbon cheek's mortices): grow the whole region by the
            // burn, sharp-cornered as the generator draws them, then find each
            // notch where the rim leaves its line and comes back onto it a few
            // points on; the notch is the slot, closed along the rim, and the
            // rim runs straight across it
            use geo::Area;
            let drawn = sheets::drawn_rings(&oldp);
            let tch = sheets::touching(&drawn);
            let least = drawn.iter().zip(&tch).filter(|(_, t)| **t).map(|(r, _)| step::ring_area(r).abs()).fold(f64::MAX, f64::min);
            let gm = sheets::grow(&g.region, burn, false);
            let mut pieces: Vec<geo::Polygon<f64>> = gm.0.clone();
            pieces.sort_by(|a, b| b.unsigned_area().partial_cmp(&a.unsigned_area()).unwrap());
            let pts = |l: &geo::LineString<f64>| -> Vec<P> {
                let mut v: Vec<P> = l.coords().map(|c| (c.x, c.y)).collect();
                if v.len() > 1 && v[0] == *v.last().unwrap() {
                    v.pop();
                }
                v
            };
            let mut slots: Vec<Vec<P>> = vec![];
            // ext: the outline (material on its left when it runs
            // anticlockwise), else a hole (on its right)
            let mut split = |ring: Vec<P>, ext: bool| -> Vec<P> {
                if step::ring_area(&ring).abs() < 50.0 * least {
                    return ring;
                }
                let n = ring.len();
                let off = |q: P, a: P, b: P| {
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    ((q.0 - a.0) * dy - (q.1 - a.1) * dx).abs() / dx.hypot(dy)
                };
                let left = (step::ring_area(&ring) > 0.0) == ext;
                let mut keep = vec![true; n];
                let mut i = 0;
                while i < n {
                    let (a, b) = (ring[(i + n - 1) % n], ring[i]);
                    let mut hit = None;
                    for k in 3..=12.min(n.saturating_sub(3)) {
                        let (c, d) = (ring[(i + k) % n], ring[(i + k + 1) % n]);
                        let ahead = (c.0 - b.0) * (b.0 - a.0) + (c.1 - b.1) * (b.1 - a.1) > 0.0;
                        if ahead && off(c, a, b) < 1e-4 && off(d, a, b) < 1e-4 {
                            // the notch proper: from the last rim point
                            // before it leaves the line to the first after
                            let mut s0 = 0;
                            while s0 + 1 < k && off(ring[(i + s0 + 1) % n], a, b) < 1e-4 {
                                s0 += 1;
                            }
                            let run: Vec<P> = (s0..=k).map(|m| ring[(i + m) % n]).collect();
                            let ar = step::ring_area(&run).abs();
                            // a notch goes into the material: from inside
                            // one, the next notch's floor is on the same
                            // line, and the rim between looks like a notch
                            // out into the air
                            let q = run[run.len() / 2];
                            let into = ((b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0) > 0.0) == left;
                            if into && ar > 0.2 * least && ar < 5.0 * least {
                                hit = Some((k, run));
                            }
                            break;
                        }
                    }
                    match hit {
                        Some((k, run)) => {
                            for m in 1..k {
                                keep[(i + m) % n] = false;
                            }
                            slots.push(run);
                            i += k;
                        }
                        None => i += 1,
                    }
                }
                ring.into_iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| p).collect()
            };
            gr = vec![split(pts(pieces[0].exterior()), true)];
            for h in pieces[0].interiors() {
                let r = pts(h);
                // a hole too small to carry notches (a lyre's knot) is drawn
                // as the sheet drew it, below
                if step::ring_area(&r).abs() >= 50.0 * least {
                    gr.push(split(r, false));
                }
            }
            for h in new_rings[1..].iter().filter(|h| step::ring_area(h).abs() < 50.0 * least) {
                let hb = near(cen(h), &old_body_holes).and_then(|ob| near(r.apply(cen(&ob)), &sheet_holes).map(|sh| (step::ring_area(&ob).abs() - step::ring_area(&sh).abs()) / perim(&ob))).unwrap_or(burn);
                let hb = (hb * 1e4).round() / 1e4;
                gr.extend(sheets::rings(&sheets::grow(&poly(h), -hb, round)).into_iter().take(1));
            }
            if std::env::var("SC_NOTCH_DEBUG").is_ok() {
                eprintln!("  notches {}: least {least:.3}, {} slots split off", f.name, slots.len());
            }
            gr.extend(slots);
        } else {
            for h in &new_rings[1..] {
                let hb = near(cen(h), &old_body_holes).and_then(|ob| near(r.apply(cen(&ob)), &sheet_holes).map(|sh| (step::ring_area(&ob).abs() - step::ring_area(&sh).abs()) / perim(&ob))).unwrap_or(burn);
                let hb = (hb * 1e4).round() / 1e4;
                let shrunk = sheets::grow(&poly(h), -hb, round);
                gr.extend(sheets::rings(&shrunk).into_iter().take(1));
            }
        }
        for ring in gr.iter_mut() {
            for p in ring.iter_mut() {
                *p = rn.apply(*p);
            }
        }
        let outline = Item { pts: gr[0].clone(), src: None, shift: (0.0, 0.0), ..oldp.outline.clone() };
        // the old through-cuts, to take their inks from
        let old_holes: Vec<&Item> = oldp.inside.iter().filter(|i| i.closed && !i.fill && i.ink != "#0000ff").collect();
        let ring_c = |p: &[P]| {
            let n = p.len() as f64;
            (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n)
        };
        let mut inside: Vec<Item> = vec![];
        for h in &gr[1..] {
            let c = ring_c(h);
            let like = old_holes.iter().min_by(|a, b| {
                let (ca, cb) = (ring_c(&a.pts), ring_c(&b.pts));
                ((ca.0 - c.0).hypot(ca.1 - c.1)).partial_cmp(&((cb.0 - c.0).hypot(cb.1 - c.1))).unwrap()
            });
            let base = like.map(|i| (*i).clone()).unwrap_or_else(|| Item { ink: "#ff8000".into(), fill: false, width: oldp.outline.width.clone(), pts: vec![], closed: true, src: None, shift: (0.0, 0.0), el: usize::MAX });
            inside.push(Item { pts: h.clone(), closed: true, src: None, shift: (0.0, 0.0), ..base });
        }
        // pockets: the old fill that covers each old pocket gives the ink
        let fills: Vec<&Item> = oldp.inside.iter().filter(|i| i.fill).collect();
        let mut used_fill: Vec<usize> = vec![];
        for (k, (_, side, reg)) in g.pockets.iter().enumerate() {
            let Some((_, _, oreg)) = f.pockets.get(k) else { continue };
            let oc_ = {
                use geo::Centroid;
                let c = oreg.centroid().unwrap();
                r.apply((c.x(), c.y()))
            };
            let near = fills.iter().enumerate().filter(|(j, _)| !used_fill.contains(j)).min_by(|a, b| {
                let (ca, cb) = (ring_c(&a.1.pts), ring_c(&b.1.pts));
                ((ca.0 - oc_.0).hypot(ca.1 - oc_.1)).partial_cmp(&((cb.0 - oc_.0).hypot(cb.1 - oc_.1))).unwrap()
            });
            let Some((j, it)) = near else { continue };
            used_fill.push(j);
            for ring in sheets::rings(reg).iter().take(1) {
                inside.push(Item { pts: ring.iter().map(|&p| rn.apply(p)).collect(), closed: true, src: None, shift: (0.0, 0.0), ..(*it).clone() });
            }
            let _ = side;
        }
        // the rest: engraved lines and the like, held from the nearest corner
        // the old part's sides, and the same sides' bounds on the new one
        let dirs = side_directions(&oldp.outline.pts, 1.5 * f.thickness, 4.0 * f.thickness);
        let (lo, ln) = (bounds(&oldp.outline.pts, &dirs), bounds(&outline.pts, &dirs));
        // the middle by area: an average of the outline's points leans toward
        // wherever it has the most of them, as a curve sampled finely does
        let area_c = |p: &[P]| {
            use geo::Centroid;
            let c = geo::Polygon::new(geo::LineString::from(p.iter().map(|&(x, y)| geo::Coord { x, y }).collect::<Vec<_>>()), vec![]).centroid().unwrap();
            (c.x(), c.y())
        };
        let (om, nm) = (area_c(&oldp.outline.pts), area_c(&outline.pts));
        // what a mark can follow, as (where it was, where it is): the corners,
        // each where the same two sides meet on the new part; the holes, each to the
        // new one nearest it (a bevel line follows its window); and, failing
        // those, the middle
        let mut anchors: Vec<(P, P)> = vec![];
        // a corner is as far off its outline after as before: where several
        // bounds touch one vertex (a curved part's facets), two of them cross
        // anywhere on the part redrawn, tens of millimetres out (a lyre's
        // tuning block)
        let (or_, nr_) = (vec![oldp.outline.pts.clone()], vec![outline.pts.clone()]);
        for (oc1, i, j) in corners(&oldp.outline.pts, &lo) {
            if let Some(nc1) = cross_lines(ln[i], ln[j]) {
                if (sheets::to_rings(oc1, &or_) - sheets::to_rings(nc1, &nr_)).abs() < 0.5 {
                    anchors.push((oc1, nc1));
                }
            }
        }
        // the holes: a group of them (a rosette) moves as one, so a hole's
        // nearest new hole can be its neighbour's; pair them through the one
        // shift that best carries the old holes onto the new, found by laying
        // the biggest old hole on each new hole about its size
        let oh: Vec<(P, f64)> = old_holes.iter().map(|h| (area_c(&h.pts), step::ring_area(&h.pts).abs())).collect();
        let nh: Vec<(P, f64)> = gr[1..].iter().map(|r| (area_c(r), step::ring_area(r).abs())).collect();
        if let Some(big) = oh.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) {
            let miss = |t: P| oh.iter().map(|(c, _)| nh.iter().map(|(d, _)| (c.0 + t.0 - d.0).hypot(c.1 + t.1 - d.1)).fold(f64::MAX, f64::min)).sum::<f64>();
            let best = nh
                .iter()
                .filter(|(_, a)| (a - big.1).abs() <= 0.25 * big.1)
                .map(|(d, _)| (d.0 - big.0 .0, d.1 - big.0 .1))
                .min_by(|x, y| miss(*x).partial_cmp(&miss(*y)).unwrap());
            if let Some(t) = best {
                for (c, _) in &oh {
                    let at = (c.0 + t.0, c.1 + t.1);
                    if let Some((d, _)) = nh.iter().min_by(|a, b| ((a.0 .0 - at.0).hypot(a.0 .1 - at.1)).partial_cmp(&((b.0 .0 - at.0).hypot(b.0 .1 - at.1))).unwrap()) {
                        anchors.push((*c, *d));
                    }
                }
            }
        }
        if anchors.is_empty() {
            anchors.push((om, nm));
        }
        if std::env::var("SC_ANCHOR_DEBUG").map_or(false, |w| w == f.name) {
            for (o, nn) in &anchors {
                eprintln!("  anchor ({:.2},{:.2}) -> ({:.2},{:.2}), moves {:.3}", o.0, o.1, nn.0, nn.1, (nn.0 - o.0).hypot(nn.1 - o.1));
            }
            for (oc1, i, j) in corners(&oldp.outline.pts, &lo) {
                let ang = (lo[i].0 .0 * lo[j].0 .1 - lo[i].0 .1 * lo[j].0 .0).abs().asin().to_degrees();
                eprintln!("  corner ({:.2},{:.2}) between lines {i} and {j}, {ang:.2} deg apart, old h {:.3}/{:.3} new h {:.3}/{:.3}", oc1.0, oc1.1, lo[i].1, lo[j].1, ln[i].1, ln[j].1);
            }
        }
        let mut carried = 0;
        // each mark as a whole (a number is several strokes, each its own
        // element): strokes within a millimetre of each other go together,
        // and their centre picks the nearest anchor to follow
        let rest: Vec<&Item> = oldp.inside.iter().filter(|it| {
            let is_hole = it.closed && !it.fill && it.ink != "#0000ff";
            let is_used_fill = it.fill && fills.iter().position(|x| std::ptr::eq(*x, *it)).map_or(false, |k| used_fill.contains(&k));
            !is_hole && !is_used_fill
        }).collect();
        let bb: Vec<(f64, f64, f64, f64)> = rest.iter().map(|i| bbox(&i.pts)).collect();
        let mut group: Vec<usize> = (0..rest.len()).collect();
        fn root(g: &mut Vec<usize>, i: usize) -> usize {
            let mut r = i;
            while g[r] != r {
                r = g[r];
            }
            g[i] = r;
            r
        }
        for i in 0..rest.len() {
            for j in i + 1..rest.len() {
                let (a, b) = (bb[i], bb[j]);
                let gap = (a.0 - b.2).max(b.0 - a.2).max(a.1 - b.3).max(b.1 - a.3);
                if gap < 1.0 && rest[i].ink == rest[j].ink {
                    let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                    group[ri] = rj;
                }
            }
        }
        let mut els: Vec<usize> = vec![];
        for i in 0..rest.len() {
            let r = root(&mut group, i);
            if !els.contains(&r) {
                els.push(r);
            }
        }
        for el in els {
            let mine: Vec<&Item> = (0..rest.len()).filter(|&i| root(&mut group, i) == el).map(|i| rest[i]).collect();
            let all: Vec<P> = mine.iter().flat_map(|i| i.pts.iter().copied()).collect();
            let c = ring_c(&all);
            let (oc1, nc1) = *anchors.iter().min_by(|a, b| ((a.0 .0 - c.0).hypot(a.0 .1 - c.1)).partial_cmp(&((b.0 .0 - c.0).hypot(b.0 .1 - c.1))).unwrap()).unwrap();
            let d = (nc1.0 - oc1.0, nc1.1 - oc1.1);
            for it in mine {
                inside.push(it.moved(d));
            }
            carried += 1;
        }
        carried_total += carried;
        let holes_n = gr.len() - 1;
        let mut np = Part { outline, inside, cut: None };
        np.cut = sheets::as_cut(&sheets::drawn_rings(&np));
        sheets_v[si].parts[pi] = np;
        sheets_v[si].changed = true;
        report.push(format!(
            "  redrawn   {:24} {} (matched within {:.3} mm; kerf {:.3}{}; {} hole{}, {} pocket{}{})",
            f.name,
            sheets_v[si].path,
            mean,
            2.0 * burn,
            if round_on[&si] { ", round corners" } else { "" },
            holes_n,
            if holes_n == 1 { "" } else { "s" },
            g.pockets.len(),
            if g.pockets.len() == 1 { "" } else { "s" },
            if carried > 0 { format!(", {carried} engraving{} carried", if carried == 1 { "" } else { "s" }) } else { String::new() }
        ));
    }
    // the sheet parts no body is: the design does not model them (the
    // kalimba's optional stiffeners), so they stay as committed
    for &si in &touched {
        if !changed_names.contains_key(&si) {
            continue;
        }
        // parts, not bodies: an "-x2" part stands for two
        let held: HashSet<usize> = body_at.iter().filter_map(|b| b.filter(|b| b.0 == si).map(|b| b.1)).collect();
        let n = sheets_v[si].parts.len() - held.len();
        if n > 0 {
            report.push(format!("  kept      {n} part{} of {} that the design does not model, as committed", if n == 1 { "" } else { "s" }, sheets_v[si].path));
        }
    }
    // does every changed sheet still lie flat, every part clear of the others?
    let gap = 3.0;
    let mut extra: Vec<(usize, Sheet)> = vec![];
    for &si in &touched {
        let s = &mut sheets_v[si];
        if !changed_names.contains_key(&si) {
            continue;
        }
        let bbs: Vec<(f64, f64, f64, f64)> = s.parts.iter().map(|p| bbox(&p.outline.pts)).collect();
        let mut clash = vec![];
        for i in 0..bbs.len() {
            for j in i + 1..bbs.len() {
                let (a, b) = (bbs[i], bbs[j]);
                if a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3 && sheets::overlap(&s.parts[i].outline.pts, &s.parts[j].outline.pts) > 1e-3 {
                    clash.push((i, j));
                }
            }
        }
        let off = bbs.iter().any(|b| b.0 < s.x0 || b.1 < s.y0 || b.2 > s.x0 + s.w || b.3 > s.y0 + s.h);
        if !clash.is_empty() || off {
            // pack again: rows of parts, tallest first, as wide as the bed,
            // onto as many beds as it takes; a part too long for the bed's
            // depth but not its width lies turned a quarter
            let (bw, bh) = (600.0, 305.0);
            let mut too_big = vec![];
            for (i, p) in s.parts.iter_mut().enumerate() {
                let b = bbs[i];
                let (w, h) = (b.2 - b.0, b.3 - b.1);
                if w <= bw - gap && h <= bh - gap {
                    continue;
                }
                if h <= bw - gap && w <= bh - gap {
                    let c = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
                    for it in std::iter::once(&mut p.outline).chain(p.inside.iter_mut()) {
                        it.pts = it.pts.iter().map(|q| (c.0 - (q.1 - c.1), c.1 + (q.0 - c.0))).collect();
                        it.src = None;
                    }
                } else {
                    too_big.push(format!("{:.0} x {:.0} mm", w, h));
                }
            }
            if !too_big.is_empty() {
                die(format!("{}: {} part{} bigger than the {bw} x {bh} mm bed any way round ({}); nothing written", s.path, too_big.len(), if too_big.len() == 1 { " is" } else { "s are" }, too_big.join(", ")));
            }
            let bbs: Vec<(f64, f64, f64, f64)> = s.parts.iter().map(|p| bbox(&p.outline.pts)).collect();
            let mut order: Vec<usize> = (0..s.parts.len()).collect();
            order.sort_by(|&a, &b| (bbs[b].3 - bbs[b].1).partial_cmp(&(bbs[a].3 - bbs[a].1)).unwrap());
            let (mut x, mut y, mut row, mut bed) = (gap / 2.0, gap / 2.0, 0.0f64, 0usize);
            let mut moves = vec![(0.0, 0.0, 0usize); s.parts.len()];
            for &i in &order {
                let (w, h) = (bbs[i].2 - bbs[i].0, bbs[i].3 - bbs[i].1);
                if x + w > bw - gap / 2.0 && x > gap / 2.0 {
                    x = gap / 2.0;
                    y += row + gap;
                    row = 0.0;
                }
                if y + h > bh - gap / 2.0 && y > gap / 2.0 {
                    bed += 1;
                    x = gap / 2.0;
                    y = gap / 2.0;
                    row = 0.0;
                }
                moves[i] = (x - bbs[i].0, y - bbs[i].1, bed);
                x += w + gap;
                row = row.max(h);
            }
            let beds = bed + 1;
            let mut parts: Vec<Vec<Part>> = vec![vec![]; beds];
            for (i, p) in std::mem::take(&mut s.parts).into_iter().enumerate() {
                let (dx, dy, k) = moves[i];
                let mut p = p;
                for it in std::iter::once(&mut p.outline).chain(p.inside.iter_mut()) {
                    *it = it.moved((dx, dy));
                }
                parts[k].push(p);
            }
            report.push(format!(
                "  packed    {} again{}: {}",
                s.path,
                if beds > 1 { format!(" onto {beds} sheets") } else { String::new() },
                if off { "a part left the sheet".to_string() } else { format!("{} pair{} of parts overlapped", clash.len(), if clash.len() == 1 { "" } else { "s" }) }
            ));
            // a bed laid out from its corner, whatever the file's origin was
            let fit = |s: &mut Sheet| {
                let all: Vec<P> = s.parts.iter().flat_map(|p| p.outline.pts.iter().copied()).collect();
                let b = bbox(&all);
                (s.x0, s.y0) = (0.0, 0.0);
                s.w = b.2 + gap / 2.0;
                s.h = b.3 + gap / 2.0;
            };
            let mut rest = parts.into_iter();
            s.parts = rest.next().unwrap();
            if beds > 1 {
                s.loose.clear(); // what lay in no part stays on no sheet it could land on a part of
            }
            fit(s);
            for (k, ps) in rest.enumerate() {
                let n = k + 2;
                let path = if s.path.contains("sheet1") {
                    s.path.replacen("sheet1", &format!("sheet{n}"), 1)
                } else {
                    format!("{}-sheet{n}.svg", s.path.trim_end_matches(".svg"))
                };
                let mut t = Sheet { path, parts: ps, loose: vec![], ..s.clone() };
                t.title = format!("{}, sheet {n} of {beds}", t.title);
                fit(&mut t);
                extra.push((si, t));
            }
        }
    }
    let ename = ep.file_name().unwrap().to_string_lossy().to_string();
    let nch: usize = changed_names.values().map(|v| v.len()).sum();
    let mut touched = touched;
    for (si, t) in extra {
        sheets_v.push(t);
        touched.push(sheets_v.len() - 1);
        changed_names.insert(sheets_v.len() - 1, changed_names[&si].clone());
    }
    for &si in &touched {
        let s = &mut sheets_v[si];
        let dest = out.join(&s.path);
        if let Some(names) = changed_names.get(&si) {
            s.title = format!("{} (from {ename})", s.title);
            s.desc = format!(
                "{} Redrawn by sc-flatten from {ename}: {} drawn afresh from the solid (outline and through-cuts with the sheet's kerf, pockets in their engrave inks); engraved lines the design does not model carried over from the committed sheet.",
                s.desc,
                names.join(", ")
            );
        }
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, sheets::write(s)).unwrap_or_else(|e| die(format!("{}: {e}", dest.display())));
        report.push(format!("  wrote     {}", dest.display()));
    }
    for l in &report {
        println!("{l}");
    }
    println!("{} bodies: {} redrawn, {} unchanged; {} engraving{} carried", old.len(), nch, old.len() - nch, carried_total, if carried_total == 1 { "" } else { "s" });
}

