//! Build a cut-speed test ladder: identical squares, one colour per speed.
//!
//!     target/release/test-ladder OUT.svg [--speeds 15,20,25,30] [--square 15]
//!
//! Settings for a sheet of plywood are a property of that sheet, not of the species on
//! the invoice. Baltic birch from two suppliers, or the same supplier two packs apart,
//! will not cut at the same speed. A ladder answers the question in about ninety seconds
//! and a strip of scrap: cut it, push each square out from below, and the fastest one
//! that drops free unaided is the ceiling. Run production 15-20% slower than that.
//!
//! **Colour here is one speed per layer, not cut order.** Every other file in these
//! repositories uses colour to say *when* a stroke is cut; a ladder uses it to say *how
//! fast*, because that is the only handle xTool Studio and XCS give you — they split an
//! import into processing layers by colour, so a separate colour is the only way to hold
//! a separate speed field. Same colour would mean one layer and one speed, which is a
//! row of identical squares and no ladder at all.
//!
//! The inks are still the house cut stages, so `make-preview` renders a ladder and
//! `svg-stroke-check` reads it. Four rungs, because those are the four stages that
//! cut: green, orange, cyan, black. Blue engraves the labels, as it always does. Violet
//! is never emitted — it means skip, and a skipped rung is a missing answer.
//!
//! Four is also the better way to use a ladder. Bracket coarse, then bisect: 15/25/35/45
//! finds the decade, a second run over the winning pair finds the number. That lands
//! closer than six rungs guessed in one pass.
//!
//! Labels are seven-segment line art rather than text, so no font substitution on import
//! can turn the numbers into something else, or into nothing.
use lasermade_tools::{die, py, text};

const SQUARE: f64 = 15.0; // square edge, mm
const GAP: f64 = 5.0; // between squares
const MARGIN: f64 = 5.0; // sheet margin
const LABEL_GAP: f64 = 3.0; // square bottom -> label top
const DIGIT_W: f64 = 3.0; // seven-segment digit cell
const DIGIT_H: f64 = 5.0;
const DIGIT_GAP: f64 = 1.2; // between digits of one label
const DOT_W: f64 = 1.0; // decimal point cell
const HAIRLINE: f64 = 0.1;

// the house cut stages, in order. Blue engraves, violet skips — see the top.
const CUT_STAGES: [&str; 4] = ["#00ff00", "#ff8000", "#00ffff", "#000000"];
const ENGRAVE: &str = "#0000ff";

// a=top, b=upper right, c=lower right, d=bottom, e=lower left, f=upper left, g=middle
fn segments_of(ch: char) -> &'static str {
    match ch {
        '0' => "abcdef", '1' => "bc", '2' => "abged", '3' => "abgcd", '4' => "fgbc",
        '5' => "afgcd", '6' => "afgedc", '7' => "abc", '8' => "abcdefg", '9' => "abcdfg",
        _ => die(format!("  no seven-segment form for {ch:?}")),
    }
}

const NOTE: &str = "<!-- CUT-SPEED LADDER - colour is one speed per layer, NOT cut order.
     Each square carries the speed engraved beneath it. In xTool Studio or XCS:
     put the blue label layer FIRST and engrave it, so the numbers are already
     burned in on squares that later drop through the honeycomb; then set every
     coloured layer to cut at 100% power, one pass, air assist at maximum, and
     give each one the speed its own label shows. -->";

