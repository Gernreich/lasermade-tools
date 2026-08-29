#!/usr/bin/env python3
"""Check a flat single-sheet cut file before it is sent to the laser.

The bore repositories have `bore_split.py --write`, which gates every net it writes.
The flat parts — bullroarer blades, buzz discs, anything cut from one sheet with holes
in it — had nothing. This is that gate. It reads the geometry rather than the drawing:
paths, rects, circles, ellipses, polygons and lines, flattened to polylines with every
enclosing transform applied, and then measured.

    python3 flat-part-check.py FILE.svg [FILE.svg ...]
    python3 flat-part-check.py --dir DIR [--dir DIR ...]
    python3 flat-part-check.py FILE.svg --min-edge 4 --bed 600x308

What it checks, and the failure each one is looking for:

  millimetre-true         a file whose user unit is not a millimetre cuts at 96/25.4
                          and looks right on screen the whole time
  fits the bed            a part wider than the bed is refused at the machine, or
                          worse, silently cropped
  cut paths are closed    an unclosed outline does not free the part, and the laser
                          lifts mid-run where the ends fail to meet
  ink is in the palette   colour is the cut order in these repositories; a colour
                          nothing recognises is a stage nothing runs
  black frees the part    the outermost path must be the last stage, or the part
                          moves while its holes are still being cut
  holes are inside        a hole outside the outline is a mark on the waste, and
                          usually means a transform was missed
  hole is big enough      the cord holes are the point of these parts
  edge distance           the cord hole is where a bullroarer fails: too little
                          material between hole and edge and it tears out

Exit status is 1 if any file fails, so it can gate a commit.

Not modelled: kerf. Every measurement is of the path as drawn, and the beam takes
its width from both sides of it — a 3.0mm hole cuts about 3.1mm and a 3.0mm wall
comes out about 2.9mm. Set --min-edge with that in hand rather than at the limit.
"""
import argparse
import math
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

SVG = "{http://www.w3.org/2000/svg}"
BED_W, BED_H = 600.0, 308.0        # xTool P2S work area, mm
MIN_EDGE = 3.0                     # mm of material between a hole and any other edge
MIN_HOLE = 2.0                     # mm across, the smallest hole worth cutting
FLATNESS = 0.05                    # mm; curve subdivision error budget
# A path can close without saying z: Inkscape writes the return point as an ordinary
# node, and the ends coincide. The beam still travels the whole loop, so that cuts
# the same. Anything under this is the same point twice.
CLOSE_TOL = 0.01                   # mm
# Two open paths can be one cut: the bullroarer blades are drawn as a top curve and a
# bottom curve meeting at the two tips, and the part is freed by the pair. Ends this
# close are the same corner, and a laser importer welds them the same way.
WELD_TOL = 0.05                    # mm

# Colour is the cut order: blue engraves, then green, orange, cyan, black. Black is
# always the cut that frees the part. Violet means skip.
ORDER = ["#0000ff", "#00ff00", "#ff8000", "#00ffff", "#000000"]
SKIP = "#8000ff"
NAMED = {"black": "#000000", "lime": "#00ff00", "blue": "#0000ff",
         "cyan": "#00ffff", "aqua": "#00ffff", "red": "#ff0000",
         "white": "#ffffff", "green": "#008000", "none": "none"}


def norm(c):
    """#00FF00, #0f0 and 'lime' are one ink."""
    c = (c or "").strip().lower()
    if c in NAMED:
        return NAMED[c]
    if re.fullmatch(r"#[0-9a-f]{3}", c):
        return "#" + "".join(ch * 2 for ch in c[1:])
    return c


# --- transforms -------------------------------------------------------------
# (a, b, c, d, e, f) as in SVG: x' = a x + c y + e,  y' = b x + d y + f

I = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def mul(m, n):
    a, b, c, d, e, f = m
    A, B, C, D, E, F = n
    return (a * A + c * B, b * A + d * B,
            a * C + c * D, b * C + d * D,
            a * E + c * F + e, b * E + d * F + f)


def apply(m, p):
    a, b, c, d, e, f = m
    x, y = p
    return (a * x + c * y + e, b * x + d * y + f)


