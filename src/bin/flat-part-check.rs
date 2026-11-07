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
use lasermade_tools::py::{self, dist, max, min};
use lasermade_tools::re::{rx, split};
use lasermade_tools::svg::*;
use lasermade_tools::{die, text};
use std::collections::BTreeSet;

const BED_W: f64 = 600.0; // xTool P2S work area, mm (cut-tested 2026-10-19)
const BED_H: f64 = 305.0;
const MIN_EDGE: f64 = 3.0; // mm of material between a hole and any other edge
const MIN_HOLE: f64 = 2.0; // mm across, the smallest hole worth cutting
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




// --- transforms -------------------------------------------------------------
// (a, b, c, d, e, f) as in SVG: x' = a x + c y + e,  y' = b x + d y + f






// --- path flattening --------------------------------------------------------







// --- shapes -----------------------------------------------------------------








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
    let r = Rx::new();
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