/// Trim the float so the file reads as millimetres and not as arithmetic.
fn fmt(n: f64) -> String {
    let s = format!("{n:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn segment(name: char, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let mid = y + h / 2.0;
    match name {
        'a' => (x, y, x + w, y),
        'b' => (x + w, y, x + w, mid),
        'c' => (x + w, mid, x + w, y + h),
        'd' => (x, y + h, x + w, y + h),
        'e' => (x, mid, x, y + h),
        'f' => (x, y, x, mid),
        _ => (x, mid, x + w, mid),
    }
}

/// Seven-segment digits for `value`, centred on cx, top edge at `top`.
///
/// A decimal point is a baseline tick in a narrow cell — 12.5 mm/s has to be
/// sayable, or the ladder cannot bisect below whole numbers.
fn label(value: f64, cx: f64, top: f64) -> String {
    let chars: Vec<char> = fmt(value).chars().collect();
    let cell = |ch: char| if ch == '.' { DOT_W } else { DIGIT_W };
    let mut width = 0.0;
    for &ch in &chars {
        width += cell(ch);
    }
    width += (chars.len() as f64 - 1.0) * DIGIT_GAP;
    let mut x = cx - width / 2.0;
    let mut out = Vec::new();
    for &ch in &chars {
        if ch == '.' {
            let y = top + DIGIT_H;
            out.push(format!("M {} {} L {} {}", fmt(x), fmt(y), fmt(x + DOT_W), fmt(y)));
        } else {
            for name in segments_of(ch).chars() {
                let (x1, y1, x2, y2) = segment(name, x, top, DIGIT_W, DIGIT_H);
                out.push(format!("M {} {} L {} {}", fmt(x1), fmt(y1), fmt(x2), fmt(y2)));
            }
        }
        x += cell(ch) + DIGIT_GAP;
    }
    out.join(" ")
}

fn build(speeds: &[f64], square: f64) -> (String, f64, f64) {
    let n = speeds.len() as f64;
    let strip = n * square + (n - 1.0) * GAP;
    let width = strip + 2.0 * MARGIN;
    let height = MARGIN + square + LABEL_GAP + DIGIT_H + MARGIN;

    let mut lines = vec![
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">",
            fmt(width), fmt(height), fmt(width), fmt(height)
        ),
        NOTE.to_string(),
    ];
    let mut labels = Vec::new();
    for (i, &speed) in speeds.iter().enumerate() {
        let x = MARGIN + i as f64 * (square + GAP);
        let ink = CUT_STAGES[i];
        lines.push(format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"{}\"/>   <!-- {} mm/s -->",
            fmt(x), fmt(MARGIN), fmt(square), fmt(square), py::repr(HAIRLINE), fmt(speed)
        ));
        labels.push(label(speed, x + square / 2.0, MARGIN + square + LABEL_GAP));
    }
    lines.push(format!(
        "  <path d=\"{}\" fill=\"none\" stroke=\"{ENGRAVE}\" stroke-width=\"{}\"/>",
        labels.join(" "),
        py::repr(HAIRLINE)
    ));
    lines.push("</svg>".into());
    lines.push(String::new());
    (lines.join("\n"), width, height)
}

fn speed_list(t: &str) -> Vec<f64> {
    let mut speeds = Vec::new();
    for part in t.split(',') {
        let Some(v) = py::float(part) else { die(format!("  not a speed: {}", py::str_repr(part.trim()))) };
        if v <= 0.0 {
            die(format!("  speed must be positive: {}", fmt(v)));
        }
        speeds.push(v);
    }
    if speeds.len() > CUT_STAGES.len() {
        die(format!(
            "  {} speeds, {} cutting stages to hold them.\n  Bracket coarse then bisect rather than reaching for violet, which means skip.",
            speeds.len(),
            CUT_STAGES.len()
        ));
    }
    for i in 0..speeds.len() {
        if speeds[..i].contains(&speeds[i]) {
            die("  a speed appears twice — two rungs, one answer");
        }
    }
    speeds
}

const USAGE: &str = "usage: test-ladder [-h] [--speeds SPEEDS] [--square SQUARE] out";

fn usage_error(msg: &str) -> ! {
    eprintln!("{USAGE}\ntest-ladder: error: {msg}");
    std::process::exit(2)
}

fn main() {
    let mut out: Option<String> = None;
    let mut speeds_arg = "15,20,25,30".to_string();
    let mut square = SQUARE;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut take = |name: &str| -> String {
            if let Some(v) = a.strip_prefix(&format!("{name}=")) {
                return v.to_string();
            }
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| usage_error(&format!("argument {name}: expected one argument")))
        };
        if a == "-h" || a == "--help" {
            println!("{USAGE}\n\npositional arguments:\n  out\n\noptional arguments:\n  -h, --help       show this help message and exit\n  --speeds SPEEDS  mm/s, comma separated, up to four\n  --square SQUARE  square edge in mm");
            return;
        } else if a == "--speeds" || a.starts_with("--speeds=") {
            speeds_arg = take("--speeds");
        } else if a == "--square" || a.starts_with("--square=") {
            let v = take("--square");
            square = py::float(&v).unwrap_or_else(|| usage_error(&format!("argument --square: invalid float value: {}", py::str_repr(&v))));
        } else if a.starts_with('-') && a.len() > 1 {
            usage_error(&format!("unrecognized arguments: {a}"));
        } else if out.is_none() {
            out = Some(a.clone());
        } else {
            usage_error(&format!("unrecognized arguments: {a}"));
        }
        i += 1;
    }
    let Some(out) = out else { usage_error("the following arguments are required: out") };
    let out = text::pathstr(&out);
    let speeds = speed_list(&speeds_arg);
    let (svg, width, height) = build(&speeds, square);
    if let Err(e) = lasermade_tools::well_formed(&svg) {
        die(format!("  not well-formed: {e}"));
    }
    let inks = &CUT_STAGES[..speeds.len()];
    std::fs::write(&out, &svg).unwrap_or_else(|e| die(format!("  {out}: {e}")));
    println!("  {out}   {} x {} mm", fmt(width), fmt(height));
    for (speed, ink) in speeds.iter().zip(inks) {
        println!("    {:>5} mm/s   {ink}", fmt(*speed));
    }
    println!("    labels        {ENGRAVE}  engrave, run first");
}