def parse_transform(s):
    m = I
    for name, args in re.findall(r"(\w+)\s*\(([^)]*)\)", s or ""):
        v = [float(x) for x in re.split(r"[\s,]+", args.strip()) if x]
        if name == "translate":
            m = mul(m, (1, 0, 0, 1, v[0], v[1] if len(v) > 1 else 0))
        elif name == "scale":
            m = mul(m, (v[0], 0, 0, v[1] if len(v) > 1 else v[0], 0, 0))
        elif name == "matrix" and len(v) == 6:
            m = mul(m, tuple(v))
        elif name == "rotate":
            t = math.radians(v[0])
            r = (math.cos(t), math.sin(t), -math.sin(t), math.cos(t), 0, 0)
            if len(v) == 3:
                m = mul(mul(m, (1, 0, 0, 1, v[1], v[2])), r)
                m = mul(m, (1, 0, 0, 1, -v[1], -v[2]))
            else:
                m = mul(m, r)
        elif name == "skewX":
            m = mul(m, (1, 0, math.tan(math.radians(v[0])), 1, 0, 0))
        elif name == "skewY":
            m = mul(m, (1, math.tan(math.radians(v[0])), 0, 1, 0, 0))
    return m


# --- path flattening --------------------------------------------------------

TOKENS = re.compile(r"[MmZzLlHhVvCcSsQqTtAa]|-?\d*\.?\d+(?:[eE][-+]?\d+)?")


def steps(*pts):
    """Segment count for a curve, from its control polygon length."""
    n = sum(math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1))
    return max(4, min(160, int(math.sqrt(n / FLATNESS)) + 4))


def cubic(p0, p1, p2, p3):
    out = []
    for i in range(1, (n := steps(p0, p1, p2, p3)) + 1):
        t = i / n
        u = 1 - t
        out.append((u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0],
                    u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1]))
    return out


def quad(p0, p1, p2):
    return cubic(p0, (p0[0] + 2/3 * (p1[0] - p0[0]), p0[1] + 2/3 * (p1[1] - p0[1])),
                 (p2[0] + 2/3 * (p1[0] - p2[0]), p2[1] + 2/3 * (p1[1] - p2[1])), p2)


