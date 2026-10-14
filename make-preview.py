#!/usr/bin/env python3
"""Build a display rendering of a cut file: readable on a page, never cuttable.

    python3 make-preview.py CUTFILE.svg [OUT.svg]        # default previews/<name>
    python3 make-preview.py CUTFILE.svg --ground=#faf7f0  # another ground colour

A cut file draws hairlines on no background. In a browser that is a nearly invisible
line on a transparency checkerboard, and on a dark page a black one disappears
entirely. The preview thickens the stroke, paints a light ground, and darkens the
inks that cannot be seen against it.

**The colours change, and that is the point.** Three of the six cut-order inks fail
the WCAG 3:1 graphics minimum on a white ground — green at 1.37:1, cyan at 1.25:1,
orange at 2.52:1 — so a faithful rendering of the palette is an unreadable picture.
The darkened equivalents keep the same hue and the same order and clear 5:1. The
cut file keeps the exact values the laser keys on; this one is for looking at.

Geometry, sheet position and stroke *order* are untouched, and the result is checked
against the source before it is written.
"""
import re
import sys
import pathlib
import xml.dom.minidom

GROUND = "#ffffff"

# hue preserved, luminance dropped until each clears 5:1 on the white ground above
# (chosen against the old cream, where the lowest was orange at 4.83:1)
DARKEN = {
    "#00ff00": "#0a7a12",   # green   1.37:1 -> 5.52:1
    "#00ffff": "#00706e",   # cyan    1.25:1 -> 5.93:1
    "#ff8000": "#a85800",   # orange  2.52:1 -> 5.17:1
    "#8000ff": "#6a00cc",   # violet  6.25:1 -> 8.45:1, for consistency of weight
    "#0000ff": "#0000ff",   # blue    8.59:1, already fine
    "#ff0000": "#cc0000",   # red     4.00:1 -> 5.89:1, a second engrave, where a file keeps its labels apart
    "#000000": "#000000",   # black  21.00:1
}

NOTE = """<!-- DISPLAY ONLY - not a cut file.
     The stroke is thickened, a light ground painted in, and the lightest inks
     darkened: green, cyan and orange sit below the 3:1 graphics minimum against any
     light background, so the cut order cannot be read from a faithful rendering. Hue
     and sequence are unchanged. Geometry and sheet position are untouched, and the
     CUT FILE keeps the exact values your laser keys on.
     Cut the file in the repository root, not this one. -->"""

DATA = re.compile(r'\bd="([^"]+)"')
VIEWBOX = re.compile(r'viewBox="([^"]+)"')
SVG_OPEN = re.compile(r"<svg\b[^>]*?>", re.S)


def build(src, stroke_fraction=0.002, min_stroke=0.05, ground=GROUND):
    vb = VIEWBOX.search(src)
    if not vb:
        raise SystemExit("  no viewBox — cannot size the ground or the stroke")
    x, y, w, h = (float(v) for v in vb.group(1).replace(",", " ").split())
    stroke = max(max(w, h) * stroke_fraction, min_stroke)

    out = SVG_OPEN.sub(
        lambda m: m.group(0) + "\n" + NOTE
        + f'\n  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{ground}"/>',
        src, count=1)

    # a hairline hint overrides the width we just set, so it has to go
    out = re.sub(r"-inkscape-stroke:\s*hairline;?", "", out)
    out = re.sub(r"vector-effect:\s*non-scaling-stroke;?", "", out)
    out = re.sub(r'\svector-effect="non-scaling-stroke"', "", out)

    out = re.sub(r"stroke-width:\s*[\d.]+", f"stroke-width:{stroke:.3f}", out)
    out = re.sub(r'stroke-width="[\d.]+"', f'stroke-width="{stroke:.3f}"', out)

    for raw, dark in DARKEN.items():
        if raw == dark:
            continue
        out = re.sub(rf"(stroke\s*:\s*){raw}", rf"\g<1>{dark}", out, flags=re.I)
        out = re.sub(rf'(stroke\s*=\s*"){raw}"', rf'\g<1>{dark}"', out, flags=re.I)

    return out, stroke


def main():
    # Same fault as md2html.py beside it: run bare this died on sys.argv[1].
    # The ground was cream (#faf7f0) until 2026-10-13 and is white now, for
    # every repository. --ground sets another; a darker one can push the
    # darkened inks below the contrast DARKEN was chosen for.
    ground, args = GROUND, []
    for a in sys.argv[1:]:
        if a.startswith("--ground="):
            ground = a.split("=", 1)[1]
            if not re.fullmatch(r"#[0-9a-fA-F]{6}", ground):
                sys.exit(f"make-preview: --ground wants #rrggbb, not {ground}")
        else:
            args.append(a)
    if not args:
        sys.exit("usage: make-preview.py CUTFILE.svg [OUTPUT.svg] [--ground=#rrggbb]")
    src_path = pathlib.Path(args[0])
    if not src_path.exists():
        sys.exit(f"make-preview: no such file: {src_path}")
    dst_path = (pathlib.Path(args[1]) if len(args) > 1
                else src_path.parent / "previews" / src_path.name)
    src = src_path.read_text()
    out, stroke = build(src, ground=ground)

    if DATA.findall(out) != DATA.findall(src):
        raise SystemExit(f"  {src_path}: path data changed — refusing to write")
    if VIEWBOX.search(out).group(1) != VIEWBOX.search(src).group(1):
        raise SystemExit(f"  {src_path}: viewBox changed — refusing to write")
    xml.dom.minidom.parseString(out)

    dst_path.parent.mkdir(exist_ok=True)
    dst_path.write_text(out)
    print(f"  {dst_path}   stroke {stroke:.3f}")


if __name__ == "__main__":
    main()
