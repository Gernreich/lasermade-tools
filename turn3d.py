#!/usr/bin/env python3
"""Turn a flat cut file into parts you can assemble and drag round in 3D.

The bores have had a turnable viewer since they were first drawn, and the flute
and the lyre-harp grew one from their generators. The flat instruments --
kalimba, slapstick, bullroarer, buzz-disc -- are hand-drawn SVGs with no
generator behind them, so their viewer is built from the cut file itself: the
same geometry flat-part-check measures, read by the same parser, so what
turns on screen is what the laser cuts.

    import turn3d
    parts = turn3d.parts('Kalimba.svg')     # [{'o', 'h', 'e', 'box', 'ink'}]
    turn3d.write('kalimba-3d.html', title, designs, note)

A part is an outline (`o`), its holes (`h`), and the engraved lines that fall
on it (`e`), all in millimetres with y pointing UP -- the SVG's y is flipped on
the way in so a part placed in 3D reads the way it reads on the sheet. Nesting
decides which is which: a ring inside no other is an outline, a ring inside an
outline is a hole, and a ring inside a hole is an outline again (an island that
falls out with the hole is waste, and is dropped). Blue is the engrave; violet
is skip and is ignored.

Each repository's viewer.py places the parts -- that is the one thing the cut
file does not say -- and calls write(). The page is self-contained apart from
three.js r128 off cdnjs, the version the lyre-harp viewer already loads.
"""
import json
import math
import os
import re
import xml.etree.ElementTree as ET

# --- the cut file's geometry ------------------------------------------------
# These functions were flat-part-check.py's, imported from it, until that gate
# became Rust on 2026-11-07 (src/bin/flat-part-check.rs). They are kept here,
# unchanged, so the turn pages still come out byte for byte the same; the Rust
# gate does the same arithmetic in the same order. Until turn3d is ported
# too, a change to how a cut file is read is made in both places.

FLATNESS = 0.05                    # mm; curve subdivision error budget


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


def ink(el, inherited=None):
    """The stroke colour this element cuts in, or None if it is not a cut.

    STROKE IS INHERITED. An element with no stroke of its own takes its
    ancestors', which is ordinary SVG and what most generators here emit: a
    <g stroke="#000000"> around a run of bare <path d="...">. Reading only the
    element's own attributes made every such path "not a cut", so a file full
    of geometry came out as "no stroked geometry" and stopped after two checks.
    Found 2026-09-09 by running this over the bore cut files, which is a thing
    nobody had done: 69 shipped files across knotwork-soundholes, living-hinge
    and the ribbon bores were getting 2 checks each where bullroarer got 9.
    The count was the tell -- 13 files and 26 checks against 5 files and 45.
    """
    style = el.get("style") or ""
    m = re.search(r"(?<![-\w])stroke\s*:\s*([^;]+)", style)
    own = m.group(1) if m else el.get("stroke")
    c = norm(own if own is not None else inherited)
    if c in ("none", "", None):
        # a filled shape with no stroke still cuts on some importers, but in these
        # repositories every cut is stroked; treat it as artwork, not geometry
        return None
    return c


def collect(root):
    """-> [(colour, points, closed)] in millimetres, transforms applied."""
    out = []
    scale = user_scale(root)

    def walk(el, m, stroke=None):
        m = mul(m, parse_transform(el.get("transform")))
        style = el.get("style") or ""
        sm = re.search(r"(?<![-\w])stroke\s*:\s*([^;]+)", style)
        here = sm.group(1) if sm else el.get("stroke")
        if here is not None:
            stroke = here
        for kid in el:
            tag = kid.tag.split("}")[-1]
            if tag in ("g", "a", "svg"):
                walk(kid, m, stroke)
                continue
            if tag in ("defs", "metadata", "title", "desc"):
                continue
            c = ink(kid, stroke)
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


ENGRAVE = '#0000ff'
SKIP = '#8000ff'
TOL = 0.05          # mm: two path ends this close are one corner


def _simplify(p, eps=0.02):
    """Drop points that sit on the line between their neighbours."""
    if len(p) < 4:
        return p
    out = [p[0]]
    for i in range(1, len(p) - 1):
        a, b, c = out[-1], p[i], p[i + 1]
        cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        if abs(cross) > eps * max(math.dist(a, c), 1e-9):
            out.append(b)
    out.append(p[-1])
    return out