def arc(p0, rx, ry, rot, large, sweep, p1):
    """Endpoint parametrisation, F.6.5 of the SVG spec."""
    if p0 == p1:
        return []
    rx, ry = abs(rx), abs(ry)
    if rx == 0 or ry == 0:
        return [p1]
    t = math.radians(rot)
    cos, sin = math.cos(t), math.sin(t)
    dx, dy = (p0[0] - p1[0]) / 2, (p0[1] - p1[1]) / 2
    x1, y1 = cos * dx + sin * dy, -sin * dx + cos * dy
    # F.6.6: grow the radii if they cannot span the chord
    lam = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry)
    if lam > 1:
        rx, ry = rx * math.sqrt(lam), ry * math.sqrt(lam)
    num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
    den = rx * rx * y1 * y1 + ry * ry * x1 * x1
    co = math.sqrt(max(0.0, num / den)) * (-1 if large == sweep else 1)
    cx1, cy1 = co * rx * y1 / ry, -co * ry * x1 / rx
    cx = cos * cx1 - sin * cy1 + (p0[0] + p1[0]) / 2
    cy = sin * cx1 + cos * cy1 + (p0[1] + p1[1]) / 2

    def ang(ux, uy, vx, vy):
        d = (math.hypot(ux, uy) * math.hypot(vx, vy)) or 1e-12
        a = math.acos(max(-1.0, min(1.0, (ux * vx + uy * vy) / d)))
        return -a if ux * vy - uy * vx < 0 else a

    th0 = ang(1, 0, (x1 - cx1) / rx, (y1 - cy1) / ry)
    dth = ang((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry)
    if not sweep and dth > 0:
        dth -= 2 * math.pi
    elif sweep and dth < 0:
        dth += 2 * math.pi
    n = max(6, min(240, int(abs(dth) * max(rx, ry) / max(FLATNESS, 1e-6)) + 6))
    out = []
    for i in range(1, n + 1):
        th = th0 + dth * i / n
        ex, ey = rx * math.cos(th), ry * math.sin(th)
        out.append((cos * ex - sin * ey + cx, sin * ex + cos * ey + cy))
    return out


def flatten_path(d):
    """-> [(points, closed)], one entry per subpath, in user units."""
    t = TOKENS.findall(d or "")
    subs, pts, closed = [], [], False
    cur = start = (0.0, 0.0)
    cmd = None
    prev_c = prev_q = None
    i = 0

    def end():
        nonlocal pts, closed
        if len(pts) > 1:
            subs.append((pts, closed))
        pts, closed = [], False

    while i < len(t):
        if re.fullmatch(r"[A-Za-z]", t[i]):
            cmd = t[i]
            i += 1
            if cmd in "Zz":
                if pts:
                    if pts[0] != pts[-1]:
                        pts.append(pts[0])
                    closed = True
                    end()
                cur = start
                continue
        if cmd is None:
            break
        rel = cmd.islower()
        C = cmd.upper()

        def num(k=1):
            nonlocal i
            v = [float(x) for x in t[i:i + k]]
            i += k
            return v

        if C == "M":
            x, y = num(2)
            end()
            cur = start = (cur[0] + x, cur[1] + y) if rel else (x, y)
            pts = [cur]
            cmd = "l" if rel else "L"
        elif C == "L":
            x, y = num(2)
            cur = (cur[0] + x, cur[1] + y) if rel else (x, y)
            pts.append(cur)
        elif C == "H":
            (x,) = num(1)
            cur = (cur[0] + x, cur[1]) if rel else (x, cur[1])
            pts.append(cur)
        elif C == "V":
            (y,) = num(1)
            cur = (cur[0], cur[1] + y) if rel else (cur[0], y)
            pts.append(cur)
        elif C in "CS":
            if C == "C":
                x1, y1, x2, y2, x, y = num(6)
                p1 = (cur[0] + x1, cur[1] + y1) if rel else (x1, y1)
            else:
                x2, y2, x, y = num(4)
                p1 = (2 * cur[0] - prev_c[0], 2 * cur[1] - prev_c[1]) if prev_c else cur
            p2 = (cur[0] + x2, cur[1] + y2) if rel else (x2, y2)
            p3 = (cur[0] + x, cur[1] + y) if rel else (x, y)
            pts += cubic(cur, p1, p2, p3)
            cur, prev_c, prev_q = p3, p2, None
            continue
        elif C in "QT":
            if C == "Q":
                x1, y1, x, y = num(4)
                p1 = (cur[0] + x1, cur[1] + y1) if rel else (x1, y1)
            else:
                x, y = num(2)
                p1 = (2 * cur[0] - prev_q[0], 2 * cur[1] - prev_q[1]) if prev_q else cur
            p2 = (cur[0] + x, cur[1] + y) if rel else (x, y)
            pts += quad(cur, p1, p2)
            cur, prev_q, prev_c = p2, p1, None
            continue
        elif C == "A":
            rx, ry, rot, la, sw, x, y = num(7)
            p1 = (cur[0] + x, cur[1] + y) if rel else (x, y)
            pts += arc(cur, rx, ry, rot, int(la), int(sw), p1)
            cur = p1
        else:
            i += 1
            continue
        prev_c = prev_q = None
    end()
    return subs


# --- shapes -----------------------------------------------------------------

def nums(s):
    return [float(x) for x in re.findall(r"-?\d*\.?\d+(?:[eE][-+]?\d+)?", s or "")]


def shape_subpaths(el):
    """-> [(points, closed)] for one drawable element, in user units."""
    tag = el.tag.split("}")[-1]
    g = el.get
    if tag == "path":
        return flatten_path(g("d"))
    if tag == "rect":
        x, y = float(g("x", 0)), float(g("y", 0))
        w, h = float(g("width", 0)), float(g("height", 0))
        if w <= 0 or h <= 0:
            return []
        p = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]
        return [(p, True)]
    if tag in ("circle", "ellipse"):
        cx, cy = float(g("cx", 0)), float(g("cy", 0))
        if tag == "circle":
            rx = ry = float(g("r", 0))
        else:
            rx, ry = float(g("rx", 0)), float(g("ry", 0))
        if rx <= 0 or ry <= 0:
            return []
        n = max(24, min(360, int(2 * math.pi * max(rx, ry) / max(FLATNESS, 1e-6)) + 8))
        p = [(cx + rx * math.cos(2 * math.pi * i / n),
              cy + ry * math.sin(2 * math.pi * i / n)) for i in range(n + 1)]
        return [(p, True)]
    if tag in ("polygon", "polyline"):
        v = nums(g("points"))
        p = list(zip(v[0::2], v[1::2]))
        if len(p) < 2:
            return []
        if tag == "polygon" and p[0] != p[-1]:
            p.append(p[0])
        return [(p, tag == "polygon")]
    if tag == "line":
        return [([(float(g("x1", 0)), float(g("y1", 0))),
                  (float(g("x2", 0)), float(g("y2", 0)))], False)]
    return []


