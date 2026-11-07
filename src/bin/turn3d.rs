//! The 3D page for a generator that draws its own parts.
//!
//!     target/release/panpipes ...   # runs: turn3d --out=PAGE.html < spec.json
//!
//! The spec is JSON on standard input:
//!
//!     {"title": ..., "note": ..., "by": ..., "spin": false,
//!      "designs": {name: {"parts": [{"o", "h", "e", "m", "x", "w"}], "ex", "view", ...}}}
//!
//! Each part is an outline `o`, holes `h` and engraving `e` in millimetres,
//! y up; `m` is its 4x4 placing matrix, or a list of matrices to chain left
//! to right; `x` is its explode direction (default [0, 0, 0]) and `w` its
//! wood (default "ply"). Every other key of a design goes to the page as it
//! is. This is what turn3d.py's placed() and write() did when the generators
//! imported it; the page comes out byte for byte the same.
use lasermade_tools::die;
use lasermade_tools::turn3d::{chain, placed_raw, write, Mat, J, STILL};
use std::io::Read;

fn mat(j: &J) -> Mat {
    j.arr().iter().map(J::nums).collect()
}

fn main() {
    let out = lasermade_tools::turn3d::out_arg("");
    if out.is_empty() {
        die("usage: turn3d --out=PAGE.html < spec.json");
    }
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src).unwrap_or_else(|e| die(e));
    let spec = J::parse(&src).unwrap_or_else(|e| die(e));
    let mut designs: Vec<(String, J)> = vec![];
    if let Some(J::O(ds)) = spec.get("designs") {
        for (name, d) in ds {
            let mut d = d.clone();
            let parts: Vec<J> = d
                .get("parts")
                .map(J::arr)
                .unwrap_or(&[])
                .iter()
                .map(|p| {
                    let m = p.get("m").unwrap_or_else(|| die("a part without m"));
                    // a list of matrices, chained, or one matrix
                    let m = if matches!(m.arr().first().and_then(|r| r.arr().first()), Some(J::A(_))) {
                        chain(&m.arr().iter().map(mat).collect::<Vec<_>>())
                    } else {
                        mat(m)
                    };
                    let x = p.get("x").map(J::nums).unwrap_or(STILL.to_vec());
                    let w = p.get("w").map(J::str).unwrap_or("ply");
                    let h: Vec<_> = p.get("h").map(J::arr).unwrap_or(&[]).iter().map(J::pts).collect();
                    let e: Vec<_> = p.get("e").map(J::arr).unwrap_or(&[]).iter().map(J::pts).collect();
                    placed_raw(&p.get("o").unwrap().pts(), &h, &e, &m, &x, w)
                })
                .collect();
            d.set("parts", J::A(parts));
            designs.push((name.clone(), d));
        }
    }
    let s = |k: &str| spec.get(k).map(J::str).unwrap_or("").to_string();
    let spin = matches!(spec.get("spin"), Some(J::B(true)));
    write(&out, &s("title"), &mut designs, &s("note"), &s("by"), spin);
}
