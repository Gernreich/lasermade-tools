//! Writing SolidCraft command scripts: what every instrument's parametric
//! design is made of (2026-11-08). A script is a list of commands that
//! `solidcraft-cli run SCRIPT --save X.sc` builds into a design, or
//! `--out X.step` into a STEP file; the generators write one per build.
//!
//! Every size in a script is an expression over its parameters, so the
//! design rebuilds when one changes. A plain number is fixed.

/// A number as the shortest text that reads back as it, -0 as 0.
pub fn n(v: f64) -> String {
    let v = if v.abs() < 5e-13 { 0.0 } else { v };
    format!("{v}")
}

/// A JSON string.
pub fn q(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A list of expressions as a JSON array.
pub fn qs<S: AsRef<str>>(v: &[S]) -> String {
    format!("[{}]", v.iter().map(|s| q(s.as_ref())).collect::<Vec<_>>().join(","))
}

pub fn v3(a: [f64; 3]) -> String {
    format!("[{},{},{}]", n(a[0]), n(a[1]), n(a[2]))
}

#[derive(Default)]
pub struct Script {
    pub c: Vec<String>,
    nsk: usize,
}

impl Script {
    pub fn new() -> Self {
        Self::default()
    }

    /// A command and its params, already JSON.
    pub fn cmd(&mut self, name: &str, params: String) {
        self.c.push(format!("{{\"command\":\"{name}\",\"params\":{params}}}"));
    }

    /// A length parameter (expr carries its unit, "3 mm").
    pub fn len(&mut self, name: &str, expr: impl AsRef<str>, comment: &str) {
        self.cmd("parameters.add", format!("{{\"name\":{},\"expression\":{},\"comment\":{}}}", q(name), q(expr.as_ref()), q(comment)));
    }

    /// A unitless parameter: a count, a ratio, a frequency.
    pub fn num(&mut self, name: &str, expr: impl AsRef<str>, comment: &str) {
        self.cmd("parameters.add", format!("{{\"name\":{},\"expression\":{},\"unit\":\"\",\"comment\":{}}}", q(name), q(expr.as_ref()), q(comment)));
    }

    /// An angle parameter ("30 deg").
    pub fn angle(&mut self, name: &str, expr: impl AsRef<str>, comment: &str) {
        self.cmd("parameters.add", format!("{{\"name\":{},\"expression\":{},\"unit\":\"deg\",\"comment\":{}}}", q(name), q(expr.as_ref()), q(comment)));
    }

    /// An axis-aligned box from `corner`, `size` long in x, y, z. op is
    /// "new" (a body named `name`), "join" or "cut" into `targets`.
    pub fn bx<S: AsRef<str>>(&mut self, name: &str, corner: [S; 3], size: [S; 3], op: &str, targets: &[&str]) {
        let body = if op == "new" { format!(",\"body_name\":{}", q(name)) } else { String::new() };
        let tg = if targets.is_empty() { String::new() } else { format!(",\"targets\":{}", qs(targets)) };
        self.cmd(
            "solid.box",
            format!(
                "{{\"corner\":{},\"length\":{},\"width\":{},\"height\":{},\"operation\":\"{op}\"{tg},\"name\":{}{body}}}",
                qs(&corner),
                q(size[0].as_ref()),
                q(size[1].as_ref()),
                q(size[2].as_ref()),
                q(name)
            ),
        );
    }

    /// Repeat `feature` `count` times, `spacing` apart along `dir`. A count
    /// must be 1 or more; for "none", make the feature miss its target.
    pub fn pattern(&mut self, feature: &str, dir: [f64; 3], count: impl AsRef<str>, spacing: impl AsRef<str>) {
        self.cmd(
            "solid.pattern.rectangular",
            format!(
                "{{\"features\":[{}],\"dir1\":{},\"count1\":{},\"spacing1\":{},\"name\":{}}}",
                q(feature),
                v3(dir),
                q(count.as_ref()),
                q(spacing.as_ref()),
                q(&format!("{feature} pattern"))
            ),
        );
    }

    /// A sketch on a plane given as origin and two directions (world
    /// numbers), lifted `offset` along its normal; returns its name.
    pub fn sketch_on(&mut self, origin: [f64; 3], x_dir: [f64; 3], y_dir: [f64; 3], offset: Option<&str>, name: &str) -> String {
        self.nsk += 1;
        let off = offset.map_or(String::new(), |o| format!(",\"offset\":{}", q(o)));
        self.cmd("sketch.create", format!("{{\"plane\":{{\"origin\":{},\"x_dir\":{},\"y_dir\":{}}}{off},\"name\":{}}}", v3(origin), v3(x_dir), v3(y_dir), q(name)));
        name.to_string()
    }

    /// A sketch on a named plane ("XY", "XZ", "YZ"), lifted `offset`.
    pub fn sketch(&mut self, plane: &str, offset: Option<&str>, name: &str) -> String {
        self.nsk += 1;
        let off = offset.map_or(String::new(), |o| format!(",\"offset\":{}", q(o)));
        self.cmd("sketch.create", format!("{{\"plane\":\"{plane}\"{off},\"name\":{}}}", q(name)));
        name.to_string()
    }

    /// A closed polyline of expression points in the open sketch.
    pub fn poly<S: AsRef<str>>(&mut self, pts: &[[S; 2]]) {
        let p: Vec<String> = pts.iter().map(|[a, b]| format!("[{},{}]", q(a.as_ref()), q(b.as_ref()))).collect();
        self.cmd("sketch.line", format!("{{\"points\":[{}],\"closed\":true}}", p.join(",")));
    }

    pub fn finish(&mut self) {
        self.cmd("sketch.finish", "{}".into());
    }

    /// Extrude a sketch `dist` (negative: against its normal). op as bx.
    pub fn extrude(&mut self, sketch: &str, dist: impl AsRef<str>, negative: bool, op: &str, targets: &[&str], name: &str) {
        let body = if op == "new" { format!(",\"body_name\":{}", q(name)) } else { String::new() };
        let tg = if targets.is_empty() { String::new() } else { format!(",\"targets\":{}", qs(targets)) };
        let dir = if negative { ",\"direction\":\"negative\"" } else { "" };
        self.cmd(
            "solid.extrude",
            format!("{{\"sketch\":{},\"distance\":{}{dir},\"operation\":\"{op}\"{tg},\"name\":{}{body}}}", q(sketch), q(dist.as_ref()), q(name)),
        );
    }

    /// A cylinder from `base` along +z (or `axis`), cut from or joined to targets.
    pub fn cylinder<S: AsRef<str>>(&mut self, name: &str, base: [S; 3], axis: Option<[f64; 3]>, diameter: &str, height: &str, op: &str, targets: &[&str]) {
        let body = if op == "new" { format!(",\"body_name\":{}", q(name)) } else { String::new() };
        let tg = if targets.is_empty() { String::new() } else { format!(",\"targets\":{}", qs(targets)) };
        let ax = axis.map_or(String::new(), |a| format!(",\"axis\":{}", v3(a)));
        self.cmd(
            "solid.cylinder",
            format!("{{\"base\":{}{ax},\"diameter\":{},\"height\":{},\"operation\":\"{op}\"{tg},\"name\":{}{body}}}", qs(&base), q(diameter), q(height), q(name)),
        );
    }

    pub fn measure(&mut self) {
        self.cmd("inspect.measure", "{}".into());
    }

    /// The script as the CLI reads it.
    pub fn text(&self) -> String {
        format!("{{\"commands\":[\n{}\n]}}\n", self.c.join(",\n"))
    }
}
