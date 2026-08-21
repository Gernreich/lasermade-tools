#!/usr/bin/env python3
"""Build a cut-speed test ladder: identical squares, one colour per speed.

    python3 test-ladder.py OUT.svg [--speeds 15,20,25,30] [--square 15]

Settings for a sheet of plywood are a property of that sheet, not of the species on
the invoice. Baltic birch from two suppliers, or the same supplier two packs apart,
will not cut at the same speed. A ladder answers the question in about ninety seconds
and a strip of scrap: cut it, push each square out from below, and the fastest one
that drops free unaided is the ceiling. Run production 15-20% slower than that.

**Colour here is one speed per layer, not cut order.** Every other file in these
repositories uses colour to say *when* a stroke is cut; a ladder uses it to say *how
fast*, because that is the only handle xTool Studio and XCS give you — they split an
import into processing layers by colour, so a separate colour is the only way to hold
a separate speed field. Same colour would mean one layer and one speed, which is a
row of identical squares and no ladder at all.

The inks are still the house cut stages, so `make-preview.py` renders a ladder and
`svg-stroke-check.py` reads it. Four rungs, because those are the four stages that
cut: green, orange, cyan, black. Blue engraves the labels, as it always does. Violet
is never emitted — it means skip, and a skipped rung is a missing answer.

Four is also the better way to use a ladder. Bracket coarse, then bisect: 15/25/35/45
finds the decade, a second run over the winning pair finds the number. That lands
closer than six rungs guessed in one pass.

Labels are seven-segment line art rather than text, so no font substitution on import
can turn the numbers into something else, or into nothing.
"""
import argparse
import pathlib
import xml.dom.minidom

SQUARE = 15.0        # square edge, mm
GAP = 5.0            # between squares
MARGIN = 5.0         # sheet margin
LABEL_GAP = 3.0      # square bottom -> label top
DIGIT_W = 3.0        # seven-segment digit cell
DIGIT_H = 5.0
DIGIT_GAP = 1.2      # between digits of one label
DOT_W = 1.0          # decimal point cell
HAIRLINE = 0.1

# the house cut stages, in order. Blue engraves, violet skips — see the docstring.
CUT_STAGES = ["#00ff00", "#ff8000", "#00ffff", "#000000"]
ENGRAVE = "#0000ff"

# a=top, b=upper right, c=lower right, d=bottom, e=lower left, f=upper left, g=middle
SEGMENTS = {
    "0": "abcdef", "1": "bc", "2": "abged", "3": "abgcd", "4": "fgbc",
    "5": "afgcd", "6": "afgedc", "7": "abc", "8": "abcdefg", "9": "abcdfg",
}

NOTE = """<!-- CUT-SPEED LADDER - colour is one speed per layer, NOT cut order.
     Each square carries the speed engraved beneath it. In xTool Studio or XCS:
     put the blue label layer FIRST and engrave it, so the numbers are already
     burned in on squares that later drop through the honeycomb; then set every
     coloured layer to cut at 100% power, one pass, air assist at maximum, and
     give each one the speed its own label shows. -->"""


def fmt(n):
    """Trim the float so the file reads as millimetres and not as arithmetic."""
    return f"{n:.3f}".rstrip("0").rstrip(".")


def segments(x, y, w, h):
    mid = y + h / 2
    return {
        "a": (x, y, x + w, y),
        "b": (x + w, y, x + w, mid),
        "c": (x + w, mid, x + w, y + h),
        "d": (x, y + h, x + w, y + h),
        "e": (x, mid, x, y + h),
        "f": (x, y, x, mid),
        "g": (x, mid, x + w, mid),
    }


def label(value, cx, top):
    """Seven-segment digits for `value`, centred on cx, top edge at `top`.

    A decimal point is a baseline tick in a narrow cell — 12.5 mm/s has to be
    sayable, or the ladder cannot bisect below whole numbers.
    """
    chars = list(fmt(value))
    cell = {ch: (DOT_W if ch == "." else DIGIT_W) for ch in set(chars)}
    width = sum(cell[ch] for ch in chars) + (len(chars) - 1) * DIGIT_GAP
    x = cx - width / 2
    out = []
    for ch in chars:
        if ch == ".":
            y = top + DIGIT_H
            out.append(f"M {fmt(x)} {fmt(y)} L {fmt(x + DOT_W)} {fmt(y)}")
        else:
            seg = segments(x, top, DIGIT_W, DIGIT_H)
            for name in SEGMENTS[ch]:
                x1, y1, x2, y2 = seg[name]
                out.append(f"M {fmt(x1)} {fmt(y1)} L {fmt(x2)} {fmt(y2)}")
        x += cell[ch] + DIGIT_GAP
    return " ".join(out)


def build(speeds, square):
    n = len(speeds)
    strip = n * square + (n - 1) * GAP
    width = strip + 2 * MARGIN
    height = MARGIN + square + LABEL_GAP + DIGIT_H + MARGIN

    squares, labels = [], []
    for i, speed in enumerate(speeds):
        x = MARGIN + i * (square + GAP)
        ink = CUT_STAGES[i]
        squares.append(
            f'  <rect x="{fmt(x)}" y="{fmt(MARGIN)}" '
            f'width="{fmt(square)}" height="{fmt(square)}" '
            f'fill="none" stroke="{ink}" stroke-width="{HAIRLINE}"/>'
            f'   <!-- {fmt(speed)} mm/s -->'
        )
        labels.append(label(speed, x + square / 2, MARGIN + square + LABEL_GAP))

    svg = "\n".join([
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'width="{fmt(width)}mm" height="{fmt(height)}mm" '
        f'viewBox="0 0 {fmt(width)} {fmt(height)}">',
        NOTE,
        *squares,
        f'  <path d="{" ".join(labels)}" fill="none" '
        f'stroke="{ENGRAVE}" stroke-width="{HAIRLINE}"/>',
        "</svg>",
        "",
    ])
    return svg, width, height


def speed_list(text):
    speeds = []
    for part in text.split(","):
        try:
            v = float(part)
        except ValueError:
            raise SystemExit(f"  not a speed: {part.strip()!r}")
        if v <= 0:
            raise SystemExit(f"  speed must be positive: {fmt(v)}")
        speeds.append(v)
    if len(speeds) > len(CUT_STAGES):
        raise SystemExit(
            f"  {len(speeds)} speeds, {len(CUT_STAGES)} cutting stages to hold them.\n"
            f"  Bracket coarse then bisect rather than reaching for violet, which means skip."
        )
    if len(speeds) != len(set(speeds)):
        raise SystemExit("  a speed appears twice — two rungs, one answer")
    return speeds


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=pathlib.Path)
    ap.add_argument("--speeds", type=speed_list, default="15,20,25,30",
                    help="mm/s, comma separated, up to four")
    ap.add_argument("--square", type=float, default=SQUARE, help="square edge in mm")
    args = ap.parse_args()

    speeds = args.speeds if isinstance(args.speeds, list) else speed_list(args.speeds)
    svg, width, height = build(speeds, args.square)
    xml.dom.minidom.parseString(svg)

    inks = CUT_STAGES[:len(speeds)]
    if len(set(inks)) != len(speeds):
        raise SystemExit("  an ink is reused — those rungs would share one speed")

    args.out.write_text(svg)
    print(f"  {args.out}   {fmt(width)} x {fmt(height)} mm")
    for speed, ink in zip(speeds, inks):
        print(f"    {fmt(speed):>5} mm/s   {ink}")
    print(f"    labels        {ENGRAVE}  engrave, run first")


if __name__ == "__main__":
    main()
