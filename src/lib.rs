//! What the lasermade-tools programs share.
//!
//! They were Python until 2026-11-07, and each was ported to give the same
//! output as before, character for character, which is how the port was
//! tested: every program run old and new over every repository and the
//! outputs compared. So this file holds Python's behaviour where it shows:
//!
//!   py     float formatting (repr, rounding), math.dist and math.hypot as
//!          CPython 3.9 computes them, Python's quoting of strings and lists
//!   text   reading a file as Python's text mode does (newlines translated),
//!          paths shown as pathlib shows them, walking a tree like rglob
//!   html   html.escape and html.unescape
//!   run    a shell command with a timeout, and a pool of threads

pub mod solidcraft;
pub mod svg;
pub mod turn3d;

pub mod py {
    use std::hint::black_box;

    /// repr(x) for a float: the shortest digits that read back as x, rounded
    /// half to even where two candidates tie (Rust's shortest form rounds the
    /// tie up), in Python's layout: 1e-05, 1e+16, 600.0.
    pub fn repr(x: f64) -> String {
        if x.is_nan() {
            return "nan".into();
        }
        if x.is_infinite() {
            return if x > 0.0 { "inf".into() } else { "-inf".into() };
        }
        if x == 0.0 {
            return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
        }
        // repr is the closest string of the shortest length that reads back,
        // ties to even. Rust's {:e} has that length but breaks ties upwards
        // (2**-25 ends ...53125 at 17 digits: repr 312, {:e} 313), so take the
        // exact value correctly rounded to that length. That one can fail to
        // read back beside a power of two, whose lower gap is half the upper:
        // 2**-44 rounds to ...801 at 16 digits, which is another double, and
        // there the shortest form, ...802, is repr's.
        let short = format!("{:e}", x);
        let nd = short.split('e').next().unwrap().chars().filter(|c| c.is_ascii_digit()).count();
        let even = format!("{:.*e}", nd - 1, x);
        let s = if even.parse::<f64>() == Ok(x) { even } else { short };
        let (mant, exp) = s.split_once('e').unwrap();
        let exp: i32 = exp.parse().unwrap();
        let sign = if mant.starts_with('-') { "-" } else { "" };
        let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
        if !(-4..16).contains(&exp) {
            let m = if digits.len() > 1 {
                format!("{}.{}", &digits[..1], &digits[1..])
            } else {
                digits
            };
            format!("{sign}{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
        } else if exp >= 0 {
            let e = exp as usize;
            if digits.len() > e + 1 {
                format!("{sign}{}.{}", &digits[..e + 1], &digits[e + 1..])
            } else {
                format!("{sign}{}{}.0", digits, "0".repeat(e + 1 - digits.len()))
            }
        } else {
            format!("{sign}0.{}{}", "0".repeat((-exp - 1) as usize), digits)
        }
    }

    /// f'{x:g}': six significant digits, trailing zeros dropped, the
    /// exponent form below 1e-4 and from 1e6.
    pub fn g(x: f64) -> String {
        if x.is_nan() {
            return "nan".into();
        }
        if x.is_infinite() {
            return if x > 0.0 { "inf".into() } else { "-inf".into() };
        }
        if x == 0.0 {
            return if x.is_sign_negative() { "-0".into() } else { "0".into() };
        }
        let s = format!("{:.5e}", x);
        let (m, e) = s.split_once('e').unwrap();
        let exp: i32 = e.parse().unwrap();
        let strip = |t: &str| -> String {
            if t.contains('.') {
                t.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                t.to_string()
            }
        };
        if (-4..6).contains(&exp) {
            strip(&format!("{:.*}", (5 - exp) as usize, x))
        } else {
            format!("{}e{}{:02}", strip(m), if exp < 0 { '-' } else { '+' }, exp.abs())
        }
    }

    /// f'{x:.{p}e}': Python's exponent has a sign and at least two digits.
    pub fn e(x: f64, p: usize) -> String {
        let s = format!("{:.*e}", p, x);
        let (m, e) = s.split_once('e').unwrap();
        let exp: i32 = e.parse().unwrap();
        format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }

    /// math.degrees
    /// base64.b64encode(b).decode()
    pub fn b64(b: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
        for c in b.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                s.push(if k <= c.len() { A[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
            }
        }
        s
    }

    /// math.log2: libm's, behind black_box so nothing is folded
    pub fn log2(x: f64) -> f64 {
        extern "C" {
            fn log2(x: f64) -> f64;
        }
        let f: unsafe extern "C" fn(f64) -> f64 = log2;
        unsafe { black_box(f)(black_box(x)) }
    }

    pub fn degrees(x: f64) -> f64 {
        x * (180.0 / std::f64::consts::PI)
    }

    /// round(x): to the nearest integer, halves to even.
    pub fn round(x: f64) -> i64 {
        x.round_ties_even() as i64
    }

    /// round(x, dp): correctly rounded to dp decimals, as Python does it.
    pub fn round_dp(x: f64, dp: usize) -> f64 {
        format!("{:.*}", dp, x).parse().unwrap()
    }

    /// CPython 3.9's vector_norm, which math.hypot and math.dist use: scaled
    /// by the largest component, summed with a compensation term. Not the
    /// same bits as f64::hypot.
    fn vector_norm(v: &[f64]) -> f64 {
        let mut max = 0.0f64;
        let mut nan = false;
        for &x in v {
            let x = x.abs();
            nan |= x.is_nan();
            if x > max {
                max = x;
            }
        }
        if max.is_infinite() {
            return max;
        }
        if nan {
            return f64::NAN;
        }
        if max == 0.0 || v.len() <= 1 {
            return max;
        }
        let (mut csum, mut frac) = (1.0f64, 0.0f64);
        for &x0 in v {
            let mut x = x0.abs();
            x /= max;
            x *= x;
            let old = csum;
            csum += x;
            frac += (old - csum) + x;
        }
        max * (csum - 1.0 + frac).sqrt()
    }

    /// math.hypot(x, y)
    pub fn hypot(x: f64, y: f64) -> f64 {
        vector_norm(&[x, y])
    }

    /// math.dist(p, q) in the plane
    pub fn dist(p: (f64, f64), q: (f64, f64)) -> f64 {
        vector_norm(&[p.0 - q.0, p.1 - q.1])
    }

    type F1 = unsafe extern "C" fn(f64) -> f64;
    extern "C" {
        fn sin(x: f64) -> f64;
        fn cos(x: f64) -> f64;
        fn tan(x: f64) -> f64;
        fn acos(x: f64) -> f64;
        fn pow(x: f64, y: f64) -> f64;
        fn atan2(y: f64, x: f64) -> f64;
        fn fmod(x: f64, y: f64) -> f64;
    }
    fn call(f: F1, x: f64) -> f64 {
        unsafe { black_box(f)(x) }
    }
    // Each through a pointer the compiler cannot see through, so a sin and a
    // cos of one angle stay two calls, as Python makes them, and are not
    // merged into one sincos that rounds differently.
    pub fn msin(x: f64) -> f64 {
        call(sin, x)
    }
    pub fn mcos(x: f64) -> f64 {
        call(cos, x)
    }
    pub fn mtan(x: f64) -> f64 {
        call(tan, x)
    }
    pub fn macos(x: f64) -> f64 {
        call(acos, x)
    }
    /// x ** y for floats: libm's pow, which is not always x * x
    pub fn powf(x: f64, y: f64) -> f64 {
        let f: unsafe extern "C" fn(f64, f64) -> f64 = pow;
        unsafe { black_box(f)(x, black_box(y)) }
    }
    /// math.atan2: libm's
    pub fn matan2(y: f64, x: f64) -> f64 {
        let f: unsafe extern "C" fn(f64, f64) -> f64 = atan2;
        unsafe { black_box(f)(y, black_box(x)) }
    }
    /// math.remainder: CPython's own m_remainder, not libm's, for finite x and
    /// y != 0 (the only case asked of it here)
    pub fn remainder(x: f64, y: f64) -> f64 {
        let f: unsafe extern "C" fn(f64, f64) -> f64 = fmod;
        let (absx, absy) = (x.abs(), y.abs());
        let m = unsafe { black_box(f)(absx, absy) };
        let c = absy - m;
        let r = if m < c {
            m
        } else if m > c {
            -c
        } else {
            m - 2.0 * unsafe { black_box(f)(0.5 * (absx - m), absy) }
        };
        1.0f64.copysign(x) * r
    }
    /// math.radians
    pub fn radians(x: f64) -> f64 {
        x * (std::f64::consts::PI / 180.0)
    }

    /// max(a, b) and min(a, b): the first unless the second is strictly
    /// beyond it, which is also what they do with a NaN.
    pub fn max(a: f64, b: f64) -> f64 {
        if b > a {
            b
        } else {
            a
        }
    }
    pub fn min(a: f64, b: f64) -> f64 {
        if b < a {
            b
        } else {
            a
        }
    }

    /// float(s): surrounding whitespace allowed, as Python allows it.
    pub fn float(s: &str) -> Option<f64> {
        let t = s.trim();
        if t.is_empty() || t.contains('_') && !t.chars().all(|c| c.is_ascii_digit() || "._-+eE".contains(c)) {
            return None;
        }
        t.replace('_', "").parse().ok()
    }

    /// repr(s) for a str: single quotes unless the text holds one and no
    /// double quote; backslash escapes for the quote, backslash and controls.
    pub fn str_repr(s: &str) -> String {
        let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
        let mut o = String::new();
        o.push(q);
        for c in s.chars() {
            match c {
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\r' => o.push_str("\\r"),
                '\t' => o.push_str("\\t"),
                c if c == q => {
                    o.push('\\');
                    o.push(c);
                }
                c if (c as u32) < 0x20 || c as u32 == 0x7f => o.push_str(&format!("\\x{:02x}", c as u32)),
                c if (0x80..0xa0).contains(&(c as u32)) => o.push_str(&format!("\\x{:02x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push(q);
        o
    }

    /// str(list_of_str): ['a', 'b']
    pub fn list_repr<S: AsRef<str>>(xs: &[S]) -> String {
        let v: Vec<String> = xs.iter().map(|x| str_repr(x.as_ref())).collect();
        format!("[{}]", v.join(", "))
    }

    /// str(list_of_int): [1, 2]
    pub fn ints_repr<T: std::fmt::Display>(xs: &[T]) -> String {
        let v: Vec<String> = xs.iter().map(|x| x.to_string()).collect();
        format!("[{}]", v.join(", "))
    }

    /// s[:n], counted in characters
    pub fn head(s: &str, n: usize) -> &str {
        match s.char_indices().nth(n) {
            Some((i, _)) => &s[..i],
            None => s,
        }
    }
}

pub mod text {
    use std::path::{Path, PathBuf};

    /// Bytes as Python's text mode reads them: UTF-8, and every \r\n or lone
    /// \r turned into \n.
    pub fn decode(b: &[u8]) -> String {
        let s = String::from_utf8_lossy(b);
        if s.contains('\r') {
            s.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            s.into_owned()
        }
    }

    /// Path.read_text(), or None if it cannot be read.
    pub fn read(p: impl AsRef<Path>) -> Option<String> {
        std::fs::read(p).ok().map(|b| decode(&b))
    }

    /// Path.read_text() that, like Python, refuses bytes that are not UTF-8.
    pub fn read_strict(p: impl AsRef<Path>) -> Result<String, String> {
        let b = std::fs::read(p).map_err(|e| e.to_string())?;
        let s = String::from_utf8(b).map_err(|e| e.to_string())?;
        Ok(if s.contains('\r') { s.replace("\r\n", "\n").replace('\r', "\n") } else { s })
    }

    /// str(Path(s)): duplicate and trailing slashes and "." components
    /// dropped, "." itself kept for an empty result.
    pub fn pathstr(s: &str) -> String {
        let abs = s.starts_with('/');
        let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
        let j = parts.join("/");
        if abs {
            format!("/{j}")
        } else if j.is_empty() {
            ".".into()
        } else {
            j
        }
    }

    /// os.path.normpath: "." and empty parts dropped, ".." folded in
    pub fn normpath(s: &str) -> String {
        let abs = s.starts_with('/');
        let mut v: Vec<&str> = Vec::new();
        for p in s.split('/') {
            match p {
                "" | "." => {}
                ".." if v.last().is_some_and(|l| *l != "..") => {
                    v.pop();
                }
                ".." if abs => {}
                _ => v.push(p),
            }
        }
        let j = v.join("/");
        if abs {
            format!("/{j}")
        } else if j.is_empty() {
            ".".into()
        } else {
            j
        }
    }

    /// os.path.abspath: joined to os.getcwd() and normalised, symlinks kept
    pub fn abspath(s: &str) -> String {
        if s.starts_with('/') {
            normpath(s)
        } else {
            let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            normpath(&format!("{cwd}/{s}"))
        }
    }

    /// os.path.relpath(path, start)
    pub fn relpath(path: &str, start: &str) -> String {
        let a = abspath(path);
        let b = abspath(start);
        let pa: Vec<&str> = a.split('/').filter(|x| !x.is_empty()).collect();
        let pb: Vec<&str> = b.split('/').filter(|x| !x.is_empty()).collect();
        let n = pa.iter().zip(&pb).take_while(|(x, y)| x == y).count();
        let mut out: Vec<&str> = vec![".."; pb.len() - n];
        out.extend(&pa[n..]);
        if out.is_empty() {
            ".".into()
        } else {
            out.join("/")
        }
    }

    /// Path(a) / b, as pathlib prints it
    pub fn join(a: &str, b: &str) -> String {
        if a.is_empty() || b.starts_with('/') {
            return pathstr(b);
        }
        pathstr(&format!("{a}/{b}"))
    }

    /// Path.parts
    pub fn parts(s: &str) -> Vec<String> {
        let p = pathstr(s);
        let mut v: Vec<String> = Vec::new();
        if p.starts_with('/') {
            v.push("/".into());
        }
        if p != "." {
            v.extend(p.split('/').filter(|x| !x.is_empty()).map(String::from));
        }
        v
    }

    /// Every entry beneath dir (files and directories), not following
    /// symlinked directories, as rglob walks; paths are dir joined to the
    /// relative part.
    pub fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    stack.push(p.clone());
                }
                out.push(p);
            }
        }
        out
    }

    /// sorted(Path(dir).rglob(pattern)) for "*.ext": files and directories
    /// whose name ends with the extension, sorted as pathlib sorts, by parts.
    pub fn rglob_ext(dir: &str, ext: &str) -> Vec<String> {
        let mut v: Vec<String> = walk(Path::new(dir))
            .into_iter()
            .filter(|p| p.file_name().map(|n| n.to_string_lossy().ends_with(ext)).unwrap_or(false))
            .map(|p| pathstr(&p.to_string_lossy()))
            .collect();
        v.sort_by(|a, b| parts(a).cmp(&parts(b)));
        v
    }

    /// `git -C dir <args>` stdout, or None if git failed.
    pub fn git(dir: impl AsRef<Path>, args: &[&str]) -> Option<String> {
        let o = std::process::Command::new("git").arg("-C").arg(dir.as_ref()).args(args).output().ok()?;
        if !o.status.success() {
            return None;
        }
        Some(decode(&o.stdout))
    }
}

pub mod html {
    /// html.escape(s, quote=True)
    pub fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }

    const NAMED: &[(&str, char)] = &[
        ("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("apos", '\''), ("nbsp", '\u{a0}'),
        ("copy", '©'), ("reg", '®'), ("trade", '™'), ("hellip", '…'), ("mdash", '—'),
        ("ndash", '–'), ("lsquo", '‘'), ("rsquo", '’'), ("ldquo", '“'), ("rdquo", '”'),
        ("times", '×'), ("deg", '°'), ("plusmn", '±'), ("micro", 'µ'), ("middot", '·'),
        ("frac12", '½'), ("frac14", '¼'), ("frac34", '¾'), ("rarr", '→'), ("larr", '←'),
        ("uarr", '↑'), ("darr", '↓'), ("le", '≤'), ("ge", '≥'), ("ne", '≠'), ("minus", '−'),
        ("asymp", '≈'), ("infin", '∞'), ("sup2", '²'), ("sup3", '³'), ("eacute", 'é'),
        ("szlig", 'ß'), ("auml", 'ä'), ("ouml", 'ö'), ("uuml", 'ü'), ("shy", '\u{ad}'),
    ];

    /// html.unescape for what pandoc and these pages write: numeric
    /// references and the common named ones. An unknown name stays as text.
    pub fn unescape(s: &str) -> String {
        if !s.contains('&') {
            return s.to_string();
        }
        let mut o = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(i) = rest.find('&') {
            o.push_str(&rest[..i]);
            rest = &rest[i..];
            let end = rest[1..].find(';').map(|e| e + 1);
            let mut done = false;
            if let Some(e) = end {
                let body = &rest[1..e];
                let c = if let Some(n) = body.strip_prefix('#') {
                    let v = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                        u32::from_str_radix(h, 16).ok()
                    } else {
                        n.parse::<u32>().ok()
                    };
                    v.map(|v| char::from_u32(v).unwrap_or('\u{fffd}'))
                } else {
                    NAMED.iter().find(|(k, _)| *k == body).map(|(_, c)| *c)
                };
                if let Some(c) = c {
                    if !body.is_empty() && body.len() < 32 {
                        o.push(c);
                        rest = &rest[e + 1..];
                        done = true;
                    }
                }
            }
            if !done {
                o.push('&');
                rest = &rest[1..];
            }
        }
        o.push_str(rest);
        o
    }
}

pub mod run {
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub struct Ran {
        pub code: Option<i32>,
        pub stdout: String,
        pub stderr: String,
        pub timed_out: bool,
    }