def _r(p):
    return [[round(x, 2), round(y, 2)] for x, y in p]


def parts(svg):
    """-> the cut file's parts, largest first, y up, in millimetres."""
    root = ET.parse(svg).getroot()
    got = collect(root)
    cuts, engr = {}, []
    for c, pts, closed in got:
        if c == SKIP:
            continue
        if c == ENGRAVE:
            engr.append([(x, -y) for x, y in pts])
            continue
        cuts.setdefault(c, []).append([(x, -y) for x, y in pts])
    rings = []
    for c, paths in cuts.items():
        closed = [p for p in paths if math.dist(p[0], p[-1]) <= TOL]
        rest = [p for p in paths if math.dist(p[0], p[-1]) > TOL]
        joined, _, _ = stitch(rest, TOL) if rest else ([], [], 0)
        for p in closed + joined:
            rings.append((c, _simplify(p[:-1] if p[0] == p[-1] else p)))
    rings.sort(key=lambda r: -abs(area(r[1] + [r[1][0]])))

    def holder(i):
        """The smallest ring that contains ring i, or None. Most of its
        vertices, not its first: parts laid edge to edge share a cut line, and
        a first vertex on that line reads as inside the neighbour."""
        pts = rings[i][1]
        best = None
        for j in range(i):
            ring = rings[j][1] + [rings[j][1][0]]
            if sum(inside(q, ring) for q in pts) * 2 > len(pts):
                best = j
        return best

    depth, owner = [], []
    for i in range(len(rings)):
        j = holder(i)
        depth.append(0 if j is None else depth[j] + 1)
        owner.append(j)
    out, index = [], {}
    for i, (c, p) in enumerate(rings):
        if depth[i] == 0:
            index[i] = len(out)
            out.append({'o': p, 'h': [], 'e': [], 'ink': c})
        elif depth[i] % 2 == 1 and owner[i] in index:
            out[index[owner[i]]]['h'].append(p)
    for e in engr:
        for prt in out:
            if inside(e[0], prt['o'] + [prt['o'][0]]) or \
               inside(e[len(e) // 2], prt['o'] + [prt['o'][0]]):
                prt['e'].append(e)
                break
    for prt in out:
        xs = [q[0] for q in prt['o']]
        ys = [q[1] for q in prt['o']]
        prt['box'] = (min(xs), min(ys), max(xs), max(ys))
    return out


def centred(prt, cx=None, cy=None):
    """The part moved so (cx, cy) -- default its bounding-box centre -- is the origin."""
    x0, y0, x1, y1 = prt['box']
    cx = (x0 + x1) / 2 if cx is None else cx
    cy = (y0 + y1) / 2 if cy is None else cy
    mv = lambda p: [(x - cx, y - cy) for x, y in p]
    q = dict(prt)
    q['o'] = mv(prt['o'])
    q['h'] = [mv(h) for h in prt['h']]
    q['e'] = [mv(e) for e in prt['e']]
    q['box'] = (x0 - cx, y0 - cy, x1 - cx, y1 - cy)
    return q


# --- placing: 4x4 matrices, row-major, that take the part's sheet plane -----
# (x, y, z in 0..3mm) to where it sits in the instrument.

def matmul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def T(x, y, z):
    return [[1, 0, 0, x], [0, 1, 0, y], [0, 0, 1, z], [0, 0, 0, 1]]


def Rx(t):
    c, s = math.cos(t), math.sin(t)
    return [[1, 0, 0, 0], [0, c, -s, 0], [0, s, c, 0], [0, 0, 0, 1]]


def Ry(t):
    c, s = math.cos(t), math.sin(t)
    return [[c, 0, s, 0], [0, 1, 0, 0], [-s, 0, c, 0], [0, 0, 0, 1]]


def Rz(t):
    c, s = math.cos(t), math.sin(t)
    return [[c, -s, 0, 0], [s, c, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]


def chain(*ms):
    out = T(0, 0, 0)
    for m in ms:
        out = matmul(out, m)
    return out


def placed(prt, m, explode=(0, 0, 0), wood='ply'):
    """One part ready for the page: geometry, where it goes, and which way it
    comes out when the explode slider is drawn."""
    return {'o': _r(prt['o']), 'h': [_r(h) for h in prt['h']], 'e': [_r(e) for e in prt['e']],
            'm': [[round(v, 4) for v in row] for row in m[:3]],
            'x': [round(v, 3) for v in explode], 'w': wood}


# --- the page ----------------------------------------------------------------

PAGE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>__TITLE__</title>
<!-- Written by __BY__ from the cut files beside it: do not edit by hand. -->
<style>
:root{--bg:#fbfbfa;--fg:#1a1c1e;--mut:#5b6067;--line:#e2e4e7;--card:#fff;--accent:#1d4ed8}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#15171a;--fg:#e6e8ea;--mut:#9aa1a9;
--line:#2b2f34;--card:#1b1e22;--accent:#7aa2f7}}
:root[data-theme=dark]{--bg:#15171a;--fg:#e6e8ea;--mut:#9aa1a9;--line:#2b2f34;--card:#1b1e22;--accent:#7aa2f7}
*{box-sizing:border-box}
[hidden]{display:none!important}
html,body{height:100%;margin:0}
body{background:var(--bg);color:var(--fg);display:flex;flex-direction:column;
font:14px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Helvetica,Arial,sans-serif}
.bar{display:flex;flex-wrap:wrap;align-items:center;gap:8px 14px;padding:10px 14px}
.bar b{font-weight:640;margin-right:4px}
button{font:500 13px ui-monospace,Menlo,monospace;color:var(--fg);background:var(--card);
border:1px solid var(--line);border-radius:4px;padding:5px 9px;cursor:pointer}
button[aria-pressed=true]{border-color:var(--accent);box-shadow:inset 0 -3px 0 var(--accent)}
button:focus-visible,input:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
label{display:inline-flex;gap:6px;align-items:center;color:var(--mut)}
input[type=range]{width:min(200px,40vw);accent-color:var(--accent)}
.stage{position:relative;flex:1;min-height:240px;touch-action:none}
canvas{display:block;width:100%;height:100%;cursor:grab}
.info{position:absolute;left:14px;bottom:10px;margin:0;font:12px ui-monospace,Menlo,monospace;
color:var(--mut);pointer-events:none}
.note{margin:0;padding:0 14px 12px;color:var(--mut);font-size:13px}
.note summary{cursor:pointer;width:max-content;max-width:100%}
.note summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.note p{margin:6px 0 0}
/* ?embed: framed on a writeup page, which carries the title and the note itself */
.embed .bar b,.embed .note{display:none}
.embed .bar{padding:0 0 10px}
.embed body{background:transparent}
.embed .stage{border:1px solid var(--line);border-radius:8px;background:var(--card)}
</style>
<script>
/* Follow the theme the site's pages store, so a framed viewer matches the page around it. */
(function () {
  var r = document.documentElement;
  function apply() { var t = null; try { t = localStorage.getItem('ctg-theme'); } catch (e) {}
    if (t === 'light' || t === 'dark') r.setAttribute('data-theme', t); else r.removeAttribute('data-theme'); }
  apply(); addEventListener('storage', function (e) { if (e.key === 'ctg-theme') apply(); });
  if (/[?&]embed\b/.test(location.search)) r.classList.add('embed');
})();
</script>
</head>
<body>
<div class="bar"><b>__TITLE__</b><span id="designs" role="group" aria-label="Design"></span>
<label__EXHIDE__>Explode <input id="explode" type="range" min="0" max="1" step="0.01" value="0"></label>
__SPIN__<button type="button" id="reset">Reset view</button></div>
<div class="stage" id="stage"><canvas id="cv" aria-label="__TITLE__ in 3D. Drag to turn; right-drag, shift-drag or two fingers to pan; scroll or pinch to zoom; double-click a part to turn round it."></canvas>
<p class="info" id="info"></p></div>
<details class="note"><summary>How to use this view</summary><p>__NOTE__</p></details>
<script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>
<script>
(function () {
  const DATA = __DATA__;
  const info = document.getElementById('info');
  if (!window.THREE) { info.textContent = 'The 3D view needs three.js, which did not load.'; return; }
  const cv = document.getElementById('cv'), stage = document.getElementById('stage');
  const renderer = new THREE.WebGLRenderer({ canvas: cv, antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(30, 2, 1, 8000);
  camera.up.set(0, 0, 1);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x8a8070, 0.85));
  const sun = new THREE.DirectionalLight(0xffffff, 0.7); sun.position.set(-300, -500, 700); scene.add(sun);
  const back = new THREE.DirectionalLight(0xffffff, 0.25); back.position.set(400, 300, -200); scene.add(back);
  const WOOD = { ply: '#d9c6a0', dark: '#b8925c', pale: '#e6d6b4' };
  const mats = {};
  const mat = w => mats[w] || (mats[w] = new THREE.MeshStandardMaterial({ color: WOOD[w] || WOOD.ply, roughness: 0.85, metalness: 0 }));
  const edgeMat = new THREE.LineBasicMaterial({ color: 0x3b2f20, transparent: true, opacity: 0.5 });
  const engMat = new THREE.LineBasicMaterial({ color: 0x5a3a18 });
  const cordMat = new THREE.MeshStandardMaterial({ color: 0xb03a2e, roughness: 0.9 });
  let group = null, holders = [], cur = null;
  const ring = (p, s) => { p.forEach((q, i) => i ? s.lineTo(q[0], q[1]) : s.moveTo(q[0], q[1])); s.closePath ? s.closePath() : 0; return s; };
  function build(name) {
    if (group) { scene.remove(group); group.traverse(o => { if (o.geometry) o.geometry.dispose(); }); }
    group = new THREE.Group(); holders = [];
    const d = DATA[name]; cur = d;
    d.parts.forEach(p => {
      const s = ring(p.o, new THREE.Shape());
      p.h.forEach(h => s.holes.push(ring(h, new THREE.Path())));
      const geo = new THREE.ExtrudeGeometry(s, { depth: 3, bevelEnabled: false });
      const h = new THREE.Group();
      h.add(new THREE.Mesh(geo, mat(p.w)));
      h.add(new THREE.LineSegments(new THREE.EdgesGeometry(geo, 25), edgeMat));
      p.e.forEach(e => {   // the engrave, a hair proud of both faces
        [3.06, -0.06].forEach(z => {
          const g = new THREE.BufferGeometry().setFromPoints(e.map(q => new THREE.Vector3(q[0], q[1], z)));
          h.add(new THREE.Line(g, engMat));
        });
      });
      const m = p.m, M = new THREE.Matrix4();
      M.set(m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1], m[2][2], m[2][3], 0, 0, 0, 1);
      h.matrixAutoUpdate = false; h.userData = { M, x: p.x };
      group.add(h); holders.push(h);
    });
    (d.cords || []).forEach(c => {   // a cord, as a tube through its points; a loop if it ends where it began
      const loop = c.length > 3 && c[0].every((v, i) => v === c[c.length - 1][i]);
      const curve = new THREE.CatmullRomCurve3((loop ? c.slice(0, -1) : c).map(q => new THREE.Vector3(q[0], q[1], q[2])), loop);
      group.add(new THREE.Mesh(new THREE.TubeGeometry(curve, Math.max(24, c.length * 16), 1.2, 8, loop), cordMat));
    });
    scene.add(group);
    target.set(d.c[0], d.c[1], d.c[2]);
    const v = d.view || [-0.6, 0.5];   // yaw, pitch; the distance is whatever fills the frame
    home.yaw = yaw = v[0]; home.pitch = pitch = v[1]; dist = home.dist = fit();
    info.textContent = name + ' · ' + d.parts.length + (d.parts.length === 1 ? ' part' : ' parts') + ' · ' + d.size;
    document.querySelectorAll('#designs button').forEach(b => b.setAttribute('aria-pressed', b.dataset.d === name));
    place(); render();
  }
  const target = new THREE.Vector3();
  const home = { yaw: -0.6, pitch: 0.5, dist: 600 };
  let { yaw, pitch, dist } = home, spin = 0;
  const ex = document.getElementById('explode');
  function place() {
    const k = parseFloat(ex.value) * (cur.ex || 40);
    holders.forEach(h => { const u = h.userData; h.matrix.copy(new THREE.Matrix4().makeTranslation(u.x[0] * k, u.x[1] * k, u.x[2] * k).multiply(u.M)); });
    if (cur.axis) {   // spinning about the cord, the way it is played
      const a = cur.axis, o = new THREE.Vector3(a[0], a[1], a[2]), dir = new THREE.Vector3(a[3], a[4], a[5]).normalize();
      group.matrixAutoUpdate = false;
      group.matrix.copy(new THREE.Matrix4().makeTranslation(o.x, o.y, o.z).multiply(new THREE.Matrix4().makeRotationAxis(dir, spin)).multiply(new THREE.Matrix4().makeTranslation(-o.x, -o.y, -o.z)));
    }
  }
  function render() {
    camera.position.set(target.x + dist * Math.cos(pitch) * Math.sin(yaw), target.y - dist * Math.cos(pitch) * Math.cos(yaw), target.z + dist * Math.sin(pitch));
    camera.lookAt(target);
    renderer.render(scene, camera);
  }
  function fit() {   // the nearest distance at which every corner of the design's box is on screen, with a margin
    const dir = new THREE.Vector3(Math.cos(home.pitch) * Math.sin(home.yaw), -Math.cos(home.pitch) * Math.cos(home.yaw), Math.sin(home.pitch));
    const right = new THREE.Vector3().crossVectors(dir.clone().negate(), camera.up).normalize();
    const upv = new THREE.Vector3().crossVectors(right, dir.clone().negate());
    const tv = Math.tan(camera.fov * Math.PI / 360) * 0.9, th = tv * camera.aspect, e = cur.e;
    let n = MIN;
    for (const sx of [-1, 1]) for (const sy of [-1, 1]) for (const sz of [-1, 1]) {
      const q = new THREE.Vector3(sx * e[0], sy * e[1], sz * e[2]), w = q.dot(dir);
      n = Math.max(n, w + Math.abs(q.dot(right)) / th, w + Math.abs(q.dot(upv)) / tv);
    }
    return n;
  }
  function size() {
    const w = stage.clientWidth, h = stage.clientHeight;
    renderer.setSize(w, h, false); camera.aspect = w / Math.max(h, 1); camera.updateProjectionMatrix();
    const atHome = dist === home.dist; home.dist = fit(); if (atHome) dist = home.dist;   // refit unless it has been zoomed
    render();
  }
  const pts = new Map(); let pinch0 = 0, mid0 = null, panning = false;
  const MIN = 10, ray = new THREE.Raycaster(), ndc = new THREE.Vector2();
  function under(cx, cy, partOnly) {   // the part under the pointer, else where its ray meets the plane through the target
    const b = cv.getBoundingClientRect();
    ndc.set((cx - b.left) / b.width * 2 - 1, -(cy - b.top) / b.height * 2 + 1);
    ray.setFromCamera(ndc, camera);
    const hit = ray.intersectObjects(holders, true).find(h => h.object.isMesh);
    if (hit || partOnly) return hit ? hit.point : null;
    const n = camera.getWorldDirection(new THREE.Vector3());
    return ray.ray.intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(n, target), new THREE.Vector3());
  }
  function pan(dx, dy) {   // the model follows the pointer
    const k = 2 * dist * Math.tan(camera.fov * Math.PI / 360) / Math.max(cv.clientHeight, 1);
    target.addScaledVector(new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld, 0), -dx * k)
          .addScaledVector(new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld, 1), dy * k);
  }
  function zoom(s, cx, cy) {   // scale the view about the point under the pointer, which stays where it is on screen
    const nd = Math.max(MIN, Math.min(5000, dist * s)), p = under(cx, cy);
    if (p) target.sub(p).multiplyScalar(nd / dist).add(p);
    dist = nd;
  }
  const two = () => { const [a, b] = [...pts.values()]; return { h: Math.hypot(a.x - b.x, a.y - b.y), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }; };
  cv.addEventListener('contextmenu', e => e.preventDefault());
  cv.addEventListener('pointerdown', e => {
    cv.setPointerCapture(e.pointerId); pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    panning = e.button === 1 || e.button === 2 || e.shiftKey;
    if (pts.size === 2) { mid0 = two(); pinch0 = mid0.h; }
  });
  cv.addEventListener('pointermove', e => {
    if (!pts.has(e.pointerId)) return;
    const prev = pts.get(e.pointerId); pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
    const dx = e.clientX - prev.x, dy = e.clientY - prev.y;
    if (pts.size === 1) {
      if (panning) pan(dx, dy);
      else { yaw -= dx * 0.008; pitch = Math.max(-1.5, Math.min(1.5, pitch + dy * 0.008)); }
    } else if (pts.size === 2 && pinch0) {   // two fingers: pinch to zoom, move together to pan
      const m = two(); pan(m.x - mid0.x, m.y - mid0.y); render();
      if (m.h) zoom(pinch0 / m.h, m.x, m.y); mid0 = m; pinch0 = m.h;
    }
    render();
  });
  const up = e => { pts.delete(e.pointerId); if (pts.size < 2) pinch0 = 0; };
  cv.addEventListener('pointerup', up); cv.addEventListener('pointercancel', up);
  cv.addEventListener('wheel', e => { e.preventDefault(); zoom(1 + Math.sign(e.deltaY) * 0.08, e.clientX, e.clientY); render(); }, { passive: false });
  cv.addEventListener('dblclick', e => {   // turn round the part under the pointer from now on
    const p = under(e.clientX, e.clientY, true); if (!p) return;
    const a = target.clone(), t0 = performance.now();
    const step = t => { const k = Math.min(1, (t - t0) / 300); target.lerpVectors(a, p, k * (2 - k)); render(); if (k < 1) requestAnimationFrame(step); };
    requestAnimationFrame(step);
  });
  ex.addEventListener('input', () => { place(); render(); });
  document.getElementById('reset').addEventListener('click', () => { yaw = home.yaw; pitch = home.pitch; dist = home.dist; target.set(cur.c[0], cur.c[1], cur.c[2]); ex.value = 0; spin = 0; place(); render(); });
  const sp = document.getElementById('spin');
  if (sp) {
    let last = 0;
    const tick = t => { if (!sp.checked) { last = 0; return; } if (last) spin += (t - last) * 0.004; last = t; place(); render(); requestAnimationFrame(tick); };
    sp.addEventListener('change', () => { if (sp.checked) requestAnimationFrame(tick); });
  }
  const bar = document.getElementById('designs'), names = Object.keys(DATA);
  if (names.length > 1) names.forEach(n => {
    const b = document.createElement('button'); b.type = 'button'; b.textContent = n; b.dataset.d = n;
    b.addEventListener('click', () => build(n)); bar.appendChild(b);
  });
  window.addEventListener('resize', size);
  if (window.ResizeObserver) new ResizeObserver(size).observe(stage);   // opening the note resizes the stage
  build(names[0]); size();
})();
</script>
</body>
</html>
"""

SPIN = '<label><input type="checkbox" id="spin"> Spin it</label>'


def bounds(design):
    """Centre, radius, size and half-extents of a design as placed, from its outline vertices."""
    xs, ys, zs = [], [], []
    for p in design['parts']:
        m = p['m']
        for x, y in p['o']:
            for z in (0, 3):
                xs.append(m[0][0] * x + m[0][1] * y + m[0][2] * z + m[0][3])
                ys.append(m[1][0] * x + m[1][1] * y + m[1][2] * z + m[1][3])
                zs.append(m[2][0] * x + m[2][1] * y + m[2][2] * z + m[2][3])
    for c in design.get('cords', []):
        for x, y, z in c:
            xs.append(x); ys.append(y); zs.append(z)
    c = [(min(v) + max(v)) / 2 for v in (xs, ys, zs)]
    e = [(max(v) - min(v)) / 2 for v in (xs, ys, zs)]
    size = ' × '.join(f'{max(v) - min(v):.0f}' for v in (xs, ys, zs) if max(v) - min(v) > 0.5) + 'mm'
    return [round(v, 2) for v in c], round(max(e), 2), size, [round(v, 2) for v in e]


def write(path, title, designs, note, by, spin=False):
    """designs: {name: {'parts': [placed(...)], 'cords': [[xyz...], a loop if last == first], 'axis': [ox,oy,oz,dx,dy,dz],
    'ex': mm the explode slider moves a part per unit of its 'x', 'view': [yaw, pitch]}};
    a third 'view' number, the distance in radii it once set, is ignored: the view opens fitted to its frame"""
    for d in designs.values():
        d['c'], d['r'], d['size'], d['e'] = bounds(d)
    page = (PAGE.replace('__DATA__', json.dumps(designs, separators=(',', ':')))
            .replace('__TITLE__', title).replace('__NOTE__', note).replace('__BY__', by)
            .replace('__SPIN__', SPIN if spin else '')
            # one part has nothing to come apart: the slider stays in the page,
            # since the script reads it, but out of sight
            .replace('__EXHIDE__', '' if any(d.get('ex', 40) for d in designs.values()) else ' hidden'))
    with open(path, 'w') as f:
        f.write(page)
    return page
