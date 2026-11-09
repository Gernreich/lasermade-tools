//! `sc-compare DIR_A DIR_B [--tol=MM] [--mark-tol=MM]` (2026-11-08): two sets of cut sheets
//! for one design, part by part. sc-flatten and sc-replay draw a SolidCraft
//! edit two ways, from the solid and through the generator, and neither is
//! checked until they agree; this is that check.
//!
//! Every sheet at the same path under both is read as its parts (each black
//! outline with what is drawn inside it). The parts are matched by shape,
//! any turn or mirror, wherever they sit on the sheet, since the two may
//! pack differently. For each pair it gives the farthest any cut line of
//! either lies from the other's (outline and through-cuts, both ways) and
//! the items it carries in each ink, and how far its marks (engraved lines,
//! fills) lie from the other's. It passes when every part has its match,
//! every pair's cuts are within --tol (default 0.05 mm) and its marks within
//! --mark-tol (default 0.5 mm: a number engraved half a millimetre over is
//! the same number), and it carries the same number of items in each ink.
use lasermade_tools::scedit::die;
use lasermade_tools::sheets;

fn svgs(root: &std::path::Path, d: &std::path::Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            svgs(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "svg") {
            out.push(p.strip_prefix(root).unwrap().to_string_lossy().to_string());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut tol = 0.05;
    let mut mark_tol = 0.5;
    let mut worst_marks: f64 = 0.0;
    let mut dirs = vec![];
    for a in &args {
        if let Some(v) = a.strip_prefix("--mark-tol=") {
            mark_tol = v.parse().unwrap_or_else(|_| die(format!("{a}: not a number")));
        } else if let Some(v) = a.strip_prefix("--tol=") {
            tol = v.parse().unwrap_or_else(|_| die(format!("{a}: not a number")));
        } else if a.starts_with('-') {
            die(format!("{a}? It takes DIR_A DIR_B, --tol=MM and --mark-tol=MM."));
        } else {
            dirs.push(std::path::PathBuf::from(a));
        }
    }
    if dirs.len() != 2 {
        die("sc-compare DIR_A DIR_B [--tol=MM]: two directories of cut sheets, as sc-flatten and sc-replay write them");
    }
    let (mut la, mut lb) = (vec![], vec![]);
    svgs(&dirs[0], &dirs[0], &mut la);
    svgs(&dirs[1], &dirs[1], &mut lb);
    la.sort();
    lb.sort();
    let mut bad = 0;
    // every part of every sheet, pooled: the two may pack them onto
    // different sheets, so a part is matched wherever it lies
    let pool = |dir: &std::path::Path, files: &[String]| -> Vec<(String, sheets::Part)> {
        let mut v = vec![];
        for f in files {
            let s = sheets::read(&dir.join(f).to_string_lossy()).unwrap_or_else(|e| die(e));
            for p in s.parts {
                v.push((f.clone(), p));
            }
        }
        v
    };
    let (sa, sb) = (pool(&dirs[0], &la), pool(&dirs[1], &lb));
    let a = sheets::Sheet { path: String::new(), x0: 0.0, y0: 0.0, w: 0.0, h: 0.0, title: String::new(), desc: String::new(), inks: vec![], parts: sa.iter().map(|x| x.1.clone()).collect(), loose: vec![], text: String::new(), changed: false };
    let b = sheets::Sheet { parts: sb.iter().map(|x| x.1.clone()).collect(), ..a.clone() };
    println!("{} parts on {} sheet{} and {} on {}", a.parts.len(), la.len(), if la.len() == 1 { "" } else { "s" }, b.parts.len(), lb.len());
    let mut worst_all: f64 = 0.0;
    let mut pairs = 0;
    {
        // mirror twins are one shape: what they carry, and which way round
        // most parts lie, tells them apart
        let fas: Vec<_> = a.parts.iter().map(|pa| sheets::part_flat(pa, "a")).collect();
        // the marks a part carries (engraved lines, fills), each with its ink
        let marks = |p: &sheets::Part, mv: Option<sheets::Rigid>| -> Vec<(String, Vec<sheets_p::P>)> {
            p.inside
                .iter()
                .filter(|i| !(i.closed && !i.fill && i.ink != "#0000ff"))
                .map(|i| {
                    // resampled every quarter millimetre, so a long straight side
                    // is measured along its length and not only at its ends
                    let mut pts: Vec<sheets_p::P> = i.pts.iter().map(|&q| mv.map_or(q, |r| r.apply(q))).collect();
                    if i.closed && pts.len() > 2 {
                        pts.push(pts[0]);
                    }
                    let mut out = vec![];
                    for w in pts.windows(2) {
                        let n = ((w[1].0 - w[0].0).hypot(w[1].1 - w[0].1) / 0.25).ceil().max(1.0) as usize;
                        for k in 0..n {
                            let t = k as f64 / n as f64;
                            out.push((w[0].0 + t * (w[1].0 - w[0].0), w[0].1 + t * (w[1].1 - w[0].1)));
                        }
                    }
                    out.extend(pts.last().copied());
                    (i.ink.clone(), out)
                })
                .collect()
        };
        let far = |x: &[(String, Vec<sheets_p::P>)], y: &[(String, Vec<sheets_p::P>)]| {
            x.iter()
                .flat_map(|(ink, pts)| {
                    let theirs: Vec<sheets_p::P> = y.iter().filter(|(k, _)| k == ink).flat_map(|(_, p)| p.iter().copied()).collect();
                    pts.iter().map(move |p| theirs.iter().map(|q| (p.0 - q.0).hypot(p.1 - q.1)).fold(f64::MAX, f64::min)).collect::<Vec<_>>()
                })
                .fold(0.0, f64::max)
        };
        let mark_gap = |pa: &sheets::Part, pb: &sheets::Part, r: sheets::Rigid| {
            let (ma, mb) = (marks(pa, Some(r)), marks(pb, None));
            if ma.is_empty() && mb.is_empty() { 0.0 } else { far(&ma, &mb).max(far(&mb, &ma)) }
        };
        let all = |only: Option<bool>| {
            let mut v: Vec<(f64, usize, usize, sheets::Rigid)> = vec![];
            for (i, fa) in fas.iter().enumerate() {
                for (j, pb) in b.parts.iter().enumerate() {
                    let ways = sheets::register_all(fa, pb, only);
                    let Some(best) = ways.first().map(|w| w.0) else { continue };
                    // of the ways that fit as well, the one whose marks agree
                    // (and of those, a symmetric part's, the one not turned
                    // over and turned least: else it lies whichever way, and
                    // the count of which way most parts lie goes astray)
                    let (gap, mean, r) = ways
                        .iter()
                        .filter(|w| w.0 < best + 0.01)
                        .map(|w| (mark_gap(&a.parts[i], pb, w.2), w.0, w.2))
                        .min_by(|x, y| {
                            let k = |w: &(f64, f64, sheets::Rigid)| (w.0, w.2.mirror, w.2.s.atan2(w.2.c).abs());
                            let (p, q) = (k(x), k(y));
                            p.0.partial_cmp(&q.0).unwrap().then(p.1.cmp(&q.1)).then(p.2.partial_cmp(&q.2).unwrap())
                        })
                        .unwrap();
                    let inks = if sheets::ink_counts(&a.parts[i]) == sheets::ink_counts(pb) { 0.0 } else { 0.5 };
                    // near-twins (a flute's seven walls) fit each other almost
                    // as well as themselves: a part found where the other
                    // sheet's part lies, on the sheet of the same name, is
                    // that part, so a small pull towards the same spot
                    let here = |p: &sheets::Part| {
                        let o = &p.outline.pts;
                        let (x0, y0, x1, y1) = o.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |s, q| (s.0.min(q.0), s.1.min(q.1), s.2.max(q.0), s.3.max(q.1)));
                        ((x0 + x1) / 2.0, (y0 + y1) / 2.0)
                    };
                    let place = if sa[i].0 == sb[j].0 {
                        let (p, q) = (here(&a.parts[i]), here(pb));
                        0.05 * ((p.0 - q.0).hypot(p.1 - q.1) / 5.0).min(1.0)
                    } else {
                        0.05
                    };
                    v.push((mean + inks + 0.01 * gap.min(50.0) + place, i, j, r));
                }
            }
            v
        };
        let mut cands = all(None);
        let near: Vec<_> = cands.iter().filter(|c| c.0 < 0.1).collect();
        let way = near.iter().filter(|c| c.3.mirror).count() * 2 > near.len();
        for c in cands.iter_mut() {
            if c.3.mirror != way {
                c.0 += 0.25;
            }
        }
        cands.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        let (mut ua, mut ub) = (vec![false; a.parts.len()], vec![false; b.parts.len()]);
        let mut matched: Vec<(usize, usize, sheets::Rigid)> = vec![];
        for (_, i, j, r) in cands {
            if !ua[i] && !ub[j] {
                ua[i] = true;
                ub[j] = true;
                matched.push((i, j, r));
            }
        }
        matched.sort_by_key(|m| m.0);
        for (i, j, r) in matched {
            let (pa, pb) = (&a.parts[i], &b.parts[j]);
            // both drawn alike, so settle the move on them (a near-round
            // plate's turn is barely held by its outline)
            let r = sheets::settle(r, &sheets::part_rings(pa), &sheets::part_rings(pb));
            let rb = sheets::part_rings(pb);
            let apart = |r: &sheets::Rigid| {
                let ra: Vec<Vec<sheets_p::P>> = sheets::part_rings(pa).iter().map(|ring| ring.iter().map(|&p| r.apply(p)).collect()).collect();
                let ab = ra.iter().flatten().map(|&p| sheets::to_rings(p, &rb)).fold(0.0, f64::max);
                let ba = rb.iter().flatten().map(|&p| sheets::to_rings(p, &ra)).fold(0.0, f64::max);
                (ab.max(ba), ra)
            };
            // the move settles the mean; on a sheet of the same name the part
            // left where it lies is a move too, and may put the worst point
            // closer (a foot piece's arc, polygon on one side, true on the other)
            let still = sheets::Rigid { c: 1.0, s: 0.0, mirror: false, t: (0.0, 0.0) };
            let r = if sa[i].0 == sb[j].0 && apart(&still).0 < apart(&r).0 { still } else { r };
            let (w, ra) = apart(&r);
            if std::env::var("SC_COMPARE_DEBUG").is_ok() {
                eprintln!("      move A to B: turn {:.4} deg, mirror {}, shift ({:.3},{:.3})", r.s.atan2(r.c).to_degrees(), r.mirror, r.t.0, r.t.1);
            }
            if std::env::var("SC_COMPARE_DEBUG").is_ok() && w > tol {
                // where on the part the cut lines part company, from B's outline's lower-left corner
                let (x0, y0) = rb[0].iter().fold((f64::MAX, f64::MAX), |s, p| (s.0.min(p.0), s.1.min(p.1)));
                let (x1, y1) = rb[0].iter().fold((f64::MIN, f64::MIN), |s, p| (s.0.max(p.0), s.1.max(p.1)));
                if let Some(p) = ra.iter().flatten().max_by(|p, q| sheets::to_rings(**p, &rb).partial_cmp(&sheets::to_rings(**q, &rb)).unwrap()) {
                    eprintln!("      A's cut at ({:.2},{:.2}) of B's {:.2} x {:.2} is {:.3} mm from B's", p.0 - x0, p.1 - y0, x1 - x0, y1 - y0, sheets::to_rings(*p, &rb));
                }
                if let Some(p) = rb.iter().flatten().max_by(|p, q| sheets::to_rings(**p, &ra).partial_cmp(&sheets::to_rings(**q, &ra)).unwrap()) {
                    eprintln!("      B's cut at ({:.2},{:.2}) of B's {:.2} x {:.2} is {:.3} mm from A's", p.0 - x0, p.1 - y0, x1 - x0, y1 - y0, sheets::to_rings(*p, &ra));
                }
            }
            worst_all = worst_all.max(w);
            pairs += 1;
            let (ca, cb) = (sheets::ink_counts(pa), sheets::ink_counts(pb));
            let inks_ok = ca == cb;
            let (ma, mb) = (marks(pa, Some(r)), marks(pb, None));
            let mw = if ma.is_empty() && mb.is_empty() { 0.0 } else { far(&ma, &mb).max(far(&mb, &ma)) };
            if std::env::var("SC_COMPARE_DEBUG").is_ok() {
                let oc = sheets::part_rings(pb)[0].iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
                let n = sheets::part_rings(pb)[0].len() as f64;
                let oc = (oc.0 / n, oc.1 / n);
                for (ink, pts) in &ma {
                    let one = |x: &[(String, Vec<sheets_p::P>)]| far(&[(ink.clone(), pts.clone())], x);
                    let d = one(&mb);
                    if d > mark_tol {
                        let c = pts.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
                        let k = pts.len() as f64;
                        eprintln!("      A mark {ink} at ({:.1},{:.1}) from B's part centre is {d:.2} mm from B's", c.0 / k - oc.0, c.1 / k - oc.1);
                    }
                }
                for (ink, pts) in &mb {
                    let d = far(&[(ink.clone(), pts.clone())], &ma);
                    if d > mark_tol {
                        let c = pts.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
                        let k = pts.len() as f64;
                        eprintln!("      B mark {ink} at ({:.1},{:.1}) from B's part centre is {d:.2} mm from A's", c.0 / k - oc.0, c.1 / k - oc.1);
                    }
                }
            }
            worst_marks = worst_marks.max(mw);
            let ok = w <= tol && inks_ok && ra.len() == rb.len() && mw <= mark_tol;
            if !ok {
                bad += 1;
            }
            let fmt = |c: &[(String, usize)]| c.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", ");
            println!(
                "  {}  {} part {} ~ {} part {}  {} cut ring{}, apart by at most {:.4} mm; marks by {:.3} mm{}",
                if ok { "pass" } else { "FAIL" },
                sa[i].0,
                i + 1,
                sb[j].0,
                j + 1,
                rb.len(),
                if rb.len() == 1 { "" } else { "s" },
                w,
                mw,
                if inks_ok { String::new() } else { format!("; inks differ: {} against {}", fmt(&ca), fmt(&cb)) }
            );
        }
        for (i, u) in ua.iter().enumerate() {
            if !u {
                bad += 1;
                println!("  FAIL  {} part {} of {} has no match", sa[i].0, i + 1, dirs[0].display());
            }
        }
        for (j, u) in ub.iter().enumerate() {
            if !u {
                bad += 1;
                println!("  FAIL  {} part {} of {} has no match", sb[j].0, j + 1, dirs[1].display());
            }
        }
    }
    println!(
        "{pairs} part pair{}, cuts at most {worst_all:.4} mm apart, marks {worst_marks:.3} mm; {}",
        if pairs == 1 { "" } else { "s" },
        if bad == 0 { format!("they agree (cuts within {tol} mm, marks within {mark_tol} mm)") } else { format!("{bad} FAIL") }
    );
    if bad > 0 {
        std::process::exit(1);
    }
}

mod sheets_p {
    pub type P = (f64, f64);
}