    /// subprocess.run(cmd, shell=True, cwd=..., env=..., capture_output=True,
    /// text=True, timeout=...)
    pub fn shell(cmd: &str, cwd: &std::path::Path, env: &[(&str, &str)], timeout: Option<Duration>) -> std::io::Result<Ran> {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(cmd).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in env {
            c.env(k, v);
        }
        let mut child = c.spawn()?;
        let mut so = child.stdout.take().unwrap();
        let mut se = child.stderr.take().unwrap();
        let ho = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = std::io::Read::read_to_end(&mut so, &mut b);
            b
        });
        let he = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = std::io::Read::read_to_end(&mut se, &mut b);
            b
        });
        let start = Instant::now();
        let mut timed_out = false;
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break s;
            }
            if let Some(t) = timeout {
                if start.elapsed() > t {
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait()?;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let out = ho.join().unwrap_or_default();
        let err = he.join().unwrap_or_default();
        Ok(Ran {
            code: status.code(),
            stdout: crate::text::decode(&out),
            stderr: crate::text::decode(&err),
            timed_out,
        })
    }

    /// Run f over items on `threads` threads, results in the items' order.
    pub fn pool<T: Send + 'static, R: Send + 'static>(
        items: Vec<T>,
        threads: usize,
        f: impl Fn(T) -> R + Send + Sync + 'static,
    ) -> Vec<R> {
        let n = items.len();
        let queue = Arc::new(Mutex::new(items.into_iter().enumerate().collect::<Vec<_>>()));
        queue.lock().unwrap().reverse();
        let results = Arc::new(Mutex::new((0..n).map(|_| None).collect::<Vec<Option<R>>>()));
        let f = Arc::new(f);
        let hs: Vec<_> = (0..threads.max(1).min(n.max(1)))
            .map(|_| {
                let (q, r, f) = (queue.clone(), results.clone(), f.clone());
                std::thread::spawn(move || loop {
                    let job = q.lock().unwrap().pop();
                    let Some((i, t)) = job else { break };
                    let v = f(t);
                    r.lock().unwrap()[i] = Some(v);
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        Arc::try_unwrap(results).ok().unwrap().into_inner().unwrap().into_iter().map(|x| x.unwrap()).collect()
    }

    pub fn cores() -> usize {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    }
}

/// Python's re, as far as these programs use it, over fancy-regex (which
/// passes plain patterns to the regex crate and handles lookaround itself).
pub mod re {
    pub use fancy_regex::{Captures, Regex};

    pub fn rx(p: &str) -> Regex {
        Regex::new(p).unwrap_or_else(|e| panic!("bad pattern {p}: {e}"))
    }
    /// re.sub with a function
    pub fn sub(r: &Regex, s: &str, f: impl Fn(&Captures) -> String) -> String {
        sub_n(r, s, 0, f)
    }
    /// re.sub(..., count=n); n = 0 for all
    pub fn sub_n(r: &Regex, s: &str, n: usize, f: impl Fn(&Captures) -> String) -> String {
        let mut o = String::with_capacity(s.len());
        let mut last = 0;
        let mut k = 0;
        for c in r.captures_iter(s) {
            let c = c.expect("regex backtrack limit");
            let m = c.get(0).unwrap();
            o.push_str(&s[last..m.start()]);
            o.push_str(&f(&c));
            last = m.end();
            k += 1;
            if n != 0 && k == n {
                break;
            }
        }
        o.push_str(&s[last..]);
        o
    }
    /// re.sub with a fixed replacement (no group references)
    pub fn sub_str(r: &Regex, s: &str, rep: &str) -> String {
        sub(r, s, |_| rep.to_string())
    }
    /// re.search(...) is not None
    pub fn has(r: &Regex, s: &str) -> bool {
        r.is_match(s).expect("regex backtrack limit")
    }
    /// group g (or "") of every match, as re.findall with one group
    pub fn findall1(r: &Regex, s: &str, g: usize) -> Vec<String> {
        r.captures_iter(s)
            .map(|c| c.expect("regex backtrack limit").get(g).map(|m| m.as_str().to_string()).unwrap_or_default())
            .collect()
    }
    /// every match's captures
    pub fn all<'t>(r: &Regex, s: &'t str) -> Vec<Captures<'t>> {
        r.captures_iter(s).map(|c| c.expect("regex backtrack limit")).collect()
    }
    pub fn first<'t>(r: &Regex, s: &'t str) -> Option<Captures<'t>> {
        r.captures(s).expect("regex backtrack limit")
    }
    /// group g of a match as &str, "" when it did not take part
    pub fn g<'t>(c: &Captures<'t>, i: usize) -> &'t str {
        c.get(i).map(|m| m.as_str()).unwrap_or("")
    }
    /// re.split
    pub fn split<'t>(r: &Regex, s: &'t str) -> Vec<&'t str> {
        let mut v = Vec::new();
        let mut last = 0;
        for m in r.find_iter(s) {
            let m = m.expect("regex backtrack limit");
            v.push(&s[last..m.start()]);
            last = m.end();
        }
        v.push(&s[last..]);
        v
    }
}