def ink(el):
    """The stroke colour this element cuts in, or None if it is not a cut."""
    style = el.get("style") or ""
    m = re.search(r"(?<![-\w])stroke\s*:\s*([^;]+)", style)
    c = norm(m.group(1) if m else el.get("stroke"))
    if c in ("none", "", None):
        # a filled shape with no stroke still cuts on some importers, but in these
        # repositories every cut is stroked; treat it as artwork, not geometry
        return None
    return c


def collect(root):
    """-> [(colour, points, closed)] in millimetres, transforms applied."""
    out = []
    scale = user_scale(root)

    def walk(el, m):
        m = mul(m, parse_transform(el.get("transform")))
        for kid in el:
            tag = kid.tag.split("}")[-1]
            if tag in ("g", "a", "svg"):
                walk(kid, m)
                continue
            if tag in ("defs", "metadata", "title", "desc"):
                continue
            c = ink(kid)
            if c is None:
                continue
            km = mul(m, parse_transform(kid.get("transform")))
            for pts, closed in shape_subpaths(kid):
                p = [apply(km, q) for q in pts]
                out.append((c, [(x * scale, y * scale) for x, y in p], closed))

    walk(root, I)
    return out


def user_scale(root):
    """Millimetres per user unit, from width/height against the viewBox."""
    vb = nums(root.get("viewBox") or "")
    w = root.get("width") or ""
    if len(vb) == 4 and vb[2] and w.strip().endswith("mm"):
        return nums(w)[0] / vb[2]
    return 1.0


# --- measurement ------------------------------------------------------------

def bbox(p):
    xs, ys = [q[0] for q in p], [q[1] for q in p]
    return min(xs), min(ys), max(xs), max(ys)


def area(p):
    a = 0.0
    for i in range(len(p) - 1):
        a += p[i][0] * p[i + 1][1] - p[i + 1][0] * p[i][1]
    return abs(a) / 2


def inside(pt, poly):
    """Ray casting, over the edges as a cycle.

    Taking the edges as poly[i]..poly[i+1] and stopping leaves out the edge that
    closes the ring. On a path whose ends only nearly meet — Inkscape writes the
    return point as an ordinary node, so they differ in the ninth decimal — that
    missing edge is a real one, and a ray passing between the two endpoint heights
    loses a crossing and reports inside as outside. BuzzDisc1's outline starts at
    3 o'clock, exactly the height of its two cord holes, so it hit this every time.
    """
    x, y = pt
    hit = False
    n = len(poly)
    for i in range(n):
        x1, y1 = poly[i]
        x2, y2 = poly[(i + 1) % n]
        if (y1 > y) != (y2 > y):
            xx = x1 + (y - y1) * (x2 - x1) / ((y2 - y1) or 1e-12)
            if x < xx:
                hit = not hit
    return hit


def centroid(p):
    return (sum(q[0] for q in p) / len(p), sum(q[1] for q in p) / len(p))


def seg_dist(p, a, b):
    ax, ay = a
    bx, by = b
    dx, dy = bx - ax, by - ay
    L = dx * dx + dy * dy
    t = 0.0 if L == 0 else max(0.0, min(1.0, ((p[0] - ax) * dx + (p[1] - ay) * dy) / L))
    return math.dist(p, (ax + t * dx, ay + t * dy))


