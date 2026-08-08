#!/usr/bin/env python3
"""Build a display rendering of a cut file: readable on a page, never cuttable.

    python3 make-preview.py CUTFILE.svg [OUT.svg]        # default previews/<name>

A cut file draws hairlines on no background. In a browser that is a nearly invisible
line on a transparency checkerboard, and on a dark page a black one disappears
entirely. The preview thickens the stroke, paints a light ground, and darkens the
inks that cannot be seen against it.

**The colours change, and that is the point.** Three of the six cut-order inks fail
the WCAG 3:1 graphics minimum on the cream ground — green at 1.28:1, cyan at 1.17:1,
orange at 2.35:1 — so a faithful rendering of the palette is an unreadable picture.
The darkened equivalents keep the same hue and the same order and clear 4.8:1. The
cut file keeps the exact values the laser keys on; this one is for looking at.

Geometry, sheet position and stroke *order* are untouched, and the result is checked
against the source before it is written.
"""
import re
import sys
import pathlib
import xml.dom.minidom

GROUND = "#faf7f0"

# hue preserved, luminance dropped until each clears 4.8:1 on the ground above
DARKEN = {
    "#00ff00": "#0a7a12",   # green   1.28:1 -> 5.16:1
    "#00ffff": "#00706e",   # cyan    1.17:1 -> 5.54:1
    "#ff8000": "#a85800",   # orange  2.35:1 -> 4.83:1
    "#8000ff": "#6a00cc",   # violet  5.84:1 -> 7.90:1, for consistency of weight
    "#0000ff": "#0000ff",   # blue    8.03:1, already fine
    "#000000": "#000000",   # black  19.63:1
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


def build(src, stroke_fraction=0.002, min_stroke=0.05):
    vb = VIEWBOX.search(src)
    if not vb:
        raise SystemExit("  no viewBox — cannot size the ground or the stroke")
    x, y, w, h = (float(v) for v in vb.group(1).replace(",", " ").split())
    stroke = max(max(w, h) * stroke_fraction, min_stroke)

    out = SVG_OPEN.sub(
        lambda m: m.group(0) + "\n" + NOTE
        + f'\n  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{GROUND}"/>',
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
    src_path = pathlib.Path(sys.argv[1])
    dst_path = (pathlib.Path(sys.argv[2]) if len(sys.argv) > 2
                else src_path.parent / "previews" / src_path.name)
    src = src_path.read_text()
    out, stroke = build(src)

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