/// sys.exit("message"): the message on stderr, status 1
pub fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

/// XML well-formedness, standing in for xml.dom.minidom.parseString
pub fn well_formed(s: &str) -> Result<(), String> {
    let opt = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    roxmltree::Document::parse_with_options(s, opt).map(|_| ()).map_err(|e| e.to_string())
}

/// A directory that removes itself, like tempfile.TemporaryDirectory.
pub struct TempDir(pub std::path::PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir();
        loop {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
            let p = base.join(format!("{tag}-{}-{}-{nanos}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            if std::fs::create_dir(&p).is_ok() {
                return TempDir(p);
            }
        }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// fnmatch.fnmatchcase: * ? [seq] [!seq], where * crosses "/" as Python's does.
pub fn fnmatchcase(name: &str, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let mut r = String::from("(?s)^");
    let mut i = 0;
    while i < p.len() {
        let c = p[i];
        i += 1;
        match c {
            '*' => r.push_str(".*"),
            '?' => r.push('.'),
            '[' => {
                let mut j = i;
                if j < p.len() && p[j] == '!' {
                    j += 1;
                }
                if j < p.len() && p[j] == ']' {
                    j += 1;
                }
                while j < p.len() && p[j] != ']' {
                    j += 1;
                }
                if j >= p.len() {
                    r.push_str("\\[");
                } else {
                    let mut stuff: String = p[i..j].iter().collect::<String>().replace('\\', "\\\\");
                    i = j + 1;
                    if let Some(s) = stuff.strip_prefix('!') {
                        stuff = format!("^{s}");
                    } else if stuff.starts_with('^') {
                        stuff = format!("\\{stuff}");
                    }
                    r.push_str(&format!("[{}]", stuff.replace('[', "\\[")));
                }
            }
            c => {
                let mut b = [0u8; 4];
                r.push_str(&fancy_regex::escape(c.encode_utf8(&mut b)));
            }
        }
    }
    r.push('$');
    re::has(&re::rx(&r), name)
}