def stitch(paths, tol):
    """Join open polylines whose ends meet. -> (rings, leftovers, joins made).

    A profile is not always one path. Drawn as two curves sharing their endpoints it
    cuts exactly the same ring, so judging each half on its own reports two open
    paths and no outline — which is what this said about four of the five bullroarer
    blades before it could stitch.
    """
    chains = [list(p) for p in paths]
    joins = 0
    while True:
        done = True
        for i in range(len(chains)):
            for j in range(len(chains)):
                if i == j:
                    continue
                a, b = chains[i], chains[j]
                if math.dist(a[0], a[-1]) <= tol:      # already a ring
                    continue
                if math.dist(a[-1], b[0]) <= tol:
                    chains[i] = a + b[1:]
                elif math.dist(a[-1], b[-1]) <= tol:
                    chains[i] = a + b[::-1][1:]
                elif math.dist(a[0], b[-1]) <= tol:
                    chains[i] = b + a[1:]
                elif math.dist(a[0], b[0]) <= tol:
                    chains[i] = b[::-1] + a[1:]
                else:
                    continue
                chains.pop(j)
                joins += 1
                done = False
                break
            if not done:
                break
        if done:
            break
    rings = [c for c in chains if len(c) > 3 and math.dist(c[0], c[-1]) <= tol]
    rest = [c for c in chains if c not in rings]
    return rings, rest, joins


def poly_dist(a, b):
    """Least distance between two polylines. Sampled at the vertices of each."""
    d = min(seg_dist(p, b[i], b[i + 1]) for p in a for i in range(len(b) - 1))
    return min(d, min(seg_dist(p, a[i], a[i + 1]) for p in b for i in range(len(a) - 1)))


# --- the gate ---------------------------------------------------------------

def check(path, bed, min_edge, min_hole, weld=WELD_TOL):
    """-> (rows, notes). A row is (name, ok, detail)."""
    rows, notes = [], []

    def row(name, ok, detail=""):
        rows.append((name, ok, detail))

    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as e:
        return [("the file parses as SVG", False, str(e))], []

    w, h = root.get("width") or "", root.get("height") or ""
    vb = nums(root.get("viewBox") or "")
    mm = w.strip().endswith("mm") and h.strip().endswith("mm")
    scale = user_scale(root)
    row("millimetre-true", bool(mm) and abs(scale - 1.0) < 0.002,
        f'width="{w}" height="{h}"' +
        ("  no viewBox" if len(vb) != 4 else
         f"  1 unit = {scale:.4f}mm" if mm else
         "  size is not in mm, so a user unit means nothing"))

    geo = collect(root)
    if not geo:
        row("the file draws something", False, "no stroked geometry")
        return rows, notes

    allp = [p for _, p, _ in geo]
    x0, y0, x1, y1 = bbox([q for p in allp for q in p])
    W, H = x1 - x0, y1 - y0
    row("fits the bed", W <= bed[0] and H <= bed[1],
        f"{W:.1f} x {H:.1f}mm on a {bed[0]:.0f} x {bed[1]:.0f} bed")

    cuts = [(c, p, cl) for c, p, cl in geo if c != SKIP]
    inks = sorted({c for c, _, _ in cuts})
    strange = [c for c in inks if c not in ORDER]
    row("ink is in the palette", not strange,
        ", ".join(inks) + (f"   unknown: {', '.join(strange)}" if strange else ""))

    # A subpath counts as closed if it says z or if its ends meet: the laser cuts the
    # same loop either way. What is left over is stitched to its own colour, because a
    # profile drawn as two curves meeting at the tips is one ring cut in two goes.
    def shut(p, cl):
        return cl or math.dist(p[0], p[-1]) <= CLOSE_TOL

    closed = [(c, p) for c, p, cl in cuts if shut(p, cl) and len(p) > 3]
    welded = sum(1 for _, p, cl in cuts if not cl and shut(p, cl))
    loose, joins = [], 0
    for c in sorted({c for c, _, _ in cuts}):
        segs = [p for cc, p, cl in cuts if cc == c and not shut(p, cl)]
        if not segs:
            continue
        rings, rest, n = stitch(segs, weld)
        joins += n
        closed += [(c, r) for r in rings]
        loose += [(c, r) for r in rest]

    made = "" if not joins else f"   ({joins} join(s) stitched)"
    made += "" if not welded else f"   ({welded} closed by coincidence, no z)"
    row("cut paths are closed", not loose,
        ("all closed" if not loose else
         f"{len(loose)} open, first at {loose[0][1][0][0]:.1f},{loose[0][1][0][1]:.1f}mm, "
         f"gap {math.dist(loose[0][1][0], loose[0][1][-1]):.3f}mm") + made)
    if not closed:
        row("something is cut out", False, "no closed cut path")
        return rows, notes

    outer_i = max(range(len(closed)), key=lambda i: area(closed[i][1]))
    outline_ink, outline = closed[outer_i]
    holes = [(c, p) for i, (c, p) in enumerate(closed) if i != outer_i]

    row("black frees the part", outline_ink == "#000000",
        f"outline cuts in {outline_ink}" +
        ("" if outline_ink == "#000000" else
         "  — black is the freeing cut" +
         ("; the open path above may be the real outline" if loose else "")))

    if outline_ink in ORDER:
        late = [c for c, _ in holes if c in ORDER and ORDER.index(c) > ORDER.index(outline_ink)]
        row("holes are cut before the outline", not late,
            "all earlier" if not late else f"{len(late)} after it: {', '.join(sorted(set(late)))}")

    out_of = [p for _, p in holes if not inside(centroid(p), outline)]
    row("holes are inside the outline", not out_of,
        f"{len(holes)} hole(s)" if not out_of else f"{len(out_of)} outside")

    if holes:
        small = []
        for _, p in holes:
            a, b, c, d = bbox(p)
            small.append(min(c - a, d - b))
        row("hole is big enough", min(small) >= min_hole,
            f"smallest {min(small):.2f}mm across, floor {min_hole:.2f}mm")

        edges = []
        for _, p in holes:
            others = [q for _, q in holes if q is not p]
            e = poly_dist(p, outline)
            for q in others:
                e = min(e, poly_dist(p, q))
            edges.append(e)
        row("edge distance", min(edges) >= min_edge,
            f"least {min(edges):.2f}mm of material, floor {min_edge:.2f}mm")
        notes.append(f"{len(holes)} hole(s); least edge {min(edges):.2f}mm, "
                     f"smallest hole {min(small):.2f}mm across")
    else:
        notes.append("no holes")

    notes.append(f"{W:.1f} x {H:.1f}mm, {len(cuts)} cut path(s), ink {', '.join(inks)}")
    return rows, notes


