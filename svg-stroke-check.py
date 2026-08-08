#!/usr/bin/env python3
"""Find SVG elements whose stroke colour is declared twice, in two places that disagree.

An element may carry its stroke in a style="" property and again as a presentation
attribute stroke="". The CSS cascade says the style property wins, and browsers and
Inkscape both agree — but not every laser importer applies the cascade, and one that
reads the attribute instead puts the part in a different cut stage. Since colour IS
the cut order in these repositories, that is a part cut at the wrong moment.

Usage:
    python3 svg-stroke-check.py FILE.svg [FILE.svg ...]
    python3 svg-stroke-check.py --dir DIR [--dir DIR ...]     # every .svg beneath
    python3 svg-stroke-check.py ... --fix                     # drop the attribute

--fix deletes the redundant presentation attribute, keeping the style property that
was winning anyway, so what the file cuts is unchanged. Path data is compared before
and after and the file is left alone if anything but the attribute moved.
"""
import argparse
import pathlib
import re
import sys
import xml.dom.minidom

DRAWABLE = r"path|rect|circle|ellipse|line|polyline|polygon|text|g|use"
ELEMENT = re.compile(rf"<(?:{DRAWABLE})\b[^>]*>", re.I)
STYLE_STROKE = re.compile(r'style="[^"]*?(?<![-\w])stroke:\s*([^;"\s]+)', re.S | re.I)
ATTR_STROKE = re.compile(r'(?:^|\s)stroke="([^"]+)"', re.S | re.I)
ATTR_STROKE_SUB = re.compile(r'\s+stroke="[^"]+"')
ID = re.compile(r'\bid="([^"]+)"')
DATA = re.compile(r'\bd="([^"]+)"')


def norm(c):
    """#00FF00 and #0f0 are the same ink; so are 'black' and '#000000'."""
    c = c.strip().lower()
    named = {"black": "#000000", "white": "#ffffff", "red": "#ff0000",
             "lime": "#00ff00", "blue": "#0000ff", "cyan": "#00ffff",
             "aqua": "#00ffff", "magenta": "#ff00ff", "fuchsia": "#ff00ff",
             "yellow": "#ffff00", "green": "#008000"}
    if c in named:
        return named[c]
    if re.fullmatch(r"#[0-9a-f]{3}", c):
        return "#" + "".join(ch * 2 for ch in c[1:])
    return c


def scan(text):
    """-> (conflicts, duplicates). Conflicts disagree; duplicates say the same thing."""
    conflicts, duplicates = [], []
    for m in ELEMENT.finditer(text):
        e = m.group(0)
        sty, att = STYLE_STROKE.search(e), ATTR_STROKE.search(e)
        if not (sty and att):
            continue
        pid = ID.search(e)
        row = (pid.group(1) if pid else "(no id)", norm(sty.group(1)), norm(att.group(1)))
        (conflicts if row[1] != row[2] else duplicates).append(row)
    return conflicts, duplicates


def fix(path, text):
    def strip(m):
        e = m.group(0)
        sty, att = STYLE_STROKE.search(e), ATTR_STROKE.search(e)
        if sty and att:
            return ATTR_STROKE_SUB.sub("", e, count=1)
        return e

    out = ELEMENT.sub(strip, text)
    if DATA.findall(out) != DATA.findall(text):
        return None, "path data changed — refusing to write"
    try:
        xml.dom.minidom.parseString(out)
    except Exception as exc:
        return None, f"result does not parse ({exc}) — refusing to write"
    return out, None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", type=pathlib.Path)
    ap.add_argument("--dir", action="append", type=pathlib.Path, default=[])
    ap.add_argument("--fix", action="store_true")
    ap.add_argument("--quiet", action="store_true", help="list only files with findings")
    args = ap.parse_args()

    targets = list(args.files)
    for d in args.dir:
        targets += sorted(d.rglob("*.svg"))
    if not targets:
        ap.error("no SVG files given")

    n_conf = n_dup = n_files = n_fixed = 0
    for f in targets:
        try:
            text = f.read_text()
        except (OSError, UnicodeDecodeError) as exc:
            print(f"  {f}: unreadable ({exc})")
            continue
        conflicts, duplicates = scan(text)
        if not conflicts and not duplicates:
            if not args.quiet:
                print(f"  ok    {f}")
            continue
        n_files += 1
        n_conf += len(conflicts)
        n_dup += len(duplicates)
        print(f"\n  {f}")
        if conflicts:
            pairs = {}
            for _, s, a in conflicts:
                pairs[(s, a)] = pairs.get((s, a), 0) + 1
            print(f"    {len(conflicts)} CONFLICTING — style wins, attribute is a different colour")
            for (s, a), n in sorted(pairs.items()):
                print(f"      x{n:<4} style {s}  vs  attribute {a}   (cuts as {s})")
            for pid, s, a in conflicts[:8]:
                print(f"        {pid}")
            if len(conflicts) > 8:
                print(f"        … and {len(conflicts) - 8} more")
        if duplicates:
            print(f"    {len(duplicates)} redundant but agreeing (harmless, still noise)")
        if args.fix:
            out, err = fix(f, text)
            if err:
                print(f"    NOT FIXED: {err}")
            else:
                f.write_text(out)
                n_fixed += 1
                print("    fixed — redundant stroke=\"\" attributes removed")

    print(f"\n  {len(targets)} file(s) scanned, {n_files} with findings: "
          f"{n_conf} conflicting, {n_dup} redundant-but-agreeing"
          + (f", {n_fixed} file(s) rewritten" if args.fix else ""))
    return 1 if n_conf else 0


if __name__ == "__main__":
    sys.exit(main())
