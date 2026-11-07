//! Build a display rendering of a cut file: readable on a page, never cuttable.
//!
//!     target/release/make-preview CUTFILE.svg [OUT.svg]        # default previews/<name>
//!     target/release/make-preview CUTFILE.svg --ground=#faf7f0  # another ground colour
//!
//! A cut file draws hairlines on no background. In a browser that is a nearly invisible
//! line on a transparency checkerboard, and on a dark page a black one disappears
//! entirely. The preview thickens the stroke, paints a light ground, and darkens the
//! inks that cannot be seen against it.
//!
//! **The colours change, and that is the point.** Three of the six cut-order inks fail
//! the WCAG 3:1 graphics minimum on a white ground — green at 1.37:1, cyan at 1.25:1,
//! orange at 2.52:1 — so a faithful rendering of the palette is an unreadable picture.
//! The darkened equivalents keep the same hue and the same order and clear 5:1. The
//! cut file keeps the exact values the laser keys on; this one is for looking at.
//!
//! Geometry, sheet position and stroke *order* are untouched, and the result is checked
//! against the source before it is written.
use lasermade_tools::re::{findall1, first, g, rx, sub, sub_n, sub_str};
use lasermade_tools::{die, py, text};

const GROUND: &str = "#ffffff";

// hue preserved, luminance dropped until each clears 5:1 on the white ground above
// (chosen against the old cream, where the lowest was orange at 4.83:1)
const DARKEN: [(&str, &str); 6] = [
    ("#00ff00", "#0a7a12"), // green   1.37:1 -> 5.52:1
    ("#00ffff", "#00706e"), // cyan    1.25:1 -> 5.93:1
    ("#ff8000", "#a85800"), // orange  2.52:1 -> 5.17:1
    ("#8000ff", "#6a00cc"), // violet  6.25:1 -> 8.45:1, for consistency of weight
    ("#0000ff", "#0000ff"), // blue    8.59:1, already fine
    ("#000000", "#000000"), // black  21.00:1
];

const NOTE: &str = "<!-- DISPLAY ONLY - not a cut file.
     The stroke is thickened, a light ground painted in, and the lightest inks
     darkened: green, cyan and orange sit below the 3:1 graphics minimum against any
     light background, so the cut order cannot be read from a faithful rendering. Hue
     and sequence are unchanged. Geometry and sheet position are untouched, and the
     CUT FILE keeps the exact values your laser keys on.
     Cut the file in the repository root, not this one. -->";

const DATA: &str = r#"\bd="([^"]+)""#;
const VIEWBOX: &str = r#"viewBox="([^"]+)""#;

fn build(src: &str, ground: &str) -> (String, f64) {
    let (stroke_fraction, min_stroke) = (0.002, 0.05);
    let Some(vb) = first(&rx(VIEWBOX), src) else {
        die("  no viewBox — cannot size the ground or the stroke")
    };
    let v: Vec<f64> = g(&vb, 1)
        .replace(',', " ")
        .split_whitespace()
        .map(|s| py::float(s).unwrap_or_else(|| die(format!("could not convert string to float: '{s}'"))))
        .collect();
    if v.len() != 4 {
        die("viewBox wants four numbers");
    }
    let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
    let stroke = py::max(py::max(w, h) * stroke_fraction, min_stroke);

    let rect = format!(
        "\n  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{ground}\"/>",
        py::repr(x), py::repr(y), py::repr(w), py::repr(h)
    );
    let mut out = sub_n(&rx(r"(?s)<svg\b[^>]*?>"), src, 1, |c| format!("{}\n{NOTE}{rect}", g(c, 0)));

    // a hairline hint overrides the width we just set, so it has to go
    out = sub_str(&rx(r"-inkscape-stroke:\s*hairline;?"), &out, "");
    out = sub_str(&rx(r"vector-effect:\s*non-scaling-stroke;?"), &out, "");
    out = sub_str(&rx(r#"\svector-effect="non-scaling-stroke""#), &out, "");

    out = sub_str(&rx(r"stroke-width:\s*[\d.]+"), &out, &format!("stroke-width:{stroke:.3}"));
    out = sub_str(&rx(r#"stroke-width="[\d.]+""#), &out, &format!("stroke-width=\"{stroke:.3}\""));

    for (raw, dark) in DARKEN {
        if raw == dark {
            continue;
        }
        out = sub(&rx(&format!(r"(?i)(stroke\s*:\s*){raw}")), &out, |c| format!("{}{dark}", g(c, 1)));
        out = sub(&rx(&format!(r#"(?i)(stroke\s*=\s*"){raw}""#)), &out, |c| format!("{}{dark}\"", g(c, 1)));
    }
    (out, stroke)
}

fn main() {
    // Run bare this used to die on a missing argument. The ground was cream
    // (#faf7f0) until 2026-10-13 and is white now, for every repository.
    // --ground sets another; a darker one can push the darkened inks below the
    // contrast DARKEN was chosen for.
    let mut ground = GROUND.to_string();
    let mut args: Vec<String> = Vec::new();
    for a in std::env::args().skip(1) {
        if let Some(gr) = a.strip_prefix("--ground=") {
            ground = gr.to_string();
            let ok = ground.len() == 7 && ground.starts_with('#') && ground[1..].chars().all(|c| c.is_ascii_hexdigit());
            if !ok {
                die(format!("make-preview: --ground wants #rrggbb, not {ground}"));
            }
        } else {
            args.push(a);
        }
    }
    if args.is_empty() {
        die("usage: make-preview CUTFILE.svg [OUTPUT.svg] [--ground=#rrggbb]");
    }
    let src_path = text::pathstr(&args[0]);
    if !std::path::Path::new(&src_path).exists() {
        die(format!("make-preview: no such file: {src_path}"));
    }
    let dst_path = if args.len() > 1 {
        text::pathstr(&args[1])
    } else {
        let p = std::path::Path::new(&src_path);
        let parent = p.parent().map(|q| q.to_string_lossy().into_owned()).unwrap_or_default();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        text::join(&text::join(&parent, "previews"), &name)
    };
    let src = text::read_strict(&src_path).unwrap_or_else(|e| die(e));
    let (out, stroke) = build(&src, &ground);

    if findall1(&rx(DATA), &out, 1) != findall1(&rx(DATA), &src, 1) {
        die(format!("  {src_path}: path data changed — refusing to write"));
    }
    let vb = |s: &str| first(&rx(VIEWBOX), s).map(|c| g(&c, 1).to_string());
    if vb(&out) != vb(&src) {
        die(format!("  {src_path}: viewBox changed — refusing to write"));
    }
    if let Err(e) = lasermade_tools::well_formed(&out) {
        die(format!("make-preview: {src_path}: not well-formed XML: {e}"));
    }

    if let Some(parent) = std::path::Path::new(&dst_path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir(parent) {
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    die(format!("make-preview: {}: {e}", parent.display()));
                }
            }
        }
    }
    std::fs::write(&dst_path, &out).unwrap_or_else(|e| die(format!("make-preview: {dst_path}: {e}")));
    println!("  {dst_path}   stroke {stroke:.3}");
}