def main(argv=None):
    ap = argparse.ArgumentParser(description="Pre-cut gate for flat single-sheet parts.")
    ap.add_argument("files", nargs="*", type=pathlib.Path)
    ap.add_argument("--dir", action="append", default=[], type=pathlib.Path,
                    help="every .svg beneath DIR (previews/ and dot-directories skipped)")
    ap.add_argument("--bed", default=f"{BED_W:.0f}x{BED_H:.0f}",
                    help=f"work area in mm, default {BED_W:.0f}x{BED_H:.0f}")
    ap.add_argument("--min-edge", type=float, default=MIN_EDGE,
                    help=f"mm of material a hole must leave, default {MIN_EDGE}")
    ap.add_argument("--min-hole", type=float, default=MIN_HOLE,
                    help=f"mm across a hole must measure, default {MIN_HOLE}")
    ap.add_argument("--weld", type=float, default=WELD_TOL,
                    help=f"mm within which two path ends are one corner, default {WELD_TOL}")
    ap.add_argument("--quiet", action="store_true", help="only the failures")
    a = ap.parse_args(argv)

    files = list(a.files)
    for d in a.dir:
        files += [f for f in sorted(d.rglob("*.svg"))
                  if "previews" not in f.parts
                  and not any(part.startswith(".") for part in f.parts)]
    if not files:
        ap.error("no files given")

    bed = [float(v) for v in re.split(r"[x,]", a.bed)]
    checks = failed = bad_files = 0
    for f in files:
        rows, notes = check(f, bed, a.min_edge, a.min_hole, a.weld)
        n = sum(1 for _, ok, _ in rows if not ok)
        checks += len(rows)
        failed += n
        bad_files += bool(n)
        if a.quiet and not n:
            continue
        print(f"\n{f}")
        for name, ok, detail in rows:
            print(f"  {'pass' if ok else 'FAIL'}  {name:<34} {detail}")
        for line in notes:
            print(f"        {line}")

    print(f"\n  {len(files)} file(s), {checks} checks, {failed} failed"
          + (f", {bad_files} file(s) bad" if failed else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
